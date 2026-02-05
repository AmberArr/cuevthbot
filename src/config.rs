use anyhow::Result;
use regex::Regex;
use std::fs::File;
use std::sync::LazyLock;

pub const CHANNEL_USERNAME: &str = "WaifuP1c";

pub static CHARACTER_REGEXES: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [
        Regex::new(r"(?:角色|char):\s*#?(\S+)").unwrap(),
        Regex::new(r"^#\d+\s+#(\S+)").unwrap(),
    ]
});

pub const SESSION_FILE: &str = "cuevthbot.session";

pub const LOADING_TEXT_FUMO: LazyLock<Vec<String>> = LazyLock::new(|| {
    let result: Result<_> = (|| {
        let file = File::open("fumosays.json")?;
        let mut sayings: Vec<String> = serde_json::from_reader(file)?;
        sayings
            .iter_mut()
            .for_each(|item| item.push_str(" ——浮世之沫"));
        Ok(sayings)
    })();
    result.expect("Failed to load fumosays")
});

pub const HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("cuevbot/0.0.1")
        .build()
        .unwrap()
});
