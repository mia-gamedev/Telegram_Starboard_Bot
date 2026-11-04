"""What the assumed 50,000-group mix costs in CPU, network and disk.

Group activity comes from workload.PROFILES; edit it to try other mixes.
The per-update numbers were measured with bench_cpu.py and on the real Telegram API.
Run: python tests/load_model.py
"""
import workload

CPU_MICROSECONDS_PER_UPDATE = 12.5
BYTES_PER_UPDATE = 767  # Average JSON size of a realistic group update.
DISK_BYTES_PER_UPDATE = 2 * 241  # Write-ahead log, then the same pages again when checkpointed.
ROUND_TRIP_MS = 20  # Measured to api.telegram.org from the development PC.
PEAK_FACTOR = 3  # Evening peak compared with the daily average.
UPDATES_PER_POLL = 100


def daily_totals() -> tuple:
    messages = links = reactions = 0
    for name, (share, group_messages, group_links, group_reactions) in workload.PROFILES.items():
        groups = round(workload.GROUPS * share)
        messages += groups * group_messages
        links += groups * group_links
        reactions += groups * group_reactions
        print(f"  {name:12} {groups:6,} groups, each per day: {group_messages:6,} messages, "
              f"{group_links:3} links, {group_reactions:5,} reactions")
    return messages, links, reactions


def main() -> None:
    messages, links, reactions = daily_totals()
    updates_per_day = messages + reactions
    average = updates_per_day / 86_400
    peak = average * PEAK_FACTOR
    seconds_per_poll = ROUND_TRIP_MS / 1000 + UPDATES_PER_POLL * CPU_MICROSECONDS_PER_UPDATE / 1e6
    ceiling = UPDATES_PER_POLL / seconds_per_poll
    print(f"Updates: {updates_per_day / 1e6:.1f} M a day = {average:,.0f}/s on average, ~{peak:,.0f}/s at peak")
    print(f"CPU: {average * CPU_MICROSECONDS_PER_UPDATE / 1e4:.2f}% of one core on average, "
          f"{peak * CPU_MICROSECONDS_PER_UPDATE / 1e4:.2f}% at peak")
    print(f"Polling limit at {ROUND_TRIP_MS} ms round trip: ~{ceiling:,.0f} updates/s ({ceiling / peak:.1f}x the peak)")
    print(f"Download: {updates_per_day * BYTES_PER_UPDATE / 1e9:.0f} GB a day, "
          f"{average * BYTES_PER_UPDATE * 8 / 1e6:.1f} Mbit/s average, {peak * BYTES_PER_UPDATE * 8 / 1e6:.1f} Mbit/s peak")
    print(f"Disk writes: {updates_per_day * DISK_BYTES_PER_UPDATE / 1e9:.0f} GB a day "
          f"({updates_per_day * DISK_BYTES_PER_UPDATE * 365 / 1e12:.1f} TB a year)")
    print(f"New links: {links:,} a day")


if __name__ == "__main__":
    main()
