import time

import commands
import database
import log
import telegram_api as telegram
from config import MAX_POST_AGE_SECONDS, MAX_TRACKED_POSTS
from database import Group, Post

POLL_WAIT_SECONDS = 50
RETRY_DELAY_SECONDS = 5
ALLOWED_UPDATES = ["message", "message_reaction"]
LINK_ENTITY_TYPES = {"url", "text_link"}


def post_age_cutoff() -> int:
    return int(time.time()) - MAX_POST_AGE_SECONDS


def handle_update(update: dict) -> None:
    if "message_reaction" in update:
        track_hearts(update["message_reaction"])
    elif "message" in update:
        handle_message(update["message"])


def handle_message(message: dict) -> None:
    if message.get("text", "").startswith("/"):
        commands.handle_command(message)
    elif is_music_post(message):
        remember_music_post(message)


def is_music_post(message: dict) -> bool:
    if not message.get("is_topic_message"):
        return False
    if "audio" in message or message.get("document", {}).get("mime_type", "").startswith("audio/"):
        return True
    entities = message.get("entities") or message.get("caption_entities") or ()
    return any(entity["type"] in LINK_ENTITY_TYPES for entity in entities)


def remember_music_post(message: dict) -> None:
    # Reaction updates lack the topic, so remember posts as they're made.
    chat_id = message["chat"]["id"]
    group = database.find_group(chat_id)
    if group is None or not group.is_set_up or message["message_thread_id"] != group.music_topic_id:
        return
    database.remember_post(chat_id, message["message_id"], message["from"]["id"], message["date"])
    # Busy groups get a shorter wait instead of unbounded storage.
    database.forget_group_posts_before(chat_id, post_age_cutoff())
    database.forget_oldest_posts(chat_id, keep=MAX_TRACKED_POSTS)


def track_hearts(reaction: dict) -> None:
    group = database.find_group(reaction["chat"]["id"])
    if group is None or not group.is_set_up:
        return
    change = heart_change(reaction, group.emoji)
    if change == 0:
        return
    post = database.find_post(group.chat_id, reaction["message_id"])
    if post is None or is_ignored_self_star(reaction, post, group):
        return
    if post.posted_at < post_age_cutoff():
        database.forget_post(group.chat_id, post.message_id)
        return
    hearts = post.hearts + change
    database.save_hearts(post.chat_id, post.message_id, hearts)
    if hearts >= group.required_hearts:
        promote_to_starboard(group, post)


def heart_change(reaction: dict, emoji: str) -> int:
    heart = {"type": "emoji", "emoji": emoji}
    return (heart in reaction["new_reaction"]) - (heart in reaction["old_reaction"])


def is_ignored_self_star(reaction: dict, post: Post, group: Group) -> bool:
    reactor_id = reaction.get("user", {}).get("id")
    return not group.self_stars and reactor_id == post.author_id


def promote_to_starboard(group: Group, post: Post) -> None:
    starboard_post = telegram.call(
        "forwardMessage",
        chat_id=group.chat_id,
        from_chat_id=group.chat_id,
        message_id=post.message_id,
        message_thread_id=group.starboard_topic_id,
    )
    database.forget_post(group.chat_id, post.message_id)
    log.info(f"Forwarded post {post.message_id} in group {group.chat_id} to the starboard")
    # The bot adds the first heart.
    telegram.call(
        "setMessageReaction",
        chat_id=group.chat_id,
        message_id=starboard_post["message_id"],
        reaction=[{"type": "emoji", "emoji": group.emoji}],
    )


def handle_safely(update: dict) -> None:
    try:
        handle_update(update)
    except Exception:
        log.error(f"Failed to handle {update}")


def handle_batch(updates: list) -> None:
    # One commit per batch instead of per change. If the bot dies mid-batch the batch is
    # rolled back, and Telegram resends it because its offset was never confirmed.
    with database.transaction():
        for update in updates:
            handle_safely(update)


def fetch_updates(offset: int) -> list:
    try:
        return telegram.call(
            "getUpdates", offset=offset, timeout=POLL_WAIT_SECONDS, allowed_updates=ALLOWED_UPDATES
        )
    except Exception as error:
        log.warning(f"Fetching updates failed ({error}); retrying in {RETRY_DELAY_SECONDS} s")
        time.sleep(RETRY_DELAY_SECONDS)
        return []


def wait_for_telegram() -> None:
    # Keep retrying at startup until Telegram is reachable (e.g. right after a reboot).
    while True:
        try:
            telegram.identify_bot()
            return
        except telegram.NETWORK_ERRORS as error:
            log.warning(f"Telegram unreachable ({error}); retrying in {RETRY_DELAY_SECONDS} s")
            time.sleep(RETRY_DELAY_SECONDS)


def confirm_handled_updates(offset: int) -> None:
    # Telegram only forgets updates once a later poll names the next offset.
    try:
        telegram.call("getUpdates", offset=offset, timeout=0, limit=1)
    except Exception:
        log.warning("Couldn't confirm the last updates; Telegram will resend them")


def main() -> None:
    # Groups that went quiet only get their old posts cleared here.
    database.forget_posts_before(post_age_cutoff())
    wait_for_telegram()
    log.info(f"Running as @{telegram.bot_account['username']}")
    offset = 0
    try:
        while True:
            updates = fetch_updates(offset)
            if updates:
                handle_batch(updates)
                offset = updates[-1]["update_id"] + 1
    finally:
        if offset:
            confirm_handled_updates(offset)


if __name__ == "__main__":
    main()
