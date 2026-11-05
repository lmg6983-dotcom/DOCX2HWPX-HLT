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
use crate::messages;
use crate::render::{Png, Renderer};
use crate::xml::{self, Element, Node};

const DOCUMENT: &str = "word/document.xml";
const RELS: &str = "word/_rels/document.xml.rels";
const TYPES: &str = "[Content_Types].xml";
const COMMENTS: &str = "word/comments.xml";
const STYLES: &str = "word/styles.xml";
const IMAGE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const EMU_PER_PT: f64 = 12700.0;
/// 크기를 적지 않은 글의 크기 — hlt-core 가 HLT 본문에 쓰는 값과 같다.
pub const DEFAULT_SIZE_PT: f64 = 12.0;
/// 수식 그림의 최소 글자 크기. 첨자가 읽히도록 본문보다 크게 둔다.
pub const MIN_MATH_PT: f64 = 14.0;

/// (코드, 설명) 목록. 코드는 AGENTS.md 의 표와 같다.
pub type Warnings = Vec<(&'static str, String)>;

#[derive(Default, Debug)]
pub struct Report {
    pub equations: usize,
    pub metafiles: usize,
    /// 그림 한 장으로 바꾼 Word 도형 · 그룹 수
    pub shapes: usize,
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
    /// 원래 DOCX 의 부분(그룹 안 그림을 읽는다).
    parts: HashMap<String, Vec<u8>>,
    /// 지금까지 만난 표 수(경고에 「문서의 n번째 표」로 쓴다).
    tables: usize,
    /// 지금 걷는 자리: 가장 가까운 앞의 식별 번호(【0012】 …). 경고에 「【0012】 부근」으로 쓴다.
    place: String,
    /// 그림 관계 id → 그 그림이 처음 나오는 자리(식별 번호).
    image_places: HashMap<String, String>,
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
        parts: parts.iter().cloned().collect(),
        tables: 0,
        place: String::new(),
        image_places: image_places(&document),
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
    n.fix_image_relationships(&mut document);
    if let Some(styles) = get(STYLES).and_then(|b| xml::parse(b).ok()) {
        apply_style_symbol_fonts(&mut document, &style_fonts(&styles));
    }
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
            let at = rel
                .get("Id")
                .and_then(|id| self.image_places.get(id))
                .cloned()
                .unwrap_or_default();
            let png = crate::render::metafile_to_png(bytes, is_emf).or_else(|e| {
                self.report.warnings.push((
                    "metafile_placeholder",
                    messages::metafile_placeholder(&at, target, &e),
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
                Err(e) => self.report.warnings.push((
                    "metafile_failed",
                    messages::metafile_failed(&at, target, &e),
                )),
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

    /// hlt-core 는 그림 관계를 모두 열어 크기를 읽고, 하나라도 못 읽으면 문서 전체를 멈춘다
    /// (`structure_invalid (docx_relationships · docx_document)`). 그래서 미리
    /// - SVG 그림은 PNG 로 바꾸고
    /// - 파일이 없거나 · 읽을 수 없거나 · 링크(`TargetMode="External"`)로만 연결된 그림은
    ///   쓰이면 「그림 없음」 자리표로(경고 `image_missing`), 쓰이지 않으면 관계를 지운다
    /// - 본문의 `a:blip` 이 `r:link` 만 가지면 `r:embed` 로, VML 이 `o:relid` 만 가지면 `r:id` 로,
    ///   없는 관계를 가리키면 자리표 관계로 잇는다.
    fn fix_image_relationships(&mut self, document: &mut Element) {
        let mut used = std::collections::HashSet::new();
        collect_image_refs(document, &mut used);
        let mut placeholder: Option<String> = None;
        let mut retarget: Vec<(String, Option<String>)> = Vec::new(); // (관계 id, 새 Target · None = 지움)
        let rels: Vec<Element> = self.rels.elements().cloned().collect();
        for rel in &rels {
            if !rel.get("Type").is_some_and(|t| t.ends_with("/image")) {
                continue;
            }
            let id = rel.get("Id").unwrap_or_default().to_string();
            let target = rel.get("Target").unwrap_or_default().to_string();
            let external = rel
                .get("TargetMode")
                .is_some_and(|m| m.eq_ignore_ascii_case("external"));
            use messages::MissingPicture as Why;
            let problem = if external {
                Some(Why::Linked)
            } else {
                match self.part_bytes(&target) {
                    None => Some(Why::NotInFile),
                    Some(bytes) if readable(&bytes) => None,
                    Some(bytes) if looks_svg(&target, &bytes) => {
                        match crate::render::svg_image_to_png(&bytes) {
                            Ok(png) => {
                                let n = self.new_files.len() + 1;
                                let name = format!("media/d2h_svg{n}.png");
                                self.new_files.push((format!("word/{name}"), png));
                                retarget.push((id.clone(), Some(name)));
                                None
                            }
                            Err(_) => Some(Why::Svg),
                        }
                    }
                    Some(_) => Some(Why::Unreadable),
                }
            };
            let Some(problem) = problem else { continue };
            if !used.contains(&id) {
                retarget.push((id, None));
                continue;
            }
            let name = match &placeholder {
                Some(name) => name.clone(),
                None => match self.renderer.glyph("[그림 없음]", DEFAULT_SIZE_PT) {
                    Ok(png) => {
                        let name = "media/d2h_missing.png".to_string();
                        self.new_files.push((format!("word/{name}"), png.bytes));
                        placeholder = Some(name.clone());
                        name
                    }
                    Err(_) => {
                        retarget.push((id, None));
                        continue;
                    }
                },
            };
            self.report.warnings.push((
                "image_missing",
                messages::picture_missing(
                    self.image_places.get(&id).map_or("", String::as_str),
                    &target,
                    problem,
                ),
            ));
            retarget.push((id, Some(name)));
        }
        self.rels.children.retain(|n| {
            !matches!(n, Node::El(r) if retarget.iter().any(|(id, to)| to.is_none() && r.get("Id") == Some(id.as_str())))
        });
        for node in &mut self.rels.children {
            let Node::El(rel) = node else { continue };
            if let Some((_, Some(to))) = retarget
                .iter()
                .find(|(id, _)| rel.get("Id") == Some(id.as_str()))
            {
                rel.attrs.retain(|(k, _)| k != "TargetMode");
                for (k, v) in &mut rel.attrs {
                    if k == "Target" {
                        *v = to.clone();
                    }
                }
            }
        }
        // 본문 쪽: r:link 만 · o:relid 만 · 없는 관계
        let ids: std::collections::HashSet<String> = self
            .rels
            .elements()
            .filter_map(|r| r.get("Id").map(str::to_string))
            .collect();
        drop_dangling_svg(document, &ids);
        let mut dangling = 0;
        fix_image_refs(document, &ids, &mut |_| {
            dangling += 1;
        });
        if dangling > 0 {
            let id = "rIdD2hMissing".to_string();
            let target = match &placeholder {
                Some(name) => Some(name.clone()),
                None => self
                    .renderer
                    .glyph("[그림 없음]", DEFAULT_SIZE_PT)
                    .ok()
                    .map(|png| {
                        let name = "media/d2h_missing.png".to_string();
                        self.new_files.push((format!("word/{name}"), png.bytes));
                        name
                    }),
            };
            if let Some(target) = target {
                self.rels.children.push(Node::El(
                    Element::new("Relationship")
                        .attr("Id", &id)
                        .attr("Type", IMAGE_REL)
                        .attr("Target", &target),
                ));
                point_dangling(document, &ids, &id);
                self.report
                    .warnings
                    .push(("image_missing", messages::pictures_dangling(dangling)));
            }
        }
    }

    /// 관계 Target → 바이트(이미 바꾼 그림 먼저).
    fn part_bytes(&self, target: &str) -> Option<Vec<u8>> {
        let path = format!(
            "word/{}",
            target.trim_start_matches('/').trim_start_matches("word/")
        );
        self.new_files
            .iter()
            .find(|(n, _)| *n == path)
            .map(|(_, b)| b.clone())
            .or_else(|| self.parts.get(&path).cloned())
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
                    if let Some(marker) = messages::marker_of(&child.text()) {
                        self.place = marker;
                    }
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
                "w:tbl" => {
                    self.tables += 1;
                    let n = self.tables;
                    if !crate::tables::has_rows(&child) {
                        self.report
                            .warnings
                            .push(("table_repaired", messages::table_empty(&self.place, n)));
                        continue;
                    }
                    if self.nk_chars {
                        let fixed = crate::tables::repair(&mut child);
                        if !fixed.fixes.is_empty() {
                            self.report.warnings.push((
                                "table_repaired",
                                messages::table_repaired(&self.place, n, &fixed.fixes),
                            ));
                        }
                    }
                    self.walk(&mut child);
                    out.push(Node::El(child));
                }
                "w:tc" if self.nk_chars => {
                    self.walk(&mut child);
                    self.flatten_cell(&mut child);
                    out.push(Node::El(child));
                }
                "w:drawing" => match self.flatten_shapes(&child) {
                    Some(drawing) => out.push(Node::El(drawing)),
                    None => {
                        self.walk(&mut child);
                        out.push(Node::El(child));
                    }
                },
                "w:object" | "w:pict" => match self.vml_image(&child) {
                    Some(drawing) => out.push(Node::El(drawing)),
                    None => {
                        self.walk(&mut child);
                        out.push(Node::El(child));
                    }
                },
                "w:r" => {
                    self.walk(&mut child);
                    special_chars(&mut child);
                    self.symbol_font_text(&mut child);
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
                messages::nested_table(&self.place),
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
        let size = math_size(el).unwrap_or(DEFAULT_SIZE_PT).max(MIN_MATH_PT);
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
                    messages::equation_as_text(&self.place, &format!("{e} ← {body}")),
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

    /// 관계 id → 그림 바이트(WMF/EMF 는 이미 바꾼 PNG) · MIME.
    fn media(&self, rel_id: &str) -> Option<(Vec<u8>, &'static str)> {
        let target = self
            .rels
            .elements()
            .find(|r| r.get("Id") == Some(rel_id))?
            .get("Target")?;
        let path = format!(
            "word/{}",
            target.trim_start_matches('/').trim_start_matches("word/")
        );
        let bytes = self
            .new_files
            .iter()
            .find(|(n, _)| *n == path)
            .map(|(_, b)| b.clone())
            .or_else(|| self.parts.get(&path).cloned())?;
        let mime = match bytes.get(..4) {
            Some([0x89, b'P', b'N', b'G']) => "image/png",
            Some([0xFF, 0xD8, ..]) => "image/jpeg",
            Some([b'G', b'I', b'F', _]) => "image/gif",
            _ => return None,
        };
        Some((bytes, mime))
    }

    /// Word 그룹 도형 · 벡터 도형(`wpg:wgp` · `wps:wsp`)을 원래 배치대로 그림 한 장으로 바꾼다.
    /// 글상자만 있는 도형(그룹 밖)은 그대로 둔다 — 글을 그림으로 굳히지 않는다.
    fn flatten_shapes(&mut self, drawing: &Element) -> Option<Element> {
        let frame = drawing
            .elements()
            .find(|e| e.name == "wp:inline" || e.name == "wp:anchor")?;
        let data = frame.first("a:graphic")?.first("a:graphicData")?;
        let root = data
            .elements()
            .find(|e| e.name == "wpg:wgp" || e.name == "wps:wsp")?;
        if root.name == "wps:wsp" && root.first("wps:txbx").is_some() {
            return None;
        }
        let extent = frame.first("wp:extent")?;
        let num = |k: &str| {
            extent
                .get(k)
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        let (cx, cy) = (num("cx"), num("cy"));
        let name = frame
            .first("wp:docPr")
            .and_then(|d| d.get("name"))
            .unwrap_or("도형")
            .to_string();
        let svg = crate::shapes::to_svg(data, cx, cy, &|id| self.media(id))?;
        for what in &svg.skipped {
            self.report
                .warnings
                .push(("shape_partial", messages::shape_partial(&self.place, what)));
        }
        let png = match crate::render::shapes_to_png(&svg.svg, svg.width_pt, svg.height_pt) {
            Ok(png) => png,
            Err(e) => {
                self.report.warnings.push((
                    "shape_failed",
                    messages::shape_failed(&self.place, &format!("{name}: {e}")),
                ));
                return None;
            }
        };
        let id = self.add_png(png);
        let (w, h) = (
            (svg.width_pt * EMU_PER_PT).round().to_string(),
            (svg.height_pt * EMU_PER_PT).round().to_string(),
        );
        let mut frame = frame.clone();
        for node in &mut frame.children {
            let Node::El(e) = node else { continue };
            match e.name.as_str() {
                "wp:extent" => *e = Element::new("wp:extent").attr("cx", &w).attr("cy", &h),
                "a:graphic" => *e = self.pic_graphic(&id, &w, &h, &format!("{name}.png")),
                _ => {}
            }
        }
        self.report.shapes += 1;
        Some(Element::new("w:drawing").child(frame))
    }

    /// 그림 하나를 담은 `a:graphic`.
    fn pic_graphic(&mut self, rel_id: &str, cx: &str, cy: &str, name: &str) -> Element {
        self.next_id += 1;
        let id = (100_000 + self.next_id).to_string();
        let pic = Element::new("pic:pic")
            .attr(
                "xmlns:pic",
                "http://schemas.openxmlformats.org/drawingml/2006/picture",
            )
            .child(
                Element::new("pic:nvPicPr")
                    .child(Element::new("pic:cNvPr").attr("id", &id).attr("name", name))
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
                            .child(Element::new("a:ext").attr("cx", cx).attr("cy", cy)),
                    )
                    .child(
                        Element::new("a:prstGeom")
                            .attr("prst", "rect")
                            .child(Element::new("a:avLst")),
                    ),
            );
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
            )
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

    /// 글꼴이 Symbol · Wingdings 인 런의 글(`a` 가 α, `Ò` 가 ®, U+F061 이 α 로 보인다) → 실제 유니코드 글자.
    /// Word 밖에서 붙여 넣은 글 · 옛 문서에 흔하다. 그대로 두면 HLT/HWPX 에서는 글꼴이 빠져 `a` · `Ò` 로 남는다.
    /// 바꾼 뒤 그 글꼴은 런에서 뗀다(바꾼 글자를 다시 기호 글꼴로 그리지 않게).
    fn symbol_font_text(&mut self, run: &mut Element) {
        let Some(font) = run
            .first("w:rPr")
            .and_then(|p| p.first("w:rFonts"))
            .and_then(|f| {
                f.get("w:ascii")
                    .or_else(|| f.get("w:hAnsi"))
                    .or_else(|| f.get("w:cs"))
            })
            .map(str::to_string)
        else {
            return;
        };
        let Some(map) = chars::symbol_font_map(&font) else {
            return;
        };
        let mut unmapped = Vec::new();
        for node in &mut run.children {
            let Node::El(el) = node else { continue };
            if el.name != "w:t" {
                continue;
            }
            let text: String = el
                .text()
                .chars()
                .map(|c| {
                    let code = c as u32;
                    if !(0x20..=0xFF).contains(&code) && !(0xF020..=0xF0FF).contains(&code) {
                        return c;
                    }
                    map(code).unwrap_or_else(|| {
                        unmapped.push(code);
                        c
                    })
                })
                .collect();
            *el = text_el(&text);
        }
        for code in unmapped {
            self.report.warnings.push((
                "symbol_unmapped",
                messages::symbol_unmapped(&self.place, &font, code),
            ));
        }
        if let Some(Node::El(rpr)) = run
            .children
            .iter_mut()
            .find(|n| matches!(n, Node::El(e) if e.name == "w:rPr"))
        {
            rpr.children
                .retain(|n| !matches!(n, Node::El(e) if e.name == "w:rFonts"));
        }
    }

    /// `w:sym` → `w:t`. Symbol · Wingdings 글꼴만 안다.
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
            let text = chars::symbol_font_map(&font).and_then(|map| map(code));
            match text {
                Some(c) => *el = Element::new("w:t").text_child(&c.to_string()),
                None => self.report.warnings.push((
                    "symbol_unmapped",
                    messages::symbol_unmapped(&self.place, &font, code),
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
                                    messages::glyph_failed(&self.place, &cluster, &e),
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

/// 스타일 id → 그 스타일(과 basedOn 조상)이 정한 글꼴(`w:rFonts` 의 ascii · hAnsi).
fn style_fonts(styles: &Element) -> HashMap<String, Option<String>> {
    let mut own: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for style in styles.elements().filter(|e| e.name == "w:style") {
        let Some(id) = style.get("w:styleId") else {
            continue;
        };
        let font = style
            .first("w:rPr")
            .and_then(|p| p.first("w:rFonts"))
            .and_then(|f| f.get("w:ascii").or_else(|| f.get("w:hAnsi")))
            .map(str::to_string);
        let based = style.prop("w:basedOn", "w:val").map(str::to_string);
        own.insert(id.to_string(), (font, based));
    }
    own.keys()
        .map(|id| {
            let mut at = Some(id.clone());
            let mut font = None;
            for _ in 0..32 {
                let Some((f, based)) = at.as_ref().and_then(|a| own.get(a)) else {
                    break;
                };
                if f.is_some() {
                    font = f.clone();
                    break;
                }
                at = based.clone();
            }
            (id.clone(), font)
        })
        .collect()
}

/// 스타일로만 Symbol · Wingdings · MT Extra 글꼴을 받은 런에 그 글꼴을 직접 단다 — 그래야 `symbol_font_text` 가
/// 글자를 바꾼다. 런에 직접 단 글꼴이 있으면 그것이 이긴다. 글자 스타일이 다른 글꼴을 정하면 문단 스타일보다 앞선다.
fn apply_style_symbol_fonts(el: &mut Element, fonts: &HashMap<String, Option<String>>) {
    if !fonts
        .values()
        .flatten()
        .any(|f| chars::symbol_font_map(f).is_some())
    {
        return;
    }
    fn runs(el: &mut Element, paragraph: Option<&str>, fonts: &HashMap<String, Option<String>>) {
        for node in &mut el.children {
            let Node::El(child) = node else { continue };
            match child.name.as_str() {
                "w:p" => {
                    let style = child
                        .first("w:pPr")
                        .and_then(|p| p.prop("w:pStyle", "w:val"))
                        .and_then(|id| fonts.get(id).cloned().flatten());
                    runs(child, style.as_deref(), fonts);
                }
                "w:r" => {
                    let rpr = child.first("w:rPr");
                    let direct = rpr
                        .and_then(|p| p.first("w:rFonts"))
                        .is_some_and(|f| f.get("w:ascii").is_some() || f.get("w:hAnsi").is_some());
                    if direct {
                        continue;
                    }
                    let font = rpr
                        .and_then(|p| p.prop("w:rStyle", "w:val"))
                        .and_then(|id| fonts.get(id).cloned().flatten())
                        .or_else(|| paragraph.map(str::to_string));
                    let Some(font) = font.filter(|f| chars::symbol_font_map(f).is_some()) else {
                        continue;
                    };
                    if child.first("w:rPr").is_none() {
                        child.children.insert(0, Node::El(Element::new("w:rPr")));
                    }
                    if let Some(Node::El(rpr)) = child
                        .children
                        .iter_mut()
                        .find(|n| matches!(n, Node::El(e) if e.name == "w:rPr"))
                    {
                        rpr.children
                            .retain(|n| !matches!(n, Node::El(e) if e.name == "w:rFonts"));
                        rpr.children.insert(
                            0,
                            Node::El(
                                Element::new("w:rFonts")
                                    .attr("w:ascii", &font)
                                    .attr("w:hAnsi", &font),
                            ),
                        );
                    }
                }
                _ => runs(child, paragraph, fonts),
            }
        }
    }
    runs(el, None, fonts);
}

/// 그림 관계 id → 그 그림이 처음 나오는 자리(가장 가까운 앞의 식별 번호).
fn image_places(document: &Element) -> HashMap<String, String> {
    fn walk(el: &Element, last: &mut String, out: &mut HashMap<String, String>) {
        if el.name == "w:p"
            && let Some(m) = messages::marker_of(&el.text())
        {
            *last = m;
        }
        let id = match el.name.as_str() {
            "a:blip" => el.get("r:embed").or_else(|| el.get("r:link")),
            "v:imagedata" => el.get("r:id").or_else(|| el.get("o:relid")),
            _ => None,
        };
        if let Some(id) = id {
            out.entry(id.to_string()).or_insert_with(|| last.clone());
        }
        for child in el.elements() {
            walk(child, last, out);
        }
    }
    let mut out = HashMap::new();
    walk(document, &mut String::new(), &mut out);
    out
}

/// hlt-core 와 같은 방법(image 크레이트, 같은 판 · 같은 기능)으로 그림 크기를 읽을 수 있는가.
fn readable(bytes: &[u8]) -> bool {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .is_some_and(|(w, h)| w > 0 && h > 0)
}

fn looks_svg(target: &str, bytes: &[u8]) -> bool {
    target.to_ascii_lowercase().ends_with(".svg")
        || String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).contains("<svg")
}

/// 본문이 그림으로 쓰는 관계 id: `a:blip` 의 r:embed(없으면 r:link), VML 의 r:id(없으면 o:relid).
/// SVG 원본(`asvg:svgBlip`)은 PNG 대체 그림이 있는 보조라 세지 않는다.
fn collect_image_refs(el: &Element, out: &mut std::collections::HashSet<String>) {
    let pick = match el.name.as_str() {
        "a:blip" => el.get("r:embed").or_else(|| el.get("r:link")),
        "v:imagedata" => el.get("r:id").or_else(|| el.get("o:relid")),
        _ => None,
    };
    if let Some(v) = pick {
        out.insert(v.to_string());
    }
    for child in el.elements() {
        collect_image_refs(child, out);
    }
}

/// 남은 관계에 없는 SVG 원본(`asvg:svgBlip`)을 뗀다 — PNG 대체 그림이 그대로 쓰인다.
fn drop_dangling_svg(el: &mut Element, ids: &std::collections::HashSet<String>) {
    el.children.retain(|n| {
        !matches!(n, Node::El(e) if e.name.ends_with(":svgBlip")
            && e.get("r:embed").is_some_and(|id| !ids.contains(id)))
    });
    for node in &mut el.children {
        if let Node::El(child) = node {
            drop_dangling_svg(child, ids);
        }
    }
}

/// `a:blip` 이 r:link 만 → r:embed, VML 이 o:relid 만 → r:id. 없는 관계는 `dangling` 으로 센다.
fn fix_image_refs(
    el: &mut Element,
    ids: &std::collections::HashSet<String>,
    dangling: &mut dyn FnMut(&str),
) {
    let key = match el.name.as_str() {
        "a:blip" => Some(("r:embed", "r:link")),
        "v:imagedata" => Some(("r:id", "o:relid")),
        _ => None,
    };
    if let Some((main, alt)) = key {
        if el.get(main).is_none()
            && let Some(v) = el.get(alt).map(str::to_string)
        {
            el.attrs.push((main.to_string(), v));
        }
        if let Some(v) = el.get(main)
            && !ids.contains(v)
        {
            dangling(v);
        }
    }
    for node in &mut el.children {
        if let Node::El(child) = node {
            fix_image_refs(child, ids, dangling);
        }
    }
}

/// 없는 관계를 가리키는 그림을 자리표 관계로.
fn point_dangling(el: &mut Element, ids: &std::collections::HashSet<String>, to: &str) {
    let main = match el.name.as_str() {
        "a:blip" => Some("r:embed"),
        "v:imagedata" => Some("r:id"),
        _ => None,
    };
    if let Some(main) = main {
        for (k, v) in &mut el.attrs {
            if k == main && !ids.contains(v.as_str()) {
                *v = to.to_string();
            }
        }
    }
    for node in &mut el.children {
        if let Node::El(child) = node {
            point_dangling(child, ids, to);
        }
    }
}

/// 글자 대신 쓰는 Word 요소: 줄 안 나뉘는 하이픈(Ctrl+Shift+-) → `-`, 선택 하이픈 → 지움.
/// hlt-core · HwpForge 는 이 요소를 읽지 않아 `1‑2` 가 `12` 가 된다.
fn special_chars(run: &mut Element) {
    run.children.retain_mut(|node| match node {
        Node::El(el) if el.name == "w:noBreakHyphen" => {
            *el = text_el("-");
            true
        }
        Node::El(el) => el.name != "w:softHyphen",
        Node::Text(_) => true,
    });
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
