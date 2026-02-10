//! File system watcher for detecting music library changes

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// File system event types
#[derive(Debug, Clone)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

/// File system watchdog
pub struct Watchdog {
    watcher: RecommendedWatcher,
    receiver: Receiver<FsEvent>,
    watched_paths: Vec<PathBuf>,
}

impl Watchdog {
    /// Create new watchdog
    pub fn new() -> Result<Self> {
        let (tx, rx) = channel();

        let event_handler = move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                Self::handle_event(&tx, event);
            }
        };

        let watcher = RecommendedWatcher::new(
            event_handler,
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        Ok(Self {
            watcher,
            receiver: rx,
            watched_paths: Vec::new(),
        })
    }

    /// Handle raw notify event
    fn handle_event(tx: &Sender<FsEvent>, event: Event) {
        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    let _ = tx.send(FsEvent::Created(path));
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    let _ = tx.send(FsEvent::Modified(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    let _ = tx.send(FsEvent::Deleted(path));
                }
            }
            EventKind::Other => {
                // Handle rename events
                if event.paths.len() == 2 {
                    let _ = tx.send(FsEvent::Renamed(
                        event.paths[0].clone(),
                        event.paths[1].clone(),
                    ));
                }
            }
            _ => {}
        }
    }

    /// Watch a directory
    pub fn watch(&mut self, path: &PathBuf) -> Result<()> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;
        self.watched_paths.push(path.clone());
        Ok(())
    }

    /// Watch multiple directories
    pub fn watch_all(&mut self, paths: &[PathBuf]) -> Result<()> {
        for path in paths {
            self.watch(path)?;
        }
        Ok(())
    }

    /// Stop watching a directory
    pub fn unwatch(&mut self, path: &PathBuf) -> Result<()> {
        self.watcher.unwatch(path)?;
        self.watched_paths.retain(|p| p != path);
        Ok(())
    }

    /// Stop watching all directories
    pub fn unwatch_all(&mut self) -> Result<()> {
        for path in self.watched_paths.clone() {
            self.watcher.unwatch(&path)?;
        }
        self.watched_paths.clear();
        Ok(())
    }

    /// Get pending events (non-blocking)
    pub fn get_events(&self) -> Vec<FsEvent> {
        let mut events = Vec::new();

        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }

        events
    }

    /// Wait for next event (blocking)
    pub fn wait_for_event(&self) -> Result<FsEvent> {
        Ok(self.receiver.recv()?)
    }

    /// Wait for event with timeout
    pub fn wait_for_event_timeout(&self, timeout: Duration) -> Option<FsEvent> {
        self.receiver.recv_timeout(timeout).ok()
    }

    /// Get watched paths
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }

    /// Check if path is audio file
    pub fn is_audio_file(path: &PathBuf) -> bool {
        const AUDIO_EXTENSIONS: &[&str] = &[
            "mp3", "flac", "ogg", "wav", "m4a", "aac", "wma", "opus", "aiff", "alac",
            "ape", "wv", "mpc", "tta", "dsf", "dff", "webm", "mka", "spx",
        ];

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Filter events to only audio file events
    pub fn filter_audio_events(events: Vec<FsEvent>) -> Vec<FsEvent> {
        events
            .into_iter()
            .filter(|event| match event {
                FsEvent::Created(path) | FsEvent::Modified(path) | FsEvent::Deleted(path) => {
                    Self::is_audio_file(path)
                }
                FsEvent::Renamed(from, to) => Self::is_audio_file(from) || Self::is_audio_file(to),
            })
            .collect()
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new().expect("Failed to create watchdog")
    }
}

/// Start the watchdog service
pub async fn start_watchdog() -> Result<()> {
    use crate::config::UserConfig;
    use crate::core::file_cache::FileCache;

    let config = UserConfig::load()?;

    if !config.enable_watchdog {
        return Ok(());
    }

    let mut watchdog = Watchdog::new()?;

    for root_dir in &config.root_dirs {
        watchdog.watch(&PathBuf::from(root_dir))?;
    }

    // Process events in a loop
    loop {
        let events = watchdog.get_events();
        if !events.is_empty() {
            let audio_events = Watchdog::filter_audio_events(events);

            if !audio_events.is_empty() {
                // invalidate file cache for changed paths
                if let Some(cache) = FileCache::get() {
                    for event in &audio_events {
                        match event {
                            FsEvent::Modified(path) | FsEvent::Deleted(path) => {
                                cache.invalidate_path(path);
                            }
                            FsEvent::Renamed(from, to) => {
                                cache.invalidate_path(from);
                                cache.invalidate_path(to);
                            }
                            FsEvent::Created(_) => {
                                // new files don't need cache invalidation
                            }
                        }
                    }
                }

                // TODO: Handle events (reindex changed files)
                tracing::info!("Detected {} audio file changes", audio_events.len());
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
