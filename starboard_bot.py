import logging
from datetime import datetime, timezone

from telegram import MessageEntity, Update
from telegram.ext import (
    Application,
    ContextTypes,
    MessageHandler,
    MessageReactionHandler,
    filters,
)

import commands
import database
from config import BOT_TOKEN, MAX_POST_AGE
from database import Group, Post

logger = logging.getLogger(__name__)
Context = ContextTypes.DEFAULT_TYPE

MUSIC_POST_FILTER = (filters.UpdateType.MESSAGE & filters.ChatType.SUPERGROUP) & (
    filters.AUDIO
    | filters.Document.AUDIO
    | filters.Entity(MessageEntity.URL)
    | filters.Entity(MessageEntity.TEXT_LINK)
    | filters.CaptionEntity(MessageEntity.URL)
    | filters.CaptionEntity(MessageEntity.TEXT_LINK)
)


async def remember_music_post(update: Update, context: Context) -> None:
    # Reaction updates lack the topic, so remember posts as they're made.
    post = update.message
    group = database.find_group(post.chat_id)
    if group and group.is_set_up and post.message_thread_id == group.music_topic_id:
        database.remember_post(post.chat_id, post.message_id, post.from_user.id, post.date)


async def track_hearts(update: Update, context: Context) -> None:
    group = database.find_group(update.effective_chat.id)
    if group is None or not group.is_set_up:
        return
    database.forget_posts_before(datetime.now(timezone.utc) - MAX_POST_AGE)
    message_id = (update.message_reaction or update.message_reaction_count).message_id
    post = database.find_post(group.chat_id, message_id)
    if post is None:
        return
    post.hearts = count_hearts(update, post, group)
    database.save_hearts(post)
    if post.hearts < group.required_hearts:
        return
    await promote_to_starboard(context, group, message_id)


def count_hearts(update: Update, post: Post, group: Group) -> int:
    heart = group.heart_reaction
    if update.message_reaction_count:
        reactions = update.message_reaction_count.reactions
        return sum(item.total_count for item in reactions if item.type == heart)
    change = update.message_reaction
    author_reacting = change.user and change.user.id == post.author_id
    if author_reacting and not group.self_stars:
        return post.hearts
    had_heart = heart in change.old_reaction
    has_heart = heart in change.new_reaction
    return post.hearts + has_heart - had_heart


async def promote_to_starboard(context: Context, group: Group, message_id: int) -> None:
    starboard_post = await context.bot.forward_message(
        chat_id=group.chat_id,
        from_chat_id=group.chat_id,
        message_id=message_id,
        message_thread_id=group.starboard_topic_id,
    )
    database.forget_post(group.chat_id, message_id)
    logger.info("Forwarded post %s in group %s to the starboard", message_id, group.chat_id)
    # The bot adds the first heart.
    await starboard_post.set_reaction(group.heart_reaction)


async def log_error(update: object, context: Context) -> None:
    logger.error("Failed to handle %s", update, exc_info=context.error)


def main() -> None:
    app = Application.builder().token(BOT_TOKEN).build()
    commands.register(app)
    app.add_handler(MessageHandler(MUSIC_POST_FILTER, remember_music_post))
    app.add_handler(MessageReactionHandler(track_hearts))
    app.add_error_handler(log_error)
    # Keep retrying at startup until Telegram is reachable (e.g. right after a reboot).
    app.run_polling(allowed_updates=Update.ALL_TYPES, bootstrap_retries=-1)


if __name__ == "__main__":
    main()
