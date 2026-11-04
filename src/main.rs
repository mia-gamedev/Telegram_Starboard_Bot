use std::convert::Infallible;
use std::panic;
use std::path::Path;
use std::process::ExitCode;

use starboard_bot::bot::{Bot, Limits};
use starboard_bot::clock::unix_now;
use starboard_bot::config::{self, DATABASE_FILE, ENV_FILE, MAX_POST_AGE_SECONDS};
use starboard_bot::database::Database;
use starboard_bot::log;
use starboard_bot::polling::identify_bot;
use starboard_bot::telegram::TelegramClient;

fn main() -> ExitCode {
    panic::set_hook(Box::new(|info| log::error(&format!("Panic: {info}"))));
    let Err(problem) = start();
    log::write("CRITICAL", &problem);
    ExitCode::FAILURE
}

fn start() -> Result<Infallible, String> {
    let token = config::read_bot_token(Path::new(ENV_FILE))
        .ok_or("BOT_TOKEN is missing or invalid in .env")?;
    let database = Database::open(Path::new(DATABASE_FILE))
        .map_err(|error| format!("Can't open {DATABASE_FILE}: {error}"))?;
    // Groups that went quiet only get their old posts cleared here.
    database
        .forget_posts_before(unix_now() - MAX_POST_AGE_SECONDS)
        .map_err(|error| format!("Can't clear old posts: {error}"))?;
    let mut api = TelegramClient::new(token).map_err(|error| error.to_string())?;
    let limits = Limits::default();
    let account = identify_bot(&mut api, limits.retry_delay)
        .map_err(|error| format!("Telegram refused the bot token: {error}"))?;
    log::info(&format!("Running as @{}", account.username));
    Bot::new(api, database, account).run()
}
