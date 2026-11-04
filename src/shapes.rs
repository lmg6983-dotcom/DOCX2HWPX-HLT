//! Word 도형(DrawingML `wpg:wgp` 그룹 · `wps:wsp` 도형) → SVG 한 장.
//!
//! hlt-core · HwpForge 는 그룹 안의 그림만(HWPX 는 첫 장만) 읽고 벡터 도형은 버린다. ChemDraw 그림을 붙인 뒤
//! 「도형으로 변환」한 반응식은 결합선 · 고리 · 화살표가 벡터 도형이라 구조식이 통째로 사라진다.
//! 그래서 그룹을 원래 배치대로 SVG 로 모아 그림 한 장으로 바꾼다(`normalize` 가 PNG 로 굽는다).
//!
//! 그리는 것: 안쪽 그룹(`wpg:grpSp`, 좌표 변환 · 회전 · 뒤집기), 그림(`pic:pic`), 사용자 경로(`a:custGeom`:
//! moveTo · lnTo · cubicBezTo · quadBezTo · close), 기본 도형(선 · 연결선 · 사각형 · 타원), 선 굵기 · 색 ·
//! 점선 · 화살 머리, 채움, 글상자 글(`wps:txbx`, 크기 · 굵게 · 기울임 · 위/아래첨자 · 가운데/오른쪽 정렬).
//! 그리지 못한 것은 `skipped` 에 모은다.

use crate::xml::Element;
use std::fmt::Write as _;

const EMU_PER_PT: f64 = 12700.0;

/// 그림 관계 id → (바이트, MIME).
pub type Media<'a> = dyn Fn(&str) -> Option<(Vec<u8>, &'static str)> + 'a;

pub struct Svg {
    pub svg: String,
    /// 그림 크기(pt) — 선 굵기만큼 바깥으로 넓힌 크기.
    pub width_pt: f64,
    pub height_pt: f64,
    /// 그리지 못한 것(사람이 읽는 설명).
    pub skipped: Vec<String>,
}

/// `a:graphicData` 하나(그룹 · 도형)를 `cx` × `cy`(EMU, `wp:extent`) 크기로 그린다.
pub fn to_svg(data: &Element, cx: f64, cy: f64, media: &Media) -> Option<Svg> {
    let mut ctx = Ctx {
        out: String::new(),
        skipped: Vec::new(),
        media,
        defs: String::new(),
        markers: 0,
        max_stroke: 0.0,
    };
    let root = data
        .elements()
        .find(|e| e.name == "wpg:wgp" || e.name == "wps:wsp")?;
    if root.name == "wpg:wgp" {
        ctx.group(root, cx, cy, 1.0, true);
    } else {
        // 도형 하나: 바깥 크기(extent)가 그 도형의 크기다.
        ctx.shape(root, Some((0.0, 0.0, cx, cy)), 1.0);
    }
    if ctx.out.is_empty() {
        return None;
    }
    let pad = ctx.max_stroke.max(EMU_PER_PT * 0.5);
    let (w, h) = (cx + 2.0 * pad, cy + 2.0 * pad);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wpt:.2}pt" height="{hpt:.2}pt" viewBox="{x:.0} {y:.0} {w:.0} {h:.0}"><defs>{defs}</defs>{body}</svg>"#,
        wpt = w / EMU_PER_PT,
        hpt = h / EMU_PER_PT,
        x = -pad,
        y = -pad,
        defs = ctx.defs,
        body = ctx.out,
    );
    Some(Svg {
        svg,
        width_pt: w / EMU_PER_PT,
        height_pt: h / EMU_PER_PT,
        skipped: ctx.skipped,
    })
}

struct Ctx<'a, 'm> {
    out: String,
    skipped: Vec<String>,
    media: &'a Media<'m>,
    defs: String,
    markers: usize,
    /// 그림 가장자리에서 잘리지 않게 넓힐 폭(EMU, 바깥 좌표).
    max_stroke: f64,
}

/// `a:xfrm` 의 off · ext · chOff · chExt · rot · flipH · flipV.
struct Xfrm {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    ch: Option<(f64, f64, f64, f64)>,
    rot: f64,
    flip_h: bool,
    flip_v: bool,
}

fn num(el: Option<&Element>, key: &str) -> f64 {
    el.and_then(|e| e.get(key))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

fn xfrm(props: Option<&Element>) -> Option<Xfrm> {
    let x = props?.first("a:xfrm")?;
    let off = x.first("a:off");
    let ext = x.first("a:ext");
    let ch = x.first("a:chExt").map(|che| {
        let cho = x.first("a:chOff");
        (
            num(cho, "x"),
            num(cho, "y"),
            num(Some(che), "cx"),
            num(Some(che), "cy"),
        )
    });
    Some(Xfrm {
        x: num(off, "x"),
        y: num(off, "y"),
        w: num(ext, "cx"),
        h: num(ext, "cy"),
        ch,
        rot: num(Some(x), "rot") / 60000.0,
        flip_h: x.get("flipH") == Some("1"),
        flip_v: x.get("flipV") == Some("1"),
    })
}

/// 자리(x, y, w, h)에 놓고 가운데를 축으로 돌리고 뒤집는 SVG 변환.
fn place(x: f64, y: f64, w: f64, h: f64, rot: f64, fh: bool, fv: bool) -> String {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut t = format!("translate({cx:.1} {cy:.1})");
    if rot != 0.0 {
        let _ = write!(t, " rotate({rot:.3})");
    }
    if fh || fv {
        let _ = write!(
            t,
            " scale({} {})",
            if fh { -1 } else { 1 },
            if fv { -1 } else { 1 }
        );
    }
    let _ = write!(t, " translate({:.1} {:.1})", -w / 2.0, -h / 2.0);
    t
}

/// 색(`a:srgbClr` · `a:schemeClr` · `a:prstClr`). 테마 색은 흔한 것만 어림한다.
fn color(fill: &Element) -> Option<String> {
    if let Some(c) = fill.first("a:srgbClr") {
        return c.get("val").map(|v| format!("#{v}"));
    }
    if let Some(c) = fill.first("a:schemeClr") {
        return Some(
            match c.get("val").unwrap_or_default() {
                "bg1" | "lt1" | "bg2" | "lt2" => "#ffffff",
                "accent1" => "#4472c4",
                "accent2" => "#ed7d31",
                "accent3" => "#a5a5a5",
                "accent4" => "#ffc000",
                "accent5" => "#5b9bd5",
                "accent6" => "#70ad47",
                _ => "#000000", // tx1 · dk1 · tx2 · dk2 …
            }
            .to_string(),
        );
    }
    if let Some(c) = fill.first("a:prstClr") {
        return c.get("val").map(str::to_string);
    }
    fill.first("a:sysClr")
        .and_then(|c| c.get("lastClr"))
        .map(|v| format!("#{v}"))
}

impl Ctx<'_, '_> {
    /// 그룹: 자식 좌표(chOff · chExt)를 바깥 자리(off · ext)로 옮긴다. `scale` = 바깥 EMU / 자식 EMU(누적).
    fn group(&mut self, g: &Element, cx: f64, cy: f64, scale: f64, top: bool) {
        let props = g.first("wpg:grpSpPr");
        let Some(x) = xfrm(props) else {
            self.skipped.push("크기 없는 그룹".into());
            return;
        };
        // 맨 바깥 그룹은 wp:extent 자리(0, 0, cx, cy)에 놓는다.
        let (ox, oy, w, h) = if top {
            (0.0, 0.0, cx, cy)
        } else {
            (x.x, x.y, x.w, x.h)
        };
        let (chx, chy, chw, chh) = x.ch.unwrap_or((0.0, 0.0, x.w, x.h));
        let sx = if chw > 0.0 { w / chw } else { 1.0 };
        let sy = if chh > 0.0 { h / chh } else { 1.0 };
        let _ = write!(
            self.out,
            r#"<g transform="{} scale({sx:.6} {sy:.6}) translate({:.1} {:.1})">"#,
            place(ox, oy, w, h, x.rot, x.flip_h, x.flip_v),
            -chx,
            -chy
        );
        let inner = scale * (sx * sy).sqrt();
        for child in g.elements() {
            match child.name.as_str() {
                "wpg:grpSp" => self.group(child, 0.0, 0.0, inner, false),
                "wps:wsp" => self.shape(child, None, inner),
                "pic:pic" => self.picture(child),
                "wpg:grpSpPr" | "wpg:cNvGrpSpPr" | "wpg:cNvPr" => {}
                "wpg:graphicFrame" => self.skipped.push("그룹 안의 차트 · 표 틀".into()),
                other => self.skipped.push(format!("그룹 안의 {other}")),
            }
        }
        self.out.push_str("</g>");
    }

    fn picture(&mut self, p: &Element) {
        let Some(x) = xfrm(p.first("pic:spPr")) else {
            return;
        };
        let Some(id) = p.find("a:blip").and_then(|b| b.get("r:embed")) else {
            self.skipped.push("연결된 그림(링크)".into());
            return;
        };
        let Some((bytes, mime)) = (self.media)(id) else {
            self.skipped.push(format!("그림 {id} 없음"));
            return;
        };
        let _ = write!(
            self.out,
            r#"<image transform="{}" width="{:.1}" height="{:.1}" preserveAspectRatio="none" href="data:{mime};base64,{}"/>"#,
            place(x.x, x.y, x.w, x.h, x.rot, x.flip_h, x.flip_v),
            x.w,
            x.h,
            base64(&bytes)
        );
    }

    /// 도형 하나. `at` = 바깥에서 정한 자리(맨 바깥 도형). `scale` = 바깥 EMU / 이 좌표 EMU.
    fn shape(&mut self, s: &Element, at: Option<(f64, f64, f64, f64)>, scale: f64) {
        let props = s.first("wps:spPr");
        let x = xfrm(props);
        let (px, py, w, h, rot, fh, fv) = match (&x, at) {
            (Some(x), None) => (x.x, x.y, x.w, x.h, x.rot, x.flip_h, x.flip_v),
            (Some(x), Some((ax, ay, aw, ah))) => (ax, ay, aw, ah, x.rot, x.flip_h, x.flip_v),
            (None, Some((ax, ay, aw, ah))) => (ax, ay, aw, ah, 0.0, false, false),
            (None, None) => return,
        };
        let style = s.first("wps:style");
        let (stroke, width, dash, head, tail) = self.line(props, style, scale);
        let fill = fill_of(props, style);
        let _ = write!(
            self.out,
            r#"<g transform="{}">"#,
            place(px, py, w, h, rot, fh, fv)
        );
        let fill = fill.unwrap_or_else(|| "none".into());
        let stroke = stroke.unwrap_or_else(|| "none".into());
        let paint = |fill: &str, stroke: &str| {
            format!(
                r#" fill="{fill}" stroke="{stroke}" stroke-width="{width:.1}"{dash} stroke-linecap="round" stroke-linejoin="round"{head}{tail}"#
            )
        };
        if let Some(geo) = props.and_then(|p| p.first("a:custGeom")) {
            for path in geo.find("a:pathLst").into_iter().flat_map(|l| l.elements()) {
                let d = path_data(path, w, h, &mut self.skipped);
                let f = if path.get("fill") == Some("none") {
                    "none"
                } else {
                    &fill
                };
                let st = if matches!(path.get("stroke"), Some("0" | "false")) {
                    "none"
                } else {
                    &stroke
                };
                let _ = write!(self.out, r#"<path d="{d}"{}/>"#, paint(f, st));
            }
        } else {
            let prst = props
                .and_then(|p| p.first("a:prstGeom"))
                .and_then(|g| g.get("prst"))
                .unwrap_or("rect");
            let both = paint(&fill, &stroke);
            let open = paint("none", &stroke);
            let d = match prst {
                "line" | "straightConnector1" => Some((format!("M0 0 L{w:.1} {h:.1}"), &open)),
                "bentConnector2" => Some((format!("M0 0 L{w:.1} 0 L{w:.1} {h:.1}"), &open)),
                "bentConnector3" => {
                    let m = w / 2.0;
                    Some((
                        format!("M0 0 L{m:.1} 0 L{m:.1} {h:.1} L{w:.1} {h:.1}"),
                        &open,
                    ))
                }
                "rect" | "flowChartProcess" | "textBox" => {
                    Some((format!("M0 0 L{w:.1} 0 L{w:.1} {h:.1} L0 {h:.1} Z"), &both))
                }
                "triangle" => Some((
                    format!("M{:.1} 0 L{w:.1} {h:.1} L0 {h:.1} Z", w / 2.0),
                    &both,
                )),
                "rightArrow" => {
                    let (a, b) = (w * 0.7, h * 0.25);
                    Some((
                        format!(
                            "M0 {b:.1} L{a:.1} {b:.1} L{a:.1} 0 L{w:.1} {:.1} L{a:.1} {h:.1} L{a:.1} {:.1} L0 {:.1} Z",
                            h / 2.0,
                            h - b,
                            h - b
                        ),
                        &both,
                    ))
                }
                _ => None,
            };
            match (d, prst) {
                (Some((d, p)), _) => {
                    let _ = write!(self.out, r#"<path d="{d}"{p}/>"#);
                }
                (None, "roundRect") => {
                    let r = w.min(h) * 0.1667;
                    let _ = write!(
                        self.out,
                        r#"<rect width="{w:.1}" height="{h:.1}" rx="{r:.1}"{both}/>"#
                    );
                }
                (None, "ellipse" | "flowChartConnector") => {
                    let _ = write!(
                        self.out,
                        r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}"{both}/>"#,
                        w / 2.0,
                        h / 2.0,
                        w / 2.0,
                        h / 2.0
                    );
                }
                (None, other) => self.skipped.push(format!("도형 모양 {other}")),
            }
        }
        if let Some(text) = s.first("wps:txbx").and_then(|t| t.first("w:txbxContent")) {
            self.text_box(text, s.first("wps:bodyPr"), w, h);
        }
        self.out.push_str("</g>");
        self.max_stroke = self.max_stroke.max(width * scale);
    }

    /// 선: (색, 굵기 EMU — 이 좌표계 단위로, 점선, 머리 표지, 꼬리 표지).
    fn line(
        &mut self,
        props: Option<&Element>,
        style: Option<&Element>,
        scale: f64,
    ) -> (Option<String>, f64, String, String, String) {
        let ln = props.and_then(|p| p.first("a:ln"));
        let from_style = style
            .and_then(|s| s.first("a:lnRef"))
            .filter(|r| r.get("idx").is_some_and(|i| i != "0"));
        let stroke = match ln {
            Some(l) if l.first("a:noFill").is_some() => None,
            Some(l) => l
                .first("a:solidFill")
                .and_then(color)
                .or_else(|| from_style.and_then(color))
                .or_else(|| Some("#000000".into())),
            None => from_style.map(|r| color(r).unwrap_or_else(|| "#000000".into())),
        };
        let w = ln
            .and_then(|l| l.get("w"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(9525.0);
        let width = w / scale.max(1e-9);
        let dash = match ln
            .and_then(|l| l.prop("a:prstDash", "val"))
            .unwrap_or("solid")
        {
            "solid" => String::new(),
            "dot" | "sysDot" => format!(r#" stroke-dasharray="{:.1} {:.1}""#, width, width * 2.0),
            _ => format!(
                r#" stroke-dasharray="{:.1} {:.1}""#,
                width * 4.0,
                width * 3.0
            ),
        };
        let mut end = |key: &str, at: &str| -> String {
            let Some(e) = ln.and_then(|l| l.first(key)) else {
                return String::new();
            };
            let kind = e.get("type").unwrap_or("none");
            if kind == "none" || stroke.is_none() {
                return String::new();
            }
            let size = |k: &str| match e.get(k).unwrap_or("med") {
                "sm" => 2.0,
                "lg" => 5.0,
                _ => 3.0,
            };
            let (len, wid) = (size("len"), size("w"));
            self.markers += 1;
            let id = format!("m{}", self.markers);
            let color = stroke.clone().unwrap_or_default();
            // 표지 좌표는 선 굵기 단위(markerUnits=strokeWidth). 끝점이 화살 끝.
            let shape = match kind {
                "oval" => format!(
                    r#"<ellipse cx="{:.2}" cy="{:.2}" rx="{:.2}" ry="{:.2}" fill="{color}"/>"#,
                    len / 2.0,
                    wid / 2.0,
                    len / 2.0,
                    wid / 2.0
                ),
                "diamond" => format!(
                    r#"<path d="M0 {:.2} L{:.2} 0 L{len:.2} {:.2} L{:.2} {wid:.2} Z" fill="{color}"/>"#,
                    wid / 2.0,
                    len / 2.0,
                    wid / 2.0,
                    len / 2.0
                ),
                "arrow" => format!(
                    r#"<path d="M0 0 L{len:.2} {:.2} L0 {wid:.2}" fill="none" stroke="{color}" stroke-width="0.8"/>"#,
                    wid / 2.0
                ),
                "stealth" => format!(
                    r#"<path d="M0 0 L{len:.2} {:.2} L0 {wid:.2} L{:.2} {:.2} Z" fill="{color}"/>"#,
                    wid / 2.0,
                    len * 0.3,
                    wid / 2.0
                ),
                _ => format!(
                    r#"<path d="M0 0 L{len:.2} {:.2} L0 {wid:.2} Z" fill="{color}"/>"#,
                    wid / 2.0
                ),
            };
            let orient = if at == "start" {
                "auto-start-reverse"
            } else {
                "auto"
            };
            let _ = write!(
                self.defs,
                r#"<marker id="{id}" markerUnits="strokeWidth" markerWidth="{len:.2}" markerHeight="{wid:.2}" refX="{:.2}" refY="{:.2}" orient="{orient}" overflow="visible">{shape}</marker>"#,
                len * 0.9,
                wid / 2.0
            );
            format!(r#" marker-{at}="url(#{id})""#)
        };
        let head = end("a:headEnd", "start");
        let tail = end("a:tailEnd", "end");
        (stroke, width, dash, head, tail)
    }

    /// 글상자 글: 문단마다 한 줄씩(줄바꿈은 하지 않는다 — 글상자 폭을 넘으면 넘친다).
    fn text_box(&mut self, content: &Element, body: Option<&Element>, w: f64, h: f64) {
        let inset = |k: &str, d: f64| {
            body.and_then(|b| b.get(k))
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(d)
        };
        let (l, t, r) = (
            inset("lIns", 91440.0),
            inset("tIns", 45720.0),
            inset("rIns", 91440.0),
        );
        let mut y = t;
        let mut lines = String::new();
        for p in content.elements().filter(|e| e.name == "w:p") {
            let jc = p
                .first("w:pPr")
                .and_then(|pp| pp.prop("w:jc", "w:val"))
                .unwrap_or("left");
            let (anchor, x) = match jc {
                "center" => ("middle", w / 2.0),
                "right" | "end" => ("end", w - r),
                _ => ("start", l),
            };
            let mut spans = String::new();
            let mut tallest: f64 = 0.0;
            for run in p.elements().filter(|e| e.name == "w:r") {
                let text = run.text();
                if text.is_empty() {
                    continue;
                }
                let rpr = run.first("w:rPr");
                let pt = rpr
                    .and_then(|r| r.prop("w:sz", "w:val"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .map_or(11.0, |half| half / 2.0);
                tallest = tallest.max(pt);
                let script = rpr.and_then(|r| r.prop("w:vertAlign", "w:val"));
                let (size, shift) = match script {
                    Some("subscript") => (pt * 0.65, r#" baseline-shift="-30%""#),
                    Some("superscript") => (pt * 0.65, r#" baseline-shift="40%""#),
                    _ => (pt, ""),
                };
                let mut attrs = format!(r#" font-size="{:.0}"{shift}"#, size * EMU_PER_PT);
                if rpr.is_some_and(|r| r.first("w:b").is_some()) {
                    attrs.push_str(r#" font-weight="bold""#);
                }
                if rpr.is_some_and(|r| r.first("w:i").is_some()) {
                    attrs.push_str(r#" font-style="italic""#);
                }
                if let Some(c) = rpr
                    .and_then(|r| r.prop("w:color", "w:val"))
                    .filter(|c| *c != "auto")
                {
                    let _ = write!(attrs, r##" fill="#{c}""##);
                }
                let _ = write!(spans, "<tspan{attrs}>{}</tspan>", escape(&text));
            }
            let size = if tallest > 0.0 { tallest } else { 11.0 };
            y += size * EMU_PER_PT;
            if !spans.is_empty() && y <= h + size * EMU_PER_PT {
                let _ = write!(
                    lines,
                    r#"<text x="{x:.0}" y="{y:.0}" text-anchor="{anchor}">{spans}</text>"#
                );
            }
            y += size * EMU_PER_PT * 0.2;
        }
        if !lines.is_empty() {
            let _ = write!(
                self.out,
                r##"<g font-family="Malgun Gothic, Batang, Arial, Times New Roman, Noto Sans CJK KR, sans-serif" fill="#000000">{lines}</g>"##
            );
        }
    }
}

fn fill_of(props: Option<&Element>, style: Option<&Element>) -> Option<String> {
    let p = props?;
    if p.first("a:noFill").is_some() {
        return None;
    }
    if let Some(f) = p.first("a:solidFill") {
        return color(f);
    }
    if p.first("a:gradFill").is_some() || p.first("a:pattFill").is_some() {
        return Some("#bfbfbf".into());
    }
    // 채움을 적지 않았으면 스타일의 fillRef(idx 0 = 없음).
    style
        .and_then(|s| s.first("a:fillRef"))
        .filter(|r| r.get("idx").is_some_and(|i| i != "0"))
        .and_then(color)
}

/// custGeom 경로 → SVG `d`. 경로 좌표(w · h)를 도형 크기로 늘린다.
fn path_data(path: &Element, w: f64, h: f64, skipped: &mut Vec<String>) -> String {
    let pw: f64 = path.get("w").and_then(|v| v.parse().ok()).unwrap_or(w);
    let ph: f64 = path.get("h").and_then(|v| v.parse().ok()).unwrap_or(h);
    let sx = if pw > 0.0 { w / pw } else { 1.0 };
    let sy = if ph > 0.0 { h / ph } else { 1.0 };
    let pt = |e: &Element| {
        format!(
            "{:.1} {:.1}",
            e.get("x")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
                * sx,
            e.get("y")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
                * sy
        )
    };
    let mut d = String::new();
    for cmd in path.elements() {
        let pts: Vec<String> = cmd
            .elements()
            .filter(|e| e.name == "a:pt")
            .map(pt)
            .collect();
        let op = match cmd.name.as_str() {
            "a:moveTo" => "M",
            "a:lnTo" => "L",
            "a:cubicBezTo" => "C",
            "a:quadBezTo" => "Q",
            "a:close" => "Z",
            other => {
                skipped.push(format!("경로 명령 {other}"));
                continue;
            }
        };
        let _ = write!(d, "{op}{} ", pts.join(" "));
    }
    d.trim_end().to_string()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn group_path_lands_in_place() {
        let xml = br#"<a:graphicData xmlns:a="a" xmlns:wpg="g" xmlns:wps="s"><wpg:wgp><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="200" cy="100"/><a:chOff x="0" y="0"/><a:chExt cx="400" cy="200"/></a:xfrm></wpg:grpSpPr>
            <wps:wsp><wps:spPr><a:xfrm><a:off x="100" y="50"/><a:ext cx="200" cy="100"/></a:xfrm><a:custGeom><a:pathLst><a:path w="20" h="10"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="20" y="10"/></a:lnTo></a:path></a:pathLst></a:custGeom><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:tailEnd type="triangle"/></a:ln></wps:spPr></wps:wsp></wpg:wgp></a:graphicData>"#;
        let el = crate::xml::parse(xml).unwrap();
        let svg = to_svg(&el, 200.0, 100.0, &|_| None).unwrap();
        assert!(svg.skipped.is_empty(), "{:?}", svg.skipped);
        assert!(svg.svg.contains("scale(0.500000 0.500000)"), "{}", svg.svg);
        assert!(
            svg.svg.contains(r#"d="M0.0 0.0 L200.0 100.0""#),
            "{}",
            svg.svg
        );
        assert!(svg.svg.contains("marker-end"), "{}", svg.svg);
        // 그룹이 절반으로 줄이니 선 굵기는 자식 좌표에서 두 배여야 바깥에서 9525 로 보인다.
        assert!(svg.svg.contains(r#"stroke-width="19050.0""#), "{}", svg.svg);
    }
}
