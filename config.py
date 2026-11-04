import os
import re

import log

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
ENV_FILE = os.path.join(BASE_DIR, ".env")
DATABASE_FILE = os.path.join(BASE_DIR, "starboard.db")

MAX_POST_AGE_SECONDS = 30 * 24 * 60 * 60
MAX_TRACKED_POSTS = 1000
DEFAULT_REQUIRED_HEARTS = 2
DEFAULT_EMOJI = "❤"
# Every emoji Telegram accepts as a reaction (Bot API, ReactionTypeEmoji).
ALLOWED_EMOJI = frozenset(
    "👍 👎 ❤ 🔥 🥰 👏 😁 🤔 🤯 😱 🤬 😢 🎉 🤩 🤮 💩 🙏 👌 🕊 🤡 🥱 🥴 😍 🐳 ❤️‍🔥 🌚 🌭 💯 🤣 ⚡ 🍌 "
    "🏆 💔 🤨 😐 🍓 🍾 💋 🖕 😈 😴 😭 🤓 👻 👨‍💻 👀 🎃 🙈 😇 😨 🤝 ✍ 🤗 🫡 🎅 🎄 ☃ 💅 🤪 🗿 🆒 "
    "💘 🙉 🦄 😘 💊 🙊 😎 👾 🤷‍♂️ 🤷 🤷‍♀️ 😡".split()
)


def read_env_file() -> dict:
    if not os.path.exists(ENV_FILE):
        return {}
    with open(ENV_FILE, encoding="utf8") as env_file:
        lines = env_file.read().splitlines()
    settings = {}
    for line in lines:
        key, separator, value = line.partition("=")
        if separator and not key.strip().startswith("#"):
            settings[key.strip()] = value.strip().strip("\"'")
    return settings


def read_bot_token() -> str:
    token = (os.environ.get("BOT_TOKEN") or read_env_file().get("BOT_TOKEN", "")).strip()
    if not re.fullmatch(r"\d+:[\w-]+", token):
        log.write("CRITICAL", "BOT_TOKEN is missing or invalid in .env")
        raise SystemExit(1)
    return token


BOT_TOKEN = read_bot_token()
