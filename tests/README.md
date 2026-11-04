# Tests and benchmarks

Each script copies the bot into a temporary folder with a test token, so nothing here touches the real `.env`, `starboard.db` or `bot.log`. Telegram is replaced by a fake, so no network or real token is needed.

| Command | What it does |
|---|---|
| `python tests/test_bot.py` | Behaviour tests: commands, posts, reactions, expiry, the cap, the polling loop, crash recovery and the HTTP reader. |
| `python tests/bench_cpu.py` | CPU per update against a full-size database (50,000 groups, 12.2 million posts). |
| `python tests/bench_memory.py` | RAM idle and under load (Windows only). |
| `python tests/load_model.py` | CPU, network and disk for the assumed group mix in `workload.py`. |
| `python tests/seed_database.py` | Rebuilds the full-size database. The benchmarks build it on first use (~20 s, ~600 MB in your temp folder). |
