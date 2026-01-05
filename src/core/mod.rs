//! Core library functions for SwingMusic

pub mod albums;
pub mod artistlib;
pub mod colorlib;
pub mod crons;
pub mod ffmpeg;
pub mod file_cache;
pub mod folder;
pub mod homepage;
pub mod images;
pub mod indexer;
pub mod lyrics;
pub mod mapstuff;
pub mod playlistlib;
pub mod populate;
pub mod recipes;
pub mod search;
pub mod silence;
pub mod sorting;
pub mod tagger;
pub mod trackslib;
pub mod transcode;
pub mod watchdogg;

pub use albums::AlbumLib;
pub use artistlib::ArtistLib;
pub use folder::FolderLib;
pub use playlistlib::PlaylistLib;
pub use search::SearchLib;
pub use sorting::SortLib;
pub use tagger::Tagger;
