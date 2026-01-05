//! In-memory stores for tracks, albums, artists, and folders

mod album_store;
mod artist_store;
mod folder_store;
mod homepage_store;
mod track_store;

pub use album_store::AlbumStore;
pub use artist_store::ArtistStore;
pub use folder_store::FolderStore;
pub use homepage_store::HomepageStore;
pub use track_store::TrackStore;
