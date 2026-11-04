import sqlite3
from dataclasses import dataclass
from datetime import datetime

from telegram import ReactionTypeEmoji

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
CREATE TABLE IF NOT EXISTS posts (
    chat_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    posted_at INTEGER NOT NULL,
    hearts INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (chat_id, message_id)
);
CREATE INDEX IF NOT EXISTS posts_by_age ON posts (posted_at);
"""
GROUP_SETTINGS = {"music_topic_id", "starboard_topic_id", "required_hearts", "emoji", "self_stars"}


@dataclass
class Group:
    chat_id: int
    music_topic_id: int | None
    starboard_topic_id: int | None
    required_hearts: int
    emoji: str
    self_stars: bool

    @property
    def is_set_up(self) -> bool:
        return None not in (self.music_topic_id, self.starboard_topic_id)

    @property
    def heart_reaction(self) -> ReactionTypeEmoji:
        return ReactionTypeEmoji(self.emoji)


@dataclass
class Post:
    chat_id: int
    message_id: int
    author_id: int
    posted_at: int
    hearts: int


# Each statement commits on its own, so every change is on disk as soon as it's made.
connection = sqlite3.connect(DATABASE_FILE, isolation_level=None)
connection.row_factory = sqlite3.Row
connection.executescript(SCHEMA)


def find_group(chat_id: int) -> Group | None:
    row = connection.execute("SELECT * FROM groups WHERE chat_id = ?", (chat_id,)).fetchone()
    if row is None:
        return None
    return Group(**{**dict(row), "self_stars": bool(row["self_stars"])})


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


def remember_post(chat_id: int, message_id: int, author_id: int, posted_at: datetime) -> None:
    connection.execute(
        "INSERT OR IGNORE INTO posts (chat_id, message_id, author_id, posted_at) VALUES (?, ?, ?, ?)",
        (chat_id, message_id, author_id, int(posted_at.timestamp())),
    )


def find_post(chat_id: int, message_id: int) -> Post | None:
    row = connection.execute(
        "SELECT * FROM posts WHERE chat_id = ? AND message_id = ?", (chat_id, message_id)
    ).fetchone()
    return Post(**row) if row else None


def save_hearts(post: Post) -> None:
    connection.execute(
        "UPDATE posts SET hearts = ? WHERE chat_id = ? AND message_id = ?",
        (post.hearts, post.chat_id, post.message_id),
    )


def forget_post(chat_id: int, message_id: int) -> None:
    connection.execute(
        "DELETE FROM posts WHERE chat_id = ? AND message_id = ?", (chat_id, message_id)
    )


def forget_posts_before(cutoff: datetime) -> None:
    connection.execute("DELETE FROM posts WHERE posted_at < ?", (int(cutoff.timestamp()),))
