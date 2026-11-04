"""RAM use, idle and while handling updates, against the full-size database (Windows only).

Run: python tests/bench_memory.py [updates, default 50000]
"""
import sys

from memusage import memory_mb

python_alone = memory_mb()

import json  # noqa: E402
import time  # noqa: E402

import sandbox  # noqa: E402
import workload  # noqa: E402

UPDATES = int(sys.argv[1]) if len(sys.argv) > 1 else 50_000

sandbox.create(full_size_database=True)
import starboard_bot  # noqa: E402
import telegram_api  # noqa: E402


def fake_call(method, **params):
    if method == "getChatMember":
        return {"status": "administrator"}
    return {"message_id": 1} if method in ("forwardMessage", "sendMessage") else True


telegram_api.call = fake_call
telegram_api.bot_account.update({"id": 1, "username": "bot"})
idle = memory_mb()
started = time.perf_counter()
for _, batch in workload.make_batches(UPDATES):
    starboard_bot.handle_batch(json.loads(batch)["result"])
busy = memory_mb()

print(f"Python alone:          {python_alone[0]:5.1f} MB")
print(f"Bot idle:              {idle[0]:5.1f} MB (private {idle[1]:.1f} MB)")
print(f"After {UPDATES:,} updates: {busy[0]:5.1f} MB (private {busy[1]:.1f} MB, peak {busy[2]:.1f} MB)")
print("Generating the test updates runs in this process too, so the last line is an upper bound.")
