# Starboard Bot

A Discord-style starboard for Telegram groups with topics. When a post in the watched topic (e.g. **Music Sharing**) gets 2 ❤ reactions, the bot forwards it to the **starboard** topic. One running bot can serve any number of groups, each with its own topics and settings.

- Only posts with an audio file or a link count. Plain chat is ignored.
- Posts can collect hearts for up to a month. Busy groups get a shorter window: the bot remembers each group's newest 1,000 links, so in a group posting 500 a day a post has about 2 days.
- Each post is forwarded once. Telegram doesn't copy reactions when forwarding, so the bot puts the first ❤ on the starboard copy and everyone can add theirs there.

## Setup

1. Create a bot with [@BotFather](https://t.me/BotFather) and put its token in `.env` as `BOT_TOKEN=...`.
2. Start the bot (see below), add it to the group and make it an **admin**. Telegram only sends reactions to admin bots.
3. As a group admin, in the group:
   - send `/setup` in the topic that should become the starboard,
   - send `/watch` in the topic whose posts can be starred.

Both work right away and are saved in the bot's database, so they survive restarts and crashes. Send them again in another topic to move the starboard or watch a different topic.

## Settings

Group admins can change the rules with `/settings` in the group. On its own, it shows the current values.

| Setting | Default | Example | What it does |
|---|---|---|---|
| `required-hearts` | 2 | `/settings required-hearts 3` | Reactions a post needs to reach the starboard. |
| `emoji` | ❤ | `/settings emoji 🔥` | The reaction that counts. Must be one Telegram allows as a reaction. |
| `self-stars` | true | `/settings self-stars false` | Whether the author's own reaction counts. |

Each group has its own settings, saved in the database too.

## Build and run it

The bot is written in Rust. Build it once with [Rust](https://rustup.rs):

```
cargo build --release
```

On Windows, Rust's GNU toolchain works without Visual Studio, as long as MSYS2's `gcc` (`C:\msys64\mingw64\bin`) is on the PATH to compile SQLite.

This makes a single program, `target/release/starboard_bot.exe`, that needs nothing else installed. Put it in a folder together with your `.env` and run it from there: it keeps `starboard.db` and `bot.log` in the folder it's started from.

If `BOT_TOKEN` is missing or invalid, the bot stops right away and says why in `bot.log`. The bot only works while it's running, so run it on a computer that stays on. Stop it with Ctrl+C, and restart it after editing `.env` by hand.

## Tests

```
cargo test                                       # behaviour and unit tests, Telegram faked
cargo test --test network -- --ignored           # talks to the real Telegram API with a fake token
cargo run --release --example bench [-- --tls]   # RAM and CPU against 50,000 groups / 12.2 million posts
```

The tests never touch your `.env`, `starboard.db` or `bot.log`. The benchmark builds its ~600 MB test database in your temp folder on first use.

## Good to know

- Only one copy of the bot can run per token.
- Anyone can add the bot to their group and set it up there; admins can only change their own group. To keep it to your own groups, send `/setjoingroups` to @BotFather and choose *Disable* once it's in them.
- Telegram doesn't tell bots which topic a reacted message is in, so the bot remembers watched posts as they're made. Posts from before it was running, or before `/watch`, never count.
- Setup, settings and tracked posts are kept in `starboard.db` (SQLite) next to the bot. Changes are saved after every batch of updates; if the bot is stopped mid-batch, Telegram sends that batch again, so restarts and crashes are fine. The bot stores only numeric Telegram IDs, no names or message text.
- It's built to be light: about 8–10 MB of RAM and roughly 7–8 µs of CPU per update, measured with 50,000 groups and 12 million tracked posts. With that many groups, keep `starboard.db` on an SSD.
- Coming from the version that kept setup in `.env`: run `/setup` and `/watch` again. `GROUP_ID`, the topic IDs and the settings in `.env` are no longer read.
- The bot logs to the console and to `bot.log`, with times in UTC. The log is capped at about 2 MB: `bot.log` plus one older `bot.log.1`.
- `.env` holds the bot token. Keep it private.
- `MAX_POST_AGE_SECONDS` and `MAX_TRACKED_POSTS` in `src/config.rs` set how long, and how many of a group's latest posts, can still be starred.
