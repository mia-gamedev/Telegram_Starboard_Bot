use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::json;

use crate::bot::{Bot, BotError};
use crate::config;
use crate::database::{Group, GroupSetting};
use crate::telegram::TelegramApi;
use crate::updates::{ChatType, Message};

const BULLET: &str = "•";

#[derive(Deserialize)]
struct ChatMember {
    status: String,
}

impl<A: TelegramApi> Bot<A> {
    pub(crate) fn handle_command(&mut self, message: &Message, text: &str) -> Result<(), BotError> {
        let mut words = text.split_whitespace();
        let command = words.next().unwrap_or_default().trim_start_matches('/');
        let arguments: Vec<&str> = words.collect();
        let (name, addressee) = command.split_once('@').unwrap_or((command, ""));
        if !addressee.is_empty() && !addressee.eq_ignore_ascii_case(&self.account.username) {
            return Ok(());
        }
        if !matches!(name, "settings" | "setup" | "watch") {
            return Ok(());
        }
        match message.chat.kind {
            ChatType::Private => self.explain_group_only(message),
            ChatType::Group | ChatType::Supergroup => {
                self.run_group_command(name, message, &arguments)
            }
            _ => Ok(()),
        }
    }

    fn run_group_command(
        &mut self,
        name: &str,
        message: &Message,
        arguments: &[&str],
    ) -> Result<(), BotError> {
        match name {
            "settings" => self.settings_command(message, arguments),
            "setup" => self.setup_command(message),
            "watch" => self.watch_command(message),
            _ => Ok(()),
        }
    }

    fn reply(&mut self, message: &Message, text: &str) -> Result<(), BotError> {
        let mut params = json!({
            "chat_id": message.chat.id,
            "text": text,
            "reply_parameters": {"message_id": message.message_id, "allow_sending_without_reply": true},
        });
        if message.is_topic_message {
            params["message_thread_id"] = json!(message.message_thread_id);
        }
        let _: IgnoredAny = self.call("sendMessage", &params)?;
        Ok(())
    }

    fn is_admin(&mut self, chat_id: i64, user_id: i64) -> Result<bool, BotError> {
        let member: ChatMember = self.call(
            "getChatMember",
            &json!({"chat_id": chat_id, "user_id": user_id}),
        )?;
        Ok(matches!(
            member.status.as_str(),
            "administrator" | "creator"
        ))
    }

    fn sent_by_admin(&mut self, message: &Message) -> Result<bool, BotError> {
        // Anonymous admins post as the group itself.
        if let Some(sender_chat) = &message.sender_chat {
            return Ok(sender_chat.id == message.chat.id);
        }
        let Some(sender) = &message.from else {
            return Ok(false);
        };
        self.is_admin(message.chat.id, sender.id)
    }

    fn allowed_to_configure(&mut self, message: &Message) -> Result<bool, BotError> {
        if self.sent_by_admin(message)? {
            return Ok(true);
        }
        self.reply(message, "Only group admins can do that.")?;
        Ok(false)
    }

    fn settings_command(&mut self, message: &Message, arguments: &[&str]) -> Result<(), BotError> {
        if !self.allowed_to_configure(message)? {
            return Ok(());
        }
        let Some((setting, rest)) = arguments.split_first() else {
            let group = self.database.load_group(message.chat.id)?;
            return self.reply(message, &describe_settings(&group));
        };
        match *setting {
            "required-hearts" => self.set_required_hearts(message, rest),
            "emoji" => self.set_emoji(message, rest),
            "self-stars" => self.set_self_stars(message, rest),
            _ => self.reply(
                message,
                "Unknown setting. Use required-hearts, emoji or self-stars.",
            ),
        }
    }

    fn set_required_hearts(
        &mut self,
        message: &Message,
        arguments: &[&str],
    ) -> Result<(), BotError> {
        let hearts: i64 = match arguments {
            [value] if value.bytes().all(|byte| byte.is_ascii_digit()) => {
                value.parse().unwrap_or(0)
            }
            _ => 0,
        };
        if hearts < 1 {
            return self.reply(
                message,
                "Usage: /settings required-hearts <positive number>",
            );
        }
        self.database
            .save_group_setting(message.chat.id, &GroupSetting::RequiredHearts(hearts))?;
        self.reply(
            message,
            &format!("Posts now need {hearts} hearts to reach the starboard."),
        )
    }

    fn set_emoji(&mut self, message: &Message, arguments: &[&str]) -> Result<(), BotError> {
        let [emoji] = arguments else {
            return self.reply(
                message,
                "Send one emoji Telegram allows as a reaction, e.g. /settings emoji 🔥",
            );
        };
        if !config::is_allowed_emoji(emoji) {
            return self.reply(
                message,
                "Send one emoji Telegram allows as a reaction, e.g. /settings emoji 🔥",
            );
        }
        self.database
            .save_group_setting(message.chat.id, &GroupSetting::Emoji((*emoji).to_string()))?;
        self.reply(message, &format!("Reaction emoji set to {emoji}."))
    }

    fn set_self_stars(&mut self, message: &Message, arguments: &[&str]) -> Result<(), BotError> {
        let choice = match arguments {
            [value] => value.to_lowercase(),
            _ => String::new(),
        };
        if choice != "true" && choice != "false" {
            return self.reply(message, "Usage: /settings self-stars <true|false>");
        }
        self.database
            .save_group_setting(message.chat.id, &GroupSetting::SelfStars(choice == "true"))?;
        self.reply(
            message,
            &format!("Counting the author's own reaction: {choice}."),
        )
    }

    fn save_current_topic(
        &mut self,
        message: &Message,
        setting: fn(i64) -> GroupSetting,
    ) -> Result<Option<Group>, BotError> {
        if !self.allowed_to_configure(message)? {
            return Ok(None);
        }
        let Some(topic_id) = message
            .message_thread_id
            .filter(|_| message.is_topic_message)
        else {
            self.reply(message, "Run this inside a topic other than General.")?;
            return Ok(None);
        };
        if !self.is_admin(message.chat.id, self.account.id)? {
            self.reply(
                message,
                "Make me a group admin first: Telegram only sends reactions to admin bots.",
            )?;
            return Ok(None);
        }
        self.database
            .save_group_setting(message.chat.id, &setting(topic_id))?;
        Ok(self.database.find_group(message.chat.id)?)
    }

    fn setup_command(&mut self, message: &Message) -> Result<(), BotError> {
        let Some(group) = self.save_current_topic(message, GroupSetting::StarboardTopic)? else {
            return Ok(());
        };
        let next_step = if group.music_topic_id.is_some() {
            ""
        } else {
            "\nNext, run /watch in the topic whose posts can be starred."
        };
        self.reply(
            message,
            &format!("This topic is now the starboard.{next_step}"),
        )
    }

    fn watch_command(&mut self, message: &Message) -> Result<(), BotError> {
        let Some(group) = self.save_current_topic(message, GroupSetting::MusicTopic)? else {
            return Ok(());
        };
        let next_step = if group.starboard_topic_id.is_some() {
            ""
        } else {
            "\nNext, run /setup in the topic that should be the starboard."
        };
        self.reply(
            message,
            &format!("Posts with audio or a link in this topic can now be starred.{next_step}"),
        )
    }

    fn explain_group_only(&mut self, message: &Message) -> Result<(), BotError> {
        self.reply(
            message,
            "My commands work in the group, for group admins: run /setup in the topic that should be \
             the starboard and /watch in the topic whose posts can be starred.",
        )
    }
}

fn describe_settings(group: &Group) -> String {
    format!(
        "Current settings:\n{BULLET} required-hearts: {}\n{BULLET} emoji: {}\n{BULLET} self-stars: {}",
        group.required_hearts, group.emoji, group.self_stars
    )
}
