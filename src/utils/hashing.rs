//! Hashing utilities

use xxhash_rust::xxh3::xxh3_64;

/// Create a case-insensitive, alphanumeric-normalized hash
///
/// # Arguments
/// * `args` - Strings to hash together
/// * `decode` - Whether to decode unicode to ASCII
///
/// # Returns
/// A 16-character hex string hash
pub fn create_hash(args: &[&str], decode: bool) -> String {
    let mut combined = String::new();

    for arg in args {
        let processed = remove_non_alnum(arg);
        combined.push_str(&processed);
    }

    if decode {
        combined = deunicode::deunicode(&combined);
    }

    let hash = xxh3_64(combined.as_bytes());
    format!("{:016x}", hash)[..11].to_string()
}

/// Remove non-alphanumeric characters and normalize
fn remove_non_alnum(token: &str) -> String {
    let lower = token.to_lowercase();
    let trimmed = lower.trim().replace(' ', "");

    let filtered: String = trimmed.chars().filter(|c| c.is_alphanumeric()).collect();

    if filtered.is_empty() {
        trimmed
    } else {
        filtered
    }
}

/// Create a hash for a track
pub fn create_track_hash(artists: &str, album: &str, title: &str) -> String {
    create_hash(&[artists, album, title], true)
}

/// Alias for compatibility
pub fn create_trackhash(filepath: &str, duration: i32) -> String {
    let input = format!("{}:{}", filepath, duration);
    let hash = xxh3_64(input.as_bytes());
    format!("{:016x}", hash)[..11].to_string()
}

/// Create a hash for an album
pub fn create_album_hash(album: &str, albumartists: &str) -> String {
    create_hash(&[album, albumartists], true)
}

/// Create a hash for an artist
pub fn create_artist_hash(name: &str) -> String {
    create_hash(&[name], true)
}

/// Create a hash for a genre
pub fn create_genre_hash(name: &str) -> String {
    create_hash(&[name], true)
}

/// Create a hash for a folder path
pub fn create_folder_hash(path: &str) -> String {
    create_hash(&[path], false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_hash() {
        let hash = create_hash(&["Test", "Artist"], true);
        assert_eq!(hash.len(), 11);

        // Same input should produce same hash
        let hash2 = create_hash(&["Test", "Artist"], true);
        assert_eq!(hash, hash2);

        // Case insensitive
        let hash3 = create_hash(&["test", "artist"], true);
        assert_eq!(hash, hash3);
    }

    #[test]
    fn test_remove_non_alnum() {
        assert_eq!(remove_non_alnum("Test Artist"), "testartist");
        assert_eq!(remove_non_alnum("AC/DC"), "acdc");
        assert_eq!(remove_non_alnum("  Spaces  "), "spaces");
    }

    #[test]
    fn test_unicode_handling() {
        // With decode
        let hash1 = create_hash(&["Café"], true);
        let hash2 = create_hash(&["Cafe"], true);
        assert_eq!(hash1, hash2);

        // Without decode
        let hash3 = create_hash(&["Café"], false);
        let hash4 = create_hash(&["Cafe"], false);
        assert_ne!(hash3, hash4);
    }
}
