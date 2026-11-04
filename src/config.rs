use std::{env, fs, path::Path};

pub const ENV_FILE: &str = ".env";
pub const DATABASE_FILE: &str = "starboard.db";
pub const MAX_POST_AGE_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const MAX_TRACKED_POSTS: i64 = 1000;
pub const DEFAULT_REQUIRED_HEARTS: i64 = 2;
pub const DEFAULT_EMOJI: &str = "\u{2764}";
/// Every emoji Telegram accepts as a reaction (Bot API, ReactionTypeEmoji).
const ALLOWED_EMOJI: &str = "👍 👎 ❤ 🔥 🥰 👏 😁 🤔 🤯 😱 🤬 😢 🎉 🤩 🤮 💩 🙏 👌 🕊 🤡 🥱 🥴 😍 🐳 ❤️‍🔥 🌚 🌭 💯 🤣 ⚡ 🍌 \
🏆 💔 🤨 😐 🍓 🍾 💋 🖕 😈 😴 😭 🤓 👻 👨‍💻 👀 🎃 🙈 😇 😨 🤝 ✍ 🤗 🫡 🎅 🎄 ☃ 💅 🤪 🗿 🆒 \
💘 🙉 🦄 😘 💊 🙊 😎 👾 🤷‍♂️ 🤷 🤷‍♀️ 😡";

pub fn is_allowed_emoji(emoji: &str) -> bool {
    ALLOWED_EMOJI.split(' ').any(|allowed| allowed == emoji)
}

pub fn read_bot_token(env_file: &Path) -> Option<String> {
    let token = env::var("BOT_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .or_else(|| read_env_value(env_file, "BOT_TOKEN"))?;
    let token = token.trim();
    is_valid_token(token).then(|| token.to_string())
}

fn read_env_value(env_file: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(env_file).ok()?;
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches(['"', '\'']).to_string())
    })
}

fn is_valid_token(token: &str) -> bool {
    let Some((bot_id, secret)) = token.split_once(':') else {
        return false;
    };
    let is_secret_char = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
    !bot_id.is_empty()
        && bot_id.bytes().all(|byte| byte.is_ascii_digit())
        && !secret.is_empty()
        && secret.bytes().all(is_secret_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_all_73_telegram_reactions() {
        assert_eq!(ALLOWED_EMOJI.split(' ').count(), 73);
        for emoji in ["❤", "🔥", "❤️‍🔥", "👨‍💻", "🤷‍♀️"] {
            assert!(is_allowed_emoji(emoji), "{emoji}");
        }
        assert!(!is_allowed_emoji("⭐"));
        assert!(!is_allowed_emoji(""));
    }

    #[test]
    fn accepts_only_well_formed_tokens() {
        assert!(is_valid_token("123456:ABC-def_1"));
        for bad in [
            "",
            "123456",
            ":abc",
            "12a:abc",
            "123:",
            "123:abc def",
            "123:abc!",
        ] {
            assert!(!is_valid_token(bad), "{bad:?}");
        }
    }
}
