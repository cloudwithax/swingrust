//! Configuration module for SwingMusic
//!
//! This module contains the application configuration structures and path management.

mod paths;
mod user_config;

pub use paths::Paths;
pub use user_config::UserConfig;

/// Default thumbnail sizes
pub const XSM_THUMB_SIZE: u32 = 64;
pub const SM_THUMB_SIZE: u32 = 96;
pub const MD_THUMB_SIZE: u32 = 256;
pub const LG_THUMB_SIZE: u32 = 512;

/// Default artist image sizes
pub const SM_ARTIST_IMG_SIZE: u32 = 128;
pub const MD_ARTIST_IMG_SIZE: u32 = 256;
pub const LG_ARTIST_IMG_SIZE: u32 = 512;
