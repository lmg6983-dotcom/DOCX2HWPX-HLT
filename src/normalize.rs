//! DOCX 를 고쳐 쓴다 — 변환기(hlt-core 의 DOCX 판독)가 놓치거나 깨뜨리는 것을 미리 그림 · 보통 글로 바꾼다.
//! - 수식(OMML) → PNG 그림 (별행 수식 문단은 가운데)
//! - WMF · EMF(OLE 수식 미리보기 · 도면) → PNG
//! - 변경 추적은 모두 받아들이고(넣은 글 · 지운 글 · 옮긴 글 · 서식 변경 · 지운 문단 표시 · 줄/칸 넣고 지우기),
//!   메모는 모두 지운다(본문 자리표 · comments*.xml 부분 · 관계 · 내용 형식)
//! - `mc:AlternateContent` 는 한쪽만
//! - `w:sym`(Symbol 글꼴) → 유니코드 글
//! - HLT 만: NK-Editor 글자 규칙([`crate::chars`])

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use crate::chars::{self, Piece};
use crate::render::{Png, Renderer};
use crate::xml::{self, Element, Node};

const DOCUMENT: &str = "word/document.xml";
const RELS: &str = "word/_rels/document.xml.rels";
const TYPES: &str = "[Content_Types].xml";
const COMMENTS: &str = "word/comments.xml";
const IMAGE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const EMU_PER_PT: f64 = 12700.0;
/// 크기를 적지 않은 글의 크기 — hlt-core 가 HLT 본문에 쓰는 값과 같다.
pub const DEFAULT_SIZE_PT: f64 = 12.0;

/// (코드, 설명) 목록. 코드는 AGENTS.md 의 표와 같다.
pub type Warnings = Vec<(&'static str, String)>;

#[derive(Default, Debug)]
pub struct Report {
    pub equations: usize,
    pub metafiles: usize,
    pub scripts: usize,
    pub substitutions: HashMap<(char, String), usize>,
    pub glyph_images: HashMap<String, usize>,
    pub warnings: Warnings,
    /// 받아들인 변경 추적 수
    pub revisions_accepted: usize,
    /// 지운 메모 수
    pub comments_removed: usize,
}

struct Normalizer<'a> {
    renderer: &'a Renderer,
    nk_chars: bool,
    rels: Element,
    new_files: Vec<(String, Vec<u8>)>,
    glyph_cache: HashMap<(String, u32), (String, f64, f64)>,
    next_id: usize,
    report: Report,
    /// 수식 하나를 그릴 때마다 (그린 수, 모두)
    progress: &'a mut dyn FnMut(usize, usize),
    math_total: usize,
}

/// `nk_chars` = HLT(NK-Editor) 글자 규칙까지 적용한다.
pub fn normalize(
    docx: &[u8],
    renderer: &Renderer,
    nk_chars: bool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(Vec<u8>, Report), String> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(docx)).map_err(|e| format!("DOCX 가 아닙니다: {e}"))?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        if file.is_dir() {
            continue;
        }
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
    let mut document = xml::parse(get(DOCUMENT).ok_or("word/document.xml 이 없습니다")?)?;
    let rels = match get(RELS) {
        Some(bytes) => xml::parse(bytes)?,
        None => Element::new("Relationships").attr(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        ),
    };
    let mut n = Normalizer {
        renderer,
        nk_chars,
        rels,
        new_files: Vec::new(),
        glyph_cache: HashMap::new(),
        next_id: 1,
        report: Report::default(),
        progress,
        math_total: count_math(&document),
    };
    n.convert_metafiles(&parts);
    for (k, v) in [
        (
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ),
        (
            "xmlns:wp",
            "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
        ),
    ] {
        if document.get(k).is_none() {
            document.attrs.push((k.into(), v.into()));
        }
    }
    accept_revisions(&mut document, &mut n.report.revisions_accepted);
    n.walk(&mut document);

    // 메모: 본문 자리표는 accept_revisions 가 지웠다. 메모 부분(comments*.xml)과 그 관계 · 내용 형식도 뺀다.
    n.report.comments_removed = get(COMMENTS)
        .and_then(|b| xml::parse(b).ok())
        .map_or(0, |c| {
            c.elements().filter(|e| e.name == "w:comment").count()
        });
    n.rels
        .children
        .retain(|r| !matches!(r, Node::El(e) if e.get("Target").is_some_and(is_comment_part)));
    let mut types = xml::parse(get(TYPES).ok_or("[Content_Types].xml 이 없습니다")?)?;
    if !types.elements().any(|e| {
        e.name == "Default"
            && e.get("Extension")
                .is_some_and(|x| x.eq_ignore_ascii_case("png"))
    }) {
        types.children.push(Node::El(
            Element::new("Default")
                .attr("Extension", "png")
                .attr("ContentType", "image/png"),
        ));
    }
    types
        .children
        .retain(|t| !matches!(t, Node::El(e) if e.get("PartName").is_some_and(is_comment_part)));
    let mut replaced: Vec<(String, Vec<u8>)> = vec![
        (DOCUMENT.into(), xml::write(&document)),
        (RELS.into(), xml::write(&n.rels)),
        (TYPES.into(), xml::write(&types)),
    ];
    replaced.append(&mut n.new_files);

    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts
        .iter()
        .filter(|(name, _)| !replaced.iter().any(|(r, _)| r == name) && !is_comment_part(name))
        .chain(replaced.iter())
    {
        out.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        out.write_all(bytes).map_err(|e| e.to_string())?;
    }
    let bytes = out.finish().map_err(|e| e.to_string())?.into_inner();
    Ok((bytes, n.report))
}

impl Normalizer<'_> {
    /// WMF · EMF 그림의 관계를 새 PNG 로 돌려 단다. 그리지 못하면 「그림 변환 실패」 그림으로.
    fn convert_metafiles(&mut self, parts: &[(String, Vec<u8>)]) {
        let mut retarget = Vec::new();
        for rel in self.rels.elements() {
            let target = rel.get("Target").unwrap_or_default();
            let lower = target.to_ascii_lowercase();
            let is_emf = lower.ends_with(".emf");
            if rel.get("Type") != Some(IMAGE_REL) || !(is_emf || lower.ends_with(".wmf")) {
                continue;
            }
            let path = format!(
                "word/{}",
                target.trim_start_matches('/').trim_start_matches("word/")
            );
            let Some((_, bytes)) = parts.iter().find(|(n, _)| *n == path) else {
                continue;
            };
            let png = crate::render::metafile_to_png(bytes, is_emf).or_else(|e| {
                self.report.warnings.push((
                    "metafile_placeholder",
                    format!("{target}: {e} — 자리표 그림을 넣습니다"),
                ));
                self.renderer
                    .glyph("[그림 변환 실패]", DEFAULT_SIZE_PT)
                    .map(|p| p.bytes)
            });
            match png {
                Ok(png) => {
                    let name = format!("media/d2h_meta{}.png", retarget.len() + 1);
                    self.new_files.push((format!("word/{name}"), png));
                    retarget.push((rel.get("Id").unwrap_or_default().to_string(), name));
                    self.report.metafiles += 1;
                }
                Err(e) => self
                    .report
                    .warnings
                    .push(("metafile_failed", format!("{target}: {e}"))),
            }
        }
        for node in &mut self.rels.children {
            if let Node::El(rel) = node
                && let Some((_, name)) = retarget
                    .iter()
                    .find(|(id, _)| Some(id.as_str()) == rel.get("Id"))
            {
                for (k, v) in &mut rel.attrs {
                    if k == "Target" {
                        *v = name.clone();
                    }
                }
            }
        }
    }

    fn walk(&mut self, el: &mut Element) {
        let old = std::mem::take(&mut el.children);
        let mut out = Vec::with_capacity(old.len());
        for node in old {
            let Node::El(mut child) = node else {
                out.push(node);
                continue;
            };
            match child.name.as_str() {
                "mc:AlternateContent" => {
                    let pick = child
                        .first("mc:Choice")
                        .filter(|c| !c.children.is_empty())
                        .or_else(|| child.first("mc:Fallback"));
                    if let Some(pick) = pick {
                        let mut pick = pick.clone();
                        self.walk(&mut pick);
                        out.extend(pick.children);
                    }
                }
                "m:oMathPara" | "m:oMath" => out.extend(self.math(&child).map(Node::El)),
                "w:p" => {
                    let display_only = child.elements().any(|e| e.name == "m:oMathPara")
                        && child.elements().all(|e| {
                            matches!(
                                e.name.as_str(),
                                "w:pPr" | "m:oMathPara" | "w:bookmarkStart" | "w:bookmarkEnd"
                            )
                        });
                    self.walk(&mut child);
                    if display_only {
                        center(&mut child);
                    }
                    out.push(Node::El(child));
                }
                "w:tc" if self.nk_chars => {
                    self.walk(&mut child);
                    self.flatten_cell(&mut child);
                    out.push(Node::El(child));
                }
                "w:object" | "w:pict" => match self.vml_image(&child) {
                    Some(drawing) => out.push(Node::El(drawing)),
                    None => {
                        self.walk(&mut child);
                        out.push(Node::El(child));
                    }
                },
                "w:r" => {
                    self.walk(&mut child);
                    self.symbols(&mut child);
                    if self.nk_chars {
                        out.extend(self.split_run(child).into_iter().map(Node::El));
                    } else {
                        out.push(Node::El(child));
                    }
                }
                _ => {
                    self.walk(&mut child);
                    out.push(Node::El(child));
                }
            }
        }
        el.children = out;
    }

    /// HLT 의 표 칸은 문단 하나뿐이다: 칸 안 문단들은 띄어 쓴 한 문단으로, 칸 안 표는 글(「칸 | 칸 / 다음 줄」)로.
    fn flatten_cell(&mut self, tc: &mut Element) {
        let blocks: Vec<&Element> = tc
            .elements()
            .filter(|e| matches!(e.name.as_str(), "w:p" | "w:tbl"))
            .collect();
        if blocks.len() <= 1 && blocks.iter().all(|b| b.name == "w:p") {
            return;
        }
        let ppr = tc.first("w:p").and_then(|p| p.first("w:pPr")).cloned();
        let mut runs: Vec<Node> = Vec::new();
        let mut nested = false;
        for block in &blocks {
            let mut piece = Vec::new();
            inline_runs(block, &mut piece, &mut nested);
            if piece.is_empty() {
                continue;
            }
            if !runs.is_empty() {
                runs.push(Node::El(Element::new("w:r").child(text_el(" "))));
            }
            runs.extend(piece);
        }
        if nested {
            self.report.warnings.push((
                "nested_table_flattened",
                "표 칸 안의 표는 HLT 에서 글(「칸 | 칸 / 다음 줄」)로 풀었습니다".into(),
            ));
        }
        let mut p = Element::new("w:p");
        p.children.extend(ppr.map(Node::El));
        p.children.extend(runs);
        tc.children
            .retain(|n| !matches!(n, Node::El(e) if matches!(e.name.as_str(), "w:p" | "w:tbl")));
        tc.children.push(Node::El(p));
    }

    fn math(&mut self, el: &Element) -> Option<Element> {
        let display = el.name == "m:oMathPara";
        let size = math_size(el).unwrap_or(DEFAULT_SIZE_PT);
        let body = crate::omml::to_typst(el);
        if body.trim().is_empty() {
            return None;
        }
        let png = match self.renderer.math(&body, display, size) {
            Ok(png) => png,
            Err(e) => {
                // 그리지 못한 수식은 글로라도 남긴다(잃지 않는다).
                self.report.warnings.push((
                    "equation_as_text",
                    format!("수식을 그리지 못해 글로 남김: {e} ← {body}"),
                ));
                return Some(
                    Element::new("w:r").child(
                        Element::new("w:t")
                            .attr("xml:space", "preserve")
                            .text_child(&el.text()),
                    ),
                );
            }
        };
        self.report.equations += 1;
        (self.progress)(self.report.equations, self.math_total);
        let id = self.add_png(png.bytes.clone());
        Some(self.image_run(&id, &png, None))
    }

    /// VML 그림(OLE 수식 미리보기 등) → 크기를 단 DrawingML. hlt-core 는 VML 크기를 읽지 않아 쪽 폭으로 늘린다.
    fn vml_image(&mut self, el: &Element) -> Option<Element> {
        let id = el.find("v:imagedata")?.get("r:id")?.to_string();
        let style = el.find("v:shape")?.get("style")?;
        let (w, h) = (css_pt(style, "width"), css_pt(style, "height"));
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let run = self.image_run(
            &id,
            &Png {
                bytes: Vec::new(),
                width_pt: w,
                height_pt: h,
            },
            None,
        );
        run.first("w:drawing").cloned()
    }

    fn add_png(&mut self, bytes: Vec<u8>) -> String {
        let n = self.new_files.len() + 1;
        let id = format!("rIdD2h{n}");
        let target = format!("media/d2h_{n}.png");
        self.new_files.push((format!("word/{target}"), bytes));
        self.rels.children.push(Node::El(
            Element::new("Relationship")
                .attr("Id", &id)
                .attr("Type", IMAGE_REL)
                .attr("Target", &target),
        ));
        id
    }

    fn image_run(&mut self, rel_id: &str, png: &Png, rpr: Option<&Element>) -> Element {
        self.next_id += 1;
        let id = (100_000 + self.next_id).to_string();
        let cx = ((png.width_pt * EMU_PER_PT).round() as u64)
            .max(1)
            .to_string();
        let cy = ((png.height_pt * EMU_PER_PT).round() as u64)
            .max(1)
            .to_string();
        let name = format!("d2h{id}.png");
        let pic = Element::new("pic:pic")
            .attr(
                "xmlns:pic",
                "http://schemas.openxmlformats.org/drawingml/2006/picture",
            )
            .child(
                Element::new("pic:nvPicPr")
                    .child(
                        Element::new("pic:cNvPr")
                            .attr("id", &id)
                            .attr("name", &name),
                    )
                    .child(Element::new("pic:cNvPicPr")),
            )
            .child(
                Element::new("pic:blipFill")
                    .child(Element::new("a:blip").attr("r:embed", rel_id))
                    .child(Element::new("a:stretch").child(Element::new("a:fillRect"))),
            )
            .child(
                Element::new("pic:spPr")
                    .child(
                        Element::new("a:xfrm")
                            .child(Element::new("a:off").attr("x", "0").attr("y", "0"))
                            .child(Element::new("a:ext").attr("cx", &cx).attr("cy", &cy)),
                    )
                    .child(
                        Element::new("a:prstGeom")
                            .attr("prst", "rect")
                            .child(Element::new("a:avLst")),
                    ),
            );
        let inline = Element::new("wp:inline")
            .attr("distT", "0")
            .attr("distB", "0")
            .attr("distL", "0")
            .attr("distR", "0")
            .child(Element::new("wp:extent").attr("cx", &cx).attr("cy", &cy))
            .child(Element::new("wp:docPr").attr("id", &id).attr("name", &name))
            .child(
                Element::new("a:graphic")
                    .attr(
                        "xmlns:a",
                        "http://schemas.openxmlformats.org/drawingml/2006/main",
                    )
                    .child(
                        Element::new("a:graphicData")
                            .attr(
                                "uri",
                                "http://schemas.openxmlformats.org/drawingml/2006/picture",
                            )
                            .child(pic),
                    ),
            );
        let mut run = Element::new("w:r");
        if let Some(rpr) = rpr {
            run.children.push(Node::El(rpr.clone()));
        }
        run.child(Element::new("w:drawing").child(inline))
    }

    /// `w:sym` → `w:t`. Symbol 글꼴만 안다.
    fn symbols(&mut self, run: &mut Element) {
        for node in &mut run.children {
            let Node::El(el) = node else { continue };
            if el.name != "w:sym" {
                continue;
            }
            let font = el.get("w:font").unwrap_or_default().to_string();
            let code = el
                .get("w:char")
                .and_then(|c| u32::from_str_radix(c, 16).ok())
                .unwrap_or(0);
            let text = if font.eq_ignore_ascii_case("Symbol") {
                chars::symbol_font(code)
            } else {
                None
            };
            match text {
                Some(c) => *el = Element::new("w:t").text_child(&c.to_string()),
                None => self.report.warnings.push((
                    "symbol_unmapped",
                    format!("{font} 글꼴 기호 U+{code:04X} 는 옮기지 못했습니다"),
                )),
            }
        }
    }

    /// NK-Editor 글자 규칙으로 런을 나눈다. 고칠 글자가 없으면 그대로.
    fn split_run(&mut self, run: Element) -> Vec<Element> {
        let changes = |e: &Element| {
            let t = e.text();
            e.name == "w:t" && !t.is_empty() && chars::split(&t) != [Piece::Text(t.clone())]
        };
        if !run.elements().any(changes) {
            return vec![run];
        }
        let rpr = run.first("w:rPr").cloned();
        let size = rpr
            .as_ref()
            .and_then(|p| p.prop("w:sz", "w:val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map_or(DEFAULT_SIZE_PT, |half| half / 2.0);
        let fresh = || {
            let mut r = Element::new("w:r");
            if let Some(p) = &rpr {
                r.children.push(Node::El(p.clone()));
            }
            r
        };
        let mut out = Vec::new();
        let mut current = fresh();
        for node in run.children {
            let Node::El(child) = node else { continue };
            if child.name == "w:rPr" {
                continue;
            }
            if child.name != "w:t" {
                current.children.push(Node::El(child));
                continue;
            }
            for piece in chars::split(&child.text()) {
                match piece {
                    Piece::Text(t) => current.children.push(Node::El(text_el(&t))),
                    Piece::Sub(from, to) => {
                        current.children.push(Node::El(text_el(&to)));
                        *self.report.substitutions.entry((from, to)).or_default() += 1;
                    }
                    Piece::Script(t, up) => {
                        self.report.scripts += 1;
                        flush(&mut out, &mut current, &fresh);
                        let mut p = rpr.clone().unwrap_or_else(|| Element::new("w:rPr"));
                        p.children
                            .retain(|n| !matches!(n, Node::El(e) if e.name == "w:vertAlign"));
                        p.children.push(Node::El(
                            Element::new("w:vertAlign")
                                .attr("w:val", if up { "superscript" } else { "subscript" }),
                        ));
                        out.push(Element::new("w:r").child(p).child(text_el(&t)));
                    }
                    Piece::Image(cluster) => {
                        flush(&mut out, &mut current, &fresh);
                        match self.glyph(&cluster, size) {
                            Ok(run) => out.push(run),
                            Err(e) => {
                                self.report.warnings.push((
                                    "glyph_failed",
                                    format!("글자 {cluster:?} 를 그리지 못했습니다: {e}"),
                                ));
                                current.children.push(Node::El(text_el(&cluster)));
                            }
                        }
                    }
                }
            }
        }
        flush(&mut out, &mut current, &fresh);
        out
    }

    fn glyph(&mut self, cluster: &str, size: f64) -> Result<Element, String> {
        *self
            .report
            .glyph_images
            .entry(cluster.to_string())
            .or_default() += 1;
        let key = (cluster.to_string(), (size * 10.0) as u32);
        let (id, w, h) = match self.glyph_cache.get(&key) {
            Some(hit) => hit.clone(),
            None => {
                let png = self.renderer.glyph(cluster, size)?;
                let (w, h) = (png.width_pt, png.height_pt);
                let id = self.add_png(png.bytes);
                self.glyph_cache.insert(key, (id.clone(), w, h));
                (id, w, h)
            }
        };
        Ok(self.image_run(
            &id,
            &Png {
                bytes: Vec::new(),
                width_pt: w,
                height_pt: h,
            },
            None,
        ))
    }
}

/// 문단 · 표의 런들을 한 줄로. 표는 칸을 「 | 」, 줄을 「 / 」로 잇는다.
fn inline_runs(block: &Element, out: &mut Vec<Node>, nested: &mut bool) {
    let sep = |s: &str| Node::El(Element::new("w:r").child(text_el(s)));
    if block.name == "w:tbl" {
        *nested = true;
        for (r, tr) in block.elements().filter(|e| e.name == "w:tr").enumerate() {
            if r > 0 {
                out.push(sep(" / "));
            }
            for (c, tc) in tr.elements().filter(|e| e.name == "w:tc").enumerate() {
                if c > 0 {
                    out.push(sep(" | "));
                }
                for inner in tc
                    .elements()
                    .filter(|e| matches!(e.name.as_str(), "w:p" | "w:tbl"))
                {
                    inline_runs(inner, out, nested);
                }
            }
        }
        return;
    }
    for child in block.elements() {
        match child.name.as_str() {
            "w:pPr" => {}
            "w:r" => out.push(Node::El(child.clone())),
            _ => inline_runs(child, out, nested),
        }
    }
}

fn text_el(t: &str) -> Element {
    Element::new("w:t")
        .attr("xml:space", "preserve")
        .text_child(t)
}

fn flush(out: &mut Vec<Element>, current: &mut Element, fresh: &dyn Fn() -> Element) {
    if current.elements().any(|e| e.name != "w:rPr") {
        out.push(std::mem::replace(current, fresh()));
    }
}

/// 그릴 수식 수(문단 수식 안의 수식은 문단 수식 하나로 센다).
fn count_math(el: &Element) -> usize {
    el.elements()
        .map(|e| match e.name.as_str() {
            "m:oMathPara" | "m:oMath" => 1,
            _ => count_math(e),
        })
        .sum()
}

/// 수식 런이 적어 둔 글자 크기(`w:sz`, 하프포인트).
fn math_size(el: &Element) -> Option<f64> {
    for child in el.elements() {
        if child.name == "w:sz" {
            return child.get("w:val")?.parse::<f64>().ok().map(|v| v / 2.0);
        }
        if let Some(size) = math_size(child) {
            return Some(size);
        }
    }
    None
}

fn center(p: &mut Element) {
    let has_ppr = p.first("w:pPr").is_some();
    if !has_ppr {
        p.children.insert(0, Node::El(Element::new("w:pPr")));
    }
    for node in &mut p.children {
        if let Node::El(ppr) = node
            && ppr.name == "w:pPr"
        {
            ppr.children
                .retain(|n| !matches!(n, Node::El(e) if e.name == "w:jc"));
            ppr.children
                .push(Node::El(Element::new("w:jc").attr("w:val", "center")));
            break;
        }
    }
}

/// VML style 의 `width:72pt` 같은 값 → pt.
fn css_pt(style: &str, key: &str) -> f64 {
    style
        .split(';')
        .filter_map(|kv| kv.split_once(':'))
        .find(|(k, _)| k.trim() == key)
        .and_then(|(_, v)| {
            let v = v.trim();
            let (num, unit) = v.split_at(v.find(|c: char| c.is_alphabetic()).unwrap_or(v.len()));
            let n: f64 = num.trim().parse().ok()?;
            Some(match unit {
                "in" => n * 72.0,
                "cm" => n * 72.0 / 2.54,
                "mm" => n * 72.0 / 25.4,
                "px" => n * 0.75,
                _ => n,
            })
        })
        .unwrap_or(0.0)
}

/// 메모 부분(comments.xml · commentsExtended.xml · commentsIds.xml · commentsExtensible.xml 과 그 관계 파일).
/// 관계 Target(`comments.xml`) · 내용 형식 PartName(`/word/comments.xml`) · zip 이름 어느 꼴이든 받는다.
fn is_comment_part(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with("comments") && (name.ends_with(".xml") || name.ends_with(".xml.rels"))
}

/// 변경 추적을 모두 받아들이고 메모 자리표를 지운다. `count` 에 받아들인 변경 수를 더한다.
/// - 넣은 글 · 옮겨 온 글(`w:ins` · `w:moveTo`): 감싼 것만 벗긴다
/// - 지운 글 · 옮겨 간 글(`w:del` · `w:moveFrom`), 지운 줄(`w:trPr/w:del`) · 지운 칸(`w:cellDel`): 버린다
/// - 서식 변경 기록(`w:rPrChange` 등 옛 서식): 버리면 새 서식이 남는다
/// - 지운 문단 표시(`w:pPr/w:rPr/w:del`): 그 문단을 다음 문단 앞에 붙인다(합친 문단은 다음 문단의 서식)
fn accept_revisions(el: &mut Element, count: &mut usize) {
    let marked = |e: &Element, props: &str, names: &[&str]| {
        e.first(props)
            .is_some_and(|p| p.elements().any(|c| names.contains(&c.name.as_str())))
    };
    let old = std::mem::take(&mut el.children);
    let mut out = Vec::with_capacity(old.len());
    // 문단 표시가 지워진 문단 — 다음 문단과 합칠 때까지 들고 있는다.
    let mut carry: Option<Element> = None;
    for node in old {
        let Node::El(mut child) = node else {
            out.push(node);
            continue;
        };
        let name = child.name.as_str();
        if carry.is_some() && matches!(name, "w:tbl" | "w:sdt" | "w:sectPr" | "w:customXml") {
            out.extend(carry.take().map(Node::El)); // 문단이 아닌 것과는 합치지 않는다
        }
        match name {
            "w:del" | "w:moveFrom" => *count += 1,
            "w:ins" | "w:moveTo" => {
                *count += 1;
                accept_revisions(&mut child, count);
                out.extend(child.children);
            }
            n if n.ends_with("PrChange")
                || matches!(n, "w:tblGridChange" | "w:numberingChange") =>
            {
                *count += 1
            }
            "w:cellIns" | "w:cellMerge" => *count += 1,
            "w:moveFromRangeStart"
            | "w:moveFromRangeEnd"
            | "w:moveToRangeStart"
            | "w:moveToRangeEnd"
            | "w:commentRangeStart"
            | "w:commentRangeEnd"
            | "w:commentReference" => {}
            n if n.starts_with("w:customXml")
                && (n.ends_with("RangeStart") || n.ends_with("RangeEnd")) => {}
            "w:tr" if marked(&child, "w:trPr", &["w:del"]) => *count += 1,
            "w:tc" if marked(&child, "w:tcPr", &["w:cellDel"]) => *count += 1,
            "w:p" => {
                let mark_deleted = child
                    .first("w:pPr")
                    .is_some_and(|p| marked(p, "w:rPr", &["w:del", "w:moveFrom"]));
                accept_revisions(&mut child, count);
                if let Some(previous) = carry.take() {
                    let at = child
                        .children
                        .iter()
                        .position(|n| !matches!(n, Node::El(e) if e.name == "w:pPr"))
                        .unwrap_or(child.children.len());
                    let runs = previous
                        .children
                        .into_iter()
                        .filter(|n| !matches!(n, Node::El(e) if e.name == "w:pPr"));
                    child.children.splice(at..at, runs);
                }
                if mark_deleted {
                    carry = Some(child); // 표시 자체(w:del)는 안에서 셌다
                } else {
                    out.push(Node::El(child));
                }
            }
            "w:r" => {
                accept_revisions(&mut child, count);
                // 메모 표시만 들었던 런은 비었다 — 버린다.
                if child.elements().any(|e| e.name != "w:rPr") {
                    out.push(Node::El(child));
                }
            }
            _ => {
                accept_revisions(&mut child, count);
                out.push(Node::El(child));
            }
        }
    }
    out.extend(carry.map(Node::El));
    el.children = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 메모는 본문 자리표뿐 아니라 부분 · 관계 · 내용 형식까지 남지 않는다.
    #[test]
    fn comments_leave_no_trace() {
        let docx = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sample.docx"
        ))
        .unwrap();
        let (bytes, report) = normalize(&docx, &Renderer::new(&[]), false, &mut |_, _| {}).unwrap();
        assert_eq!(report.comments_removed, 1);
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        assert!(!names.iter().any(|n| n.contains("comments")), "{names:?}");
        for part in [DOCUMENT, RELS, TYPES] {
            let mut text = String::new();
            zip.by_name(part)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            assert!(!text.contains("comment"), "{part}");
            assert!(
                !text.contains("w:del ") && !text.contains("w:ins ") && !text.contains("Change"),
                "{part}"
            );
        }
    }
}
