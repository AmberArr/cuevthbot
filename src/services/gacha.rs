use crate::config::HTTP_CLIENT;
use crate::models::prize::{Prize, PrizePhoto};
use crate::models::user::User;
use crate::services::danbooru::danbooru;
use crate::store::STORE;
use crate::utils::is_same_date_in_hkt;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use chrono::prelude::*;
use futures::future::try_join_all;
use grammers_client::message::{Button, InputMessage, ReplyMarkup};
use image::{DynamicImage, ImageReader, RgbaImage, imageops::FilterType};
use lol_html::{HtmlRewriter, Settings, element};
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;

enum PrizeType {
    /// Prize from channel @WaifuP1c, Some(post_id), None -> random
    ChannelPrize(Option<i32>),
    /// Prize from Danbooru, with search tag and display name
    DanbooruPrize { tag: String, name: String },
    /// Telegram user as prize, with uid
    #[allow(dead_code)]
    UserPrize(i64),
    /// My custom prize
    #[allow(dead_code)]
    OtherPrize,
}

#[tracing::instrument(skip(user))]
pub async fn pull(user: &User, n: usize) -> Result<Vec<Prize>> {
    let prize_type = if let Some(special) = &user.special {
        PrizeType::DanbooruPrize {
            tag: special.search_tag.clone(),
            name: special.display_name.clone(),
        }
    } else {
        PrizeType::ChannelPrize(None)
    };

    match prize_type {
        PrizeType::ChannelPrize(Some(post_id)) => {
            let mut store = STORE.get().await?;
            let mut result: Vec<Prize> = vec![];
            result.reserve_exact(n);
            for _ in 0..n {
                let prize = store
                    .get_prize_from_channel_post(post_id)
                    .await?
                    .ok_or(anyhow!("Bad post_id"))?;
                result.push(prize);
            }
            Ok(result)
        }
        PrizeType::ChannelPrize(None) => {
            let max_post_id = get_channel_max_post_id().await?;
            try_join_all((0..n).map(|_| async { pull_channel_prize(max_post_id).await })).await
        }
        PrizeType::DanbooruPrize { tag, name } => {
            let photos = danbooru(&tag, &name, n).await?;
            Ok(photos.into_iter().collect())
        }
        PrizeType::UserPrize(_) => {
            todo!()
        }
        PrizeType::OtherPrize => {
            // Not implemented yet
            todo!()
        }
    }
}

#[tracing::instrument(skip(user))]
pub async fn single_pull(user: &User) -> Result<Prize> {
    let mut prize = pull(user, 1).await?;
    Ok(prize.pop().unwrap())
}

#[tracing::instrument(skip(user))]
pub async fn ten_pulls(user: &User) -> Result<(InputMessage, PrizePhoto)> {
    tracing::debug!(user = user.id, "Running 10 pulls start");
    let result = pull(user, 10).await?;
    tracing::debug!(user = user.id, "Running 10 pulls end");
    let client = {
        let mut store = STORE.get().await?;
        let _ = store.ten_pull_cache.insert(user.id, result.clone());
        store.client.clone()
    };
    let mut names = Vec::with_capacity(result.len());
    let mut urls = Vec::with_capacity(result.len());
    tracing::debug!(user = user.id, "Download start");
    let imgs = try_join_all(result.into_iter().map(|prize| {
        // ugly but convenient
        names.push(prize.name);
        urls.push(prize.url);
        async {
            let bytes = match prize.photo {
                PrizePhoto::TelegramPhoto(photo) => {
                    let mut download = client.iter_download(&photo);
                    let mut bytes = vec![];
                    while let Some(chunk) = download.next().await? {
                        bytes.extend(chunk);
                    }
                    Ok(bytes.into())
                }
                PrizePhoto::Url(url) => {
                    let bytes = HTTP_CLIENT.get(url).send().await?.bytes().await?;
                    Ok(bytes)
                }
                _ => Err(anyhow!("bad")),
            }?;
            let img = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()?
                .decode()?;
            let ok: Result<_> = Ok(img);
            ok
        }
    }))
    .await?;
    tracing::debug!(user = user.id, "Download end");

    tracing::debug!(user = user.id, "Composite end");
    let photo = PrizePhoto::File {
        name: "composite.jpg".into(),
        content: create_composite(imgs)?,
    };
    tracing::debug!(user = user.id, "Composite end");

    // We can just put prize info in the data though.
    // Currently we just don't.
    let mut button_data = [0u8; 15];
    button_data[0..6].copy_from_slice(b"option");
    button_data[6..14].copy_from_slice(&user.id.to_be_bytes());
    let buttons = [1..=5, 6..=10].map(|range| {
        range
            .map(|i| {
                button_data[14] = i;
                Button::data(i.to_string(), button_data.to_vec())
            })
            .collect::<Vec<_>>()
    });
    use grammers_tl_types::enums::MessageEntity;
    use grammers_tl_types::types::MessageEntityTextUrl;
    let mut buffer = String::with_capacity(512);
    let mut offset = 0i32;
    let entities = names
        .into_iter()
        .zip(urls)
        .enumerate()
        .map(|(i, (name, url))| {
            let prefix = format!("{}. ", i + 1);
            offset += prefix.encode_utf16().count() as i32;
            buffer.push_str(&prefix);
            let name_len = name.encode_utf16().count() as i32;
            let entity = MessageEntity::TextUrl(MessageEntityTextUrl {
                offset,
                length: name_len,
                url,
            });
            buffer.push_str(&name);
            buffer.push('\n');
            offset += name_len + 1;
            entity
        })
        .collect::<Vec<_>>();
    tracing::debug!(text = ?buffer, entities = ?entities);
    let input_message = InputMessage::new()
        .text(buffer)
        .fmt_entities(entities)
        .reply_markup(ReplyMarkup::from_buttons(&buttons));
    Ok((input_message, photo))
}

pub fn create_composite(images: Vec<DynamicImage>) -> Result<Bytes> {
    let ratios = images
        .iter()
        .map(|img| img.width() as f64 / img.height() as f64)
        .collect::<Vec<_>>();
    let layout_config = crate::layout::LayoutConfig {
        container_width: 1300.,
        target_row_height: vec![440., 500.],
        target_row_height_tolerance: 0.1,
        edge_case_min_row_height_factor: 0.8,
        edge_case_max_row_height_factor: 1.5,
        ..Default::default()
    };

    let layout_result = crate::layout::compute(&ratios, &layout_config).unwrap();
    let placements = layout_result.boxes;

    // 1. Determine the required canvas size
    let max_w = layout_config.container_width;
    let max_h = layout_result.container_height;

    // 2. Initialize a transparent canvas
    let mut canvas = RgbaImage::new(max_w.ceil() as u32, max_h.ceil() as u32);

    // 3. Resize and overlay
    for (img, p) in images.into_iter().zip(placements.into_iter()) {
        let target_w = p.width.round() as u32;
        let target_h = p.height.round() as u32;

        // Resize the image to fit the placement
        let resized = img.resize_exact(target_w, target_h, FilterType::Lanczos3);

        // Overlay onto the canvas
        // Note: replace() or overlay() are common. overlay() handles transparency.
        image::imageops::overlay(
            &mut canvas,
            &resized,
            p.left.round() as i64,
            p.top.round() as i64,
        );
    }

    // Output as jpeg because telegram basically always converts our image to jpeg anyway.
    // We also tried webp crate to create lossy webp, which only save us ~100kB. So whatever.
    let mut buffer = vec![];
    let final_img = DynamicImage::ImageRgba8(canvas);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 60);
    final_img.write_with_encoder(encoder)?;
    Ok(buffer.into())
}

#[tracing::instrument(skip(max_post_id))]
async fn pull_channel_prize(max_post_id: i32) -> Result<Prize> {
    // retry 100 times. it probably successes in a few tries, so we just lock it here.
    let mut store = STORE.get().await?;
    for _i in 0..100 {
        let post_id = rand::random_range(1..=max_post_id);
        match store.get_prize_from_channel_post(post_id).await {
            Ok(Some(x)) => return Ok(x.into()),
            Ok(None) => { /* just retry */ }
            Err(e) => {
                tracing::warn!("Failed to fetch post {}: {}", post_id, e);
                // don't waste time if we just cannot read posts.
                if e.is::<grammers_client::InvocationError>() {
                    return Err(e);
                }
            }
        }
    }
    Err(anyhow!("Could be unlucky like this??"))
}

#[tracing::instrument]
async fn get_channel_max_post_id() -> Result<i32> {
    let (max_post_id, cache_time) = {
        let store = STORE.get().await?;
        (
            store.channel_max_post_id,
            store.channel_max_post_id_cache_time,
        )
    };
    let now = Utc::now();
    if is_same_date_in_hkt(cache_time, now) {
        return Ok(max_post_id);
    }

    // refresh
    tracing::info!("Refeshing max post id...");
    let new_max_post_id = {
        let url = "https://t.me/s/WaifuP1c";
        let html = HTTP_CLIENT.get(url).send().await?.text().await?;
        let output = Rc::new(RefCell::new(String::new()));

        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![element!(".tgme_widget_message", |el| {
                    if let Some(attr) = el.get_attribute("data-post") {
                        output.replace(attr);
                    }

                    Ok(())
                })],
                ..Settings::default()
            },
            |_: &[u8]| {},
        );

        rewriter.write(html.as_bytes())?;
        rewriter.end()?;

        let final_text = output.borrow().trim().to_string();
        final_text[9..].parse::<i32>().ok()
    };

    let mut store = STORE.get().await?;
    if let Some(new_max_post_id) = new_max_post_id {
        tracing::info!("New max post id: {}", new_max_post_id);
        store.update_channel_max_post_id(new_max_post_id);
        return Ok(new_max_post_id);
    }
    Err(anyhow!("Cannot extract last post id"))
}
