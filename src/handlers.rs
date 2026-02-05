use crate::config::LOADING_TEXT_FUMO;
use crate::models::prize::PrizePhoto;
use crate::services::gacha::{single_pull, ten_pulls};
use crate::store::STORE;
use anyhow::{Result, anyhow};
use grammers_client::Client;
use grammers_client::message::{Button, InputMessage, ReplyMarkup};
use grammers_client::update::{Article, Update};
use rand::prelude::*;
use rand::rng;

#[tracing::instrument(skip(client, update))]
pub async fn handle_update(client: Client, update: Update) -> Result<()> {
    tracing::debug!(?update);
    match update {
        Update::NewMessage(message) if message.text() == "/start" => {
            message
                .respond(InputMessage::new().text("I'm alive!"))
                .await?;
        }
        Update::InlineQuery(query) => {
            handle_inline_query(query).await?;
        }
        Update::InlineSend(query) => {
            handle_inline_send(client, query).await?;
        }
        Update::CallbackQuery(query) => {
            handle_callback_query(client, query).await?;
        }
        _ => {}
    }
    Ok(())
}

#[tracing::instrument(skip(query))]
async fn handle_inline_query(query: grammers_client::update::InlineQuery) -> Result<()> {
    if let Some(sender) = query.sender() {
        let sender_id = sender.id().bare_id();

        let mut store = STORE.get().await?;
        let _ = store.get_user_info_or_create(sender_id);

        let thumb_url = "https://img.icons8.com/ios/150/FFFFFF/gift--v1.png";

        let fumo_says = LOADING_TEXT_FUMO
            .choose(&mut rng())
            .cloned()
            .unwrap_or("Loading".into());
        let button = [Button::data("🔮 在路上", b"loading")];
        let msg = InputMessage::new()
            .text(&fumo_says)
            .reply_markup(ReplyMarkup::from_buttons_row(&button));
        let answer = Article::new("每日老婆", msg)
            .id("single_pull")
            .thumb_url(thumb_url);

        let msg1 = InputMessage::new()
            .text(fumo_says)
            .reply_markup(ReplyMarkup::from_buttons_row(&button));
        let answer1 = Article::new("十连 (WIP)", msg1)
            .id("ten_pulls")
            .description("可能会抽到奇怪的东西（？）")
            .thumb_url(thumb_url);

        query.answer([answer, answer1]).send().await?;
    }
    Ok(())
}

#[tracing::instrument(skip(client, query))]
async fn handle_inline_send(
    client: Client,
    query: grammers_client::update::InlineSend,
) -> Result<()> {
    let sender = query
        .sender()
        .ok_or(anyhow!("handle_inline_send: no sender"))?;
    let sender_name = sender.full_name();
    let sender_id = sender.id().bare_id();
    tracing::info!(user_id = sender_id, "Processing inline send (pull)");

    let (user, maybe_prize) = {
        let mut store = STORE.get().await?;
        let user = store.get_user_info_or_create(sender_id).await?;
        let maybe_prize = if user.has_pulled_today() {
            user.last_gacha.clone()
        } else {
            None
        };
        (user, maybe_prize)
    };

    let input_message: InputMessage;
    let photo: PrizePhoto;

    match query.result_id() {
        "single_pull" => {
            let prize = if let Some(prize0) = maybe_prize {
                prize0
            } else {
                single_pull(&user).await?
            };
            STORE
                .get()
                .await?
                .update_user_gacha(sender_id, prize.clone())
                .await?;

            // From grammers-client/src/parsers/markdown.rs:
            // Parse a message containing CommonMark-flavored markdown into plain text and the list of formatting entities understood by Telegram.
            // This is not the same as the markdown understood by Telegram's HTTP Bot API.
            //
            // so use \\\n to insert a line break
            let message_text = format!(
                "亲爱的[{}](tg://user?id={})\\\n今天的老婆是 [{}]({})",
                sender_name, sender_id, prize.name, prize.url,
            );
            input_message = InputMessage::new().markdown(message_text);
            photo = prize.photo;
        }
        "ten_pulls" => {
            (input_message, photo) = ten_pulls(&user).await?;
        }
        _ => return Err(anyhow!("unexpected msg_id")),
    };

    match photo {
        PrizePhoto::File {
            name: filename,
            content,
        } => {
            let len = content.len();
            let mut cursor = std::io::Cursor::new(content);
            let uploaded = client.upload_stream(&mut cursor, len, filename).await?;
            query.edit_message(input_message.photo(uploaded)).await?;
        }
        PrizePhoto::Url(photo_url) => {
            query
                .edit_message(input_message.photo_url(photo_url))
                .await?;
        }
        PrizePhoto::TelegramPhoto(photo) => {
            let photo = photo.clone().into();
            query.edit_message(input_message.copy_media(&photo)).await?;
        }
    }

    Ok(())
}

#[tracing::instrument(skip(client, query))]
async fn handle_callback_query(
    client: Client,
    query: grammers_client::update::CallbackQuery,
) -> Result<()> {
    let data = query.data();
    // Not a ten pull button
    if !data.starts_with(b"option") || data.len() < 15 {
        query.answer().send().await?;
        return Ok(());
    }
    let user_id = i64::from_be_bytes(data[6..14].try_into().unwrap());
    let sender = query
        .sender()
        .ok_or(anyhow!("handle_inline_send: no sender"))?;
    // A user clicks another's button
    if sender.id().bare_id() != user_id {
        query.answer().send().await?;
        return Ok(());
    }

    let sender_name = match sender {
        grammers_client::peer::Peer::User(user) => user.full_name(),
        _ => sender.name().unwrap_or("").to_owned(),
    };
    let sender_id = sender.id().bare_id();
    tracing::info!(
        user_id = sender_id,
        "Processing callback query (ten pull button)"
    );

    if let Some(mut prizes) = STORE.get().await?.ten_pull_cache.remove(&user_id) {
        let prize = prizes.swap_remove(data[14] as usize - 1);
        let message_text = format!(
            "亲爱的[{}](tg://user?id={})\\\n今天的老婆是 [{}]({})",
            sender_name, sender_id, prize.name, prize.url,
        );
        let input_message = InputMessage::new().markdown(message_text);
        match prize.photo {
            PrizePhoto::File {
                name: filename,
                content,
            } => {
                let len = content.len();
                let mut cursor = std::io::Cursor::new(content);
                let uploaded = client.upload_stream(&mut cursor, len, filename).await?;
                query.answer().edit(input_message.photo(uploaded)).await?;
            }
            PrizePhoto::Url(photo_url) => {
                query
                    .answer()
                    .edit(input_message.photo_url(photo_url))
                    .await?;
            }
            PrizePhoto::TelegramPhoto(photo) => {
                let photo = photo.clone().into();
                query
                    .answer()
                    .edit(input_message.copy_media(&photo))
                    .await?;
            }
        }
    }

    // WIP
    return Ok(());
}