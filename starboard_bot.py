import logging
from dataclasses import dataclass
from datetime import datetime, timezone

from telegram import MessageEntity, Update
from telegram.ext import (
    Application,
    ContextTypes,
    MessageHandler,
    MessageReactionHandler,
    PicklePersistence,
    filters,
)

import commands
from config import CONFIG, DATA_FILE, MAX_POST_AGE

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


@dataclass
class MusicPost:
    posted_at: datetime
    hearts: int
    author_id: int


async def remember_music_post(update: Update, context: Context) -> None:
    # Reaction updates lack the topic, so remember posts by their message id.
    post = update.message
    music_topic = (CONFIG.group_id, CONFIG.music_topic_id)
    if CONFIG.is_set_up and (post.chat_id, post.message_thread_id) == music_topic:
        context.bot_data[post.message_id] = MusicPost(post.date, 0, post.from_user.id)


async def track_hearts(update: Update, context: Context) -> None:
    if not CONFIG.is_set_up or update.effective_chat.id != CONFIG.group_id:
        return
    forget_old_posts(context.bot_data)
    post_id = (update.message_reaction or update.message_reaction_count).message_id
    post = context.bot_data.get(post_id)
    if not isinstance(post, MusicPost):
        return
    post.hearts = count_hearts(update, post)
    if post.hearts < CONFIG.required_hearts:
        return
    await promote_to_starboard(context, post_id)


def forget_old_posts(bot_data: dict) -> None:
    cutoff = datetime.now(timezone.utc) - MAX_POST_AGE
    for message_id, post in list(bot_data.items()):
        if isinstance(post, MusicPost) and post.posted_at < cutoff:
            del bot_data[message_id]


def count_hearts(update: Update, post: MusicPost) -> int:
    heart = CONFIG.heart_reaction
    if update.message_reaction_count:
        reactions = update.message_reaction_count.reactions
        return sum(item.total_count for item in reactions if item.type == heart)
    change = update.message_reaction
    author_reacting = change.user and change.user.id == post.author_id
    if author_reacting and not CONFIG.self_stars:
        return post.hearts
    had_heart = heart in change.old_reaction
    has_heart = heart in change.new_reaction
    return post.hearts + has_heart - had_heart


async def promote_to_starboard(context: Context, post_id: int) -> None:
    starboard_post = await context.bot.forward_message(
        chat_id=CONFIG.group_id,
        from_chat_id=CONFIG.group_id,
        message_id=post_id,
        message_thread_id=CONFIG.starboard_topic_id,
    )
    del context.bot_data[post_id]
    logger.info("Forwarded post %s to the starboard", post_id)
    # The bot adds the first heart.
    await starboard_post.set_reaction(CONFIG.heart_reaction)


async def log_error(update: object, context: Context) -> None:
    logger.error("Failed to handle %s", update, exc_info=context.error)


def main() -> None:
    # Save every 5 s: the background task is stopped by killing the process.
    persistence = PicklePersistence(DATA_FILE, update_interval=5)
    app = Application.builder().token(CONFIG.bot_token).persistence(persistence).build()
    commands.register(app)
    app.add_handler(MessageHandler(MUSIC_POST_FILTER, remember_music_post))
    app.add_handler(MessageReactionHandler(track_hearts))
    app.add_error_handler(log_error)
    if not CONFIG.is_set_up:
        logger.warning("Not set up yet: run /setup in the starboard topic and /watch in the music topic")
    # Keep retrying at startup until Telegram is reachable (e.g. right after a reboot).
    app.run_polling(allowed_updates=Update.ALL_TYPES, bootstrap_retries=-1)


if __name__ == "__main__":
    main()
