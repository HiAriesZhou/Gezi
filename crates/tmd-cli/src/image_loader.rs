//! Bounded image IO and rasterization. Called only by reader worker threads.
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageEncoder, ImageFormat, ImageReader, RgbaImage};

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_SIDE: u32 = 2048;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Local(PathBuf),
    Remote(String),
}

pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<RgbaImage>,
}

pub fn source(destination: &str, markdown: &Path, remote: bool) -> Result<Source, &'static str> {
    let destination = destination.trim();
    if destination.is_empty() || destination.chars().any(char::is_control) {
        return Err("invalid image path");
    }
    let address = if destination.starts_with("//") {
        format!("https:{destination}")
    } else {
        destination.to_owned()
    };
    let address = address.split('#').next().unwrap_or_default().to_owned();
    if let Ok(uri) = address.parse::<ureq::http::Uri>()
        && let Some(scheme) = uri.scheme_str()
    {
        if !matches!(scheme, "http" | "https") || !remote {
            return Err("remote images disabled or unsupported scheme");
        }
        if uri
            .authority()
            .is_none_or(|authority| authority.as_str().contains('@'))
        {
            return Err("image URL must not contain credentials");
        }
        return Ok(Source::Remote(address));
    }
    if destination.contains("://")
        || destination.starts_with("data:")
        || destination.starts_with("file:")
    {
        return Err("unsupported image scheme");
    }
    let path = destination.split(['?', '#']).next().unwrap_or_default();
    let mut decoded = Vec::new();
    let mut bytes = path.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let a = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("invalid path escape")?;
            let b = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or("invalid path escape")?;
            decoded.push((a * 16 + b) as u8);
        } else {
            decoded.push(byte);
        }
    }
    let path = String::from_utf8(decoded).map_err(|_| "invalid path encoding")?;
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err("invalid image path");
    }
    Ok(Source::Local(
        markdown.parent().unwrap_or(Path::new(".")).join(path),
    ))
}

pub fn load(source: &Source) -> Result<Raster, String> {
    let bytes = match source {
        Source::Local(path) => {
            // Refuse devices/FIFOs so an image cannot keep a worker blocked forever.
            if !std::fs::metadata(path)
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err("not a regular image file".into());
            }
            read_limited(std::fs::File::open(path).map_err(|e| e.to_string())?)?
        }
        Source::Remote(url) => {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(8)))
                .max_redirects(3)
                .user_agent("tmd image reader")
                .build()
                .into();
            let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
            read_limited(response.body_mut().as_reader())?
        }
    };
    decode(&bytes)
}

fn read_limited(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("image exceeds 8 MiB".into());
    }
    Ok(bytes)
}

fn dimensions(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        Err("image dimensions exceed limit".into())
    } else {
        Ok(())
    }
}

pub fn decode(bytes: &[u8]) -> Result<Raster, String> {
    if bytes.len() > MAX_BYTES {
        return Err("image exceeds 8 MiB".into());
    }
    let (pixels, width, height) = match image::guess_format(bytes) {
        Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg)) => {
            let reader = ImageReader::with_format(Cursor::new(bytes), format);
            let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
            dimensions(w, h)?;
            let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(128 * 1024 * 1024);
            limits.max_image_width = Some(16384);
            limits.max_image_height = Some(16384);
            reader.limits(limits);
            let image = reader
                .decode()
                .map_err(|e| e.to_string())?
                .thumbnail(MAX_SIDE, MAX_SIDE);
            let (width, height) = (image.width(), image.height());
            (image.to_rgba8(), width, height)
        }
        Ok(_) => return Err("only PNG, JPEG and static SVG are supported".into()),
        Err(_) => {
            let (png, width, height) = svg(bytes)?;
            let pixels = image::load_from_memory(&png)
                .map_err(|e| e.to_string())?
                .to_rgba8();
            (pixels, width, height)
        }
    };
    Ok(Raster {
        width,
        height,
        pixels: Arc::new(pixels),
    })
}

pub fn prepare(raster: &Raster, width: u32, height: u32) -> Result<String, String> {
    dimensions(width, height)?;
    let resized;
    let pixels = if width == raster.width && height == raster.height {
        raster.pixels.as_ref()
    } else {
        resized = image::imageops::resize(
            raster.pixels.as_ref(),
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
        &resized
    };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            pixels.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    if png.len() > MAX_BYTES {
        return Err("raster exceeds transfer limit".into());
    }
    Ok(STANDARD.encode(png))
}

fn svg(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    // Bound parser work before constructing an SVG tree. Scripts and external
    // resources are never evaluated; embedded images are deliberately omitted.
    if bytes.starts_with(b"\x1f\x8b")
        || bytes
            .windows(9)
            .any(|part| part.eq_ignore_ascii_case(b"<!DOCTYPE"))
        || bytes.len() > 2 * 1024 * 1024
        || bytes.iter().filter(|&&b| b == b'<').count() > 20000
    {
        return Err("SVG exceeds complexity limit".into());
    }
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        // Load a small trusted system font set, rather than scanning user fonts.
        for path in [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/PingFang.ttc",
        ] {
            let _ = db.load_font_file(path);
        }
        db.set_sans_serif_family("Arial");
        Arc::new(db)
    });
    let options = resvg::usvg::Options {
        fontdb: fonts.clone(),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = (MAX_SIDE as f32 / size.width().max(size.height())).min(1.0);
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    dimensions(width, height)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("invalid SVG size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    Ok((png, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_use_markdown_directory_and_decode_after_query_removal() {
        let md = Path::new("/tmp/docs/README.md");
        assert_eq!(
            source("assets/a%20b.png?v=2", md, false).unwrap(),
            Source::Local("/tmp/docs/assets/a b.png".into())
        );
        assert!(source("https://example.test/badge", md, false).is_err());
        assert!(source("https://user@example.test/image", md, true).is_err());
        for path in [
            "data:abc",
            "file:///etc/file",
            "ftp://example.test/a",
            "%00.png",
            "bad%Q0",
        ] {
            assert!(source(path, md, true).is_err());
        }
    }

    #[test]
    fn sniffs_extensionless_svg_and_rejects_invalid_and_oversized_data() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="90" height="20"><rect width="90" height="20" fill="red"/></svg>"#;
        let raster = decode(svg).unwrap();
        assert_eq!((raster.width, raster.height), (90, 20));
        assert!(
            STANDARD
                .decode(prepare(&raster, 90, 20).unwrap())
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        assert!(decode(b"<html>not an image</html>").is_err());
        assert!(decode(b"\x1f\x8bcompressed svg").is_err());
        assert!(decode(b"<!DOCTYPE svg [<!ENTITY a 'expanded'>]><svg/>").is_err());
        assert!(dimensions(20000, 20000).is_err());
        assert!(read_limited(std::io::repeat(0)).is_err());
    }

    #[test]
    fn png_and_jpeg_decode_to_rgba_and_prepare_an_exact_aspect_ratio() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            120,
            20,
            image::Rgb([20, 100, 200]),
        ));
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let raster = decode(bytes.get_ref()).unwrap();
            let prepared = STANDARD.decode(prepare(&raster, 60, 10).unwrap()).unwrap();
            let image = image::load_from_memory(&prepared).unwrap();
            assert_eq!((image.width(), image.height()), (60, 10));
        }
    }
}
