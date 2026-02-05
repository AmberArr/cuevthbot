#![feature(try_blocks)]
mod config;
mod db;
mod handlers;
mod layout;
mod models;
mod services;
mod store;
mod utils;

use crate::config::SESSION_FILE;
use crate::handlers::handle_update;
use crate::store::STORE;

use anyhow::{Context, Result};
use grammers_client::Client;
use grammers_mtsender::SenderPool;
use grammers_session::storages::SqliteSession;
use std::env;
use std::sync::Arc;
use tokio::task;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // 1. Precise Environment Handling
    // We use .context() so you know EXACTLY which variable is missing.
    let api_id = env::var("API_ID")
        .context("Missing API_ID in environment")?
        .parse()
        .context("API_ID must be a valid integer")?;
    let api_hash = env::var("API_HASH").context("Missing API_HASH")?;
    let token = env::var("BOT_TOKEN").context("Missing BOT_TOKEN")?;

    // 2. Persistent Session
    tracing::info!("Connecting to database...");
    let session = Arc::new(
        SqliteSession::open(SESSION_FILE)
            .await
            .context("Failed to initialize SQLite session file")?,
    );

    // 3. Network Setup
    let mut connection_params: grammers_mtsender::ConnectionParams = Default::default();
    connection_params.proxy_url = env::var("all_proxy").ok();
    if let Some(proxy_url) = &connection_params.proxy_url {
        tracing::info!("connecting with proxy: {}", proxy_url);
    }
    let SenderPool {
        runner,
        handle,
        updates,
    } = SenderPool::with_configuration(Arc::clone(&session), api_id, connection_params);
    let client = Client::new(handle);

    // 4. The "Background Heartbeat"
    // We spawn the runner but keep the handle to monitor if it dies.
    let mut runner_handle = tokio::spawn(runner.run());

    // 5. Auth Check
    if !client.is_authorized().await? {
        tracing::info!("Logging in as bot...");
        client.bot_sign_in(&token, &api_hash).await?;
    }

    let me = client.get_me().await?;
    tracing::info!("Logged in as @{}", me.username().unwrap_or("unknown"));

    // 6. Global State Injection
    STORE.init(client.clone()).await?;
    tracing::info!("Store ready");

    // 7. Robust Event Loop
    let mut update_stream = client.stream_updates(updates, Default::default()).await;
    tracing::info!("Listening for updates... Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            // Priority 1: Graceful Shutdown
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("\nShutdown signal received. Cleaning up...");
                break;
            }

            // Priority 2: Monitor the Network Runner
            // If the background connection task ends, the bot is dead.
            exit_status = &mut runner_handle => {
                return Err(anyhow::anyhow!("Network runner task terminated: {:?}", exit_status));
            }

            // Priority 3: Process Updates
            // We match instead of using '?' to prevent a single bad update from killing the bot.
            update = update_stream.next() => {
                match update {
                    Ok(update) => {
                        let bot = client.clone();
                        task::spawn(async move {
                            if let Err(e) = handle_update(bot, update).await {
                                tracing::error!("Handler Error: {:#}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Update stream encountered a glitch: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}
