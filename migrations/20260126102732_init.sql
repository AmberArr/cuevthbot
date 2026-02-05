-- just all in one lol
CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY,
    special_prize_seed TEXT,
    waifu_name TEXT,
    waifu_url TEXT,
    last_gacha_time DATETIME NOT NULL DEFAULT (datetime(0, 'unixepoch')),

    prize_json TEXT,

    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
