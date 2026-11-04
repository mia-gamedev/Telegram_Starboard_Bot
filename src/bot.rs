use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use serde::Deserialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_json::{Value, json};

use crate::clock::unix_now;
use crate::config::{MAX_POST_AGE_SECONDS, MAX_TRACKED_POSTS};
use crate::database::{Database, Group, Post};
use crate::log;
use crate::telegram::{self, ApiError, TelegramApi};
use crate::updates::{EntityKind, Message, RawUpdate, Reaction, ReactionType};

#[derive(Debug, Clone, Deserialize)]
pub struct BotAccount {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_post_age_seconds: i64,
    pub max_tracked_posts: i64,
    pub retry_delay: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_post_age_seconds: MAX_POST_AGE_SECONDS,
            max_tracked_posts: MAX_TRACKED_POSTS,
            retry_delay: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub enum BotError {
    Database(rusqlite::Error),
    Telegram(ApiError),
    BadUpdate(serde_json::Error),
}

impl fmt::Display for BotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database: {error}"),
            Self::Telegram(error) => write!(formatter, "Telegram: {error}"),
            Self::BadUpdate(error) => write!(formatter, "unreadable update: {error}"),
        }
    }
}

impl From<rusqlite::Error> for BotError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<ApiError> for BotError {
    fn from(error: ApiError) -> Self {
        Self::Telegram(error)
    }
}

impl From<serde_json::Error> for BotError {
    fn from(error: serde_json::Error) -> Self {
        Self::BadUpdate(error)
    }
}

#[derive(Deserialize)]
struct SentMessage {
    message_id: i64,
}

pub struct Bot<A: TelegramApi> {
    pub api: A,
    pub database: Database,
    pub account: BotAccount,
    pub limits: Limits,
}

impl<A: TelegramApi> Bot<A> {
    pub fn new(api: A, database: Database, account: BotAccount) -> Self {
        Self {
            api,
            database,
            account,
            limits: Limits::default(),
        }
    }

    pub fn call<T: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &Value,
    ) -> Result<T, ApiError> {
        let body = self.api.post(method, &params.to_string())?;
        telegram::parse_reply(method, &body)
    }

    pub fn handle_safely(&mut self, update: &RawUpdate<'_>) {
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| self.handle_update(update)));
        let problem = match outcome {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error.to_string(),
            Err(_) => "the bot crashed (see the panic logged above)".to_string(),
        };
        log::error(&format!(
            "Failed to handle update {}: {problem}\n{}",
            update.update_id,
            update.body()
        ));
    }

    pub fn handle_update(&mut self, update: &RawUpdate<'_>) -> Result<(), BotError> {
        if let Some(raw) = update.message_reaction {
            return self.track_hearts(&serde_json::from_str(raw.get())?);
        }
        if let Some(raw) = update.message {
            return self.handle_message(&serde_json::from_str(raw.get())?);
        }
        Ok(())
    }

    fn handle_message(&mut self, message: &Message) -> Result<(), BotError> {
        if let Some(command) = message.text.as_deref().filter(|text| text.starts_with('/')) {
            return self.handle_command(message, command);
        }
        if is_music_post(message) {
            self.remember_music_post(message)?;
        }
        Ok(())
    }

    fn remember_music_post(&mut self, message: &Message) -> Result<(), BotError> {
        // Reaction updates lack the topic, so remember posts as they're made.
        let chat_id = message.chat.id;
        let Some(group) = self.database.find_group(chat_id)?.filter(Group::is_set_up) else {
            return Ok(());
        };
        let Some(author) = message.from.as_ref() else {
            return Ok(());
        };
        if message.message_thread_id != group.music_topic_id {
            return Ok(());
        }
        let post = Post {
            chat_id,
            message_id: message.message_id,
            author_id: author.id,
            posted_at: message.date,
            hearts: 0,
        };
        self.database.remember_post(&post)?;
        // Busy groups get a shorter wait instead of unbounded storage.
        self.database
            .forget_group_posts_before(chat_id, self.post_age_cutoff())?;
        self.database
            .forget_oldest_posts(chat_id, self.limits.max_tracked_posts)?;
        Ok(())
    }

    fn track_hearts(&mut self, reaction: &Reaction) -> Result<(), BotError> {
        let Some(group) = self
            .database
            .find_group(reaction.chat.id)?
            .filter(Group::is_set_up)
        else {
            return Ok(());
        };
        let change = heart_change(reaction, &group.emoji);
        if change == 0 {
            return Ok(());
        }
        let Some(post) = self
            .database
            .find_post(group.chat_id, reaction.message_id)?
        else {
            return Ok(());
        };
        if is_ignored_self_star(reaction, &post, &group) {
            return Ok(());
        }
        if post.posted_at < self.post_age_cutoff() {
            self.database.forget_post(group.chat_id, post.message_id)?;
            return Ok(());
        }
        let hearts = post.hearts + change;
        self.database
            .save_hearts(group.chat_id, post.message_id, hearts)?;
        if hearts >= group.required_hearts {
            self.promote_to_starboard(&group, &post)?;
        }
        Ok(())
    }

    fn promote_to_starboard(&mut self, group: &Group, post: &Post) -> Result<(), BotError> {
        let forward = json!({
            "chat_id": group.chat_id,
            "from_chat_id": group.chat_id,
            "message_id": post.message_id,
            "message_thread_id": group.starboard_topic_id,
        });
        let starboard_post: SentMessage = self.call("forwardMessage", &forward)?;
        self.database.forget_post(group.chat_id, post.message_id)?;
        log::info(&format!(
            "Forwarded post {} in group {} to the starboard",
            post.message_id, group.chat_id
        ));
        // The bot adds the first heart.
        let first_heart = json!({
            "chat_id": group.chat_id,
            "message_id": starboard_post.message_id,
            "reaction": [{"type": "emoji", "emoji": group.emoji}],
        });
        let _: IgnoredAny = self.call("setMessageReaction", &first_heart)?;
        Ok(())
    }

    pub fn post_age_cutoff(&self) -> i64 {
        unix_now() - self.limits.max_post_age_seconds
    }
}

pub fn is_music_post(message: &Message) -> bool {
    if !message.is_topic_message {
        return false;
    }
    let is_audio_file = message
        .document
        .as_ref()
        .and_then(|document| document.mime_type.as_deref())
        .is_some_and(|mime_type| mime_type.starts_with("audio/"));
    if message.audio.is_some() || is_audio_file {
        return true;
    }
    message
        .entities
        .iter()
        .chain(&message.caption_entities)
        .any(|entity| matches!(entity.kind, EntityKind::Url | EntityKind::TextLink))
}

fn heart_change(reaction: &Reaction, emoji: &str) -> i64 {
    let has_heart = |reactions: &[ReactionType]| {
        reactions
            .iter()
            .any(|item| item.kind == "emoji" && item.emoji.as_deref() == Some(emoji))
    };
    i64::from(has_heart(&reaction.new_reaction)) - i64::from(has_heart(&reaction.old_reaction))
}

fn is_ignored_self_star(reaction: &Reaction, post: &Post, group: &Group) -> bool {
    !group.self_stars
        && reaction
            .user
            .as_ref()
            .is_some_and(|user| user.id == post.author_id)
}
