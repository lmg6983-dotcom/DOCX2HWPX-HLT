//! 고쳐 쓴 DOCX(수식은 이미 그림) → HWPX. 쓰기는 HwpForge 가 한다.
//! 표는 칸 합치기(gridSpan · vMerge) · 칸 안 여러 문단 · 표 안 표까지 옮긴다.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use hwpforge_core::document::Document;
use hwpforge_core::image::{Image, ImageStore};
use hwpforge_core::paragraph::Paragraph;
use hwpforge_core::run::Run;
use hwpforge_core::section::Section;
use hwpforge_core::table::{Table, TableCell, TableRow};
use hwpforge_core::{PageSettings, ParagraphMeasure};
use hwpforge_foundation::{
    Alignment, CharShapeIndex, HwpUnit, ParaShapeIndex, UnderlineType, VerticalPosition,
};
use hwpforge_smithy_hwpx::HwpxEncoder;
use hwpforge_smithy_hwpx::style_store::{HwpxCharShape, HwpxParaShape, HwpxStyleStore};

use crate::normalize::DEFAULT_SIZE_PT;
use crate::xml::{self, Element};

/// A4 · 좌우 20mm 의 본문 폭(HwpUnit = 1/100pt). 이보다 넓은 그림 · 표는 줄인다.
const TEXT_WIDTH: i32 = 48189;
const FONT: &str = "함초롬바탕";

#[derive(Hash, PartialEq, Eq, Clone, Copy, Default)]
struct CharKey {
    bold: bool,
    italic: bool,
    underline: bool,
    /// 0 보통 · 1 위 · 2 아래
    vpos: u8,
    /// 하프포인트
    size: u32,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct ParaKey {
    align: u8,
    indent: i32,
    left: i32,
}

struct Writer {
    store: HwpxStyleStore,
    chars: HashMap<CharKey, CharShapeIndex>,
    paras: HashMap<ParaKey, ParaShapeIndex>,
    images: ImageStore,
    image_count: usize,
    rels: HashMap<String, String>,
    media: HashMap<String, Vec<u8>>,
    warnings: crate::normalize::Warnings,
}

pub fn convert(docx: &[u8]) -> Result<(Vec<u8>, crate::normalize::Warnings), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(docx)).map_err(|e| e.to_string())?;
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut f = zip.by_name(name).ok()?;
        let mut b = Vec::new();
        f.read_to_end(&mut b).ok()?;
        Some(b)
    };
    let document = xml::parse(&read("word/document.xml").ok_or("word/document.xml 이 없습니다")?)?;
    let mut rels = HashMap::new();
    let mut media = HashMap::new();
    if let Some(bytes) = read("word/_rels/document.xml.rels") {
        for rel in xml::parse(&bytes)?.elements() {
            let (Some(id), Some(target)) = (rel.get("Id"), rel.get("Target")) else {
                continue;
            };
            if rel.get("TargetMode") == Some("External") {
                continue;
            }
            let path = format!(
                "word/{}",
                target.trim_start_matches('/').trim_start_matches("word/")
            );
            if let Some(bytes) = read(&path) {
                media.insert(id.to_string(), bytes);
            }
            rels.insert(id.to_string(), path);
        }
    }
    let mut w = Writer {
        store: HwpxStyleStore::with_default_fonts(FONT),
        chars: HashMap::new(),
        paras: HashMap::new(),
        images: ImageStore::new(),
        image_count: 0,
        rels,
        media,
        warnings: Vec::new(),
    };
    // 0번 모양은 보통 글 · 왼쪽 정렬로 둔다.
    w.char_shape(CharKey {
        size: (DEFAULT_SIZE_PT * 2.0) as u32,
        ..Default::default()
    });
    w.para_shape(ParaKey {
        align: 0,
        indent: 0,
        left: 0,
    });

    let body = document.first("w:body").ok_or("w:body 가 없습니다")?;
    let mut paragraphs = Vec::new();
    w.blocks(body, TEXT_WIDTH, &mut paragraphs);
    if paragraphs.is_empty() {
        paragraphs.push(w.empty_paragraph());
    }
    let mut doc = Document::new();
    doc.add_section(Section::with_paragraphs(paragraphs, PageSettings::a4()));
    let validated = doc.validate().map_err(|e| format!("HWPX 구조 검사: {e}"))?;
    let bytes = HwpxEncoder::encode(&validated, &w.store, &w.images)
        .map_err(|e| format!("HWPX 쓰기: {e}"))?;
    Ok((bytes, w.warnings))
}

impl Writer {
    fn char_shape(&mut self, key: CharKey) -> CharShapeIndex {
        if let Some(i) = self.chars.get(&key) {
            return *i;
        }
        let mut shape = HwpxCharShape::default();
        shape.bold = key.bold;
        shape.italic = key.italic;
        if key.underline {
            shape.underline_type = UnderlineType::Bottom;
        }
        shape.vertical_position = match key.vpos {
            1 => VerticalPosition::Superscript,
            2 => VerticalPosition::Subscript,
            _ => VerticalPosition::Normal,
        };
        if let Ok(h) = HwpUnit::from_pt(f64::from(key.size) / 2.0) {
            shape.height = h;
        }
        let i = self.store.push_char_shape(shape);
        self.chars.insert(key, i);
        i
    }

    fn para_shape(&mut self, key: ParaKey) -> ParaShapeIndex {
        if let Some(i) = self.paras.get(&key) {
            return *i;
        }
        let mut shape = HwpxParaShape::default();
        shape.alignment = match key.align {
            1 => Alignment::Center,
            2 => Alignment::Right,
            3 => Alignment::Justify,
            _ => Alignment::Left,
        };
        let unit =
            |v: i32| ParagraphMeasure::from_hwp_unit(HwpUnit::new(v).unwrap_or(HwpUnit::ZERO));
        shape.indent = unit(key.indent);
        shape.margin_left = unit(key.left);
        let i = self.store.push_para_shape(shape);
        self.paras.insert(key, i);
        i
    }

    fn empty_paragraph(&mut self) -> Paragraph {
        Paragraph::with_runs(
            vec![Run::text("", CharShapeIndex::new(0))],
            ParaShapeIndex::new(0),
        )
    }

    /// 본문 · 칸 · 내용 컨트롤 안의 문단과 표.
    fn blocks(&mut self, parent: &Element, width: i32, out: &mut Vec<Paragraph>) {
        for el in parent.elements() {
            match el.name.as_str() {
                "w:p" => self.paragraph(el, width, out),
                "w:tbl" => {
                    let table = self.table(el, width);
                    let ps = self.para_shape(ParaKey {
                        align: 0,
                        indent: 0,
                        left: 0,
                    });
                    out.push(Paragraph::with_runs(
                        vec![Run::table(table, CharShapeIndex::new(0))],
                        ps,
                    ));
                }
                "w:sdt" => {
                    if let Some(content) = el.first("w:sdtContent") {
                        self.blocks(content, width, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, p: &Element, width: i32, out: &mut Vec<Paragraph>) {
        let ppr = p.first("w:pPr");
        let align = match ppr.and_then(|x| x.prop("w:jc", "w:val")) {
            Some("center") => 1,
            Some("right" | "end") => 2,
            Some("both" | "distribute") => 3,
            _ => 0,
        };
        let ind = ppr.and_then(|x| x.first("w:ind"));
        let twips = |name| {
            ind.and_then(|i| i.get(name))
                .and_then(|v| v.parse::<i32>().ok())
        };
        let indent = twips("w:firstLine")
            .or_else(|| twips("w:hanging").map(|h| -h))
            .unwrap_or(0)
            * 5;
        let left = twips("w:left").or_else(|| twips("w:start")).unwrap_or(0) * 5;
        let ps = self.para_shape(ParaKey {
            align,
            indent,
            left,
        });
        let page_break_before = ppr
            .and_then(|x| x.first("w:pageBreakBefore"))
            .is_some_and(|e| e.get("w:val").is_none_or(on));

        let mut runs = Vec::new();
        let mut breaks = Vec::new();
        self.inline(p, width, &mut runs, &mut breaks);
        // 쪽 나눔(`w:br w:type="page"`)은 문단을 그 자리에서 나눈다.
        let mut start = 0;
        let mut first = true;
        for at in breaks.into_iter().chain([usize::MAX]) {
            let end = at.min(runs.len());
            let mut piece: Vec<Run> = runs[start..end].to_vec();
            if piece.is_empty() {
                piece.push(Run::text("", CharShapeIndex::new(0)));
            }
            let mut para = Paragraph::with_runs(piece, ps);
            para.page_break = if first { page_break_before } else { true };
            out.push(para);
            first = false;
            start = end;
        }
    }

    /// 문단 안의 런들. 쪽 나눔 자리는 `breaks` 에 런 번호로.
    fn inline(&mut self, el: &Element, width: i32, runs: &mut Vec<Run>, breaks: &mut Vec<usize>) {
        for child in el.elements() {
            match child.name.as_str() {
                "w:pPr" | "w:del" | "w:moveFrom" => {}
                "w:r" => self.run(child, width, runs, breaks),
                _ => self.inline(child, width, runs, breaks),
            }
        }
    }

    fn run(&mut self, r: &Element, width: i32, runs: &mut Vec<Run>, breaks: &mut Vec<usize>) {
        let rpr = r.first("w:rPr");
        let flag = |name| {
            rpr.and_then(|p| p.first(name))
                .is_some_and(|e| e.get("w:val").is_none_or(on))
        };
        let key = CharKey {
            bold: flag("w:b"),
            italic: flag("w:i"),
            underline: rpr
                .and_then(|p| p.prop("w:u", "w:val"))
                .is_some_and(|v| v != "none"),
            vpos: match rpr.and_then(|p| p.prop("w:vertAlign", "w:val")) {
                Some("superscript") => 1,
                Some("subscript") => 2,
                _ => 0,
            },
            size: rpr
                .and_then(|p| p.prop("w:sz", "w:val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or((DEFAULT_SIZE_PT * 2.0) as u32),
        };
        let cs = self.char_shape(key);
        let mut text = String::new();
        let flush = |text: &mut String, runs: &mut Vec<Run>| {
            if !text.is_empty() {
                runs.push(Run::text(std::mem::take(text), cs));
            }
        };
        for child in r.elements() {
            match child.name.as_str() {
                "w:t" => text.push_str(&child.text()),
                "w:tab" | "w:ptab" => text.push('\t'),
                "w:cr" => text.push('\n'),
                "w:noBreakHyphen" => text.push('-'),
                "w:br" if child.get("w:type") == Some("page") => {
                    flush(&mut text, runs);
                    breaks.push(runs.len());
                }
                "w:br" => text.push('\n'),
                "w:drawing" | "w:pict" | "w:object" => {
                    flush(&mut text, runs);
                    if let Some(image) = self.image(child, width) {
                        runs.push(Run::image(image, cs));
                    }
                }
                _ => {}
            }
        }
        flush(&mut text, runs);
    }

    fn image(&mut self, el: &Element, width: i32) -> Option<Image> {
        let id = el
            .find("a:blip")
            .and_then(|b| b.get("r:embed"))
            .or_else(|| el.find("v:imagedata").and_then(|b| b.get("r:id")))?;
        let Some(bytes) = self.media.get(id).cloned() else {
            self.warnings
                .push(("image_missing", format!("그림 {id} 의 파일이 없습니다")));
            return None;
        };
        let path = self.rels.get(id).cloned().unwrap_or_default();
        let ext = path
            .rsplit('.')
            .next()
            .unwrap_or("png")
            .to_ascii_lowercase();
        // 크기: DrawingML(EMU · VML 은 고쳐 쓰기에서 이미 옮겼다) → 화소(96dpi)
        let (mut w, mut h) = match el.find("wp:extent") {
            Some(extent) => {
                let emu = |k| {
                    extent
                        .get(k)
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(0.0)
                        / 127.0
                };
                (emu("cx"), emu("cy"))
            }
            None => (0.0, 0.0),
        };
        if w <= 0.0 || h <= 0.0 {
            let (pw, ph) = png_size(&bytes).unwrap_or((96, 96));
            (w, h) = (f64::from(pw) * 75.0, f64::from(ph) * 75.0);
        }
        if w > f64::from(width) {
            h *= f64::from(width) / w;
            w = f64::from(width);
        }
        self.image_count += 1;
        let name = format!("image{}.{ext}", self.image_count);
        self.images.insert(name.clone(), bytes);
        let unit = |v: f64| HwpUnit::new(v.round().max(1.0) as i32).ok();
        Some(Image::from_path(
            format!("BinData/{name}"),
            unit(w)?,
            unit(h)?,
        ))
    }

    fn table(&mut self, tbl: &Element, max_width: i32) -> Table {
        let mut grid: Vec<i32> = tbl
            .first("w:tblGrid")
            .map(|g| {
                g.elements()
                    .filter(|c| c.name == "w:gridCol")
                    .map(|c| c.get("w:w").and_then(|v| v.parse().ok()).unwrap_or(0) * 5)
                    .collect()
            })
            .unwrap_or_default();
        // 칸 자리 잡기: (줄, 시작 칸, 합친 칸 수, vMerge, tc)
        struct Slot<'a> {
            col: usize,
            span: usize,
            merge: Option<bool>, // Some(true) = 이어짐, Some(false) = 시작
            tc: Option<&'a Element>,
        }
        let mut rows: Vec<(Vec<Slot>, bool)> = Vec::new();
        for tr in tbl.elements().filter(|e| e.name == "w:tr") {
            let trpr = tr.first("w:trPr");
            let count = |name| {
                trpr.and_then(|p| p.prop(name, "w:val"))
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0)
            };
            let header = trpr.is_some_and(|p| p.first("w:tblHeader").is_some());
            let mut slots = Vec::new();
            let mut col = 0;
            let before = count("w:gridBefore");
            if before > 0 {
                slots.push(Slot {
                    col,
                    span: before,
                    merge: None,
                    tc: None,
                });
                col += before;
            }
            for tc in cells(tr) {
                let tcpr = tc.first("w:tcPr");
                let span = tcpr
                    .and_then(|p| p.prop("w:gridSpan", "w:val"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1usize)
                    .max(1);
                let merge = tcpr
                    .and_then(|p| p.first("w:vMerge"))
                    .map(|m| m.get("w:val") != Some("restart"));
                slots.push(Slot {
                    col,
                    span,
                    merge,
                    tc: Some(tc),
                });
                col += span;
            }
            let after = count("w:gridAfter");
            if after > 0 {
                slots.push(Slot {
                    col,
                    span: after,
                    merge: None,
                    tc: None,
                });
                col += after;
            }
            if grid.len() < col {
                // 표 격자가 모자라면(손으로 만진 표) 칸 너비(tcW)나 고른 너비로 채운다.
                let fill = slots
                    .last()
                    .and_then(|s| s.tc)
                    .and_then(|tc| tc.first("w:tcPr"))
                    .and_then(|p| p.prop("w:tcW", "w:w"))
                    .and_then(|v| v.parse::<i32>().ok())
                    .map_or(1500 * 5, |v| v * 5);
                grid.resize(col, fill);
            }
            rows.push((slots, header));
        }
        let total: i32 = grid.iter().sum::<i32>().max(1);
        let scale = if total > max_width {
            f64::from(max_width) / f64::from(total)
        } else {
            1.0
        };
        let width_of = |col: usize, span: usize| {
            ((f64::from(grid[col..(col + span).min(grid.len())].iter().sum::<i32>()) * scale)
                as i32)
                .max(200)
        };

        let mut out_rows = Vec::new();
        for (r, (slots, header)) in rows.iter().enumerate() {
            let mut cells_out = Vec::new();
            for slot in slots {
                if slot.merge == Some(true) {
                    continue; // 위 칸에 합쳐진 칸
                }
                let row_span = if slot.merge == Some(false) {
                    1 + rows[r + 1..]
                        .iter()
                        .take_while(|(next, _)| {
                            next.iter()
                                .any(|s| s.col == slot.col && s.merge == Some(true))
                        })
                        .count()
                } else {
                    1
                };
                let width = width_of(slot.col, slot.span);
                let mut paragraphs = Vec::new();
                if let Some(tc) = slot.tc {
                    // 칸 안 여백을 빼고 칸 너비 안에 그림 · 표를 넣는다.
                    self.blocks(tc, (width - 1020).max(200), &mut paragraphs);
                }
                if paragraphs.is_empty() {
                    paragraphs.push(self.empty_paragraph());
                }
                let unit = HwpUnit::new(width).unwrap_or(HwpUnit::ZERO);
                cells_out.push(TableCell::with_span(
                    paragraphs,
                    unit,
                    slot.span as u16,
                    row_span as u16,
                ));
            }
            if cells_out.is_empty() {
                // 모든 칸이 위에 합쳐진 줄도 빈 줄로는 둘 수 없다 — 그 줄은 건너뛴다.
                continue;
            }
            let mut row = TableRow::new(cells_out);
            row.is_header = *header && r == 0;
            out_rows.push(row);
        }
        let mut table = Table::new(out_rows);
        table.repeat_header = rows.first().is_some_and(|(_, h)| *h);
        table
    }
}

/// 줄의 칸들. `w:sdt`·`w:customXml` 로 싸인 칸도 꺼낸다.
fn cells(tr: &Element) -> Vec<&Element> {
    let mut out = Vec::new();
    for e in tr.elements() {
        match e.name.as_str() {
            "w:tc" => out.push(e),
            "w:sdt" => {
                if let Some(c) = e.first("w:sdtContent") {
                    out.extend(cells(c));
                }
            }
            "w:customXml" => out.extend(cells(e)),
            _ => {}
        }
    }
    out
}

fn on(v: &str) -> bool {
    !matches!(v, "0" | "false" | "off")
}

/// PNG 머리의 화소 크기. 다른 형식은 모른다(None).
fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() > 24 && b.starts_with(b"\x89PNG") {
        let be = |i: usize| u32::from_be_bytes(b[i..i + 4].try_into().unwrap());
        return Some((be(16), be(20)));
    }
    None
}
