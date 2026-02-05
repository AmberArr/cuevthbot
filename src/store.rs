use crate::config::*;
use anyhow::{Result, anyhow};
use chrono::prelude::*;
use grammers_client::Client;
use grammers_session::types::PeerRef;
use std::collections::HashMap;
use tokio::sync::{Mutex, OnceCell};

use crate::db::Database;
use crate::models::prize::{Prize, PrizePhoto, PrizeSource};
use crate::models::user::{User, SpecialPrize};

#[derive(Clone)]
pub struct Store {
    db: Database,
    /// key: post_id
    pub prizes: HashMap<i32, Prize>,
    /// key: user_id
    pub ten_pull_cache: HashMap<i64, Vec<Prize>>,
    pub client: Client,

    pub channel_max_post_id: i32,
    pub channel_max_post_id_cache_time: DateTime<Utc>,
    pub waifu_pic_channel: PeerRef,
}

impl Store {
    pub async fn new(client: Client, waifu_pic_channel: PeerRef) -> Result<Self> {
        Ok(Self {
            db: Database::new().await?,
            prizes: HashMap::new(),
            ten_pull_cache: HashMap::new(),
            client,
            channel_max_post_id: 0,
            channel_max_post_id_cache_time: DateTime::UNIX_EPOCH,
            waifu_pic_channel: waifu_pic_channel,
        })
    }

    // it calls get_prize_from_channel_post to resolve prizes, so it is &mut self
    pub async fn get_user(&mut self, user_id: i64) -> Result<Option<User>> {
        let dto = match self.db.get_user_by_id(user_id).await? {
            Some(dto) => dto,
            None => return Ok(None),
        };
        if let Some(prize_json) = dto.prize_json {
            let source: PrizeSource = serde_json::from_str(&prize_json)?;
            let prize = match source {
                PrizeSource::Telegram { post_id } => {
                    self.get_prize_from_channel_post(post_id).await?
                }
                PrizeSource::File { file_name: _ } => None,
                PrizeSource::Url { photo_url } => {
                    try {
                        Prize {
                            name: dto.waifu_name.clone()?,
                            url: dto.waifu_url?,
                            photo: PrizePhoto::Url(photo_url),
                        }
                    }
                }
            };
            Ok(Some(User {
                id: dto.user_id,
                last_gacha: prize,
                last_gacha_time: dto.last_gacha_time.and_utc(),
                special: dto.special_prize_seed.map(|seed| SpecialPrize {
                    search_tag: seed.clone(),
                    display_name: dto.waifu_name.unwrap_or(seed),
                }),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_user_info_or_create(&mut self, user_id: i64) -> Result<User> {
        let created = self.db.new_user(user_id).await?;
        if created {
            Ok(User {
                id: user_id,
                last_gacha: None,
                last_gacha_time: DateTime::UNIX_EPOCH,
                special: None,
            })
        } else if let Some(user) = self.get_user(user_id).await? {
            Ok(user)
        } else {
            Err(anyhow::anyhow!("User exists but fetch failed"))
        }
    }

    pub async fn update_user_gacha(&self, user_id: i64, prize: Prize) -> Result<bool> {
        self.db.update_gacha(user_id, prize).await
    }

    pub fn update_channel_max_post_id(&mut self, max_post_id: i32) {
        self.channel_max_post_id = max_post_id;
        self.channel_max_post_id_cache_time = Utc::now();
    }

    // it will update prize cache, so it is &mut self
    #[tracing::instrument(skip(self, post_id))]
    pub async fn get_prize_from_channel_post(&mut self, post_id: i32) -> Result<Option<Prize>> {
        if let Some(cached) = self.prizes.get(&post_id) {
            return Ok(Some(cached.clone()));
        }
        let channel = self.waifu_pic_channel;

        if let Some(msg) = self
            .client
            .get_messages_by_id(channel, &[post_id])
            .await?
            .pop()
            .flatten()
        {
            if let Some(photo) = msg.photo() {
                let tmp: String;
                let message: &str = if msg.text().is_empty() {
                    tmp = crate::utils::parse_tg_embed_get_text(post_id).await?;
                    &tmp
                } else {
                    msg.text()
                };
                if let Some(result) = CHARACTER_REGEXES
                    .iter()
                    .find_map(|reg| reg.captures(message))
                {
                    let url = format!("https://t.me/{CHANNEL_USERNAME}/{post_id}");
                    let prize = Prize{
                        name: result[1].to_owned(),
                        url,
                        photo: PrizePhoto::TelegramPhoto(photo)
                    };
                    self.prizes.insert(post_id, prize.clone());
                    return Ok(Some(prize));
                }
            }
        }
        Ok(None)
    }
}

pub struct StoreWrapper {
    inner: OnceCell<Mutex<Store>>,
}

impl StoreWrapper {
    pub const fn const_new() -> Self {
        Self {
            inner: OnceCell::const_new(),
        }
    }

    pub async fn init(&self, client: Client) -> Result<()> {
        let waifu_pic_channel = init_waifu_channel_info(&client).await?;
        let inner = Store::new(client, waifu_pic_channel).await?;
        self.inner
            .set(Mutex::new(inner))
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(())
    }

    pub async fn get<'a>(&'a self) -> Result<tokio::sync::MutexGuard<'a, Store>> {
        let mutex = self
            .inner
            .get()
            .ok_or(anyhow!("Store is not initialized"))?;
        let store = mutex.lock().await;
        Ok(store)
    }
}

pub async fn init_waifu_channel_info(client: &Client) -> Result<PeerRef> {
    tracing::info!("loading Waifu channel info");

    let peerinfo = client.resolve_username("WaifuP1c").await?.ok_or(anyhow!(
        "Could not find Waifu channel in GetChannels response"
    ))?;
    tracing::info!("loading Waifu channel info ok");
    Ok(peerinfo.to_ref().await.unwrap())
}

pub static STORE: StoreWrapper = StoreWrapper::const_new();
