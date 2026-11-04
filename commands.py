import database
import telegram_api as telegram
from config import ALLOWED_EMOJI
from database import Group

BULLET = "•"
ADMIN_STATUSES = {"administrator", "creator"}
GROUP_CHAT_TYPES = {"group", "supergroup"}


def handle_command(message: dict) -> None:
    command, *args = message["text"].split()
    name, _, addressee = command[1:].partition("@")
    if addressee and addressee.lower() != telegram.bot_account["username"].lower():
        return
    handler = GROUP_COMMANDS.get(name)
    if handler is None:
        return
    chat_type = message["chat"]["type"]
    if chat_type == "private":
        explain_group_only(message)
    elif chat_type in GROUP_CHAT_TYPES:
        handler(message, args)


def reply(message: dict, text: str) -> None:
    params = {
        "chat_id": message["chat"]["id"],
        "text": text,
        "reply_parameters": {"message_id": message["message_id"], "allow_sending_without_reply": True},
    }
    if message.get("is_topic_message"):
        params["message_thread_id"] = message["message_thread_id"]
    telegram.call("sendMessage", **params)


def describe_settings(group: Group) -> str:
    return (
        "Current settings:\n"
        f"{BULLET} required-hearts: {group.required_hearts}\n"
        f"{BULLET} emoji: {group.emoji}\n"
        f"{BULLET} self-stars: {str(group.self_stars).lower()}"
    )


def is_admin(chat_id: int, user_id: int) -> bool:
    member = telegram.call("getChatMember", chat_id=chat_id, user_id=user_id)
    return member["status"] in ADMIN_STATUSES


def sent_by_admin(message: dict) -> bool:
    # Anonymous admins post as the group itself.
    if "sender_chat" in message:
        return message["sender_chat"]["id"] == message["chat"]["id"]
    return is_admin(message["chat"]["id"], message["from"]["id"])


def allowed_to_configure(message: dict) -> bool:
    if sent_by_admin(message):
        return True
    reply(message, "Only group admins can do that.")
    return False


def set_required_hearts(message: dict, args: list) -> None:
    hearts = int(args[0]) if len(args) == 1 and args[0].isdecimal() else 0
    if hearts < 1:
        reply(message, "Usage: /settings required-hearts <positive number>")
        return
    database.save_group_setting(message["chat"]["id"], "required_hearts", hearts)
    reply(message, f"Posts now need {hearts} hearts to reach the starboard.")


def set_emoji(message: dict, args: list) -> None:
    if len(args) != 1 or args[0] not in ALLOWED_EMOJI:
        reply(message, "Send one emoji Telegram allows as a reaction, e.g. /settings emoji 🔥")
        return
    database.save_group_setting(message["chat"]["id"], "emoji", args[0])
    reply(message, f"Reaction emoji set to {args[0]}.")


def set_self_stars(message: dict, args: list) -> None:
    choice = args[0].lower() if len(args) == 1 else ""
    if choice not in {"true", "false"}:
        reply(message, "Usage: /settings self-stars <true|false>")
        return
    database.save_group_setting(message["chat"]["id"], "self_stars", choice == "true")
    reply(message, f"Counting the author's own reaction: {choice}.")


SETTING_HANDLERS = {
    "required-hearts": set_required_hearts,
    "emoji": set_emoji,
    "self-stars": set_self_stars,
}


def settings_command(message: dict, args: list) -> None:
    if not allowed_to_configure(message):
        return
    if not args:
        reply(message, describe_settings(database.load_group(message["chat"]["id"])))
        return
    setting, *rest = args
    handler = SETTING_HANDLERS.get(setting)
    if handler is None:
        reply(message, "Unknown setting. Use required-hearts, emoji or self-stars.")
        return
    handler(message, rest)


def save_current_topic(message: dict, topic_setting: str) -> Group | None:
    if not allowed_to_configure(message):
        return None
    if not message.get("is_topic_message"):
        reply(message, "Run this inside a topic other than General.")
        return None
    chat_id = message["chat"]["id"]
    if not is_admin(chat_id, telegram.bot_account["id"]):
        reply(message, "Make me a group admin first: Telegram only sends reactions to admin bots.")
        return None
    database.save_group_setting(chat_id, topic_setting, message["message_thread_id"])
    return database.find_group(chat_id)


def setup_command(message: dict, args: list) -> None:
    group = save_current_topic(message, "starboard_topic_id")
    if group is None:
        return
    next_step = "" if group.music_topic_id else (
        "\nNext, run /watch in the topic whose posts can be starred."
    )
    reply(message, f"This topic is now the starboard.{next_step}")


def watch_command(message: dict, args: list) -> None:
    group = save_current_topic(message, "music_topic_id")
    if group is None:
        return
    next_step = "" if group.starboard_topic_id else (
        "\nNext, run /setup in the topic that should be the starboard."
    )
    reply(message, f"Posts with audio or a link in this topic can now be starred.{next_step}")


def explain_group_only(message: dict) -> None:
    reply(
        message,
        "My commands work in the group, for group admins: run /setup in the topic that should be "
        "the starboard and /watch in the topic whose posts can be starred.",
    )


GROUP_COMMANDS = {"settings": settings_command, "setup": setup_command, "watch": watch_command}
