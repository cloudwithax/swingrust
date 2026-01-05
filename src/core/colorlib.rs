//! Color extraction from images

use anyhow::Result;
use image::GenericImageView;
use std::path::Path;

/// Color library for extracting dominant colors from images
pub struct ColorLib;

impl ColorLib {
    /// Extract dominant color from image file
    pub fn extract_dominant(image_path: &Path) -> Result<String> {
        let img = image::open(image_path)?;

        // Resize for faster processing
        let thumbnail = img.thumbnail(100, 100);

        // Sample colors
        let mut colors: Vec<(u8, u8, u8)> = Vec::new();

        for (_, _, pixel) in thumbnail.pixels() {
            let rgba = pixel.0;
            colors.push((rgba[0], rgba[1], rgba[2]));
        }

        // Find dominant color using k-means-like approach
        let dominant = Self::find_dominant_color(&colors);

        Ok(Self::rgb_to_hex(dominant))
    }

    /// Extract dominant color from image bytes
    pub fn extract_from_bytes(data: &[u8]) -> Result<String> {
        let img = image::load_from_memory(data)?;

        // Resize for faster processing
        let thumbnail = img.thumbnail(100, 100);

        // Sample colors
        let mut colors: Vec<(u8, u8, u8)> = Vec::new();

        for (_, _, pixel) in thumbnail.pixels() {
            let rgba = pixel.0;
            colors.push((rgba[0], rgba[1], rgba[2]));
        }

        let dominant = Self::find_dominant_color(&colors);

        Ok(Self::rgb_to_hex(dominant))
    }

    /// Find dominant color from list of colors
    fn find_dominant_color(colors: &[(u8, u8, u8)]) -> (u8, u8, u8) {
        if colors.is_empty() {
            return (128, 128, 128);
        }

        // Filter out very dark and very light colors
        let filtered: Vec<_> = colors
            .iter()
            .filter(|(r, g, b)| {
                let brightness = (*r as f32 + *g as f32 + *b as f32) / 3.0;
                brightness > 30.0 && brightness < 225.0
            })
            .cloned()
            .collect();

        let colors_to_use = if filtered.is_empty() {
            colors
        } else {
            &filtered
        };

        // Simple average for now (could be improved with k-means)
        let mut sum_r: u64 = 0;
        let mut sum_g: u64 = 0;
        let mut sum_b: u64 = 0;

        for (r, g, b) in colors_to_use {
            sum_r += *r as u64;
            sum_g += *g as u64;
            sum_b += *b as u64;
        }

        let count = colors_to_use.len() as u64;

        (
            (sum_r / count) as u8,
            (sum_g / count) as u8,
            (sum_b / count) as u8,
        )
    }

    /// Convert RGB to hex string
    pub fn rgb_to_hex(rgb: (u8, u8, u8)) -> String {
        format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
    }

    /// Convert hex string to RGB
    pub fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
        let hex = hex.trim_start_matches('#');

        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some((r, g, b))
    }

    /// Calculate color brightness (0-255)
    pub fn brightness(hex: &str) -> u8 {
        if let Some((r, g, b)) = Self::hex_to_rgb(hex) {
            ((r as u16 + g as u16 + b as u16) / 3) as u8
        } else {
            128
        }
    }

    /// Check if color is dark
    pub fn is_dark(hex: &str) -> bool {
        Self::brightness(hex) < 128
    }

    /// Get contrasting text color (black or white)
    pub fn get_text_color(bg_hex: &str) -> String {
        if Self::is_dark(bg_hex) {
            "#ffffff".to_string()
        } else {
            "#000000".to_string()
        }
    }

    /// Lighten a color
    pub fn lighten(hex: &str, amount: f32) -> String {
        if let Some((r, g, b)) = Self::hex_to_rgb(hex) {
            let r = (r as f32 + (255.0 - r as f32) * amount).min(255.0) as u8;
            let g = (g as f32 + (255.0 - g as f32) * amount).min(255.0) as u8;
            let b = (b as f32 + (255.0 - b as f32) * amount).min(255.0) as u8;
            Self::rgb_to_hex((r, g, b))
        } else {
            hex.to_string()
        }
    }

    /// Darken a color
    pub fn darken(hex: &str, amount: f32) -> String {
        if let Some((r, g, b)) = Self::hex_to_rgb(hex) {
            let r = (r as f32 * (1.0 - amount)) as u8;
            let g = (g as f32 * (1.0 - amount)) as u8;
            let b = (b as f32 * (1.0 - amount)) as u8;
            Self::rgb_to_hex((r, g, b))
        } else {
            hex.to_string()
        }
    }
}
