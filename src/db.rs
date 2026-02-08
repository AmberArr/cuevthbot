use crate::models::prize::{Prize, PrizePhoto, PrizeSource};
use crate::models::user::UserDTO;
use anyhow::{Context, Result};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use tracing::instrument;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    #[instrument]
    pub async fn new() -> Result<Self> {
        let db_url = std::env::var("DATABASE_URL").context("Invalid DATABASE_URL")?;

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .context("Failed to connect to database")?;

        let db = Self { pool };
        Ok(db)
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserDTO>> {
        let user_dto: Option<UserDTO> = sqlx::query_as!(
            UserDTO,
            r#"
SELECT
    user_id,
    special_prize_seed,
    waifu_name,
    waifu_url,
    last_gacha_time,
    prize_json
FROM
    users
where
    user_id = ?
    "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user_dto)
    }

    pub async fn new_user(&self, user_id: i64) -> Result<bool> {
        sqlx::query!(
            r#"INSERT OR IGNORE INTO users (user_id) VALUES (?)"#,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected() != 0)
        .map_err(|e| e.into())
    }

    pub async fn update_gacha(&self, user_id: i64, prize: Prize) -> Result<bool> {
        let mut tx = self.pool.begin().await?;

        let prize_source = match prize.photo {
            PrizePhoto::TelegramPhoto(_) => {
                // parse the last segment of the URL as the Post ID
                let maybe_post_id = prize
                    .url
                    .split('/')
                    .last()
                    .and_then(|s| s.parse::<i32>().ok());

                if let Some(post_id) = maybe_post_id {
                    PrizeSource::Telegram { post_id }
                } else {
                    anyhow::bail!("Could not extract Post ID from URL: {}", prize.url);
                }
            }

            PrizePhoto::Url(photo_url) => PrizeSource::Url { photo_url },

            PrizePhoto::File { .. } => {
                anyhow::bail!("File prizes are not supported for persistence yet");
            }
        };

        let now = chrono::Utc::now();
        let prize_json = serde_json::to_string(&prize_source)?;

        let rows = sqlx::query!(
            r#"
UPDATE users
SET
    waifu_url = ?,
    last_gacha_time = ?,
    prize_json = ?
WHERE user_id = ?
            "#,
            prize.url,
            now,
            prize_json,
            user_id
        )
        .execute(&mut *tx)
        .await
        .context("Failed to update user cache")?;

        tx.commit().await.context("Failed to commit transaction")?;

        Ok(rows.rows_affected() > 0)
    }
}
