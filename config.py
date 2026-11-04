import logging
import os
import re
from datetime import timedelta
from logging.handlers import RotatingFileHandler
from pathlib import Path

from dotenv import load_dotenv
from telegram.constants import ReactionEmoji

ENV_FILE = Path(__file__).with_name(".env")
DATABASE_FILE = Path(__file__).with_name("starboard.db")
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


def read_bot_token() -> str:
    load_dotenv(ENV_FILE)
    token = os.environ.get("BOT_TOKEN", "").strip()
    if not re.fullmatch(r"\d+:[\w-]+", token):
        logger.critical("BOT_TOKEN is missing or invalid in .env")
        raise SystemExit(1)
    return token


BOT_TOKEN = read_bot_token()
