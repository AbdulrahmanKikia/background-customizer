//! Image decode for still wallpapers.
//!
//! Video decode arrives in a later milestone.

use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, ImageError};

/// RGBA8 pixels ready for a GPU texture upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(4))
            .ok_or_else(|| "image dimensions overflow".to_string())?;
        if pixels.len() != expected {
            return Err(format!(
                "expected {expected} RGBA bytes, got {}",
                pixels.len()
            ));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Scale with a cover fit so the image fills `target_width` x `target_height`.
    pub fn resized_to_cover(&self, target_width: u32, target_height: u32) -> Result<Self, String> {
        if target_width == 0 || target_height == 0 {
            return Err("target size must be non-zero".to_string());
        }
        if self.width == target_width && self.height == target_height {
            return Ok(self.clone());
        }
        let source = DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
                .ok_or_else(|| "RGBA buffer does not match the image size".to_string())?,
        );
        let filled = source.resize_to_fill(target_width, target_height, FilterType::Triangle);
        let rgba = filled.to_rgba8();
        Self::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw())
    }
}

/// Decode an image file into RGBA8.
pub fn decode_path(path: &Path) -> Result<RgbaImage, String> {
    let decoded = image::open(path).map_err(format_image_error)?;
    let rgba = decoded.to_rgba8();
    RgbaImage::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw())
}

/// Decode image bytes into RGBA8.
pub fn decode_bytes(bytes: &[u8]) -> Result<RgbaImage, String> {
    let decoded = image::load_from_memory(bytes).map_err(format_image_error)?;
    let rgba = decoded.to_rgba8();
    RgbaImage::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw())
}

fn format_image_error(error: ImageError) -> String {
    format!("failed to decode image: {error}")
}

#[cfg(test)]
mod tests {
    use super::{decode_bytes, RgbaImage};
    use image::{ImageBuffer, ImageFormat, Rgb};

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&rgba);
        }
        RgbaImage::from_rgba8(width, height, pixels).expect("solid")
    }

    #[test]
    fn cover_resize_fills_the_target() {
        let image = solid(100, 50, [1, 2, 3, 255]);
        let fitted = image.resized_to_cover(40, 40).expect("resize");
        assert_eq!((fitted.width, fitted.height), (40, 40));
        assert_eq!(fitted.pixels.len(), 40 * 40 * 4);
    }

    #[test]
    fn decode_bytes_reads_a_png() {
        let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_pixel(2, 2, Rgb([255, 0, 0]));
        let mut bytes = Vec::new();
        rgb.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode");
        let image = decode_bytes(&bytes).expect("png");
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(&image.pixels[..4], &[255, 0, 0, 255]);
    }
}
