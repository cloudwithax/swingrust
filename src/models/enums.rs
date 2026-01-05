//! Enums for SwingMusic

use serde::{Deserialize, Serialize};

/// Album version types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlbumVersion {
    // Quality
    Explicit,
    HiRes,
    // Edition
    Deluxe,
    SuperDeluxe,
    Complete,
    Legacy,
    Special,
    Collectors,
    Archive,
    Limited,
    // Anniversary
    Anniversary,
    Diamond,
    Centennial,
    Golden,
    Platinum,
    Silver,
    Ultimate,
    // Format
    Expanded,
    Extended,
    Bonus,
    Original,
    Mono,
    Stereo,
    // Location
    International,
    Uk,
    Us,
    // Style
    Acoustic,
    Instrumental,
    Unplugged,
    // Season
    Summer,
    Winter,
    Spring,
    Fall,
    // Technical
    Audio360,
    Remastered,
    Reissue,
    Remix,
    ReRecorded,
}

impl AlbumVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlbumVersion::Explicit => "explicit",
            AlbumVersion::HiRes => "hi-res",
            AlbumVersion::Deluxe => "deluxe",
            AlbumVersion::SuperDeluxe => "super deluxe",
            AlbumVersion::Complete => "complete",
            AlbumVersion::Legacy => "legacy",
            AlbumVersion::Special => "special",
            AlbumVersion::Collectors => "collector's",
            AlbumVersion::Archive => "archive",
            AlbumVersion::Limited => "limited",
            AlbumVersion::Anniversary => "anniversary",
            AlbumVersion::Diamond => "diamond",
            AlbumVersion::Centennial => "centennial",
            AlbumVersion::Golden => "golden",
            AlbumVersion::Platinum => "platinum",
            AlbumVersion::Silver => "silver",
            AlbumVersion::Ultimate => "ultimate",
            AlbumVersion::Expanded => "expanded",
            AlbumVersion::Extended => "extended",
            AlbumVersion::Bonus => "bonus",
            AlbumVersion::Original => "original",
            AlbumVersion::Mono => "mono",
            AlbumVersion::Stereo => "stereo",
            AlbumVersion::International => "international",
            AlbumVersion::Uk => "uk",
            AlbumVersion::Us => "us",
            AlbumVersion::Acoustic => "acoustic",
            AlbumVersion::Instrumental => "instrumental",
            AlbumVersion::Unplugged => "unplugged",
            AlbumVersion::Summer => "summer",
            AlbumVersion::Winter => "winter",
            AlbumVersion::Spring => "spring",
            AlbumVersion::Fall => "fall",
            AlbumVersion::Audio360 => "360 audio",
            AlbumVersion::Remastered => "remastered",
            AlbumVersion::Reissue => "reissue",
            AlbumVersion::Remix => "remix",
            AlbumVersion::ReRecorded => "re-recorded",
        }
    }

    pub fn all_keywords() -> Vec<&'static str> {
        vec![
            "explicit",
            "hi-res",
            "deluxe",
            "super deluxe",
            "complete",
            "legacy",
            "special",
            "collector's",
            "archive",
            "limited",
            "anniversary",
            "diamond",
            "centennial",
            "golden",
            "platinum",
            "silver",
            "ultimate",
            "expanded",
            "extended",
            "bonus",
            "original",
            "mono",
            "stereo",
            "international",
            "uk",
            "us",
            "acoustic",
            "instrumental",
            "unplugged",
            "summer",
            "winter",
            "spring",
            "fall",
            "360 audio",
            "remastered",
            "reissue",
            "remix",
            "re-recorded",
        ]
    }

    pub fn get_regex_pattern() -> String {
        Self::all_keywords().join("|")
    }
}

/// Sort order
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    pub fn is_reversed(&self) -> bool {
        matches!(self, SortOrder::Descending)
    }
}

/// Track sort keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackSortKey {
    #[default]
    Default,
    Title,
    Album,
    Artists,
    Duration,
    Bitrate,
    Date,
    Playcount,
    Lastplayed,
    Filepath,
}

/// Album sort keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlbumSortKey {
    #[default]
    Title,
    Artists,
    Date,
    Duration,
    Trackcount,
    Playcount,
    Lastplayed,
    CreatedDate,
}

/// Artist sort keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtistSortKey {
    #[default]
    Name,
    Trackcount,
    Albumcount,
    Duration,
    Playcount,
    Lastplayed,
    CreatedDate,
}

/// Folder sort keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderSortKey {
    #[default]
    Default,
    Name,
    Lastmod,
    Trackcount,
}

/// Time period for statistics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimePeriod {
    #[default]
    AllTime,
    Year,
    Month,
    Week,
    Day,
}

impl TimePeriod {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimePeriod::AllTime => "alltime",
            TimePeriod::Year => "year",
            TimePeriod::Month => "month",
            TimePeriod::Week => "week",
            TimePeriod::Day => "day",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "alltime" | "all" => Some(TimePeriod::AllTime),
            "year" => Some(TimePeriod::Year),
            "month" => Some(TimePeriod::Month),
            "week" => Some(TimePeriod::Week),
            "day" => Some(TimePeriod::Day),
            _ => None,
        }
    }
}

/// Trend direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Rising,
    #[default]
    Stable,
    Falling,
}

impl Trend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Trend::Rising => "rising",
            Trend::Stable => "stable",
            Trend::Falling => "falling",
        }
    }
}
