import logging
import os
import re
from dataclasses import dataclass
from datetime import timedelta
from logging.handlers import RotatingFileHandler
from pathlib import Path

from dotenv import load_dotenv
from telegram import ReactionTypeEmoji
from telegram.constants import ReactionEmoji

ENV_FILE = Path(__file__).with_name(".env")
DATA_FILE = Path(__file__).with_name("bot_data.pickle")
LOG_FILE = Path(__file__).with_name("bot.log")

log_file = RotatingFileHandler(LOG_FILE, maxBytes=10**6, backupCount=1, encoding="utf8")
logging.basicConfig(
    format="%(asctime)s %(levelname)s %(message)s",
    level=logging.INFO,
    handlers=[logging.StreamHandler(), log_file],
)
logging.getLogger("httpx").setLevel(logging.WARNING)  # request logs contain the token
logger = logging.getLogger(__name__)

MAX_POST_AGE = timedelta(days=30)
DEFAULT_REQUIRED_HEARTS = 2
DEFAULT_EMOJI = ReactionEmoji.RED_HEART.value
ALLOWED_EMOJI = frozenset(emoji.value for emoji in ReactionEmoji)


@dataclass
class Config:
    bot_token: str
    group_id: int | None
    music_topic_id: int | None
    starboard_topic_id: int | None
    required_hearts: int
    emoji: str
    self_stars: bool

    @property
    def is_set_up(self) -> bool:
        return None not in (self.group_id, self.music_topic_id, self.starboard_topic_id)

    @property
    def heart_reaction(self) -> ReactionTypeEmoji:
        return ReactionTypeEmoji(self.emoji)


def read_setting(name: str, pattern: str) -> str:
    value = os.environ.get(name, "").strip()
    if not re.fullmatch(pattern, value):
        logger.critical("%s is missing or invalid in .env", name)
        raise SystemExit(1)
    return value


def read_optional_setting(name: str, pattern: str) -> str | None:
    value = os.environ.get(name, "").strip()
    if not value:
        return None
    if not re.fullmatch(pattern, value):
        logger.warning("%s is set but invalid in .env; ignoring it", name)
        return None
    return value


def read_emoji() -> str:
    value = os.environ.get("EMOJI", "").strip()
    if value in ALLOWED_EMOJI:
        return value
    if value:
        logger.warning("EMOJI in .env isn't a reaction Telegram allows; ignoring it")
    return DEFAULT_EMOJI


def normalize_group_id(value: str | None) -> int | None:
    # Message links show the group as 1234567890; the Bot API calls it -1001234567890.
    return int("-100" + value.removeprefix("-100")) if value else None


def to_int(value: str | None) -> int | None:
    return int(value) if value else None


def load_config() -> Config:
    load_dotenv(ENV_FILE)
    return Config(
        bot_token=read_setting("BOT_TOKEN", r"\d+:[\w-]+"),
        group_id=normalize_group_id(read_optional_setting("GROUP_ID", r"(-100)?\d+")),
        music_topic_id=to_int(read_optional_setting("MUSIC_TOPIC_ID", r"\d+")),
        starboard_topic_id=to_int(read_optional_setting("STARBOARD_TOPIC_ID", r"\d+")),
        required_hearts=to_int(read_optional_setting("REQUIRED_HEARTS", r"[1-9]\d*"))
        or DEFAULT_REQUIRED_HEARTS,
        emoji=read_emoji(),
        self_stars=read_optional_setting("SELF_STARS", "true|false") != "false",
    )


def save_config(changes: dict) -> None:
    write_env({name.upper(): env_text(value) for name, value in changes.items()})
    for name, value in changes.items():
        setattr(CONFIG, name, value)


def env_text(value: object) -> str:
    return str(value).lower() if isinstance(value, bool) else str(value)


def write_env(values: dict) -> None:
    lines = ENV_FILE.read_text(encoding="utf8").splitlines() if ENV_FILE.exists() else []
    pending = dict(values)
    for index, line in enumerate(lines):
        key = line.split("=", 1)[0].strip()
        if key in pending:
            lines[index] = f"{key}={pending.pop(key)}"
    lines.extend(f"{key}={value}" for key, value in pending.items())
    # Write a copy and swap it in, so a crash mid-write can't wipe the token.
    temp_file = ENV_FILE.with_name(".env.tmp")
    temp_file.write_text("\n".join(lines) + "\n", encoding="utf8")
    temp_file.replace(ENV_FILE)


CONFIG = load_config()
