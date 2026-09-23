use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader, Limits};

use crate::{fsutil::write_atomic, models::DisplaySettings, protocol::hex_sha256};

/// IntelliJ keeps the decoded image in memory for every frame, so oversized sources are
/// scaled down before they ever reach the IDE.
pub const MAX_EDGE: u32 = 3840;
const MAX_SOURCE_EDGE: u32 = 16384;
const KEEP_RECENT_FILES: usize = 4;

pub struct RenderedImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn render(
    bytes: &[u8],
    mime_type: &str,
    display: &DisplaySettings,
) -> Result<RenderedImage, String> {
    let declared = match mime_type {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/webp" => ImageFormat::WebP,
        "image/gif" => ImageFormat::Gif,
        other => return Err(format!("不支持的图片类型：{other}。")),
    };
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("读取图片失败：{error}"))?;
    match reader.format() {
        Some(actual) if actual == declared => {}
        Some(actual) => {
            return Err(format!(
                "图片实际格式是 {actual:?}，与声明的 {mime_type} 不一致。"
            ))
        }
        None => return Err("无法识别图片格式。".to_string()),
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    let mut reader = reader;
    reader.limits(limits);
    let mut image = reader
        .decode()
        .map_err(|error| format!("解码图片失败：{error}"))?;

    if image.width().max(image.height()) > MAX_EDGE {
        image = image.resize(MAX_EDGE, MAX_EDGE, FilterType::CatmullRom);
    }
    if display.blur > 0.0 {
        image = image.fast_blur(display.blur as f32);
    }
    let mut rgba = image.to_rgba8();
    if display.overlay_opacity > 0.0 {
        let alpha = display.overlay_opacity as f32;
        let [red, green, blue] = display.overlay_color.map(f32::from);
        for pixel in rgba.pixels_mut() {
            for (channel, overlay) in pixel.0.iter_mut().take(3).zip([red, green, blue]) {
                *channel = (f32::from(*channel) * (1.0 - alpha) + overlay * alpha).round() as u8;
            }
        }
    }
    let (width, height) = rgba.dimensions();
    let opaque = rgba.pixels().all(|pixel| pixel.0[3] == u8::MAX);
    let output = if opaque {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
    } else {
        DynamicImage::ImageRgba8(rgba)
    };
    let mut png = Cursor::new(Vec::new());
    output
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| format!("编码 PNG 失败：{error}"))?;
    Ok(RenderedImage {
        png: png.into_inner(),
        width,
        height,
    })
}

/// Content-addressed so a new image always gets a new path; IntelliJ caches decoded
/// wallpapers by path and would otherwise keep showing the previous picture.
pub fn store(media_dir: &Path, png: &[u8]) -> Result<PathBuf, String> {
    let path = media_dir.join(format!("{}.png", hex_sha256(png)));
    if !path.is_file() {
        write_atomic(&path, png)?;
    }
    Ok(path)
}

pub fn prune(media_dir: &Path, keep: &[PathBuf]) {
    let Ok(entries) = fs::read_dir(media_dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_media_file_name(path))
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect();
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in files.into_iter().skip(KEEP_RECENT_FILES) {
        if !keep.contains(&path) {
            let _ = fs::remove_file(path);
        }
    }
}

fn is_media_file_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".png") else {
        return false;
    };
    stem.len() == 64
        && stem
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn png_bytes(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
        let image = RgbImage::from_pixel(width, height, Rgb(color));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn rejects_mismatched_declared_format() {
        let bytes = png_bytes(4, 4, [1, 2, 3]);
        let error = render(&bytes, "image/jpeg", &DisplaySettings::default())
            .err()
            .expect("mismatched format must fail");
        assert!(error.contains("不一致"), "{error}");
    }

    #[test]
    fn applies_overlay_and_keeps_png() {
        let bytes = png_bytes(4, 4, [200, 100, 0]);
        let display = DisplaySettings {
            overlay_color: [0, 0, 0],
            overlay_opacity: 0.5,
            ..DisplaySettings::default()
        };
        let rendered = render(&bytes, "image/png", &display).unwrap();
        let decoded = image::load_from_memory_with_format(&rendered.png, ImageFormat::Png)
            .unwrap()
            .to_rgb8();
        assert_eq!(decoded.get_pixel(0, 0).0, [100, 50, 0]);
    }

    #[test]
    fn downsizes_large_sources() {
        let bytes = png_bytes(MAX_EDGE * 2, 10, [9, 9, 9]);
        let rendered = render(&bytes, "image/png", &DisplaySettings::default()).unwrap();
        assert_eq!(rendered.width, MAX_EDGE);
        assert!(rendered.height <= 10);
    }

    #[test]
    fn stores_by_content_and_prunes_old_files() {
        let root = std::env::temp_dir().join(format!("rebased-media-{}", uuid::Uuid::new_v4()));
        let mut stored = Vec::new();
        for shade in 0..6u8 {
            let png = png_bytes(2, 2, [shade, shade, shade]);
            let path = store(&root, &png).unwrap();
            assert!(is_media_file_name(&path));
            assert_eq!(store(&root, &png).unwrap(), path);
            stored.push(path);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        prune(&root, &[stored[0].clone()]);
        assert!(stored[0].exists(), "explicitly kept file must survive");
        assert!(!stored[1].exists());
        assert!(stored[5].exists());
        let _ = fs::remove_dir_all(root);
    }
}
