from telegram import Chat, Message, Update
from telegram.constants import ChatMemberStatus
from telegram.ext import Application, CommandHandler, ContextTypes, filters

import config
from config import CONFIG

Context = ContextTypes.DEFAULT_TYPE

BULLET = "•"
ADMIN_STATUSES = {ChatMemberStatus.ADMINISTRATOR, ChatMemberStatus.OWNER}
GROUP_COMMAND = filters.UpdateType.MESSAGE & filters.ChatType.GROUPS
PRIVATE_COMMAND = filters.UpdateType.MESSAGE & filters.ChatType.PRIVATE


def describe_settings() -> str:
    return (
        "Current settings:\n"
        f"{BULLET} required-hearts: {CONFIG.required_hearts}\n"
        f"{BULLET} emoji: {CONFIG.emoji}\n"
        f"{BULLET} self-stars: {str(CONFIG.self_stars).lower()}"
    )


async def is_admin(chat: Chat, user_id: int) -> bool:
    member = await chat.get_member(user_id)
    return member.status in ADMIN_STATUSES


async def sent_by_admin(message: Message) -> bool:
    # Anonymous admins post as the group itself.
    if message.sender_chat:
        return message.sender_chat.id == message.chat_id
    return await is_admin(message.chat, message.from_user.id)


async def allowed_to_configure(message: Message) -> bool:
    if CONFIG.group_id not in (None, message.chat_id):
        await message.reply_text(
            "I'm set up for another group. To move me, remove GROUP_ID from .env and restart me."
        )
        return False
    if not await sent_by_admin(message):
        await message.reply_text("Only group admins can do that.")
        return False
    return True


async def set_required_hearts(message: Message, rest: list) -> None:
    hearts = int(rest[0]) if len(rest) == 1 and rest[0].isdigit() else 0
    if hearts < 1:
        await message.reply_text("Usage: /settings required-hearts <positive number>")
        return
    config.save_config({"required_hearts": hearts})
    await message.reply_text(f"Posts now need {hearts} hearts to reach the starboard.")


async def set_emoji(message: Message, rest: list) -> None:
    if len(rest) != 1 or rest[0] not in config.ALLOWED_EMOJI:
        await message.reply_text(
            "Send one emoji Telegram allows as a reaction, e.g. /settings emoji 🔥"
        )
        return
    config.save_config({"emoji": rest[0]})
    await message.reply_text(f"Reaction emoji set to {rest[0]}.")


async def set_self_stars(message: Message, rest: list) -> None:
    choice = rest[0].lower() if len(rest) == 1 else ""
    if choice not in {"true", "false"}:
        await message.reply_text("Usage: /settings self-stars <true|false>")
        return
    config.save_config({"self_stars": choice == "true"})
    await message.reply_text(f"Counting the author's own reaction: {choice}.")


async def settings_command(update: Update, context: Context) -> None:
    message = update.effective_message
    if not await allowed_to_configure(message):
        return
    if not context.args:
        await message.reply_text(describe_settings())
        return
    setting, *rest = context.args
    match setting:
        case "required-hearts":
            await set_required_hearts(message, rest)
        case "emoji":
            await set_emoji(message, rest)
        case "self-stars":
            await set_self_stars(message, rest)
        case _:
            await message.reply_text("Unknown setting. Use required-hearts, emoji or self-stars.")


async def save_current_topic(message: Message, topic_setting: str) -> bool:
    if not await allowed_to_configure(message):
        return False
    if not message.is_topic_message:
        await message.reply_text("Run this inside a topic other than General.")
        return False
    if not await is_admin(message.chat, message.get_bot().id):
        await message.reply_text(
            "Make me a group admin first: Telegram only sends reactions to admin bots."
        )
        return False
    config.save_config({"group_id": message.chat_id, topic_setting: message.message_thread_id})
    return True


async def setup_command(update: Update, context: Context) -> None:
    message = update.effective_message
    if not await save_current_topic(message, "starboard_topic_id"):
        return
    next_step = "" if CONFIG.music_topic_id else (
        "\nNext, run /watch in the topic whose posts can be starred."
    )
    await message.reply_text(f"This topic is now the starboard.{next_step}")


async def watch_command(update: Update, context: Context) -> None:
    message = update.effective_message
    if not await save_current_topic(message, "music_topic_id"):
        return
    next_step = "" if CONFIG.starboard_topic_id else (
        "\nNext, run /setup in the topic that should be the starboard."
    )
    await message.reply_text(f"Posts with audio or a link in this topic can now be starred.{next_step}")


async def explain_group_only(update: Update, context: Context) -> None:
    await update.effective_message.reply_text(
        "My commands work in the group, for group admins: run /setup in the topic that should be "
        "the starboard and /watch in the topic whose posts can be starred."
    )


def register(app: Application) -> None:
    app.add_handler(CommandHandler("settings", settings_command, GROUP_COMMAND))
    app.add_handler(CommandHandler("setup", setup_command, GROUP_COMMAND))
    app.add_handler(CommandHandler("watch", watch_command, GROUP_COMMAND))
    app.add_handler(
        CommandHandler(["settings", "setup", "watch"], explain_group_only, PRIVATE_COMMAND)
    )
