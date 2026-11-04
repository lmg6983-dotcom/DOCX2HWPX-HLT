//! HLT 그림 다듬기. HLT 는 그림을 비압축 TIFF 로 넣고 작은 창(4KB)으로만 압축하므로, JPEG 잔물결이
//! 그대로 용량이 된다. 눈에 보이지 않는 값만 정리한다: 인쇄 해상도를 넘는 화소, 흰 바탕 · 회색의
//! 잔물결, 채널 하위 2비트. 수식 · 글자 그림(`d2h_*`)은 손대지 않는다.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};

use crate::xml::{self, Element};

/// 기본 상한. 도면을 실제 크기로 넣었을 때 인쇄 품질이 유지되는 값.
pub const DEFAULT_MAX_DPI: u32 = 300;
const EMU_PER_INCH: f64 = 914_400.0;
/// 이 값 이상이면 흰 바탕으로 본다.
const WHITE_FROM: u8 = 245;
/// 채널 차이가 이 값 이하면 회색으로 본다.
const GRAY_SPREAD: u8 = 8;

const DOCUMENT: &str = "word/document.xml";
const RELS: &str = "word/_rels/document.xml.rels";

/// 그림 하나: 흰 바탕과 회색을 정리하고 채널을 6비트로 줄인다. 모두 회색이면 회색 그림으로.
pub fn clean(rgb: &RgbImage) -> DynamicImage {
    let mut out = rgb.clone();
    let mut all_gray = true;
    for pixel in out.pixels_mut() {
        let [r, g, b] = pixel.0;
        let (lo, hi) = (r.min(g).min(b), r.max(g).max(b));
        pixel.0 = if lo >= WHITE_FROM {
            [255, 255, 255]
        } else if hi - lo <= GRAY_SPREAD {
            let v = quantize(((u16::from(r) + u16::from(g) + u16::from(b)) / 3) as u8);
            [v, v, v]
        } else {
            all_gray = false;
            [quantize(r), quantize(g), quantize(b)]
        };
    }
    if all_gray {
        let (w, h) = out.dimensions();
        let luma: Vec<u8> = out.pixels().map(|p| p.0[0]).collect();
        if let Some(gray) = GrayImage::from_raw(w, h, luma) {
            return DynamicImage::ImageLuma8(gray);
        }
    }
    DynamicImage::ImageRgb8(out)
}

/// 하위 2비트를 버리되 255 는 255 로 남긴다(`v | v >> 6`).
fn quantize(v: u8) -> u8 {
    (v & 0xFC) | (v >> 6)
}

/// 표시 크기(EMU)에서 `max_dpi` 를 넘는 그림의 화소 크기. 넘지 않으면 `None`.
fn resized(width: u32, height: u32, cx: u64, max_dpi: u32) -> Option<(u32, u32)> {
    let limit = (cx as f64 / EMU_PER_INCH * f64::from(max_dpi)).ceil() as u32;
    if limit == 0 || width <= limit {
        return None;
    }
    let h = (f64::from(height) * f64::from(limit) / f64::from(width)).round() as u32;
    Some((limit, h.max(1)))
}

fn extents(el: &Element, parts: &HashMap<String, String>, out: &mut HashMap<String, u64>) {
    if matches!(el.name.as_str(), "wp:inline" | "wp:anchor") {
        let cx = el
            .first("wp:extent")
            .and_then(|e| e.get("cx"))
            .and_then(|v| v.parse::<u64>().ok());
        let rid = el.find("a:blip").and_then(|b| b.get("r:embed"));
        if let (Some(cx), Some(part)) = (cx, rid.and_then(|r| parts.get(r))) {
            let best = out.entry(part.clone()).or_insert(0);
            *best = (*best).max(cx);
        }
    }
    for child in el.elements() {
        extents(child, parts, out);
    }
}

/// 정리한 DOCX 와 다듬은 그림 수. 읽지 못하는 그림은 그대로 둔다.
pub fn optimize(docx: &[u8], max_dpi: u32) -> Result<(Vec<u8>, usize), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(docx)).map_err(|e| e.to_string())?;
    let mut parts = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        parts.push((file.name().to_string(), bytes));
    }
    let get = |name: &str| {
        parts
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.as_slice())
    };
    let rels = xml::parse(get(RELS).ok_or("관계 파일이 없습니다")?)?;
    let targets: HashMap<String, String> = rels
        .elements()
        .filter_map(|r| {
            let target = r.get("Target")?;
            Some((
                r.get("Id")?.to_string(),
                format!(
                    "word/{}",
                    target.trim_start_matches('/').trim_start_matches("word/")
                ),
            ))
        })
        .collect();
    let document = xml::parse(get(DOCUMENT).ok_or("본문이 없습니다")?)?;
    let mut widths = HashMap::new();
    extents(&document, &targets, &mut widths);

    let mut count = 0;
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in &parts {
        let cx = widths.get(name).copied();
        let rendered = name
            .rsplit('/')
            .next()
            .is_some_and(|n| n.starts_with("d2h_"));
        let new = match cx {
            Some(cx) if !rendered => shrink(bytes, cx, max_dpi),
            _ => None,
        };
        count += usize::from(new.is_some());
        out.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        out.write_all(new.as_deref().unwrap_or(bytes))
            .map_err(|e| e.to_string())?;
    }
    Ok((out.finish().map_err(|e| e.to_string())?.into_inner(), count))
}

fn shrink(bytes: &[u8], cx: u64, max_dpi: u32) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let decoded = match resized(decoded.width(), decoded.height(), cx, max_dpi) {
        Some((w, h)) => decoded.resize_exact(w, h, FilterType::Lanczos3),
        None => decoded,
    };
    let mut png = Vec::new();
    clean(&decoded.to_rgb8())
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn near_white_and_gray_become_exact() {
        let mut im = RgbImage::from_pixel(2, 1, Rgb([250, 247, 252]));
        im.put_pixel(1, 0, Rgb([100, 104, 98]));
        let out = clean(&im);
        assert!(matches!(out, DynamicImage::ImageLuma8(_)));
        let luma = out.to_luma8();
        assert_eq!(luma.get_pixel(0, 0).0, [255]);
        assert_eq!(luma.get_pixel(1, 0).0, [quantize(100)]);
    }

    #[test]
    fn color_stays_color_within_three_levels() {
        let im = RgbImage::from_pixel(1, 1, Rgb([200, 90, 30]));
        let out = clean(&im);
        assert!(matches!(out, DynamicImage::ImageRgb8(_)));
        let p = out.to_rgb8().get_pixel(0, 0).0;
        for (a, b) in p.iter().zip([200_u8, 90, 30]) {
            assert!(a.abs_diff(b) <= 3);
        }
    }

    #[test]
    fn only_oversized_pictures_shrink() {
        let inch = EMU_PER_INCH as u64;
        assert_eq!(resized(1500, 750, inch * 5, 300), None);
        assert_eq!(resized(6000, 3000, inch * 5, 300), Some((1500, 750)));
    }

    #[test]
    fn quantize_keeps_black_and_white() {
        assert_eq!(quantize(0), 0);
        assert_eq!(quantize(255), 255);
    }
}
