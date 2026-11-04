//! Behaviour tests: every command, post and reaction path, with Telegram replaced by a fake.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;
use std::{env, fs};

use serde_json::{Value, json};
use starboard_bot::bot::{Bot, BotAccount};
use starboard_bot::clock::unix_now;
use starboard_bot::database::{Database, Post};
use starboard_bot::telegram::{ApiError, TelegramApi};
use starboard_bot::updates::RawUpdate;

const GROUP_A: i64 = -1_001_111_111_111;
const GROUP_B: i64 = -1_002_222_222_222;
const CHANNEL: i64 = -1_005_555_555_555;
const BOT_ID: i64 = 123_456;
const ADMIN: i64 = 10;
const MEMBER: i64 = 20;
const AUTHOR: i64 = 30;
const ANONYMOUS_ADMIN_BOT: i64 = 1_087_968_824;
const CHANNEL_BOT: i64 = 136_817_688;
const A_MUSIC: i64 = 3;
const A_STARBOARD: i64 = 5;
const B_MUSIC: i64 = 8;
const B_STARBOARD: i64 = 7;
const HEART: &str = "\u{2764}";
const FIRE: &str = "🔥";
const DAY: i64 = 24 * 60 * 60;

#[derive(Default)]
struct FakeTelegram {
    calls: Vec<(String, Value)>,
    statuses: HashMap<i64, &'static str>,
    queued_polls: VecDeque<Result<Value, ApiError>>,
}

impl TelegramApi for FakeTelegram {
    fn post(&mut self, method: &str, body: &str) -> Result<String, ApiError> {
        let params: Value = serde_json::from_str(body).expect("requests are JSON");
        self.calls.push((method.to_string(), params.clone()));
        let result = match method {
            "getChatMember" => {
                json!({"status": self.statuses[&params["user_id"].as_i64().unwrap()]})
            }
            "getUpdates" => self.queued_polls.pop_front().unwrap_or(Ok(json!([])))?,
            "forwardMessage" | "sendMessage" => json!({"message_id": 999}),
            _ => json!(true),
        };
        Ok(json!({"ok": true, "result": result}).to_string())
    }
}

struct TestBot {
    bot: Bot<FakeTelegram>,
}

impl TestBot {
    fn new() -> Self {
        Self::with_database(Path::new(":memory:"))
    }

    fn with_database(path: &Path) -> Self {
        send_logs_to_a_temporary_folder();
        let statuses = [
            (ADMIN, "administrator"),
            (MEMBER, "member"),
            (AUTHOR, "member"),
            (BOT_ID, "administrator"),
        ];
        let api = FakeTelegram {
            statuses: HashMap::from(statuses),
            ..FakeTelegram::default()
        };
        let account = BotAccount {
            id: BOT_ID,
            username: "Star_Bot".into(),
        };
        let mut bot = Bot::new(api, Database::open(path).expect("database opens"), account);
        bot.limits.retry_delay = Duration::ZERO;
        Self { bot }
    }

    /// Two groups, A and B, set up by their admins; A counts 🔥 and ignores its authors' own reactions.
    fn with_two_groups() -> Self {
        let mut test = Self::new();
        test.send(message(GROUP_A, ADMIN).text("/setup").thread(A_STARBOARD));
        test.send(message(GROUP_A, ADMIN).text("/watch").thread(A_MUSIC));
        test.send(message(GROUP_B, ADMIN).text("/watch").thread(B_MUSIC));
        test.send(message(GROUP_B, ADMIN).text("/setup").thread(B_STARBOARD));
        test.send(message(GROUP_A, ADMIN).text(&format!("/settings emoji {FIRE}")));
        test.send(message(GROUP_A, ADMIN).text("/settings self-stars false"));
        test
    }

    fn send(&mut self, update: impl Into<Value>) {
        let text = update.into().to_string();
        let raw: RawUpdate<'_> = serde_json::from_str(&text).expect("test updates are well formed");
        self.bot.api.calls.clear();
        self.bot.handle_safely(&raw);
    }

    fn sent(&self, method: &str) -> Vec<Value> {
        self.bot
            .api
            .calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    }

    fn last_reply(&self) -> Option<String> {
        self.sent("sendMessage")
            .last()
            .map(|params| params["text"].as_str().unwrap().to_string())
    }

    fn is_tracked(&self, chat_id: i64, message_id: i64) -> bool {
        self.bot
            .database
            .find_post(chat_id, message_id)
            .unwrap()
            .is_some()
    }

    fn hearts(&self, chat_id: i64, message_id: i64) -> Option<i64> {
        self.bot
            .database
            .find_post(chat_id, message_id)
            .unwrap()
            .map(|post| post.hearts)
    }

    fn remember(&self, chat_id: i64, message_id: i64, posted_at: i64) {
        let post = Post {
            chat_id,
            message_id,
            author_id: AUTHOR,
            posted_at,
            hearts: 0,
        };
        self.bot.database.remember_post(&post).unwrap();
    }
}

/// The bot logs to bot.log in the current folder; tests must never touch the real one.
fn send_logs_to_a_temporary_folder() {
    static MOVE_ONCE: Once = Once::new();
    MOVE_ONCE.call_once(|| {
        let folder = test_folder();
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        env::set_current_dir(&folder).unwrap();
    });
}

fn test_folder() -> PathBuf {
    env::temp_dir().join("starboard-rust-tests")
}

struct MessageBuilder {
    data: Value,
}

fn message(chat_id: i64, sender: i64) -> MessageBuilder {
    let chat_type = if chat_id > 0 { "private" } else { "supergroup" };
    MessageBuilder {
        data: json!({
            "message_id": 1, "date": unix_now(), "from": {"id": sender, "is_bot": false},
            "chat": {"id": chat_id, "type": chat_type},
        }),
    }
}

impl MessageBuilder {
    fn text(mut self, text: &str) -> Self {
        self.data["text"] = json!(text);
        if text.starts_with('/') {
            let length = text.split_whitespace().next().unwrap().len();
            self.data["entities"] = json!([{"type": "bot_command", "offset": 0, "length": length}]);
        } else if let Some(start) = text.find("http") {
            self.data["entities"] =
                json!([{"type": "url", "offset": start, "length": text.len() - start}]);
        }
        self
    }

    fn thread(mut self, topic_id: i64) -> Self {
        self.data["message_thread_id"] = json!(topic_id);
        self.data["is_topic_message"] = json!(true);
        self
    }

    fn id(self, message_id: i64) -> Self {
        self.field("message_id", json!(message_id))
    }

    fn date(self, date: i64) -> Self {
        self.field("date", json!(date))
    }

    fn sender_chat(self, chat_id: i64) -> Self {
        let chat_type = if chat_id == GROUP_A || chat_id == GROUP_B {
            "supergroup"
        } else {
            "channel"
        };
        self.field("sender_chat", json!({"id": chat_id, "type": chat_type}))
    }

    fn field(mut self, name: &str, value: Value) -> Self {
        self.data[name] = value;
        self
    }
}

impl From<MessageBuilder> for Value {
    fn from(builder: MessageBuilder) -> Self {
        json!({"update_id": 1, "message": builder.data})
    }
}

fn reaction(chat_id: i64, message_id: i64, sender: i64, old: &[&str], new: &[&str]) -> Value {
    let emojis = |items: &[&str]| {
        items
            .iter()
            .map(|emoji| json!({"type": "emoji", "emoji": emoji}))
            .collect::<Vec<_>>()
    };
    json!({"update_id": 1, "message_reaction": {
        "chat": {"id": chat_id, "type": "supergroup"}, "message_id": message_id, "user": {"id": sender},
        "date": unix_now(), "old_reaction": emojis(old), "new_reaction": emojis(new),
    }})
}

// --- Setting up a group ---

#[test]
fn setup_in_a_private_chat_explains_how_to_use_the_bot() {
    let mut test = TestBot::new();
    test.send(message(ADMIN, ADMIN).text("/setup"));
    assert!(test.last_reply().unwrap().contains("work in the group"));
}

#[test]
fn setup_by_a_non_admin_is_refused_and_saves_nothing() {
    let mut test = TestBot::new();
    test.send(message(GROUP_A, MEMBER).text("/setup").thread(A_STARBOARD));
    assert_eq!(test.last_reply().unwrap(), "Only group admins can do that.");
    assert!(test.bot.database.find_group(GROUP_A).unwrap().is_none());
}

#[test]
fn setup_in_the_general_topic_is_refused() {
    let mut test = TestBot::new();
    test.send(message(GROUP_A, ADMIN).text("/setup"));
    assert!(test.last_reply().unwrap().contains("inside a topic"));
}

#[test]
fn setup_addressed_to_another_bot_is_ignored() {
    let mut test = TestBot::new();
    test.send(
        message(GROUP_A, ADMIN)
            .text("/setup@other_bot")
            .thread(A_STARBOARD),
    );
    assert!(test.bot.api.calls.is_empty());
}

#[test]
fn setup_names_the_next_step_and_replies_in_the_same_topic() {
    let mut test = TestBot::new();
    test.send(
        message(GROUP_A, ADMIN)
            .text("/setup@star_bot")
            .thread(A_STARBOARD),
    );
    let reply = test.last_reply().unwrap();
    assert!(
        reply.contains("now the starboard") && reply.contains("/watch"),
        "{reply}"
    );
    let params = test.sent("sendMessage").pop().unwrap();
    assert_eq!(params["message_thread_id"], A_STARBOARD);
    assert_eq!(params["reply_parameters"]["message_id"], 1);
}

#[test]
fn watch_is_refused_while_the_bot_is_not_an_admin() {
    let mut test = TestBot::new();
    test.send(message(GROUP_A, ADMIN).text("/setup").thread(A_STARBOARD));
    test.bot.api.statuses.insert(BOT_ID, "member");
    test.send(message(GROUP_A, ADMIN).text("/watch").thread(A_MUSIC));
    assert!(test.last_reply().unwrap().contains("Make me a group admin"));
    assert_eq!(
        test.bot
            .database
            .find_group(GROUP_A)
            .unwrap()
            .unwrap()
            .music_topic_id,
        None
    );
}

#[test]
fn watch_after_setup_needs_no_next_step() {
    let mut test = TestBot::new();
    test.send(message(GROUP_A, ADMIN).text("/setup").thread(A_STARBOARD));
    test.send(message(GROUP_A, ADMIN).text("/watch").thread(A_MUSIC));
    assert_eq!(
        test.last_reply().unwrap(),
        "Posts with audio or a link in this topic can now be starred."
    );
}

#[test]
fn groups_are_set_up_independently() {
    let test = TestBot::with_two_groups();
    let a = test.bot.database.find_group(GROUP_A).unwrap().unwrap();
    let b = test.bot.database.find_group(GROUP_B).unwrap().unwrap();
    assert!(a.is_set_up() && b.is_set_up());
    assert_eq!(
        (a.starboard_topic_id, b.starboard_topic_id),
        (Some(A_STARBOARD), Some(B_STARBOARD))
    );
}

// --- Settings ---

#[test]
fn an_anonymous_admin_sees_the_default_settings() {
    let mut test = TestBot::new();
    test.send(
        message(GROUP_A, ANONYMOUS_ADMIN_BOT)
            .text("/settings")
            .sender_chat(GROUP_A),
    );
    let expected =
        format!("Current settings:\n• required-hearts: 2\n• emoji: {HEART}\n• self-stars: true");
    assert_eq!(test.last_reply().unwrap(), expected);
}

#[test]
fn a_post_made_as_the_linked_channel_is_refused() {
    let mut test = TestBot::new();
    test.send(
        message(GROUP_A, CHANNEL_BOT)
            .text("/settings required-hearts 1")
            .sender_chat(CHANNEL),
    );
    assert_eq!(test.last_reply().unwrap(), "Only group admins can do that.");
}

#[test]
fn required_hearts_must_be_a_plain_positive_number() {
    let mut test = TestBot::new();
    for value in ["\u{b2}", "0", "-3", "+3", "two"] {
        test.send(message(GROUP_A, ADMIN).text(&format!("/settings required-hearts {value}")));
        assert!(test.last_reply().unwrap().starts_with("Usage"), "{value}");
    }
    test.send(message(GROUP_A, ADMIN).text("/settings required-hearts 4"));
    assert_eq!(
        test.bot
            .database
            .find_group(GROUP_A)
            .unwrap()
            .unwrap()
            .required_hearts,
        4
    );
}

#[test]
fn an_unknown_setting_is_explained() {
    let mut test = TestBot::new();
    test.send(message(GROUP_A, ADMIN).text("/settings colour red"));
    assert!(test.last_reply().unwrap().starts_with("Unknown setting"));
}

#[test]
fn settings_change_only_their_own_group() {
    let test = TestBot::with_two_groups();
    let a = test.bot.database.find_group(GROUP_A).unwrap().unwrap();
    let b = test.bot.database.find_group(GROUP_B).unwrap().unwrap();
    assert_eq!(
        (a.emoji.as_str(), a.self_stars, a.required_hearts),
        (FIRE, false, 2)
    );
    assert_eq!(
        (b.emoji.as_str(), b.self_stars, b.required_hearts),
        (HEART, true, 2)
    );
}

// --- Tracking posts ---

#[test]
fn links_audio_files_and_captioned_links_in_the_watched_topic_are_tracked() {
    let mut test = TestBot::with_two_groups();
    test.send(
        message(GROUP_A, AUTHOR)
            .text("listen https://example.com/a")
            .thread(A_MUSIC)
            .id(100),
    );
    test.send(
        message(GROUP_A, AUTHOR)
            .text("just chatting")
            .thread(A_MUSIC)
            .id(101),
    );
    test.send(
        message(GROUP_A, AUTHOR)
            .text("https://example.com/x")
            .thread(A_STARBOARD)
            .id(102),
    );
    test.send(
        message(GROUP_A, AUTHOR)
            .thread(A_MUSIC)
            .id(103)
            .field("document", json!({"mime_type": "audio/mpeg"})),
    );
    let captioned = message(GROUP_A, AUTHOR)
        .thread(A_MUSIC)
        .id(104)
        .field("photo", json!([{}]));
    test.send(captioned.field(
        "caption_entities",
        json!([{"type": "url", "offset": 2, "length": 11}]),
    ));
    test.send(
        message(GROUP_A, AUTHOR)
            .thread(A_MUSIC)
            .id(105)
            .field("photo", json!([{}])),
    );
    let tracked: Vec<i64> = (100..=105)
        .filter(|id| test.is_tracked(GROUP_A, *id))
        .collect();
    assert_eq!(tracked, [100, 103, 104]);
}

#[test]
fn the_same_message_id_is_tracked_separately_per_group() {
    let mut test = TestBot::with_two_groups();
    test.send(
        message(GROUP_A, AUTHOR)
            .text("https://example.com/a")
            .thread(A_MUSIC)
            .id(100),
    );
    test.send(
        message(GROUP_B, AUTHOR)
            .text("https://example.com/b")
            .thread(B_MUSIC)
            .id(100),
    );
    test.send(reaction(GROUP_B, 100, MEMBER, &[], &[HEART]));
    assert_eq!(
        (test.hearts(GROUP_A, 100), test.hearts(GROUP_B, 100)),
        (Some(0), Some(1))
    );
}

#[test]
fn a_link_already_older_than_thirty_days_is_not_kept() {
    let mut test = TestBot::with_two_groups();
    let old = unix_now() - 31 * DAY;
    test.send(
        message(GROUP_A, AUTHOR)
            .text("https://example.com/old")
            .thread(A_MUSIC)
            .id(106)
            .date(old),
    );
    assert!(!test.is_tracked(GROUP_A, 106));
}

// --- Counting hearts ---

fn with_tracked_posts() -> TestBot {
    let mut test = TestBot::with_two_groups();
    test.send(
        message(GROUP_A, AUTHOR)
            .text("listen https://example.com/a")
            .thread(A_MUSIC)
            .id(100),
    );
    test.send(
        message(GROUP_B, AUTHOR)
            .text("listen https://example.com/b")
            .thread(B_MUSIC)
            .id(100),
    );
    test
}

#[test]
fn the_authors_own_reaction_is_ignored_when_self_stars_is_off() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_A, 100, AUTHOR, &[], &[FIRE]));
    assert_eq!(test.hearts(GROUP_A, 100), Some(0));
}

#[test]
fn a_reaction_in_another_emoji_is_ignored() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_A, 100, MEMBER, &[], &[HEART]));
    assert_eq!(test.hearts(GROUP_A, 100), Some(0));
}

#[test]
fn switching_to_the_emoji_and_removing_it_again_nets_zero() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_A, 100, MEMBER, &[HEART], &[FIRE]));
    assert_eq!(test.hearts(GROUP_A, 100), Some(1));
    test.send(reaction(GROUP_A, 100, MEMBER, &[FIRE], &[]));
    assert_eq!(test.hearts(GROUP_A, 100), Some(0));
}

#[test]
fn reaching_the_required_hearts_forwards_to_the_groups_own_starboard() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_A, 100, MEMBER, &[], &[FIRE]));
    test.send(reaction(GROUP_A, 100, ADMIN, &[], &[FIRE]));
    let expected = json!({"chat_id": GROUP_A, "from_chat_id": GROUP_A, "message_id": 100, "message_thread_id": A_STARBOARD});
    assert_eq!(test.sent("forwardMessage"), [expected]);
    let first_heart = json!({"chat_id": GROUP_A, "message_id": 999, "reaction": [{"type": "emoji", "emoji": FIRE}]});
    assert_eq!(test.sent("setMessageReaction"), [first_heart]);
    assert!(!test.is_tracked(GROUP_A, 100));
}

#[test]
fn the_authors_own_reaction_counts_when_self_stars_is_on() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_B, 100, MEMBER, &[], &[HEART]));
    test.send(reaction(GROUP_B, 100, AUTHOR, &[], &[HEART]));
    let expected = json!({"chat_id": GROUP_B, "from_chat_id": GROUP_B, "message_id": 100, "message_thread_id": B_STARBOARD});
    assert_eq!(test.sent("forwardMessage"), [expected]);
}

#[test]
fn a_forwarded_post_is_not_forwarded_again() {
    let mut test = with_tracked_posts();
    test.send(reaction(GROUP_A, 100, MEMBER, &[], &[FIRE]));
    test.send(reaction(GROUP_A, 100, ADMIN, &[], &[FIRE]));
    test.send(reaction(GROUP_A, 100, AUTHOR, &[], &[FIRE]));
    assert!(test.sent("forwardMessage").is_empty());
}

// --- Expiry and the cap ---

#[test]
fn a_reaction_on_an_expired_post_is_ignored_and_clears_it() {
    let mut test = TestBot::with_two_groups();
    test.remember(GROUP_A, 50, unix_now() - 31 * DAY);
    test.send(reaction(GROUP_A, 50, MEMBER, &[], &[FIRE]));
    assert!(!test.is_tracked(GROUP_A, 50));
    assert!(test.sent("forwardMessage").is_empty());
}

#[test]
fn a_new_link_clears_only_its_own_groups_expired_posts() {
    let mut test = TestBot::with_two_groups();
    let old = unix_now() - 31 * DAY;
    test.remember(GROUP_A, 51, old);
    test.remember(GROUP_B, 50, old);
    test.send(
        message(GROUP_A, AUTHOR)
            .text("https://example.com/new")
            .thread(A_MUSIC)
            .id(300),
    );
    assert!(!test.is_tracked(GROUP_A, 51) && test.is_tracked(GROUP_A, 300));
    assert!(test.is_tracked(GROUP_B, 50));
}

#[test]
fn the_startup_sweep_clears_quiet_groups() {
    let test = TestBot::with_two_groups();
    test.remember(GROUP_B, 52, unix_now() - 31 * DAY);
    test.remember(GROUP_A, 53, unix_now());
    test.bot
        .database
        .forget_posts_before(test.bot.post_age_cutoff())
        .unwrap();
    assert!(!test.is_tracked(GROUP_B, 52) && test.is_tracked(GROUP_A, 53));
}

#[test]
fn the_cap_keeps_only_a_groups_newest_posts() {
    let mut test = TestBot::with_two_groups();
    test.bot.limits.max_tracked_posts = 3;
    for message_id in 301..=305 {
        test.send(
            message(GROUP_A, AUTHOR)
                .text("https://example.com/c")
                .thread(A_MUSIC)
                .id(message_id),
        );
    }
    let tracked: Vec<i64> = (301..=305)
        .filter(|id| test.is_tracked(GROUP_A, *id))
        .collect();
    assert_eq!(tracked, [303, 304, 305]);
}

// --- Polling and recovery ---

fn queue_poll(test: &mut TestBot, updates: Value) {
    test.bot.api.queued_polls.push_back(Ok(updates));
}

#[test]
fn polling_asks_only_for_messages_and_reactions_and_confirms_handled_updates() {
    let mut test = TestBot::with_two_groups();
    let link: Value = message(GROUP_A, AUTHOR)
        .text("https://e.com/l")
        .thread(A_MUSIC)
        .id(400)
        .into();
    queue_poll(
        &mut test,
        json!([{"update_id": 40, "message": link["message"]}, {"update_id": 41}]),
    );
    let offset = test.bot.poll_once(0);
    assert_eq!(offset, 42);
    let poll = test.sent("getUpdates").pop().unwrap();
    assert_eq!(
        poll["allowed_updates"],
        json!(["message", "message_reaction"])
    );
    assert_eq!(
        test.bot.poll_once(offset),
        offset,
        "an empty poll keeps the offset"
    );
    assert_eq!(test.sent("getUpdates").last().unwrap()["offset"], 42);
}

#[test]
fn a_broken_update_does_not_stop_the_rest_of_the_batch() {
    let mut test = TestBot::with_two_groups();
    let link: Value = message(GROUP_A, AUTHOR)
        .text("https://e.com/l")
        .thread(A_MUSIC)
        .id(400)
        .into();
    queue_poll(
        &mut test,
        json!([{"update_id": 40, "message": {"broken": true}}, {"update_id": 41, "message": link["message"]}]),
    );
    assert_eq!(test.bot.poll_once(0), 42);
    assert!(test.is_tracked(GROUP_A, 400));
}

#[test]
fn a_failed_fetch_is_retried_from_the_same_offset() {
    let mut test = TestBot::new();
    test.bot
        .api
        .queued_polls
        .push_back(Err(ApiError::Network("simulated drop".into())));
    assert_eq!(test.bot.poll_once(7), 7);
}

#[test]
fn a_batch_cut_off_by_a_crash_is_rolled_back() {
    send_logs_to_a_temporary_folder();
    let path = test_folder().join("rollback.db");
    let link: Value = message(GROUP_A, AUTHOR)
        .text("https://e.com/1")
        .thread(A_MUSIC)
        .id(500)
        .into();
    {
        let mut test = TestBot::with_database(&path);
        for (thread, command) in [(A_STARBOARD, "/setup"), (A_MUSIC, "/watch")] {
            test.send(message(GROUP_A, ADMIN).text(command).thread(thread));
        }
        test.bot.database.begin().unwrap();
        test.send(link);
        assert!(test.is_tracked(GROUP_A, 500));
    } // The bot "dies" here, before the batch is committed.
    let reopened = TestBot::with_database(&path);
    assert!(!reopened.is_tracked(GROUP_A, 500));
    assert!(
        reopened
            .bot
            .database
            .find_group(GROUP_A)
            .unwrap()
            .unwrap()
            .is_set_up()
    );
}

#[test]
fn a_crashing_update_is_logged_with_the_reason() {
    let mut test = TestBot::new();
    test.send(json!({"update_id": 77, "message_reaction": {"chat": {}}}));
    let log = fs::read_to_string(test_folder().join("bot.log")).unwrap();
    let entry = log
        .lines()
        .find(|line| line.contains("Failed to handle update 77"))
        .expect("the failure is logged");
    assert!(entry.contains("missing field `id`"), "{entry}");
}
