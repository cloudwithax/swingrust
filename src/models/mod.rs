//! Data models for SwingMusic
//!
//! This module contains all the core data structures used throughout the application.

mod album;
mod artist;
mod enums;
mod favorite;
mod folder;
mod lastfm;
mod mix;
mod playlist;
mod plugins;
mod stats;
mod track;
mod user;

pub use album::Album;
pub use artist::Artist;
pub use favorite::{Favorite, FavoriteType};
pub use folder::Folder;
pub use mix::Mix;
pub use playlist::{Playlist, PlaylistSettings};
pub use stats::TrackLog;
pub use track::Track;
pub use user::{User, UserRole};

#[allow(unused_imports)]
pub use artist::{ArtistRef, SimilarArtist, SimilarArtistEntry};
#[allow(unused_imports)]
pub use enums::*;
#[allow(unused_imports)]
pub use lastfm::LastfmArtist;
#[allow(unused_imports)]
pub use mix::MixSourceType;
#[allow(unused_imports)]
pub use plugins::{Plugin, PluginSettings};

/// Reference to an artist (used in track/album artist lists)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtistRefItem {
    pub name: String,
    pub artisthash: String,
}

impl ArtistRefItem {
    pub fn new(name: String, artisthash: String) -> Self {
        Self { name, artisthash }
    }
}

/// Reference to a genre
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GenreRef {
    pub name: String,
    pub genrehash: String,
}

impl GenreRef {
    pub fn new(name: String, genrehash: String) -> Self {
        Self { name, genrehash }
    }
}
