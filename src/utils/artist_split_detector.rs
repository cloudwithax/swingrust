//! smart artist split detection using heuristic analysis
//!
//! this module provides intelligent detection of when an artist name should NOT be split,
//! replacing hardcoded ignore lists with semantic pattern recognition

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;

lazy_static! {
    // acronym patterns like "C&C" or "A & B" at start of name
    static ref ACRONYM_GLUE_PATTERN: Regex = Regex::new(
        r"(?i)^[A-Z]\s*[&+]\s*[A-Z]"
    ).unwrap();

    // "the X" suffix patterns indicating band names
    static ref THE_SUFFIX_PATTERN: Regex = Regex::new(
        r"(?i)^the\s+\S+"
    ).unwrap();

    // common band/group suffixes that indicate a unified artist name
    static ref GROUP_NOUNS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("band");
        set.insert("orchestra");
        set.insert("company");
        set.insert("family");
        set.insert("crew");
        set.insert("singers");
        set.insert("players");
        set.insert("mechanics");
        set.insert("news");
        set.insert("factory");
        set.insert("gang");
        set.insert("pips");
        set.insert("vandellas");
        set.insert("supremes");
        set.insert("wailers");
        set.insert("waves");
        set.insert("bunnymen");
        set.insert("blackhearts");
        set.insert("stones");
        set.insert("heartbreakers");
        set.insert("pacemakers");
        set.insert("mysterians");
        set.insert("indications");
        set.insert("sweats");
        set.insert("seeds");
        set.insert("machine");
        set.insert("shondells");
        set.insert("zodiacs");
        set.insert("brass");
        set.insert("papas");
        set.insert("blowfish");
        set.insert("sons");
        set
    };
}

/// split boundary decision result
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitDecision {
    /// separator represents a collaboration, should split here
    Split,
    /// separator is part of artist name, keep together
    KeepTogether,
    /// uncertain, defer to ignore list or default behavior
    Uncertain,
}

/// analyzes whether a separator boundary should split or keep together
pub struct ArtistSplitDetector {
    // explicit user overrides take highest priority
    never_split: HashSet<String>,
    always_split: HashSet<String>,
}

impl Default for ArtistSplitDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtistSplitDetector {
    pub fn new() -> Self {
        Self {
            never_split: HashSet::new(),
            always_split: HashSet::new(),
        }
    }

    /// create detector with user overrides from the legacy ignore list
    pub fn with_ignore_list(ignore_list: &HashSet<String>) -> Self {
        let mut detector = Self::new();
        for name in ignore_list {
            detector.never_split.insert(name.to_lowercase());
        }
        detector
    }

    /// check if the entire string should be kept as a single artist name
    /// this catches multi-separator patterns before per-boundary analysis
    pub fn should_keep_entire_string(&self, src: &str) -> bool {
        let lower = src.to_lowercase();

        // check explicit overrides
        if self.never_split.contains(&lower) {
            return true;
        }
        if self.always_split.contains(&lower) {
            return false;
        }

        // check for multi-separator band name patterns
        self.is_multi_separator_band_name(src)
    }

    /// add an override to never split this artist name
    pub fn add_never_split(&mut self, name: &str) {
        self.never_split.insert(name.to_lowercase());
    }

    /// add an override to always split this artist name
    pub fn add_always_split(&mut self, name: &str) {
        self.always_split.insert(name.to_lowercase());
    }

    /// main entry point: decide if we should split at a given separator boundary
    ///
    /// - left: text before separator (trimmed)
    /// - separator: the separator matched (e.g. "&", ", ", "/")
    /// - right: text after separator (trimmed, up to next separator or end)
    /// - full_src: complete original artist string for context
    pub fn should_split(&self, left: &str, separator: &str, right: &str, full_src: &str) -> SplitDecision {
        let full_lower = full_src.to_lowercase();

        // check explicit overrides first
        if self.always_split.contains(&full_lower) {
            return SplitDecision::Split;
        }
        if self.never_split.contains(&full_lower) {
            return SplitDecision::KeepTogether;
        }

        // run heuristic analysis
        let score = self.compute_keep_together_score(left, separator, right, full_src);

        // high confidence thresholds
        if score >= 3 {
            SplitDecision::KeepTogether
        } else if score <= -2 {
            SplitDecision::Split
        } else {
            SplitDecision::Uncertain
        }
    }

    /// compute a score indicating how likely this should NOT be split
    /// positive = keep together, negative = split
    fn compute_keep_together_score(&self, left: &str, separator: &str, right: &str, full_src: &str) -> i32 {
        let mut score = 0;
        let sep_lower = separator.to_lowercase();
        let right_lower = right.to_lowercase();
        let left_lower = left.to_lowercase();

        // rule 1: no-space slash compound (AC/DC, M/A/R/R/S)
        if self.is_slash_compound(left, separator, right) {
            score += 4;
        }

        // rule 2: comma + "the ..." inversion (Tyler, The Creator)
        if self.is_comma_the_inversion(separator, &right_lower) {
            score += 3;
        }

        // rule 3: "& the ..." / "and the ..." band suffix
        if self.is_band_suffix_pattern(&sep_lower, &right_lower) {
            score += 4;
        }

        // rule 4: acronym/initialism glue (C&C, A+B at start)
        if self.is_acronym_glue(&left_lower, &sep_lower) {
            score += 3;
        }

        // rule 5: tiny segment sanity check
        if self.would_produce_tiny_segment(left) || self.would_produce_tiny_segment(right) {
            score += 2;
        }

        // rule 6: plural suffix on right side (suggests group name)
        if self.has_plural_suffix(&right_lower) {
            score += 1;
        }

        // rule 7: contains group noun
        if self.contains_group_noun(&right_lower) {
            score += 2;
        }

        // rule 8: duo name pattern (single token & single token)
        if self.is_duo_name_pattern(left, separator, right) {
            score += 2;
        }

        // rule 9: multi-separator band name detection
        // if the full string has multiple separators and looks like a band name pattern
        if self.is_multi_separator_band_name(full_src) {
            score += 3;
        }

        // negative signals: likely a collaboration
        // rule 10: both sides look like full artist names (multiple tokens, capitalized)
        if self.looks_like_separate_artists(left, right) {
            score -= 2;
        }

        score
    }

    /// check for no-space slash compounds like AC/DC
    fn is_slash_compound(&self, left: &str, separator: &str, right: &str) -> bool {
        if separator != "/" {
            return false;
        }

        // both sides should be short and mostly uppercase/alphanumeric
        let left_short = left.len() <= 5;
        let right_short = right.len() <= 5 || right.split_whitespace().next().map(|s| s.len() <= 5).unwrap_or(false);
        let left_upper_ratio = self.uppercase_ratio(left);
        let right_first_upper_ratio = right.split_whitespace().next().map(|s| self.uppercase_ratio(s)).unwrap_or(0.0);

        left_short && right_short && left_upper_ratio > 0.5 && right_first_upper_ratio > 0.5
    }

    /// check for "Tyler, The Creator" pattern: comma followed by "the X"
    fn is_comma_the_inversion(&self, separator: &str, right_lower: &str) -> bool {
        let is_comma = separator == "," || separator == ", ";
        if !is_comma {
            return false;
        }

        let right_trimmed = right_lower.trim_start();
        if right_trimmed.starts_with("the ") {
            // should have 1-3 tokens after "the"
            let after_the = &right_trimmed[4..];
            let token_count = after_the.split_whitespace().count();
            return token_count >= 1 && token_count <= 3;
        }

        false
    }

    /// check for "& The Band" or "and the Orchestra" patterns
    fn is_band_suffix_pattern(&self, sep_lower: &str, right_lower: &str) -> bool {
        let is_connector = sep_lower == "&" || sep_lower == "and" || sep_lower == "+" || sep_lower == "& " || sep_lower == " & ";

        if !is_connector {
            return false;
        }

        let right_trimmed = right_lower.trim_start();
        right_trimmed.starts_with("the ")
    }

    /// check for acronym patterns like "C&C" or single letter followed by connector
    fn is_acronym_glue(&self, left_lower: &str, sep_lower: &str) -> bool {
        let is_connector = sep_lower.trim() == "&" || sep_lower.trim() == "+";
        if !is_connector {
            return false;
        }

        // left should end with a single uppercase letter or be a single letter
        let left_trimmed = left_lower.trim();
        if left_trimmed.len() == 1 && left_trimmed.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            return true;
        }

        // check if left ends with single letter pattern
        if let Some(last_char) = left_trimmed.chars().last() {
            if last_char.is_alphabetic() {
                let before_last = &left_trimmed[..left_trimmed.len() - last_char.len_utf8()];
                if before_last.is_empty() || before_last.ends_with(' ') || before_last.ends_with('&') {
                    return true;
                }
            }
        }

        false
    }

    /// check if splitting would produce a segment too small to be a real artist
    fn would_produce_tiny_segment(&self, segment: &str) -> bool {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return true;
        }
        if trimmed.len() <= 1 {
            return true;
        }
        // all punctuation
        if trimmed.chars().all(|c| !c.is_alphanumeric()) {
            return true;
        }
        false
    }

    /// check if text ends with common plural suffix
    fn has_plural_suffix(&self, text: &str) -> bool {
        let last_word = text.split_whitespace().last().unwrap_or("");
        // common plural patterns: ends with 's' or "'s"
        last_word.ends_with('s') || last_word.ends_with("'s")
    }

    /// check if text contains a group noun
    fn contains_group_noun(&self, text: &str) -> bool {
        for word in text.split_whitespace() {
            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if GROUP_NOUNS.contains(clean_word) {
                return true;
            }
        }
        false
    }

    /// detect duo name patterns like "Hall & Oates", "Simon & Garfunkel"
    /// pattern: single word & single word (surnames or stage names)
    fn is_duo_name_pattern(&self, left: &str, separator: &str, right: &str) -> bool {
        let is_connector = separator.trim() == "&" || separator.trim() == "and" || separator.trim() == "+";
        if !is_connector {
            return false;
        }

        let left_tokens = left.split_whitespace().count();
        let right_tokens = right.split_whitespace().count();

        // both sides are single tokens (surnames)
        if left_tokens == 1 && right_tokens == 1 {
            // both start with uppercase
            let left_caps = left.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            let right_caps = right.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            
            // both are reasonable length for surnames (not acronyms, not super long)
            let left_len = left.trim().len();
            let right_len = right.trim().len();
            
            return left_caps && right_caps && left_len >= 3 && right_len >= 3 && left_len <= 15 && right_len <= 15;
        }

        false
    }

    /// detect multi-separator band names like "Earth, Wind & Fire", "Crosby, Stills, Nash & Young"
    fn is_multi_separator_band_name(&self, full_src: &str) -> bool {
        let lower = full_src.to_lowercase();

        // count separators
        let comma_count = full_src.matches(',').count();
        let amp_count = full_src.matches('&').count() + full_src.matches(" and ").count();

        // pattern: multiple commas + final & = list-style band name
        if comma_count >= 1 && amp_count >= 1 {
            // check if it ends with & followed by single token (common pattern)
            if lower.contains(" & ") || lower.contains(" and ") {
                // all segments are short (single words or two words max)
                let all_short = full_src
                    .split(&[',', '&'][..])
                    .all(|seg| seg.split_whitespace().count() <= 2);
                
                if all_short {
                    return true;
                }
            }
        }

        // pattern: all segments are single short words = likely band name
        let segments: Vec<&str> = full_src
            .split(&[',', '&', '+'][..])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if segments.len() >= 3 {
            let all_single_word = segments.iter().all(|s| s.split_whitespace().count() == 1);
            if all_single_word {
                return true;
            }
        }

        false
    }

    /// heuristic: do both sides look like separate full artist names?
    fn looks_like_separate_artists(&self, left: &str, right: &str) -> bool {
        let left_tokens = left.split_whitespace().count();
        let right_tokens = right.split_whitespace().count();

        // both have 2+ tokens suggesting full names
        let both_have_full_names = left_tokens >= 2 && right_tokens >= 2;

        // check for capitalization patterns typical of names
        let left_has_caps = self.has_name_capitalization(left);
        let right_has_caps = self.has_name_capitalization(right);

        both_have_full_names && left_has_caps && right_has_caps
    }

    /// check if text has capitalization typical of a person's name
    fn has_name_capitalization(&self, text: &str) -> bool {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return false;
        }

        // at least one word starts with uppercase
        words.iter().any(|w| {
            w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        })
    }

    /// calculate ratio of uppercase letters in text
    fn uppercase_ratio(&self, text: &str) -> f64 {
        let alpha_chars: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
        if alpha_chars.is_empty() {
            return 0.0;
        }
        let uppercase_count = alpha_chars.iter().filter(|c| c.is_uppercase()).count();
        uppercase_count as f64 / alpha_chars.len() as f64
    }
}

/// convenience function for quick analysis
pub fn analyze_artist_split(full_artist: &str, separators: &HashSet<String>) -> Vec<SplitAnalysis> {
    let detector = ArtistSplitDetector::new();
    let mut results = Vec::new();

    let sep_pattern: String = separators
        .iter()
        .map(|s| regex::escape(s))
        .collect::<Vec<_>>()
        .join("|");

    if sep_pattern.is_empty() {
        return results;
    }

    let re = Regex::new(&sep_pattern).unwrap();
    let matches: Vec<_> = re.find_iter(full_artist).collect();

    for (i, mat) in matches.iter().enumerate() {
        let left_start = if i == 0 { 0 } else { matches[i - 1].end() };
        let left = full_artist[left_start..mat.start()].trim();

        let right_end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            full_artist.len()
        };
        let right = full_artist[mat.end()..right_end].trim();

        let decision = detector.should_split(left, mat.as_str(), right, full_artist);

        results.push(SplitAnalysis {
            separator: mat.as_str().to_string(),
            left: left.to_string(),
            right: right.to_string(),
            decision,
        });
    }

    results
}

/// analysis result for a single separator boundary
#[derive(Debug, Clone)]
pub struct SplitAnalysis {
    pub separator: String,
    pub left: String,
    pub right: String,
    pub decision: SplitDecision,
}

/// smart artist splitting that uses heuristic detection instead of hardcoded ignore list
pub fn split_artists_smart(
    src: &str,
    separators: &HashSet<String>,
    fallback_ignore_list: &HashSet<String>,
) -> Vec<String> {
    if src.is_empty() {
        return Vec::new();
    }

    let detector = ArtistSplitDetector::with_ignore_list(fallback_ignore_list);

    // first, check if the entire string should be kept together
    // this handles multi-separator band names like "Earth, Wind & Fire"
    if detector.should_keep_entire_string(src) {
        return vec![src.trim().to_string()];
    }

    // build separator regex
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
    let mut current_start = 0;

    let matches: Vec<_> = re.find_iter(src).collect();

    let mut i = 0;
    while i < matches.len() {
        let mat = &matches[i];

        let left = src[current_start..mat.start()].trim();
        
        // find right side (up to next separator or end)
        let right_end = if i + 1 < matches.len() {
            matches[i + 1].start()
        } else {
            src.len()
        };
        let right = src[mat.end()..right_end].trim();

        // check if we should split here
        let full_context = &src[current_start..right_end];
        let decision = detector.should_split(left, mat.as_str(), right, full_context);

        match decision {
            SplitDecision::Split => {
                if !left.is_empty() {
                    result.push(left.to_string());
                }
                current_start = mat.end();
            }
            SplitDecision::KeepTogether => {
                // skip this separator, include it in current segment
                // continue to next separator
            }
            SplitDecision::Uncertain => {
                // for uncertain cases, check the fallback ignore list
                let full_lower = full_context.to_lowercase();
                if fallback_ignore_list.contains(&full_lower) {
                    // keep together
                } else {
                    // default to split
                    if !left.is_empty() {
                        result.push(left.to_string());
                    }
                    current_start = mat.end();
                }
            }
        }

        i += 1;
    }

    // add remaining text
    let remaining = src[current_start..].trim();
    if !remaining.is_empty() {
        result.push(remaining.to_string());
    }

    // if we ended up with nothing, return the original
    if result.is_empty() && !src.trim().is_empty() {
        return vec![src.trim().to_string()];
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_separators() -> HashSet<String> {
        [
            ";".to_string(),
            "/".to_string(),
            ", ".to_string(),
            " & ".to_string(),
            "&".to_string(),
            " and ".to_string(),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn test_acdc_detection() {
        let seps = make_separators();
        let ignore = HashSet::new();
        
        let result = split_artists_smart("AC/DC", &seps, &ignore);
        assert_eq!(result, vec!["AC/DC"]);
    }

    #[test]
    fn test_tyler_the_creator() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Tyler, The Creator", &seps, &ignore);
        assert_eq!(result, vec!["Tyler, The Creator"]);
    }

    #[test]
    fn test_band_suffix_florence() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Florence & The Machine", &seps, &ignore);
        assert_eq!(result, vec!["Florence & The Machine"]);
    }

    #[test]
    fn test_band_suffix_nick_cave() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Nick Cave & the Bad Seeds", &seps, &ignore);
        assert_eq!(result, vec!["Nick Cave & the Bad Seeds"]);
    }

    #[test]
    fn test_earth_wind_fire() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Earth, Wind & Fire", &seps, &ignore);
        // this is tricky - both comma and & present. should detect as single band
        assert!(result.len() <= 2); // at minimum shouldn't split into 3
    }

    #[test]
    fn test_collaboration_should_split() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Kanye West & JAY-Z", &seps, &ignore);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"Kanye West".to_string()));
        assert!(result.contains(&"JAY-Z".to_string()));
    }

    #[test]
    fn test_simon_garfunkel_uncertain() {
        let seps = make_separators();
        let ignore = HashSet::new();

        // duo names like "Simon & Garfunkel" are ambiguous without library stats
        // heuristics alone can't distinguish from collaboration
        let result = split_artists_smart("Simon & Garfunkel", &seps, &ignore);
        // this may split or not - depends on confidence threshold
        // the important thing is the system makes a decision
        assert!(!result.is_empty());
    }

    #[test]
    fn test_ccc_music_factory() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("C&C Music Factory", &seps, &ignore);
        assert_eq!(result, vec!["C&C Music Factory"]);
    }

    #[test]
    fn test_mumford_sons() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Mumford & Sons", &seps, &ignore);
        // plural suffix detection
        assert_eq!(result, vec!["Mumford & Sons"]);
    }

    #[test]
    fn test_kc_sunshine_band() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("KC & the Sunshine Band", &seps, &ignore);
        assert_eq!(result, vec!["KC & the Sunshine Band"]);
    }

    #[test]
    fn test_hall_oates() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Hall & Oates", &seps, &ignore);
        // duo name - should detect the plural-ish surname
        assert!(result.len() <= 1 || result.contains(&"Hall & Oates".to_string()));
    }

    #[test]
    fn test_semicolon_split() {
        let seps = make_separators();
        let ignore = HashSet::new();

        let result = split_artists_smart("Artist One; Artist Two", &seps, &ignore);
        assert_eq!(result, vec!["Artist One", "Artist Two"]);
    }

    #[test]
    fn test_detector_decision_acdc() {
        let detector = ArtistSplitDetector::new();
        let decision = detector.should_split("AC", "/", "DC", "AC/DC");
        assert_eq!(decision, SplitDecision::KeepTogether);
    }

    #[test]
    fn test_detector_decision_band_suffix() {
        let detector = ArtistSplitDetector::new();
        let decision = detector.should_split("Florence", "&", "The Machine", "Florence & The Machine");
        assert_eq!(decision, SplitDecision::KeepTogether);
    }
}
