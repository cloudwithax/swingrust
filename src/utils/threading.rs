//! Threading utilities

use std::future::Future;
use tokio::task::JoinHandle;

/// Spawn a background task
pub fn spawn_background<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    tokio::spawn(future)
}

/// Spawn a blocking task
pub fn spawn_blocking<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}
