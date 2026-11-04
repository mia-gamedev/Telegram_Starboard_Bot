import os
import sys
import time

LOG_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bot.log")
OLD_LOG_FILE = LOG_FILE + ".1"
MAX_LOG_BYTES = 10**6


def write(level: str, text: str) -> None:
    line = f"{time.strftime('%Y-%m-%d %H:%M:%S')} {level} {text}\n"
    # A background task has no console, so there may be no stderr.
    if sys.stderr:
        sys.stderr.write(line)
    try:
        if os.path.exists(LOG_FILE) and os.path.getsize(LOG_FILE) > MAX_LOG_BYTES:
            os.replace(LOG_FILE, OLD_LOG_FILE)
        with open(LOG_FILE, "a", encoding="utf8") as log_file:
            log_file.write(line)
    except OSError:
        pass  # Losing a log line must never stop the bot.


def info(text: str) -> None:
    write("INFO", text)


def warning(text: str) -> None:
    write("WARNING", text)


def error(text: str) -> None:
    write("ERROR", f"{text}\n{describe_current_exception()}")


def describe_current_exception() -> str:
    # Written out by hand: the traceback module alone costs ~2.7 MB of RAM.
    error = sys.exc_info()[1]
    if error is None:
        return ""
    lines = ["Traceback (most recent call last):"]
    step = error.__traceback__
    while step is not None:
        code = step.tb_frame.f_code
        lines.append(f'  File "{code.co_filename}", line {step.tb_lineno}, in {code.co_name}')
        step = step.tb_next
    lines.append(f"{type(error).__name__}: {error}")
    return "\n".join(lines)
