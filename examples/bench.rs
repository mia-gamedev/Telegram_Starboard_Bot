//! RAM and CPU of the bot against a full-size database: 50,000 groups after a month of use.
//! Run: cargo run --release --example bench
//! Add `-- --tls` to also hold a real TLS connection to Telegram (fake token) while measuring RAM.

use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{env, fs};

use rusqlite::{Connection, params};
use starboard_bot::bot::{Bot, BotAccount};
use starboard_bot::clock::unix_now;
use starboard_bot::database::Database;
use starboard_bot::telegram::{ApiError, TelegramApi, TelegramClient};

const GROUPS: i64 = 50_000;
const BIG_GROUPS: i64 = 5_500;
const UPDATES_PER_POLL: usize = 100;
const POLLS: usize = 500;
const MUSIC: i64 = 3;
const STARBOARD: i64 = 5;
/// (name, share of groups, messages, links, reactions per group per day), as in the Python model.
const PROFILES: [(&str, f64, f64, f64, f64); 3] = [
    ("huge (1%)", 0.01, 20_000.0, 500.0, 5_000.0),
    ("big (10%)", 0.10, 2_000.0, 50.0, 500.0),
    ("small (89%)", 0.89, 200.0, 5.0, 20.0),
];

fn main() {
    let database = fresh_copy(&full_size_database());
    let (at_start, _, _) = process_memory_mb();
    let _live_connection = env::args()
        .any(|argument| argument == "--tls")
        .then(open_real_connection);
    let mut bot = new_bot(&database, Box::new(Workload::new(1).into_batches()));
    let (idle, idle_private, _) = process_memory_mb();
    poll(&mut bot);
    let (busy, busy_private, peak) = process_memory_mb();
    let batches: Vec<String> = Workload::new(2).into_batches().take(POLLS).collect();
    bot.api = BenchTelegram {
        batches: Box::new(batches.into_iter()),
    };
    let (cpu_started, wall_started) = (process_cpu_seconds(), Instant::now());
    poll(&mut bot);
    let updates = (POLLS * UPDATES_PER_POLL) as f64;
    let cpu_us = (process_cpu_seconds() - cpu_started) / updates * 1e6;
    let wall_us = wall_started.elapsed().as_secs_f64() / updates * 1e6;
    println!(
        "RAM at start {at_start:.1} MB | idle {idle:.1} MB (private {idle_private:.1}) | after {} updates {busy:.1} MB (private {busy_private:.1}, peak {peak:.1})",
        POLLS * UPDATES_PER_POLL
    );
    println!(
        "CPU {cpu_us:.1} us and wall {wall_us:.1} us per update -> one core keeps up with ~{:.0} updates/s",
        1e6 / wall_us
    );
    print_load_model(cpu_us);
}

/// A kept-alive TLS connection like the real bot holds; Telegram refuses the fake token, which is fine.
fn open_real_connection() -> TelegramClient {
    let mut client = TelegramClient::new("123456:TEST-token".into()).unwrap();
    let reply = client.post("getMe", "{}");
    println!(
        "Real TLS connection to Telegram: {}",
        if reply.is_ok() { "open" } else { "failed" }
    );
    client
}

fn new_bot(database: &Path, batches: Box<dyn Iterator<Item = String>>) -> Bot<BenchTelegram> {
    let account = BotAccount {
        id: 1,
        username: "bench_bot".into(),
    };
    Bot::new(
        BenchTelegram { batches },
        Database::open(database).unwrap(),
        account,
    )
}

fn poll(bot: &mut Bot<BenchTelegram>) {
    let mut offset = 0;
    for _ in 0..POLLS {
        offset = bot.poll_once(offset);
    }
}

struct BenchTelegram {
    batches: Box<dyn Iterator<Item = String>>,
}

impl TelegramApi for BenchTelegram {
    fn post(&mut self, method: &str, _body: &str) -> Result<String, ApiError> {
        if method == "getUpdates" {
            return Ok(self
                .batches
                .next()
                .unwrap_or_else(|| r#"{"ok":true,"result":[]}"#.into()));
        }
        Ok(r#"{"ok":true,"result":{"message_id":1,"status":"administrator"}}"#.into())
    }
}

// --- The full-size database, built once and reused ---

fn full_size_database() -> PathBuf {
    let path = env::temp_dir()
        .join("starboard-tests")
        .join("full_size_rust.db");
    if !path.exists() {
        seed(&path);
    }
    path
}

fn fresh_copy(source: &Path) -> PathBuf {
    let copy = source.with_file_name("bench_copy.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{suffix}", copy.display()));
    }
    fs::copy(source, &copy).unwrap();
    copy
}

fn seed(path: &Path) {
    println!(
        "Building the full-size database once: 50,000 groups, 12.2 million posts (~600 MB)..."
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    drop(Database::open(path).unwrap()); // The bot's own schema.
    let mut connection = Connection::open(path).unwrap();
    let now = unix_now();
    for first in (0..GROUPS).step_by(5_000) {
        let transaction = connection.transaction().unwrap();
        for index in first..first + 5_000 {
            seed_group(&transaction, index, now);
        }
        transaction.commit().unwrap();
    }
    connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
}

fn seed_group(connection: &Connection, index: i64, now: i64) {
    let chat_id = group_id(index);
    connection
        .prepare_cached(
            "INSERT INTO groups (chat_id, music_topic_id, starboard_topic_id) VALUES (?1, ?2, ?3)",
        )
        .unwrap()
        .execute(params![chat_id, MUSIC, STARBOARD])
        .unwrap();
    let mut add_post = connection
        .prepare_cached("INSERT INTO posts VALUES (?1, ?2, ?3, ?4, ?5)")
        .unwrap();
    let posts = if index < BIG_GROUPS { 1000 } else { 150 };
    for n in 0..posts {
        add_post
            .execute(params![
                chat_id,
                1_000_000 + n,
                6_000_000_000_i64 + n % 300,
                now - n * 600,
                n % 2
            ])
            .unwrap();
    }
}

fn group_id(index: i64) -> i64 {
    -1_001_000_000_000 - index
}

// --- Realistic updates: 80% chat, 3% links in the watched topic, 17% reactions ---

struct Workload {
    random: u64,
    next_update_id: i64,
    links: Vec<(i64, i64)>,
}

impl Workload {
    fn new(seed: u64) -> Self {
        Self {
            random: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            next_update_id: 1,
            links: Vec::new(),
        }
    }

    fn into_batches(mut self) -> impl Iterator<Item = String> {
        std::iter::from_fn(move || Some(self.batch()))
    }

    fn batch(&mut self) -> String {
        let updates: Vec<String> = (0..UPDATES_PER_POLL).map(|_| self.update()).collect();
        format!(r#"{{"ok":true,"result":[{}]}}"#, updates.join(","))
    }

    fn update(&mut self) -> String {
        let update_id = self.next_update_id;
        self.next_update_id += 1;
        let index = self.below(GROUPS as u64) as i64;
        let message_id = 50_000_000 + update_id;
        let roll = self.below(100);
        let body = if roll < 80 {
            let thread = [MUSIC, 9, 11][self.below(3) as usize];
            let text = "haha yes, see you all at the gig tonight, who brings the speakers?";
            format!(
                r#""message":{}"#,
                topic_message(index, message_id, thread, text, "")
            )
        } else if roll < 83 {
            self.links.push((index, message_id));
            let entities = r#","entities":[{"type":"url","offset":9,"length":53}]"#;
            let text = "this one https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC";
            format!(
                r#""message":{}"#,
                topic_message(index, message_id, MUSIC, text, entities)
            )
        } else {
            let target = self.reaction_target(index, message_id);
            let emoji = ["\u{2764}", "👍", "🔥"][self.below(3) as usize];
            format!(r#""message_reaction":{}"#, reaction(target, emoji))
        };
        format!(r#"{{"update_id":{update_id},{body}}}"#)
    }

    fn reaction_target(&mut self, index: i64, message_id: i64) -> (i64, i64) {
        if !self.links.is_empty() && self.below(10) < 3 {
            let pick = self.below(self.links.len() as u64) as usize;
            return self.links[pick];
        }
        (index, message_id - 5)
    }

    fn below(&mut self, limit: u64) -> u64 {
        // xorshift64*: fast, deterministic, good enough for test data.
        self.random ^= self.random >> 12;
        self.random ^= self.random << 25;
        self.random ^= self.random >> 27;
        self.random.wrapping_mul(0x2545_F491_4F6C_DD1D) % limit
    }
}

fn user(id: i64) -> String {
    format!(
        r#"{{"id":{id},"is_bot":false,"first_name":"Alex","last_name":"Example","username":"user{id}","language_code":"de"}}"#
    )
}

fn chat(index: i64) -> String {
    format!(
        r#"{{"id":{},"title":"Group {index}","type":"supergroup","is_forum":true}}"#,
        group_id(index)
    )
}

fn topic_message(index: i64, message_id: i64, thread: i64, text: &str, entities: &str) -> String {
    let topic_start = format!(
        r#"{{"message_id":{thread},"from":{},"chat":{},"date":1700000000,"message_thread_id":{thread},"forum_topic_created":{{"name":"Music","icon_color":7322096}},"is_topic_message":true}}"#,
        user(42),
        chat(index)
    );
    format!(
        r#"{{"message_id":{message_id},"from":{},"chat":{},"date":{},"message_thread_id":{thread},"is_topic_message":true,"text":"{text}"{entities},"reply_to_message":{topic_start}}}"#,
        user(5_000_000_000 + message_id % 9973),
        chat(index),
        unix_now()
    )
}

fn reaction((index, message_id): (i64, i64), emoji: &str) -> String {
    format!(
        r#"{{"chat":{},"message_id":{message_id},"user":{},"date":{},"old_reaction":[],"new_reaction":[{{"type":"emoji","emoji":"{emoji}"}}]}}"#,
        chat(index),
        user(6_000_000_000 + message_id % 7919),
        unix_now()
    )
}

// --- The load model for the assumed group mix ---

fn print_load_model(cpu_us: f64) {
    let (mut messages, mut reactions) = (0.0, 0.0);
    for (_, share, group_messages, _, group_reactions) in PROFILES {
        messages += GROUPS as f64 * share * group_messages;
        reactions += GROUPS as f64 * share * group_reactions;
    }
    let average = (messages + reactions) / 86_400.0;
    let peak = average * 3.0;
    let ceiling = UPDATES_PER_POLL as f64 / (0.020 + UPDATES_PER_POLL as f64 * cpu_us / 1e6);
    println!(
        "50,000-group mix: {average:.0} updates/s average, ~{peak:.0}/s at peak -> CPU {:.2}% of one core average, {:.2}% at peak; polling limit at 20 ms round trip ~{ceiling:.0} updates/s",
        average * cpu_us / 1e4,
        peak * cpu_us / 1e4
    );
}

// --- Process RAM and CPU, read from Windows ---

#[cfg(windows)]
mod windows_process {
    #[repr(C)]
    #[derive(Default)]
    pub struct MemoryCounters {
        pub size: u32,
        pub page_fault_count: u32,
        pub peak_working_set: usize,
        pub working_set: usize,
        pub quota_peak_paged_pool: usize,
        pub quota_paged_pool: usize,
        pub quota_peak_non_paged_pool: usize,
        pub quota_non_paged_pool: usize,
        pub pagefile: usize,
        pub peak_pagefile: usize,
        pub private_bytes: usize,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct FileTime {
        pub low: u32,
        pub high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetCurrentProcess() -> isize;
        pub fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut MemoryCounters,
            size: u32,
        ) -> i32;
        pub fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
}

#[cfg(windows)]
fn process_memory_mb() -> (f64, f64, f64) {
    use windows_process::*;
    let mut counters = MemoryCounters {
        size: size_of::<MemoryCounters>() as u32,
        ..Default::default()
    };
    // SAFETY: the counters struct matches PROCESS_MEMORY_COUNTERS_EX and its size is passed along.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) };
    let megabytes = |bytes: usize| bytes as f64 / 1_048_576.0;
    (
        megabytes(counters.working_set),
        megabytes(counters.private_bytes),
        megabytes(counters.peak_working_set),
    )
}

#[cfg(windows)]
fn process_cpu_seconds() -> f64 {
    use windows_process::*;
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: all four out-pointers are valid FileTime structs.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    let seconds =
        |time: FileTime| ((u64::from(time.high) << 32) | u64::from(time.low)) as f64 / 1e7;
    seconds(kernel) + seconds(user)
}

#[cfg(not(windows))]
fn process_memory_mb() -> (f64, f64, f64) {
    (0.0, 0.0, 0.0)
}

#[cfg(not(windows))]
fn process_cpu_seconds() -> f64 {
    0.0
}
