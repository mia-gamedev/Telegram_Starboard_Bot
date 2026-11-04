//! Talks to the real Telegram API with a fake token. Run with: cargo test --test network -- --ignored

use std::time::Instant;

use starboard_bot::telegram::{ApiError, TelegramApi, TelegramClient, parse_reply};

#[test]
#[ignore = "needs the internet"]
fn a_fake_token_is_refused_over_one_kept_alive_connection() {
    let mut client = TelegramClient::new("123456:TEST-token".into()).unwrap();
    for attempt in 1..=3 {
        let started = Instant::now();
        let body = client.post("getMe", "{}").expect("Telegram is reachable");
        let refused = parse_reply::<bool>("getMe", &body);
        println!("call {attempt}: {} ms", started.elapsed().as_millis());
        assert!(
            matches!(refused, Err(ApiError::Rejected(problem)) if problem == "getMe failed: Unauthorized")
        );
    }
}
