//! Text parsing utilities for music metadata

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Pattern for "(prod. X)" or "[prod. X]"
    static ref PROD_BY_PATTERN: Regex = Regex::new(
        r"(?i)[\[\(]\s*prod\.?\s*(?:by\s+)?[^\]\)]+[\]\)]"
    ).unwrap();

    // Pattern for featured artists in title
    static ref FEAT_PATTERN: Regex = Regex::new(
        r"(?i)[\[\(]\s*(?:feat\.?|ft\.?|featuring)\s+([^\]\)]+)[\]\)]"
    ).unwrap();

    // Pattern for remaster info in brackets
    static ref REMASTER_BRACKET_PATTERN: Regex = Regex::new(
        r"(?i)[\[\(][^\]\)]*(?:remaster|remastered|reissue)[^\]\)]*[\]\)]"
    ).unwrap();

    // Pattern for remaster info with dash
    static ref REMASTER_DASH_PATTERN: Regex = Regex::new(
        r"(?i)\s*[-–—]\s*(?:\d{4}\s+)?(?:remaster|remastered|reissue).*$"
    ).unwrap();

    // Pattern for album version info in brackets
    static ref VERSION_BRACKET_PATTERN: Regex = Regex::new(
        r"(?i)[\[\(]([^\]\)]*(?:deluxe|expanded|remaster|anniversary|edition|version|bonus|special|limited|collector)[^\]\)]*)[\]\)]"
    ).unwrap();

    // Pattern for anniversary text (e.g., "25th anniversary")
    static ref ANNIVERSARY_PATTERN: Regex = Regex::new(
        r"(?i)(\d+(?:st|nd|rd|th)?\s*anniversary)"
    ).unwrap();
}

/// Split artist string by separators, preserving ignored artists
pub fn split_artists(
    src: &str,
    separators: &std::collections::HashSet<String>,
    ignore_list: &std::collections::HashSet<String>,
) -> Vec<String> {
    if src.is_empty() {
        return Vec::new();
    }

    // Check if the entire string is in the ignore list
    if ignore_list.contains(&src.to_lowercase()) {
        return vec![src.trim().to_string()];
    }

    // Build a regex pattern from separators
    let sep_pattern: String = separators
        .iter()
        .map(|s| regex::escape(s))
        .collect::<Vec<_>>()
        .join("|");

    if sep_pattern.is_empty() {
        return vec![src.trim().to_string()];
    }

    let re = Regex::new(&sep_pattern).unwrap();

    let mut result = Vec::new();
    let mut last_end = 0;
    let src_lower = src.to_lowercase();

    for mat in re.find_iter(src) {
        if mat.start() < last_end {
            continue;
        }

        let before = &src[last_end..mat.start()];
        let trimmed = before.trim();

        if !trimmed.is_empty() {
            // Check if this part + separator is in ignore list
            let potential_combined = format!("{}{}", trimmed.to_lowercase(), mat.as_str());
            let remaining = &src_lower[mat.end()..];

            let mut found_ignored = false;
            for ignored in ignore_list {
                if ignored.starts_with(&potential_combined) {
                    // Find next separator
                    if let Some(next_mat) = re.find(&src[mat.end()..]) {
                        let full_part = &src[last_end..mat.end() + next_mat.start()];
                        if ignore_list.contains(&full_part.to_lowercase()) {
                            result.push(full_part.trim().to_string());
                            last_end = mat.end() + next_mat.end();
                            found_ignored = true;
                            break;
                        }
                    }
                }
            }

            if !found_ignored {
                result.push(trimmed.to_string());
                last_end = mat.end();
            }
        } else {
            last_end = mat.end();
        }
    }

    // Add remaining
    let remaining = &src[last_end..];
    let trimmed = remaining.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }

    result
}

/// Remove "(prod. by X)" from track title
pub fn remove_prod_by(title: &str) -> String {
    PROD_BY_PATTERN.replace_all(title, "").trim().to_string()
}

/// Extract featured artists from title
pub fn extract_featured_artists(title: &str) -> (String, Vec<String>) {
    let mut featured = Vec::new();

    for cap in FEAT_PATTERN.captures_iter(title) {
        if let Some(artists) = cap.get(1) {
            for artist in artists.as_str().split(&[',', '&'][..]) {
                let trimmed = artist.trim();
                if !trimmed.is_empty() {
                    featured.push(trimmed.to_string());
                }
            }
        }
    }

    let clean_title = FEAT_PATTERN.replace_all(title, "").trim().to_string();

    (clean_title, featured)
}

/// Extract base album title (without version info)
pub fn get_base_album_title(title: &str) -> String {
    let mut result = title.to_string();

    // Remove version info in brackets
    result = VERSION_BRACKET_PATTERN.replace_all(&result, "").to_string();

    // Clean up extra spaces
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    result.trim().to_string()
}

/// Extract album versions from title
pub fn get_album_versions(title: &str) -> Vec<String> {
    let mut versions = Vec::new();

    for cap in VERSION_BRACKET_PATTERN.captures_iter(title) {
        if let Some(version) = cap.get(1) {
            versions.push(version.as_str().trim().to_string());
        }
    }

    versions
}

/// Extract anniversary text from title
pub fn get_anniversary_text(title: &str) -> Option<String> {
    ANNIVERSARY_PATTERN
        .captures(title)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

/// Remove remaster info from title
pub fn remove_remaster_info(title: &str) -> String {
    let mut result = title.to_string();

    // Remove bracketed remaster info
    result = REMASTER_BRACKET_PATTERN
        .replace_all(&result, "")
        .to_string();

    // Remove dash-separated remaster info
    result = REMASTER_DASH_PATTERN.replace_all(&result, "").to_string();

    // Clean up extra spaces
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    result.trim().to_string()
}

/// Parse filename to extract track info
pub fn parse_filename(filename: &str) -> Option<(Option<i32>, String, String)> {
    // Remove extension
    let name = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())?
        .to_string();

    // Pattern: "01 - Artist - Title" or "01. Artist - Title"
    let pattern1 = Regex::new(r"^(\d+)[\.\s]+[-–]?\s*(.+?)\s*[-–]\s*(.+)$").unwrap();
    if let Some(cap) = pattern1.captures(&name) {
        let track_num: i32 = cap.get(1)?.as_str().parse().ok()?;
        let artist = cap.get(2)?.as_str().trim().to_string();
        let title = cap.get(3)?.as_str().trim().to_string();
        return Some((Some(track_num), artist, title));
    }

    // Pattern: "01 - Title"
    let pattern2 = Regex::new(r"^(\d+)[\.\s]+[-–]?\s*(.+)$").unwrap();
    if let Some(cap) = pattern2.captures(&name) {
        let track_num: i32 = cap.get(1)?.as_str().parse().ok()?;
        let title = cap.get(2)?.as_str().trim().to_string();
        return Some((Some(track_num), String::new(), title));
    }

    // Pattern: "Artist - Title"
    let pattern3 = Regex::new(r"^(.+?)\s*[-–]\s*(.+)$").unwrap();
    if let Some(cap) = pattern3.captures(&name) {
        let artist = cap.get(1)?.as_str().trim().to_string();
        let title = cap.get(2)?.as_str().trim().to_string();
        return Some((None, artist, title));
    }

    // Fallback: just the filename as title
    Some((None, String::new(), name))
}

/// Clean a title by removing common metadata
pub fn clean_title(title: &str) -> String {
    let mut result = title.to_string();

    // Remove prod. by
    result = PROD_BY_PATTERN.replace_all(&result, "").to_string();

    // Remove remaster info
    result = REMASTER_BRACKET_PATTERN
        .replace_all(&result, "")
        .to_string();
    result = REMASTER_DASH_PATTERN.replace_all(&result, "").to_string();

    // Clean up extra whitespace
    result = result.trim().to_string();
    result = Regex::new(r"\s+")
        .unwrap()
        .replace_all(&result, " ")
        .to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_split_artists() {
        let seps: HashSet<String> = [";".to_string(), "/".to_string()].into_iter().collect();
        let ignore: HashSet<String> = ["ac/dc".to_string()].into_iter().collect();

        let result = split_artists("Artist1; Artist2", &seps, &ignore);
        assert_eq!(result, vec!["Artist1", "Artist2"]);

        let result = split_artists("AC/DC", &seps, &ignore);
        assert_eq!(result, vec!["AC/DC"]);
    }

    #[test]
    fn test_remove_prod_by() {
        assert_eq!(remove_prod_by("Song (prod. Producer)"), "Song");
        assert_eq!(remove_prod_by("Song [Prod. by Producer]"), "Song");
        assert_eq!(remove_prod_by("Song"), "Song");
    }

    #[test]
    fn test_extract_featured_artists() {
        let (title, feat) = extract_featured_artists("Song (feat. Artist1, Artist2)");
        assert_eq!(title, "Song");
        assert_eq!(feat, vec!["Artist1", "Artist2"]);

        let (title, feat) = extract_featured_artists("Song (ft. Artist)");
        assert_eq!(title, "Song");
        assert_eq!(feat, vec!["Artist"]);
    }

    #[test]
    fn test_get_base_album_title() {
        assert_eq!(get_base_album_title("Album (Deluxe Edition)"), "Album");
        assert_eq!(get_base_album_title("Album [Remastered]"), "Album");
    }

    #[test]
    fn test_parse_filename() {
        let result = parse_filename("01 - Artist - Title.mp3");
        assert_eq!(
            result,
            Some((Some(1), "Artist".to_string(), "Title".to_string()))
        );

        let result = parse_filename("Artist - Title.mp3");
        assert_eq!(
            result,
            Some((None, "Artist".to_string(), "Title".to_string()))
        );
    }

    #[test]
    fn test_split_artists_with_comma() {
        let seps: HashSet<String> = [";".to_string(), "/".to_string(), ", ".to_string()]
            .into_iter()
            .collect();
        let ignore: HashSet<String> = ["tyler, the creator".to_string()].into_iter().collect();

        // Test separation
        let result = split_artists("Kanye West, JAY-Z", &seps, &ignore);
        assert_eq!(result, vec!["Kanye West", "JAY-Z"]);

        // Test ignore
        let result = split_artists("Tyler, The Creator", &seps, &ignore);
        assert_eq!(result, vec!["Tyler, The Creator"]);

        // Test ignore with additional artists
        let result = split_artists("Tyler, The Creator, Another Artist", &seps, &ignore);
        assert_eq!(result, vec!["Tyler, The Creator", "Another Artist"]);
    }
}
