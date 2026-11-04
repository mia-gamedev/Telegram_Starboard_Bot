"""Behaviour tests: every command, post and reaction path, with Telegram replaced by a fake.

Run: python tests/test_bot.py
"""
import sys
import time

import sandbox

RUN_DIR = sandbox.create()

import database  # noqa: E402
import starboard_bot  # noqa: E402
import telegram_api  # noqa: E402

GROUP_A, GROUP_B, CHANNEL = -1001111111111, -1002222222222, -1005555555555
BOT_ID, ADMIN, MEMBER, AUTHOR, ANON_BOT = 123456, 10, 20, 30, 1087968824
A_MUSIC, A_STARBOARD, B_MUSIC, B_STARBOARD = 3, 5, 8, 7
HEART, FIRE = "❤", "🔥"
DAY = 24 * 60 * 60
STATUS = {ADMIN: "administrator", MEMBER: "member", AUTHOR: "member", BOT_ID: "administrator"}
calls, queued_updates = [], []
failures = 0


def fake_call(method, **params):
    calls.append((method, params))
    if method == "getMe":
        return {"id": BOT_ID, "username": "Star_Bot"}
    if method == "getChatMember":
        return {"status": STATUS[params["user_id"]]}
    if method == "getUpdates":
        if params["timeout"] == 0:
            return []
        if not queued_updates:
            raise KeyboardInterrupt
        return queued_updates.pop(0)
    return {"message_id": 999} if method in ("forwardMessage", "sendMessage") else True


telegram_api.call = fake_call
starboard_bot.RETRY_DELAY_SECONDS = 0


def check(name, condition, detail=""):
    global failures
    failures += not condition
    print(f"{'PASS' if condition else 'FAIL'}  {name}" + (f"  -> {detail}" if not condition else ""))


def message(chat_id, sender, text="", thread=None, message_id=1, sender_chat=None, date=None, **extra):
    chat_type = "private" if chat_id > 0 else "supergroup"
    data = {"message_id": message_id, "date": date or int(time.time()), "from": {"id": sender, "is_bot": False},
            "chat": {"id": chat_id, "type": chat_type}, **extra}
    if text:
        data["text"] = text
    if thread:
        data |= {"message_thread_id": thread, "is_topic_message": True}
    if "http" in text:
        start = text.index("http")
        data["entities"] = [{"type": "url", "offset": start, "length": len(text) - start}]
    if sender_chat:
        data["sender_chat"] = {"id": sender_chat, "type": "supergroup" if sender_chat in (GROUP_A, GROUP_B) else "channel"}
    return {"update_id": 1, "message": data}


def reaction(chat_id, message_id, sender, old, new):
    emojis = lambda items: [{"type": "emoji", "emoji": e} for e in items]  # noqa: E731
    return {"update_id": 1, "message_reaction": {
        "chat": {"id": chat_id, "type": "supergroup"}, "message_id": message_id, "user": {"id": sender},
        "date": int(time.time()), "old_reaction": emojis(old), "new_reaction": emojis(new)}}


def send(update):
    calls.clear()
    starboard_bot.handle_safely(update)


def sent(method):
    return [params for name, params in calls if name == method]


def last_reply():
    replies = sent("sendMessage")
    return replies[-1]["text"] if replies else None


def tracked(chat_id):
    rows = database.connection.execute("SELECT message_id FROM posts WHERE chat_id = ? ORDER BY message_id", (chat_id,))
    return [row[0] for row in rows]


def hearts(chat_id, message_id):
    post = database.find_post(chat_id, message_id)
    return post.hearts if post else None


def configure():
    send(message(ADMIN, ADMIN, "/setup"))
    check("private /setup explains group use", "work in the group" in (last_reply() or ""), last_reply())
    send(message(GROUP_A, MEMBER, "/setup", thread=A_STARBOARD))
    check("non-admin /setup refused, nothing saved", last_reply() == "Only group admins can do that." and database.find_group(GROUP_A) is None, last_reply())
    send(message(GROUP_A, ADMIN, "/setup"))
    check("/setup in General refused", "inside a topic" in (last_reply() or ""), last_reply())
    send(message(GROUP_A, ADMIN, "/setup@other_bot", thread=A_STARBOARD))
    check("/setup@other_bot ignored", not calls, calls)
    send(message(GROUP_A, ADMIN, "/setup@star_bot", thread=A_STARBOARD))
    check("/setup@ourbot accepted with next step", "now the starboard" in last_reply() and "/watch" in last_reply(), last_reply())
    params = sent("sendMessage")[-1]
    check("reply goes to the same topic, quoting the command", params["message_thread_id"] == A_STARBOARD and params["reply_parameters"]["message_id"] == 1, params)
    STATUS[BOT_ID] = "member"
    send(message(GROUP_A, ADMIN, "/watch", thread=A_MUSIC))
    check("/watch refused while bot isn't admin", "Make me a group admin" in (last_reply() or "") and database.find_group(GROUP_A).music_topic_id is None, last_reply())
    STATUS[BOT_ID] = "administrator"
    send(message(GROUP_A, ADMIN, "/watch", thread=A_MUSIC))
    check("admin /watch accepted, no next step", last_reply() == "Posts with audio or a link in this topic can now be starred.", last_reply())
    send(message(GROUP_B, ADMIN, "/watch", thread=B_MUSIC))
    send(message(GROUP_B, ADMIN, "/setup", thread=B_STARBOARD))
    a, b = database.find_group(GROUP_A), database.find_group(GROUP_B)
    check("both groups set up independently", a.is_set_up and b.is_set_up and (a.starboard_topic_id, b.starboard_topic_id) == (A_STARBOARD, B_STARBOARD), (a, b))


def change_settings():
    send(message(GROUP_A, ANON_BOT, "/settings", sender_chat=GROUP_A))
    check("anonymous admin sees defaults", last_reply() == f"Current settings:\n• required-hearts: 2\n• emoji: {HEART}\n• self-stars: true", last_reply())
    send(message(GROUP_A, 136817688, "/settings required-hearts 1", sender_chat=CHANNEL))
    check("post as linked channel refused", last_reply() == "Only group admins can do that.", last_reply())
    send(message(GROUP_A, ADMIN, "/settings required-hearts ²"))
    check("'/settings required-hearts ²' gets usage, no crash", (last_reply() or "").startswith("Usage"), last_reply())
    send(message(GROUP_A, ADMIN, "/settings colour red"))
    check("unknown setting explained", (last_reply() or "").startswith("Unknown setting"), last_reply())
    send(message(GROUP_A, ADMIN, f"/settings emoji {FIRE}"))
    send(message(GROUP_A, ADMIN, "/settings self-stars false"))
    a, b = database.find_group(GROUP_A), database.find_group(GROUP_B)
    check("group A settings changed", (a.emoji, a.self_stars, a.required_hearts) == (FIRE, False, 2), a)
    check("group B keeps its defaults", (b.emoji, b.self_stars, b.required_hearts) == (HEART, True, 2), b)


def track_posts():
    send(message(GROUP_A, AUTHOR, "listen https://example.com/a", thread=A_MUSIC, message_id=100))
    send(message(GROUP_B, AUTHOR, "listen https://example.com/b", thread=B_MUSIC, message_id=100))
    send(message(GROUP_A, AUTHOR, "just chatting", thread=A_MUSIC, message_id=101))
    send(message(GROUP_A, AUTHOR, "https://example.com/x", thread=A_STARBOARD, message_id=102))
    send(message(GROUP_A, AUTHOR, thread=A_MUSIC, message_id=103, document={"mime_type": "audio/mpeg"}))
    send(message(GROUP_A, AUTHOR, thread=A_MUSIC, message_id=104, photo=[{}], caption="x https://y.z",
                 caption_entities=[{"type": "url", "offset": 2, "length": 11}]))
    send(message(GROUP_A, AUTHOR, thread=A_MUSIC, message_id=105, photo=[{}]))
    check("links, audio files and captioned links in the watched topic are tracked", tracked(GROUP_A) == [100, 103, 104], tracked(GROUP_A))
    check("same message id tracked separately per group", tracked(GROUP_B) == [100], tracked(GROUP_B))
    old = int(time.time()) - 31 * DAY
    send(message(GROUP_A, AUTHOR, "https://example.com/old", thread=A_MUSIC, message_id=106, date=old))
    check("a link already older than 30 days isn't kept", 106 not in tracked(GROUP_A), tracked(GROUP_A))


def star_posts():
    send(reaction(GROUP_A, 100, AUTHOR, [], [FIRE]))
    check("A: author's own reaction ignored (self-stars off)", hearts(GROUP_A, 100) == 0)
    send(reaction(GROUP_A, 100, MEMBER, [], [HEART]))
    check("A: heart ignored because A counts fire", hearts(GROUP_A, 100) == 0)
    send(reaction(GROUP_B, 100, MEMBER, [], [HEART]))
    check("B: heart counts in B only", (hearts(GROUP_A, 100), hearts(GROUP_B, 100)) == (0, 1))
    send(reaction(GROUP_A, 100, MEMBER, [HEART], [FIRE]))
    send(reaction(GROUP_A, 100, MEMBER, [FIRE], []))
    check("A: switching to fire then removing it nets zero", hearts(GROUP_A, 100) == 0)
    send(reaction(GROUP_A, 100, MEMBER, [], [FIRE]))
    send(reaction(GROUP_A, 100, ADMIN, [], [FIRE]))
    check_forward("A", GROUP_A, A_STARBOARD, FIRE)
    send(reaction(GROUP_B, 100, AUTHOR, [], [HEART]))
    check_forward("B (self-stars on)", GROUP_B, B_STARBOARD, HEART)
    send(reaction(GROUP_A, 100, AUTHOR, [], [FIRE]))
    check("no second forward", not sent("forwardMessage"))


def check_forward(label, chat_id, topic, emoji):
    forwards = sent("forwardMessage")
    expected = {"chat_id": chat_id, "from_chat_id": chat_id, "message_id": 100, "message_thread_id": topic}
    check(f"{label}: forwarded to its own starboard topic", forwards == [expected], forwards)
    check(f"{label}: bot reacts with the group's emoji", sent("setMessageReaction") == [{"chat_id": chat_id, "message_id": 999, "reaction": [{"type": "emoji", "emoji": emoji}]}], sent("setMessageReaction"))
    check(f"{label}: forwarded post forgotten", database.find_post(chat_id, 100) is None)


def expiry_and_cap():
    old = int(time.time()) - 31 * DAY
    database.remember_post(GROUP_A, 50, AUTHOR, old)
    database.remember_post(GROUP_B, 50, AUTHOR, old)
    send(reaction(GROUP_A, 50, MEMBER, [], [FIRE]))
    check("reaction on an expired post is ignored and clears it", database.find_post(GROUP_A, 50) is None and not sent("forwardMessage"))
    database.remember_post(GROUP_A, 51, AUTHOR, old)
    send(message(GROUP_A, AUTHOR, "https://example.com/new", thread=A_MUSIC, message_id=300))
    check("a new link clears its own group's expired posts", 51 not in tracked(GROUP_A) and 300 in tracked(GROUP_A), tracked(GROUP_A))
    check("other groups' expired posts are left for later", tracked(GROUP_B) == [50], tracked(GROUP_B))
    database.forget_posts_before(starboard_bot.post_age_cutoff())
    check("startup sweep clears quiet groups", tracked(GROUP_B) == [], tracked(GROUP_B))
    starboard_bot.MAX_TRACKED_POSTS = 3
    for message_id in range(301, 306):
        send(message(GROUP_A, AUTHOR, "https://example.com/c", thread=A_MUSIC, message_id=message_id))
    check("cap keeps only the group's newest posts", tracked(GROUP_A) == [303, 304, 305], tracked(GROUP_A))
    starboard_bot.MAX_TRACKED_POSTS = 1000


def query_plans():
    queries = {
        "find_post": "SELECT * FROM posts WHERE chat_id = 1 AND message_id = 1",
        "save_hearts": "UPDATE posts SET hearts = 1 WHERE chat_id = 1 AND message_id = 1",
        "forget_post": "DELETE FROM posts WHERE chat_id = 1 AND message_id = 1",
        "forget_group_posts_before": "DELETE FROM posts WHERE chat_id = 1 AND posted_at < 5",
        "forget_oldest_posts": "DELETE FROM posts WHERE chat_id = 1 AND message_id <= (SELECT message_id FROM posts WHERE chat_id = 1 ORDER BY message_id DESC LIMIT 1 OFFSET 5)",
        "find_group": "SELECT * FROM groups WHERE chat_id = 1",
    }
    for name, sql in queries.items():
        plan = " | ".join(row[-1] for row in database.connection.execute("EXPLAIN QUERY PLAN " + sql))
        check(f"{name} goes straight to its rows", "SCAN" not in plan, plan)


def polling_loop():
    queued_updates.extend([
        [{"update_id": 40, "message": {"broken": True}}, message(GROUP_A, AUTHOR, "https://e.com/l", thread=A_MUSIC, message_id=400) | {"update_id": 41}],
    ])
    calls.clear()
    try:
        starboard_bot.main()
    except KeyboardInterrupt:
        pass
    polls = sent("getUpdates")
    check("loop survives a broken update and handles the next", 400 in tracked(GROUP_A), tracked(GROUP_A))
    check("polls only messages and reactions", polls[0]["allowed_updates"] == ["message", "message_reaction"], polls[0])
    check("next poll confirms handled updates", polls[1]["offset"] == 42, polls)
    check("exit confirms the last batch", polls[-1] == {"offset": 42, "timeout": 0, "limit": 1}, polls[-1])


def interrupted_batch_rolls_back():
    real_handle_update = starboard_bot.handle_update

    def handle_or_interrupt(update):
        if update.get("interrupt"):
            raise KeyboardInterrupt
        real_handle_update(update)

    starboard_bot.handle_update = handle_or_interrupt
    queued_updates.extend([
        [message(GROUP_A, AUTHOR, "https://e.com/1", thread=A_MUSIC, message_id=500) | {"update_id": 50}],
        [message(GROUP_A, AUTHOR, "https://e.com/2", thread=A_MUSIC, message_id=501) | {"update_id": 51},
         {"update_id": 52, "interrupt": True}],
    ])
    calls.clear()
    try:
        starboard_bot.main()
    except KeyboardInterrupt:
        pass
    starboard_bot.handle_update = real_handle_update
    check("completed batch is saved", 500 in tracked(GROUP_A), tracked(GROUP_A))
    check("interrupted batch is rolled back entirely", 501 not in tracked(GROUP_A), tracked(GROUP_A))
    check("interrupted batch isn't confirmed, so Telegram resends it", sent("getUpdates")[-1]["offset"] == 51, sent("getUpdates")[-1])


def crash_is_logged():
    send({"update_id": 1, "message_reaction": {"chat": {}}})
    log_text = (RUN_DIR / "bot.log").read_text(encoding="utf8")
    entry = log_text[log_text.rfind("ERROR Failed to handle"):]
    check("a crashing update is logged with where it failed", "Traceback (most recent call last):" in entry
          and 'starboard_bot.py", line' in entry and entry.rstrip().endswith("KeyError: 'id'"), entry[-300:])
    check("logging a crash didn't load the traceback module", "traceback" not in sys.modules)


def http_reader():
    import io
    stream = io.BytesIO(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: keep-alive\r\n\r\n{\"ok\":true}")
    headers = telegram_api.read_response_headers(stream)
    check("reads a Content-Length reply", telegram_api.read_response_body(stream, headers) == b'{"ok":true}', headers)
    stream = io.BytesIO(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"ok\r\n7;x=y\r\n\":true}\r\n0\r\n\r\n")
    headers = telegram_api.read_response_headers(stream)
    check("reads a chunked reply", telegram_api.read_response_body(stream, headers) == b'{"ok":true}', headers)
    stream = io.BytesIO(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{\"ok\":tr")
    headers = telegram_api.read_response_headers(stream)
    try:
        telegram_api.read_response_body(stream, headers)
        check("a reply cut short is an error", False)
    except ConnectionError:
        check("a reply cut short is an error", True)
    try:
        telegram_api.read_response_headers(io.BytesIO(b""))
        check("a closed connection is an error", False)
    except ConnectionError:
        check("a closed connection is an error", True)


def network_error_retry():
    attempts = []

    def flaky_call(method, **params):
        attempts.append(method)
        if len(attempts) == 1:
            raise ConnectionResetError("simulated drop")
        raise KeyboardInterrupt

    telegram_api.call = flaky_call
    try:
        starboard_bot.fetch_updates(0)
        starboard_bot.fetch_updates(0)
    except KeyboardInterrupt:
        pass
    check("a dropped connection is retried instead of crashing", attempts == ["getUpdates", "getUpdates"], attempts)
    telegram_api.call = fake_call


telegram_api.bot_account.update({"id": BOT_ID, "username": "Star_Bot"})
configure()
change_settings()
track_posts()
star_posts()
expiry_and_cap()
query_plans()
polling_loop()
interrupted_batch_rolls_back()
crash_is_logged()
http_reader()
network_error_retry()
print(f"{failures} failure(s)")
