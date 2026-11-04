"""CPU per update for the bot alone, against the full-size database.

Test updates are generated before the clock starts. Run: python tests/bench_cpu.py [updates] [seed]
"""
import json
import sys
import time

import sandbox
import workload

UPDATES = int(sys.argv[1]) if len(sys.argv) > 1 else 50_000
SEED = int(sys.argv[2]) if len(sys.argv) > 2 else 2

sandbox.create(full_size_database=True)
import starboard_bot  # noqa: E402
import telegram_api  # noqa: E402


def fake_call(method, **params):
    return {"message_id": 1} if method in ("forwardMessage", "sendMessage") else True


telegram_api.call = fake_call
batches = list(workload.make_batches(UPDATES, seed=SEED))
cpu_started, wall_started = time.process_time(), time.perf_counter()
for _, batch in batches:
    starboard_bot.handle_batch(json.loads(batch)["result"])
cpu = (time.process_time() - cpu_started) / UPDATES * 1e6
wall = (time.perf_counter() - wall_started) / UPDATES * 1e6
print(f"{UPDATES:,} updates: {cpu:.1f} us CPU and {wall:.1f} us wall per update "
      f"-> one core keeps up with ~{1e6 / wall:,.0f} updates/s")
