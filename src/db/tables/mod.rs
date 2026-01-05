//! Database table operations

mod collection_table;
mod favorite_table;
mod libdata_table;
mod mix_table;
mod page_table;
mod playlist_table;
mod plugin_table;
mod scrobble_table;
mod similar_artist_table;
mod track_table;
mod user_table;

pub use collection_table::CollectionTable;
pub use favorite_table::FavoriteTable;
pub use playlist_table::PlaylistTable;
pub use plugin_table::PluginTable;
pub use scrobble_table::ScrobbleTable;
pub use track_table::TrackTable;
pub use user_table::UserTable;

pub use mix_table::MixTable;
pub use similar_artist_table::SimilarArtistTable;
