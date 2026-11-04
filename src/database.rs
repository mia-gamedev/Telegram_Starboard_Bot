use std::path::Path;

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Result, Row, params};

use crate::config::{DEFAULT_EMOJI, DEFAULT_REQUIRED_HEARTS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub chat_id: i64,
    pub music_topic_id: Option<i64>,
    pub starboard_topic_id: Option<i64>,
    pub required_hearts: i64,
    pub emoji: String,
    pub self_stars: bool,
}

impl Group {
    pub fn is_set_up(&self) -> bool {
        self.music_topic_id.is_some() && self.starboard_topic_id.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Post {
    pub chat_id: i64,
    pub message_id: i64,
    pub author_id: i64,
    pub posted_at: i64,
    pub hearts: i64,
}

pub enum GroupSetting {
    MusicTopic(i64),
    StarboardTopic(i64),
    RequiredHearts(i64),
    Emoji(String),
    SelfStars(bool),
}

impl GroupSetting {
    fn column_and_value(&self) -> (&'static str, Value) {
        match self {
            Self::MusicTopic(topic_id) => ("music_topic_id", Value::Integer(*topic_id)),
            Self::StarboardTopic(topic_id) => ("starboard_topic_id", Value::Integer(*topic_id)),
            Self::RequiredHearts(hearts) => ("required_hearts", Value::Integer(*hearts)),
            Self::Emoji(emoji) => ("emoji", Value::Text(emoji.clone())),
            Self::SelfStars(enabled) => ("self_stars", Value::Integer(i64::from(*enabled))),
        }
    }
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        // WAL with synchronous=NORMAL saves without waiting on the disk for every change;
        // a crash can't corrupt the file, at worst the last moment before a power cut is lost.
        connection
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        // 500 KB instead of SQLite's 2 MB default: measured no slower, since the OS file cache serves misses.
        connection.pragma_update(None, "cache_size", -500)?;
        connection.execute_batch(&schema())?;
        Ok(Self { connection })
    }

    pub fn begin(&self) -> Result<()> {
        self.connection.execute_batch("BEGIN")
    }

    pub fn commit(&self) -> Result<()> {
        self.connection.execute_batch("COMMIT")
    }

    pub fn rollback(&self) -> Result<()> {
        self.connection.execute_batch("ROLLBACK")
    }

    pub fn find_group(&self, chat_id: i64) -> Result<Option<Group>> {
        self.connection
            .prepare_cached(
                "SELECT chat_id, music_topic_id, starboard_topic_id, required_hearts, emoji, self_stars \
                 FROM groups WHERE chat_id = ?1",
            )?
            .query_row([chat_id], group_from_row)
            .optional()
    }

    /// The group's settings, saving the defaults first if the group is new.
    pub fn load_group(&self, chat_id: i64) -> Result<Group> {
        self.connection
            .prepare_cached("INSERT OR IGNORE INTO groups (chat_id) VALUES (?1)")?
            .execute([chat_id])?;
        self.find_group(chat_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn save_group_setting(&self, chat_id: i64, setting: &GroupSetting) -> Result<()> {
        let (column, value) = setting.column_and_value();
        let sql = format!(
            "INSERT INTO groups (chat_id, {column}) VALUES (?1, ?2) \
             ON CONFLICT (chat_id) DO UPDATE SET {column} = excluded.{column}"
        );
        self.connection
            .prepare_cached(&sql)?
            .execute(params![chat_id, value])?;
        Ok(())
    }

    pub fn remember_post(&self, post: &Post) -> Result<()> {
        self.connection
            .prepare_cached(
                "INSERT OR IGNORE INTO posts (chat_id, message_id, author_id, posted_at) VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![post.chat_id, post.message_id, post.author_id, post.posted_at])?;
        Ok(())
    }

    pub fn find_post(&self, chat_id: i64, message_id: i64) -> Result<Option<Post>> {
        self.connection
            .prepare_cached(
                "SELECT chat_id, message_id, author_id, posted_at, hearts FROM posts \
                 WHERE chat_id = ?1 AND message_id = ?2",
            )?
            .query_row([chat_id, message_id], post_from_row)
            .optional()
    }

    pub fn save_hearts(&self, chat_id: i64, message_id: i64, hearts: i64) -> Result<()> {
        self.connection
            .prepare_cached("UPDATE posts SET hearts = ?1 WHERE chat_id = ?2 AND message_id = ?3")?
            .execute([hearts, chat_id, message_id])?;
        Ok(())
    }

    pub fn forget_post(&self, chat_id: i64, message_id: i64) -> Result<()> {
        self.connection
            .prepare_cached("DELETE FROM posts WHERE chat_id = ?1 AND message_id = ?2")?
            .execute([chat_id, message_id])?;
        Ok(())
    }

    pub fn forget_group_posts_before(&self, chat_id: i64, cutoff: i64) -> Result<()> {
        self.connection
            .prepare_cached("DELETE FROM posts WHERE chat_id = ?1 AND posted_at < ?2")?
            .execute([chat_id, cutoff])?;
        Ok(())
    }

    pub fn forget_posts_before(&self, cutoff: i64) -> Result<()> {
        self.connection
            .prepare_cached("DELETE FROM posts WHERE posted_at < ?1")?
            .execute([cutoff])?;
        Ok(())
    }

    pub fn forget_oldest_posts(&self, chat_id: i64, keep: i64) -> Result<()> {
        // Message ids grow over time, so everything below the newest `keep` ids is older.
        self.connection
            .prepare_cached(
                "DELETE FROM posts WHERE chat_id = ?1 AND message_id <= (\
                 SELECT message_id FROM posts WHERE chat_id = ?1 ORDER BY message_id DESC LIMIT 1 OFFSET ?2)",
            )?
            .execute([chat_id, keep])?;
        Ok(())
    }
}

fn schema() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS groups (
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
        ) WITHOUT ROWID;"
    )
}

fn group_from_row(row: &Row<'_>) -> Result<Group> {
    Ok(Group {
        chat_id: row.get(0)?,
        music_topic_id: row.get(1)?,
        starboard_topic_id: row.get(2)?,
        required_hearts: row.get(3)?,
        emoji: row.get(4)?,
        self_stars: row.get(5)?,
    })
}

fn post_from_row(row: &Row<'_>) -> Result<Post> {
    Ok(Post {
        chat_id: row.get(0)?,
        message_id: row.get(1)?,
        author_id: row.get(2)?,
        posted_at: row.get(3)?,
        hearts: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Database {
        Database::open(Path::new(":memory:")).expect("in-memory database opens")
    }

    #[test]
    fn every_post_and_group_query_goes_straight_to_its_rows() {
        let database = in_memory();
        let queries = [
            "SELECT * FROM groups WHERE chat_id = 1",
            "SELECT * FROM posts WHERE chat_id = 1 AND message_id = 1",
            "UPDATE posts SET hearts = 1 WHERE chat_id = 1 AND message_id = 1",
            "DELETE FROM posts WHERE chat_id = 1 AND message_id = 1",
            "DELETE FROM posts WHERE chat_id = 1 AND posted_at < 5",
            "DELETE FROM posts WHERE chat_id = 1 AND message_id <= (SELECT message_id FROM posts \
             WHERE chat_id = 1 ORDER BY message_id DESC LIMIT 1 OFFSET 5)",
        ];
        for sql in queries {
            let mut statement = database
                .connection
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap();
            let plan: Vec<String> = statement
                .query_map([], |row| row.get(3))
                .unwrap()
                .collect::<Result<_>>()
                .unwrap();
            assert!(!plan.join(" | ").contains("SCAN"), "{sql}: {plan:?}");
        }
    }

    #[test]
    fn new_groups_get_the_default_settings() {
        let group = in_memory().load_group(-100).unwrap();
        assert_eq!(
            (
                group.required_hearts,
                group.emoji.as_str(),
                group.self_stars
            ),
            (2, "\u{2764}", true)
        );
        assert!(!group.is_set_up());
    }

    #[test]
    fn cap_keeps_the_newest_posts_of_that_group_only() {
        let database = in_memory();
        for message_id in 1..=5 {
            for chat_id in [-1, -2] {
                let post = Post {
                    chat_id,
                    message_id,
                    author_id: 7,
                    posted_at: 0,
                    hearts: 0,
                };
                database.remember_post(&post).unwrap();
            }
        }
        database.forget_oldest_posts(-1, 3).unwrap();
        let kept = |chat_id| {
            (1..=5)
                .filter(|id| database.find_post(chat_id, *id).unwrap().is_some())
                .collect::<Vec<_>>()
        };
        assert_eq!(kept(-1), [3, 4, 5]);
        assert_eq!(kept(-2), [1, 2, 3, 4, 5]);
    }
}
