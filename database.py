import sqlite3
from collections import namedtuple
from contextlib import contextmanager

from config import DATABASE_FILE, DEFAULT_EMOJI, DEFAULT_REQUIRED_HEARTS

SCHEMA = f"""
CREATE TABLE IF NOT EXISTS groups (
    chat_id INTEGER PRIMARY KEY,
    music_topic_id INTEGER,
    starboard_topic_id INTEGER,
    required_hearts INTEGER NOT NULL DEFAULT {DEFAULT_REQUIRED_HEARTS},
    emoji TEXT NOT NULL DEFAULT '{DEFAULT_EMOJI}',
    self_stars INTEGER NOT NULL DEFAULT 1
);
-- WITHOUT ROWID keeps posts sorted by group and message, with no hidden row id or extra index.
CREATE TABLE IF NOT EXISTS posts (
    chat_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    posted_at INTEGER NOT NULL,
    hearts INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (chat_id, message_id)
) WITHOUT ROWID;
"""
GROUP_SETTINGS = {"music_topic_id", "starboard_topic_id", "required_hearts", "emoji", "self_stars"}


# namedtuple rather than dataclass: dataclasses would load ~1.5 MB of extra modules.
class Group(namedtuple("Group", "chat_id music_topic_id starboard_topic_id required_hearts emoji self_stars")):
    __slots__ = ()

    @property
    def is_set_up(self) -> bool:
        return None not in (self.music_topic_id, self.starboard_topic_id)


Post = namedtuple("Post", "chat_id message_id author_id posted_at hearts")


connection = sqlite3.connect(DATABASE_FILE, isolation_level=None)
# WAL with synchronous=NORMAL saves without waiting on the disk for every change;
# a crash can't corrupt the file, at worst the last moment of changes before a power cut is lost.
connection.execute("PRAGMA journal_mode = WAL")
connection.execute("PRAGMA synchronous = NORMAL")
# 500 KB instead of SQLite's 2 MB default: measured no slower, since the OS file cache serves misses.
connection.execute("PRAGMA cache_size = -500")
connection.executescript(SCHEMA)


@contextmanager
def transaction():
    connection.execute("BEGIN")
    with connection:
        yield


def find_group(chat_id: int) -> Group | None:
    row = connection.execute("SELECT * FROM groups WHERE chat_id = ?", (chat_id,)).fetchone()
    if row is None:
        return None
    *columns, self_stars = row
    return Group(*columns, bool(self_stars))


def load_group(chat_id: int) -> Group:
    connection.execute("INSERT OR IGNORE INTO groups (chat_id) VALUES (?)", (chat_id,))
    return find_group(chat_id)


def save_group_setting(chat_id: int, setting: str, value: object) -> None:
    # Column names can't be query parameters, so only known columns get into the SQL.
    if setting not in GROUP_SETTINGS:
        raise ValueError(f"Unknown group setting: {setting}")
    connection.execute(
        f"INSERT INTO groups (chat_id, {setting}) VALUES (?, ?) "
        f"ON CONFLICT (chat_id) DO UPDATE SET {setting} = excluded.{setting}",
        (chat_id, value),
    )


def remember_post(chat_id: int, message_id: int, author_id: int, posted_at: int) -> None:
    connection.execute(
        "INSERT OR IGNORE INTO posts (chat_id, message_id, author_id, posted_at) VALUES (?, ?, ?, ?)",
        (chat_id, message_id, author_id, posted_at),
    )


def forget_oldest_posts(chat_id: int, keep: int) -> None:
    # Message ids grow over time, so everything below the newest `keep` ids is older.
    connection.execute(
        "DELETE FROM posts WHERE chat_id = ? AND message_id <= ("
        "SELECT message_id FROM posts WHERE chat_id = ? ORDER BY message_id DESC LIMIT 1 OFFSET ?)",
        (chat_id, chat_id, keep),
    )


def find_post(chat_id: int, message_id: int) -> Post | None:
    row = connection.execute(
        "SELECT * FROM posts WHERE chat_id = ? AND message_id = ?", (chat_id, message_id)
    ).fetchone()
    return Post(*row) if row else None


def save_hearts(chat_id: int, message_id: int, hearts: int) -> None:
    connection.execute(
        "UPDATE posts SET hearts = ? WHERE chat_id = ? AND message_id = ?",
        (hearts, chat_id, message_id),
    )


def forget_post(chat_id: int, message_id: int) -> None:
    connection.execute(
        "DELETE FROM posts WHERE chat_id = ? AND message_id = ?", (chat_id, message_id)
    )


def forget_group_posts_before(chat_id: int, cutoff: int) -> None:
    connection.execute("DELETE FROM posts WHERE chat_id = ? AND posted_at < ?", (chat_id, cutoff))


def forget_posts_before(cutoff: int) -> None:
    connection.execute("DELETE FROM posts WHERE posted_at < ?", (cutoff,))
