"""Build the full-size benchmark database: 50,000 groups after a month of use.

Huge and big groups sit at the 1,000-post cap, small ones at 150 posts: 12.2 million posts.
The schema comes from the bot's own database module. Run directly to rebuild:
    python tests/seed_database.py
"""
import shutil
import time

import sandbox
import workload

BIG_GROUPS = 5_500
POSTS_PER_BIG_GROUP = 1000
POSTS_PER_SMALL_GROUP = 150
GROUPS_PER_COMMIT = 5_000


def posts_for(index: int, now: int):
    count = POSTS_PER_BIG_GROUP if index < BIG_GROUPS else POSTS_PER_SMALL_GROUP
    chat_id = workload.group_id(index)
    return ((chat_id, 1_000_000 + n, 6_000_000_000 + n % 300, now - n * 600, n % 2) for n in range(count))


def seed(connection) -> None:
    now = int(time.time())
    groups = ((workload.group_id(i), workload.MUSIC, workload.STARBOARD) for i in range(workload.GROUPS))
    connection.execute("BEGIN")
    connection.executemany(
        "INSERT INTO groups (chat_id, music_topic_id, starboard_topic_id) VALUES (?, ?, ?)", groups
    )
    connection.execute("COMMIT")
    for first in range(0, workload.GROUPS, GROUPS_PER_COMMIT):
        connection.execute("BEGIN")
        for index in range(first, first + GROUPS_PER_COMMIT):
            connection.executemany("INSERT INTO posts VALUES (?, ?, ?, ?, ?)", posts_for(index, now))
        connection.execute("COMMIT")


def main() -> None:
    directory = sandbox.create()
    import database

    print("Building the full-size benchmark database (about 20 s, ~600 MB)...")
    seed(database.connection)
    database.connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    database.connection.close()
    sandbox.FULL_SIZE_DATABASE.parent.mkdir(exist_ok=True)
    shutil.copyfile(directory / "starboard.db", sandbox.FULL_SIZE_DATABASE)
    print(f"Saved to {sandbox.FULL_SIZE_DATABASE}")


if __name__ == "__main__":
    main()
