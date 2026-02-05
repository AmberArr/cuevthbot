use crate::config::CHANNEL_USERNAME;
use crate::config::HTTP_CLIENT;
use anyhow::Result;
use chrono::prelude::*;
use lol_html::{HtmlRewriter, Settings, element, text};
use std::cell::RefCell;
use std::rc::Rc;

pub fn is_same_date_in_hkt(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let aa = a.with_timezone(&tz);
    let bb = b.with_timezone(&tz);
    aa.year() == bb.year() && aa.month() == bb.month() && aa.day() == bb.day()
}

pub async fn parse_tg_embed_get_text(post_id: i32) -> Result<String> {
    let html = HTTP_CLIENT
        .get(format!("https://t.me/{CHANNEL_USERNAME}/{post_id}?embed=1"))
        .send()
        .await?
        .text()
        .await?;
    let output = Rc::new(RefCell::new(String::new()));

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                text!(".tgme_widget_message_text", |t| {
                    output.borrow_mut().push_str(t.as_str());
                    Ok(())
                }),
                element!(".tgme_widget_message_text br", |_| {
                    output.borrow_mut().push_str("\n");
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |_: &[u8]| {},
    );

    rewriter.write(html.as_bytes())?;
    rewriter.end()?;

    let final_text = output.borrow().trim().to_string();

    Ok(final_text)
}
