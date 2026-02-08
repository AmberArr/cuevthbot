use crate::models::prize::Prize;
use crate::utils::is_same_date_in_hkt;
use chrono::{DateTime, NaiveDateTime, Utc};

#[derive(Clone)]
pub struct SpecialPrize {
    pub search_tag: String,
    pub display_name: String,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct User {
    pub id: i64,
    pub last_gacha: Option<Prize>,
    pub last_gacha_time: DateTime<Utc>,
    pub special: Option<SpecialPrize>,
}

impl User {
    pub fn has_pulled_today(&self) -> bool {
        let now = Utc::now();
        is_same_date_in_hkt(self.last_gacha_time, now)
    }
}

pub struct UserDTO {
    pub user_id: i64,
    pub special_prize_seed: Option<String>,
    pub waifu_name: Option<String>,
    pub waifu_url: Option<String>,
    pub last_gacha_time: NaiveDateTime,
    pub prize_json: Option<String>, // PrizeSource
}
