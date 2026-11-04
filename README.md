# Starboard Bot

A Discord-style starboard for a Telegram group with topics. When a post in the watched topic (e.g. **Music Sharing**) gets 2 ❤ reactions, the bot forwards it to the **starboard** topic.

- Only posts with an audio file or a link count. Plain chat is ignored.
- Posts older than a month no longer count.
- Each post is forwarded once. Telegram doesn't copy reactions when forwarding, so the bot puts the first ❤ on the starboard copy and everyone can add theirs there.

## Setup

1. Create a bot with [@BotFather](https://t.me/BotFather) and put its token in `.env` as `BOT_TOKEN=...` (`install.ps1` asks for it if it's missing).
2. Start the bot (see below), add it to the group and make it an **admin**. Telegram only sends reactions to admin bots.
3. As a group admin, in the group:
   - send `/setup` in the topic that should become the starboard,
   - send `/watch` in the topic whose posts can be starred.

Both work right away and are saved to `.env`, so they survive restarts and crashes. Send them again in another topic to move the starboard or watch a different topic.

## Settings

Group admins can change the rules with `/settings` in the group. On its own, it shows the current values.

| Setting | Default | Example | What it does |
|---|---|---|---|
| `required-hearts` | 2 | `/settings required-hearts 3` | Reactions a post needs to reach the starboard. |
| `emoji` | ❤ | `/settings emoji 🔥` | The reaction that counts. Must be one Telegram allows as a reaction. |
| `self-stars` | true | `/settings self-stars false` | Whether the author's own reaction counts. |

Settings are saved to `.env` too.

## Run it

With Python 3.10 or newer:

```
pip install -r requirements.txt
python starboard_bot.py
```

If `BOT_TOKEN` is missing or invalid, the bot stops right away. The bot only works while it's running, so run it on a computer that stays on. Stop it with Ctrl+C, and restart it after editing `.env` by hand.

### In the background on Windows

From an Administrator PowerShell in this folder:

```
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

This checks the token with Telegram and installs a `StarboardBot` task that starts with Windows and restarts the bot if it stops. `uninstall.ps1` removes it.

## Good to know

- Only one copy of the bot can run per token.
- To stop anyone from adding the bot to other groups, send `/setjoingroups` to @BotFather and choose *Disable* once the bot is in your group.
- Once set up, the bot ignores `/setup`, `/watch` and `/settings` from other groups. To move it to another group, remove `GROUP_ID` from `.env` and restart it.
- Telegram doesn't tell bots which topic a reacted message is in, so the bot remembers watched posts as they're made. Posts from before it was running, or before `/watch`, never count.
- Tracked posts are saved in `bot_data.pickle`, so restarts are fine.
- The bot logs to the console and to `bot.log` next to it. The log is capped at about 2 MB: `bot.log` plus one older `bot.log.1`.
- `.env` holds the bot token. Keep it private.
- `MAX_POST_AGE` in `config.py` sets how long posts can be starred.
