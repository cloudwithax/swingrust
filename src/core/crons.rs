//! Cron jobs for periodic tasks

use anyhow::Result;
use std::time::Duration;
use tokio::time;

/// Start all cron jobs
pub async fn start_cron_jobs() -> Result<()> {
    // Periodic cleanup job (runs every hour)
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = cleanup_task().await {
                tracing::error!("Cleanup task error: {}", e);
            }
        }
    });

    // Periodic scan job (runs every 6 hours)
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(21600));
        loop {
            interval.tick().await;
            if let Err(e) = periodic_scan().await {
                tracing::error!("Periodic scan error: {}", e);
            }
        }
    });

    Ok(())
}

/// Cleanup old data
async fn cleanup_task() -> Result<()> {
    use crate::db::DbEngine;

    let db = DbEngine::get()?;

    // Clean up old scrobbles (older than 1 year)
    sqlx::query("DELETE FROM scrobble WHERE timestamp < datetime('now', '-1 year')")
        .execute(db.pool())
        .await?;

    tracing::info!("Cleanup task completed");
    Ok(())
}

/// Periodic scan of music folders
async fn periodic_scan() -> Result<()> {
    use crate::config::UserConfig;
    use crate::core::indexer::Indexer;

    let config = UserConfig::load()?;

    if !config.enable_periodic_scans {
        return Ok(());
    }

    tracing::info!("Starting periodic scan...");

    let indexer = Indexer::from_config(&config);
    let _tracks = indexer.index()?;

    tracing::info!("Periodic scan completed");
    Ok(())
}
