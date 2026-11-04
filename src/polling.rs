use std::thread;
use std::time::Duration;

use serde_json::json;

use crate::bot::{Bot, BotAccount};
use crate::log;
use crate::telegram::{self, ApiError, TelegramApi};
use crate::updates::RawUpdate;

const POLL_WAIT_SECONDS: u64 = 50;
const ALLOWED_UPDATES: [&str; 2] = ["message", "message_reaction"];

impl<A: TelegramApi> Bot<A> {
    pub fn run(&mut self) -> ! {
        let mut offset = 0;
        loop {
            offset = self.poll_once(offset);
        }
    }

    /// Fetches and handles one batch of updates, and returns the offset that confirms them.
    pub fn poll_once(&mut self, offset: i64) -> i64 {
        let params = json!({"offset": offset, "timeout": POLL_WAIT_SECONDS, "allowed_updates": ALLOWED_UPDATES});
        let body = match self.api.post("getUpdates", &params.to_string()) {
            Ok(body) => body,
            Err(error) => return self.retry_later(offset, &error),
        };
        let updates: Vec<RawUpdate<'_>> = match telegram::parse_reply("getUpdates", &body) {
            Ok(updates) => updates,
            Err(error) => return self.retry_later(offset, &error),
        };
        let Some(last) = updates.last() else {
            return offset;
        };
        let next_offset = last.update_id + 1;
        if let Err(error) = self.handle_batch(&updates) {
            log::error(&format!(
                "Couldn't save a batch of updates ({error}); Telegram will resend it"
            ));
            return offset;
        }
        next_offset
    }

    fn handle_batch(&mut self, updates: &[RawUpdate<'_>]) -> rusqlite::Result<()> {
        // One commit per batch instead of per change. If the bot dies mid-batch the batch is
        // rolled back, and Telegram resends it because its offset was never confirmed.
        self.database.begin()?;
        for update in updates {
            self.handle_safely(update);
        }
        let committed = self.database.commit();
        if committed.is_err() {
            let _ = self.database.rollback();
        }
        committed
    }

    fn retry_later(&self, offset: i64, error: &ApiError) -> i64 {
        let delay = self.limits.retry_delay;
        log::warning(&format!(
            "Fetching updates failed ({error}); retrying in {} s",
            delay.as_secs()
        ));
        thread::sleep(delay);
        offset
    }
}

/// Asks Telegram who the bot is, waiting out network trouble (e.g. right after a reboot).
pub fn identify_bot(
    api: &mut impl TelegramApi,
    retry_delay: Duration,
) -> Result<BotAccount, ApiError> {
    loop {
        let reply = api.post("getMe", "{}");
        match reply.and_then(|body| telegram::parse_reply("getMe", &body)) {
            Ok(account) => return Ok(account),
            Err(error) if error.is_network() => {
                log::warning(&format!(
                    "Telegram unreachable ({error}); retrying in {} s",
                    retry_delay.as_secs()
                ));
                thread::sleep(retry_delay);
            }
            Err(error) => return Err(error),
        }
    }
}
