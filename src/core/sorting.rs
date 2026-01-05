//! Sorting utilities for tracks, albums, artists

use std::cmp::Ordering;

use crate::models::{Album, Artist, Track};

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Sort field for tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSortBy {
    Title,
    Album,
    Artist,
    Duration,
    DateAdded,
    TrackNumber,
    DiscNumber,
    Year,
    Bitrate,
    PlayCount,
    LastPlayed,
}

/// Sort field for albums
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSortBy {
    Title,
    Artist,
    Year,
    TrackCount,
    Duration,
    DateAdded,
    PlayCount,
}

/// Sort field for artists
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistSortBy {
    Name,
    TrackCount,
    AlbumCount,
    PlayCount,
}

/// Sorting library
pub struct SortLib;

impl SortLib {
    /// Sort tracks by field
    pub fn sort_tracks(tracks: &mut [Track], by: TrackSortBy, order: SortOrder) {
        tracks.sort_by(|a, b| {
            let cmp = match by {
                TrackSortBy::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                TrackSortBy::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
                TrackSortBy::Artist => a.artist().to_lowercase().cmp(&b.artist().to_lowercase()),
                TrackSortBy::Duration => a.duration.cmp(&b.duration),
                TrackSortBy::DateAdded => a.date.cmp(&b.date),
                TrackSortBy::TrackNumber => a.track.cmp(&b.track),
                TrackSortBy::DiscNumber => {
                    let dc = a.disc.cmp(&b.disc);
                    if dc == Ordering::Equal {
                        a.track.cmp(&b.track)
                    } else {
                        dc
                    }
                }
                TrackSortBy::Year => a.date.cmp(&b.date),
                TrackSortBy::Bitrate => a.bitrate.cmp(&b.bitrate),
                TrackSortBy::PlayCount => Ordering::Equal, // Requires external data
                TrackSortBy::LastPlayed => Ordering::Equal, // Requires external data
            };

            match order {
                SortOrder::Ascending => cmp,
                SortOrder::Descending => cmp.reverse(),
            }
        });
    }

    /// Sort tracks by disc and track number (for album view)
    pub fn sort_tracks_album_order(tracks: &mut [Track]) {
        tracks.sort_by(|a, b| {
            let disc_cmp = a.disc.cmp(&b.disc);
            if disc_cmp != Ordering::Equal {
                disc_cmp
            } else {
                a.track.cmp(&b.track)
            }
        });
    }

    /// Sort albums by field
    pub fn sort_albums(albums: &mut [Album], by: AlbumSortBy, order: SortOrder) {
        albums.sort_by(|a, b| {
            let cmp = match by {
                AlbumSortBy::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                AlbumSortBy::Artist => a
                    .albumartist()
                    .to_lowercase()
                    .cmp(&b.albumartist().to_lowercase()),
                AlbumSortBy::Year => a.date.cmp(&b.date),
                AlbumSortBy::TrackCount => a.count().cmp(&b.count()),
                AlbumSortBy::Duration => a.duration.cmp(&b.duration),
                AlbumSortBy::DateAdded => a.date.cmp(&b.date),
                // Models store playcount without underscore
                AlbumSortBy::PlayCount => a.playcount.cmp(&b.playcount),
            };

            match order {
                SortOrder::Ascending => cmp,
                SortOrder::Descending => cmp.reverse(),
            }
        });
    }

    /// Sort artists by field
    pub fn sort_artists(artists: &mut [Artist], by: ArtistSortBy, order: SortOrder) {
        artists.sort_by(|a, b| {
            let cmp = match by {
                ArtistSortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ArtistSortBy::TrackCount => a.trackcount.cmp(&b.trackcount),
                ArtistSortBy::AlbumCount => a.albumcount.cmp(&b.albumcount),
                // Models store playcount without underscore
                ArtistSortBy::PlayCount => a.playcount.cmp(&b.playcount),
            };

            match order {
                SortOrder::Ascending => cmp,
                SortOrder::Descending => cmp.reverse(),
            }
        });
    }

    /// Parse sort parameter string (e.g., "title:asc", "year:desc")
    pub fn parse_track_sort(sort: &str) -> (TrackSortBy, SortOrder) {
        let parts: Vec<&str> = sort.split(':').collect();

        let by = match parts.first().map(|s| *s) {
            Some("title") => TrackSortBy::Title,
            Some("album") => TrackSortBy::Album,
            Some("artist") => TrackSortBy::Artist,
            Some("duration") => TrackSortBy::Duration,
            Some("date_added") | Some("created") => TrackSortBy::DateAdded,
            Some("track") => TrackSortBy::TrackNumber,
            Some("disc") => TrackSortBy::DiscNumber,
            Some("year") | Some("date") => TrackSortBy::Year,
            Some("bitrate") => TrackSortBy::Bitrate,
            Some("playcount") => TrackSortBy::PlayCount,
            Some("lastplayed") => TrackSortBy::LastPlayed,
            _ => TrackSortBy::Title,
        };

        let order = match parts.get(1).map(|s| *s) {
            Some("desc") => SortOrder::Descending,
            _ => SortOrder::Ascending,
        };

        (by, order)
    }

    /// Parse album sort parameter
    pub fn parse_album_sort(sort: &str) -> (AlbumSortBy, SortOrder) {
        let parts: Vec<&str> = sort.split(':').collect();

        let by = match parts.first().map(|s| *s) {
            Some("title") => AlbumSortBy::Title,
            Some("artist") => AlbumSortBy::Artist,
            Some("year") | Some("date") => AlbumSortBy::Year,
            Some("trackcount") | Some("tracks") => AlbumSortBy::TrackCount,
            Some("duration") => AlbumSortBy::Duration,
            Some("date_added") | Some("created") => AlbumSortBy::DateAdded,
            Some("playcount") => AlbumSortBy::PlayCount,
            _ => AlbumSortBy::Title,
        };

        let order = match parts.get(1).map(|s| *s) {
            Some("desc") => SortOrder::Descending,
            _ => SortOrder::Ascending,
        };

        (by, order)
    }

    /// Parse artist sort parameter
    pub fn parse_artist_sort(sort: &str) -> (ArtistSortBy, SortOrder) {
        let parts: Vec<&str> = sort.split(':').collect();

        let by = match parts.first().map(|s| *s) {
            Some("name") => ArtistSortBy::Name,
            Some("trackcount") | Some("tracks") => ArtistSortBy::TrackCount,
            Some("albumcount") | Some("albums") => ArtistSortBy::AlbumCount,
            Some("playcount") => ArtistSortBy::PlayCount,
            _ => ArtistSortBy::Name,
        };

        let order = match parts.get(1).map(|s| *s) {
            Some("desc") => SortOrder::Descending,
            _ => SortOrder::Ascending,
        };

        (by, order)
    }
}
