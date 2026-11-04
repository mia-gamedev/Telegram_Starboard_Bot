//! The parts of Telegram updates the bot reads; serde skips every other field.

use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::value::RawValue;

/// One update with its message or reaction left unparsed, so a malformed one only fails itself.
#[derive(Deserialize)]
pub struct RawUpdate<'a> {
    pub update_id: i64,
    #[serde(borrow)]
    pub message: Option<&'a RawValue>,
    #[serde(borrow)]
    pub message_reaction: Option<&'a RawValue>,
}

impl RawUpdate<'_> {
    pub fn body(&self) -> &str {
        self.message_reaction
            .or(self.message)
            .map_or("", |raw| raw.get())
    }
}

#[derive(Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub date: i64,
    pub chat: Chat,
    pub from: Option<User>,
    pub sender_chat: Option<Chat>,
    pub message_thread_id: Option<i64>,
    #[serde(default)]
    pub is_topic_message: bool,
    pub text: Option<String>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub caption_entities: Vec<Entity>,
    pub audio: Option<IgnoredAny>,
    pub document: Option<Document>,
}

#[derive(Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: ChatType,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ChatType {
    Private,
    Group,
    Supergroup,
    Channel,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub struct User {
    pub id: i64,
}

#[derive(Deserialize)]
pub struct Entity {
    #[serde(rename = "type")]
    pub kind: EntityKind,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Url,
    TextLink,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub struct Document {
    pub mime_type: Option<String>,
}

#[derive(Deserialize)]
pub struct Reaction {
    pub chat: Chat,
    pub message_id: i64,
    pub user: Option<User>,
    pub old_reaction: Vec<ReactionType>,
    pub new_reaction: Vec<ReactionType>,
}

#[derive(Deserialize)]
pub struct ReactionType {
    #[serde(rename = "type")]
    pub kind: String,
    pub emoji: Option<String>,
}
