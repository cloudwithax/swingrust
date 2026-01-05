//! Tag writer - write metadata back to audio files

use anyhow::Result;
use lofty::{Accessor, ItemKey, Probe, Tag, TagExt, TagType, TaggedFileExt};
use std::path::Path;

/// Tag writer for updating audio file metadata
pub struct Tagger;

impl Tagger {
    /// Write tags to a file
    pub fn write_tags(
        path: &Path,
        title: Option<&str>,
        album: Option<&str>,
        artist: Option<&str>,
        album_artist: Option<&str>,
        track: Option<u32>,
        disc: Option<u32>,
        year: Option<i32>,
        genre: Option<&str>,
    ) -> Result<()> {
        let mut tagged_file = Probe::open(path)?.read()?;

        // Get or create primary tag
        let tag = match tagged_file.primary_tag_mut() {
            Some(t) => t,
            None => {
                // Try to determine best tag type for format
                let tag_type = Self::get_tag_type(&tagged_file);
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        };

        if let Some(t) = title {
            tag.set_title(t.to_string());
        }

        if let Some(a) = album {
            tag.set_album(a.to_string());
        }

        if let Some(a) = artist {
            tag.set_artist(a.to_string());
        }

        if let Some(aa) = album_artist {
            tag.insert_text(ItemKey::AlbumArtist, aa.to_string());
        }

        if let Some(t) = track {
            tag.set_track(t);
        }

        if let Some(d) = disc {
            tag.set_disk(d);
        }

        if let Some(y) = year {
            tag.set_year(y as u32);
        }

        if let Some(g) = genre {
            tag.set_genre(g.to_string());
        }

        // Save changes
        tag.save_to_path(path)?;

        Ok(())
    }

    /// Get best tag type for file format
    fn get_tag_type(file: &lofty::TaggedFile) -> TagType {
        match file.file_type() {
            lofty::FileType::Mpeg => TagType::Id3v2,
            lofty::FileType::Flac => TagType::VorbisComments,
            lofty::FileType::Opus => TagType::VorbisComments,
            lofty::FileType::Mp4 => TagType::Mp4Ilst,
            lofty::FileType::Aiff => TagType::Id3v2,
            lofty::FileType::Wav => TagType::Id3v2,
            _ => TagType::Id3v2,
        }
    }

    /// Read embedded cover art
    pub fn read_cover(path: &Path) -> Result<Option<Vec<u8>>> {
        let tagged_file = Probe::open(path)?.read()?;

        if let Some(tag) = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())
        {
            for picture in tag.pictures() {
                if picture.pic_type() == lofty::PictureType::CoverFront {
                    return Ok(Some(picture.data().to_vec()));
                }
            }
            // Return any picture if no front cover found
            if let Some(picture) = tag.pictures().first() {
                return Ok(Some(picture.data().to_vec()));
            }
        }

        Ok(None)
    }

    /// Write cover art to file
    pub fn write_cover(path: &Path, image_data: &[u8], mime_type: &str) -> Result<()> {
        let mut tagged_file = Probe::open(path)?.read()?;

        let tag = match tagged_file.primary_tag_mut() {
            Some(t) => t,
            None => {
                let tag_type = Self::get_tag_type(&tagged_file);
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        };

        let mime = match mime_type {
            "image/jpeg" | "jpeg" | "jpg" => lofty::MimeType::Jpeg,
            "image/png" | "png" => lofty::MimeType::Png,
            "image/gif" | "gif" => lofty::MimeType::Gif,
            "image/bmp" | "bmp" => lofty::MimeType::Bmp,
            _ => lofty::MimeType::Unknown(mime_type.to_string()),
        };

        let picture = lofty::Picture::new_unchecked(
            lofty::PictureType::CoverFront,
            Some(mime),
            None,
            image_data.to_vec(),
        );

        // Remove existing front covers
        tag.remove_picture_type(lofty::PictureType::CoverFront);

        // Add new cover
        tag.push_picture(picture);

        // Save
        tag.save_to_path(path)?;

        Ok(())
    }

    /// Set title tag
    pub fn set_title(&self, path: &Path, title: &str) -> Result<()> {
        Self::write_tags(path, Some(title), None, None, None, None, None, None, None)
    }

    /// Set artist tag
    pub fn set_artist(&self, path: &Path, artist: &str) -> Result<()> {
        Self::write_tags(path, None, None, Some(artist), None, None, None, None, None)
    }

    /// Set album tag
    pub fn set_album(&self, path: &Path, album: &str) -> Result<()> {
        Self::write_tags(path, None, Some(album), None, None, None, None, None, None)
    }

    /// Set genre tag
    pub fn set_genre(&self, path: &Path, genre: &str) -> Result<()> {
        Self::write_tags(path, None, None, None, None, None, None, None, Some(genre))
    }

    /// Set year tag
    pub fn set_year(&self, path: &Path, year: u32) -> Result<()> {
        Self::write_tags(
            path,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(year as i32),
            None,
        )
    }

    /// Get all tags from file
    pub fn read_all_tags(path: &Path) -> Result<std::collections::HashMap<String, String>> {
        let tagged_file = Probe::open(path)?.read()?;
        let mut tags = std::collections::HashMap::new();

        if let Some(tag) = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())
        {
            if let Some(t) = tag.title() {
                tags.insert("title".to_string(), t.to_string());
            }
            if let Some(a) = tag.album() {
                tags.insert("album".to_string(), a.to_string());
            }
            if let Some(a) = tag.artist() {
                tags.insert("artist".to_string(), a.to_string());
            }
            if let Some(g) = tag.genre() {
                tags.insert("genre".to_string(), g.to_string());
            }
            if let Some(t) = tag.track() {
                tags.insert("track".to_string(), t.to_string());
            }
            if let Some(d) = tag.disk() {
                tags.insert("disc".to_string(), d.to_string());
            }
            if let Some(y) = tag.year() {
                tags.insert("year".to_string(), y.to_string());
            }
            if let Some(aa) = tag.get_string(&ItemKey::AlbumArtist) {
                tags.insert("album_artist".to_string(), aa.to_string());
            }
        }

        Ok(tags)
    }
}
