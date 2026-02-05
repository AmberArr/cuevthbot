use bytes::Bytes;
use grammers_client::media::Photo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Prize {
    pub name: String,
    pub url: String,
    pub photo: PrizePhoto,
}

#[derive(Clone, Debug)]
pub enum PrizePhoto {
    TelegramPhoto(Photo),
    File { name: String, content: Bytes },
    Url(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum PrizeSource {
    Telegram { post_id: i32 },
    File { file_name: String },
    Url { photo_url: String },
}
