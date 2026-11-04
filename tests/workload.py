"""Realistic update batches: 80% chat, 3% links in the watched topic, 17% reactions."""
import json
import random
import time

GROUPS = 50_000
MUSIC, STARBOARD = 3, 5
# Assumed group mix: (share of groups, messages/day, links/day, reactions/day)
PROFILES = {
    "huge (1%)": (0.01, 20_000, 500, 5_000),
    "big (10%)": (0.10, 2_000, 50, 500),
    "small (89%)": (0.89, 200, 5, 20),
}


def group_id(index):
    return -1001000000000 - index


def user(uid):
    return {"id": uid, "is_bot": False, "first_name": "Alex", "last_name": "Example",
            "username": f"user{uid}", "language_code": "de"}


def chat(index):
    return {"id": group_id(index), "title": f"Group {index}", "type": "supergroup", "is_forum": True}


def topic_message(index, message_id, thread, text, entities=None):
    message = {
        "message_id": message_id, "from": user(5_000_000_000 + message_id % 9973), "chat": chat(index),
        "date": int(time.time()), "message_thread_id": thread, "is_topic_message": True, "text": text,
        "reply_to_message": {"message_id": thread, "from": user(42), "chat": chat(index), "date": 1700000000,
                             "message_thread_id": thread, "forum_topic_created": {"name": "Music", "icon_color": 7322096},
                             "is_topic_message": True},
    }
    if entities:
        message["entities"] = entities
    return message


def reaction(index, message_id, emoji):
    return {"chat": chat(index), "message_id": message_id, "user": user(6_000_000_000 + message_id % 7919),
            "date": int(time.time()), "old_reaction": [], "new_reaction": [{"type": "emoji", "emoji": emoji}]}


def make_update(rng, update_id, link_ids):
    index = rng.randrange(GROUPS)
    message_id = 50_000_000 + update_id
    roll = rng.random()
    if roll < 0.80:
        text = "haha yes, see you all at the gig tonight, who brings the speakers?"
        return "chat", {"message": topic_message(index, message_id, rng.choice([MUSIC, 9, 11]), text)}
    if roll < 0.83:
        url = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"
        link_ids.append((index, message_id))
        return "link", {"message": topic_message(index, message_id, MUSIC, f"this one {url}",
                                                 [{"type": "url", "offset": 9, "length": len(url)}])}
    target = rng.choice(link_ids) if link_ids and rng.random() < 0.3 else (index, message_id - 5)
    return "reaction", {"message_reaction": reaction(target[0], target[1], rng.choice(["❤", "👍", "🔥"]))}


def make_batches(count, batch_size=100, seed=1):
    """Yield (kinds, encoded getUpdates reply) one batch at a time, so test data doesn't sit in RAM."""
    rng, link_ids = random.Random(seed), []
    for start in range(1, count + 1, batch_size):
        kinds, updates = [], []
        for update_id in range(start, min(start + batch_size, count + 1)):
            kind, body = make_update(rng, update_id, link_ids)
            kinds.append(kind)
            updates.append({"update_id": update_id, **body})
        yield kinds, json.dumps({"ok": True, "result": updates}).encode()
