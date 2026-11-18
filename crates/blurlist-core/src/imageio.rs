//! Reading photos from disk.
//!
//! Every photo is decoded once, rotated according to its EXIF orientation and
//! converted to 8-bit RGB. All further work (detection, blurring, export)
//! happens on these upright pixels, so exported files never depend on an
//! orientation tag that is about to be stripped.

use std::io::BufReader;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbImage};

use crate::error::{Error, Result};

/// Extensions Blurlist can decode, check and export.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "webp", "tif", "tiff", "bmp"];

/// Media we recognise but cannot process. These are listed in scan results so
/// nobody assumes they were checked.
pub const UNSUPPORTED_MEDIA_EXTENSIONS: &[&str] = &[
    "heic", "heif", "avif", "gif", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "srw", "pef", "mp4", "mov",
    "m4v", "avi", "mkv", "webm", "3gp", "mts", "psd",
];

/// What kind of file a path is, judged by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Supported,
    UnsupportedMedia,
    Other,
}

pub fn classify_path(path: &Path) -> FileKind {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return FileKind::Other,
    };
    if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        FileKind::Supported
    } else if UNSUPPORTED_MEDIA_EXTENSIONS.contains(&ext.as_str()) {
        FileKind::UnsupportedMedia
    } else {
        FileKind::Other
    }
}

/// A decoded, upright photo.
pub struct LoadedImage {
    pub rgb: RgbImage,
    /// Container format the file was decoded from.
    pub format: ImageFormat,
    /// Embedded ICC colour profile. Kept on export because it carries colour
    /// information only, never people or places.
    pub icc_profile: Option<Vec<u8>>,
    /// Whether the source had an alpha channel (it is flattened onto white).
    pub had_alpha: bool,
}

/// Decode a photo from disk, applying its EXIF orientation.
pub fn load_image(path: &Path) -> Result<LoadedImage> {
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let reader = ImageReader::new(BufReader::new(file)).with_guessed_format().map_err(|e| Error::io(path, e))?;
    let format = reader.format().ok_or_else(|| Error::UnsupportedImage {
        path: path.to_path_buf(),
        reason: "unrecognised file format".into(),
    })?;
    let mut decoder = reader.into_decoder().map_err(|e| Error::decode(path, e))?;
    let orientation = decoder.orientation().ok();
    let icc_profile = decoder.icc_profile().ok().flatten();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| Error::decode(path, e))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    let had_alpha = img.color().has_alpha();
    let rgb = to_rgb8(img);
    Ok(LoadedImage { rgb, format, icc_profile, had_alpha })
}

/// Decode an in-memory image (used for stored reference crops).
pub fn load_image_bytes(bytes: &[u8]) -> Result<RgbImage> {
    let img = image::load_from_memory(bytes).map_err(|e| Error::Image(e.to_string()))?;
    Ok(to_rgb8(img))
}

/// Convert to RGB8, compositing any transparency onto white so transparent
/// regions don't turn black.
fn to_rgb8(img: DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.into_rgb8();
    }
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbImage::new(w, h);
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        let a = src[3] as u32;
        for c in 0..3 {
            dst[c] = ((src[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        }
    }
    out
}

/// Downscale so the longer side is at most `max_side`, never upscaling.
pub fn fit_within(img: &RgbImage, max_side: u32) -> RgbImage {
    let (w, h) = img.dimensions();
    let long = w.max(h);
    if long <= max_side {
        return img.clone();
    }
    let s = max_side as f32 / long as f32;
    let nw = ((w as f32 * s).round() as u32).max(1);
    let nh = ((h as f32 * s).round() as u32).max(1);
    image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle)
}

/// Encode an RGB image as a baseline JPEG with no metadata at all.
pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity((img.width() * img.height() / 4) as usize);
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
    img.write_with_encoder(enc).map_err(|e| Error::Image(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_extensions() {
        assert_eq!(classify_path(Path::new("a/b/IMG_1.JPG")), FileKind::Supported);
        assert_eq!(classify_path(Path::new("x.heic")), FileKind::UnsupportedMedia);
        assert_eq!(classify_path(Path::new("notes.txt")), FileKind::Other);
        assert_eq!(classify_path(Path::new("README")), FileKind::Other);
    }

    #[test]
    fn fit_within_never_upscales() {
        let img = RgbImage::new(100, 50);
        assert_eq!(fit_within(&img, 400).dimensions(), (100, 50));
        assert_eq!(fit_within(&img, 50).dimensions(), (50, 25));
    }

    #[test]
    fn jpeg_roundtrip_has_no_exif() {
        let img = RgbImage::from_pixel(32, 16, image::Rgb([200, 10, 10]));
        let bytes = encode_jpeg(&img, 90).unwrap();
        assert!(!bytes.windows(4).any(|w| w == b"Exif"));
        let back = load_image_bytes(&bytes).unwrap();
        assert_eq!(back.dimensions(), (32, 16));
    }
}
