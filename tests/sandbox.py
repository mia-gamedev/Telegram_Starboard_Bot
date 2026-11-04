"""Run the bot from a throwaway copy, so tests never touch the real .env, database or log."""
import atexit
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

BOT_DIR = Path(__file__).resolve().parent.parent
TEST_TOKEN = "123456:TEST-token"
FULL_SIZE_DATABASE = Path(tempfile.gettempdir()) / "starboard-tests" / "full_size.db"


def create(full_size_database: bool = False) -> Path:
    directory = Path(tempfile.mkdtemp(prefix="starboard-test-"))
    for module in BOT_DIR.glob("*.py"):
        shutil.copy(module, directory)
    (directory / ".env").write_text(f"BOT_TOKEN={TEST_TOKEN}\n", encoding="utf8")
    if full_size_database:
        build_full_size_database()
        shutil.copyfile(FULL_SIZE_DATABASE, directory / "starboard.db")
    sys.path.insert(0, str(directory))
    atexit.register(remove, directory)
    return directory


def build_full_size_database() -> None:
    # Built once in its own process (so it doesn't skew memory measurements), then reused.
    if not FULL_SIZE_DATABASE.exists():
        subprocess.run([sys.executable, str(Path(__file__).with_name("seed_database.py"))], check=True)


def remove(directory: Path) -> None:
    database = sys.modules.get("database")
    if database is not None:
        database.connection.close()
    shutil.rmtree(directory, ignore_errors=True)
