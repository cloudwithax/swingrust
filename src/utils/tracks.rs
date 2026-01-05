//! Track utilities

use std::collections::HashMap;

use crate::models::Track;

/// Remove duplicate tracks, keeping highest bitrate
pub fn remove_duplicates(tracks: Vec<Track>, sort: bool) -> Vec<Track> {
    let mut groups: HashMap<String, Vec<Track>> = HashMap::new();

    for track in tracks {
        groups
            .entry(track.trackhash.clone())
            .or_default()
            .push(track);
    }

    let mut result: Vec<Track> = groups
        .into_values()
        .map(|mut group| {
            // Sort by bitrate descending and take the first (highest)
            group.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));
            group.into_iter().next().unwrap()
        })
        .collect();

    if sort {
        // Sort by disc and track number
        result.sort_by_key(|t| t.sort_position());
    }

    result
}

/// Sort tracks by disc and track number
pub fn sort_by_disc_and_track(tracks: &mut [Track]) {
    tracks.sort_by_key(|t| t.sort_position());
}

/// Remove remaster info from track title
pub fn remove_remaster_info(title: &str) -> String {
    let patterns = vec![
        r"\(\d{4}\s+Remaster(ed)?\)",
        r"\[\d{4}\s+Remaster(ed)?\]",
        r"-\s+\d{4}\s+Remaster(ed)?",
        r"\(Remaster(ed)?\)",
        r"\[Remaster(ed)?\]",
    ];

    let mut result = title.to_string();
    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    result.trim().to_string()
}

/// Balance a tracklist to ensure artist diversity
pub fn balance_tracklist(tracks: Vec<Track>, min_gap: usize) -> Vec<Track> {
    if tracks.len() <= min_gap {
        return tracks;
    }

    let mut result = Vec::with_capacity(tracks.len());
    let mut remaining: Vec<Track> = tracks;
    let mut recent_artists: Vec<String> = Vec::new();

    while !remaining.is_empty() {
        // Find a track that doesn't violate the gap rule
        let mut found_idx = None;

        for (i, track) in remaining.iter().enumerate() {
            let violates = track
                .artisthashes
                .iter()
                .any(|hash| recent_artists.iter().take(min_gap).any(|h| h == hash));

            if !violates {
                found_idx = Some(i);
                break;
            }
        }

        // If no track found that satisfies gap, take the first one
        let idx = found_idx.unwrap_or(0);
        let track = remaining.remove(idx);

        // Update recent artists
        for hash in &track.artisthashes {
            recent_artists.insert(0, hash.clone());
        }
        if recent_artists.len() > min_gap * 2 {
            recent_artists.truncate(min_gap * 2);
        }

        result.push(track);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_duplicates() {
        let mut track1 = Track::new();
        track1.trackhash = "hash1".to_string();
        track1.bitrate = 320;

        let mut track2 = Track::new();
        track2.trackhash = "hash1".to_string();
        track2.bitrate = 128;

        let mut track3 = Track::new();
        track3.trackhash = "hash2".to_string();
        track3.bitrate = 256;

        let tracks = vec![track1, track2, track3];
        let result = remove_duplicates(tracks, false);

        assert_eq!(result.len(), 2);

        // Should keep higher bitrate version
        let hash1_track = result.iter().find(|t| t.trackhash == "hash1").unwrap();
        assert_eq!(hash1_track.bitrate, 320);
    }
}
