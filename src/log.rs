use std::fs::{self, OpenOptions};
use std::io::{self, Write};

use crate::clock::{unix_now, utc_timestamp};

const LOG_FILE: &str = "bot.log";
const OLD_LOG_FILE: &str = "bot.log.1";
const MAX_LOG_BYTES: u64 = 1_000_000;

pub fn info(text: &str) {
    write("INFO", text);
}

pub fn warning(text: &str) {
    write("WARNING", text);
}

pub fn error(text: &str) {
    write("ERROR", text);
}

pub fn write(level: &str, text: &str) {
    let line = format!("{} UTC {level} {text}\n", utc_timestamp(unix_now()));
    // A background task has no console, and losing a log line must never stop the bot.
    let _ = io::stderr().write_all(line.as_bytes());
    let _ = append_to_log_file(&line);
}

fn append_to_log_file(line: &str) -> io::Result<()> {
    let is_full = fs::metadata(LOG_FILE).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES);
    if is_full {
        fs::rename(LOG_FILE, OLD_LOG_FILE)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?
        .write_all(line.as_bytes())
}
