use crate::config::HTTP_CLIENT;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::env;

use crate::models::prize::{Prize, PrizePhoto};

#[tracing::instrument]
pub async fn danbooru(tag: &str, display_name: &str, n: usize) -> Result<Vec<Prize>> {
    if tag.is_empty() {
        return Err(anyhow!("tag is empty"));
    }
    tracing::info!("Searching Danbooru for '{}' (limit: {})", tag, n);
    let host = "https://danbooru.donmai.us/posts.json";
    let tags = format!("-nude -ai-assisted -rating:e solo {tag}");
    let params = [
        ("tags", tags.as_ref()),
        ("random", "1"),
        ("limit", &n.to_string()),
    ];
    let danbooru_user = env::var("DANBOORU_USER").expect("DANBOORU_USER invalid");
    let danbooru_key = env::var("DANBOORU_KEY").expect("DANBOORU_KEY invalid");

    let response: Value = HTTP_CLIENT
        .get(host)
        .query(&params)
        .basic_auth(danbooru_user, Some(danbooru_key))
        .send()
        .await?
        .json()
        .await?;

    // Log response if debugging needed, but maybe too verbose for info
    // tracing::debug!("Danbooru response: {:?}", response);

    let tasks = response
        .as_array()
        .ok_or(anyhow!("Missing root array"))?
        .iter()
        .filter_map(|post| {
            let danbooru_post_id = post["id"].as_u64()?;
            let danbooru_post_url = format!("https://danbooru.donmai.us/posts/{danbooru_post_id}");

            let variants = post["media_asset"]["variants"].as_array()?;
            let photo_url = variants
                .iter()
                .find(|item| item["type"] == "720x720")
                .or_else(|| variants.get(0))
                .and_then(|item| item["url"].as_str())?;
            Some(Ok(Prize {
                name: display_name.to_owned(),
                url: danbooru_post_url,
                photo: PrizePhoto::Url(photo_url.to_owned()),
            }))
        })
        .collect();

    tasks
}
