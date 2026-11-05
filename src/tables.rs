//! 표 구조 바로잡기(HLT). hlt-core 의 DOCX 표 판독은 엄격해서 한 표라도 어긋나면 문서 전체가
//! `geometry_invalid (docx_table)` 로 멈춘다(화학 명세서처럼 표 · 병합이 많은 문서에서 보고됨).
//! Word 는 아래 꼴을 모두 그럴듯하게 그리므로, Word 가 그리는 모습에 가깝게 미리 고쳐 넘긴다.
//!
//! - 줄 · 칸을 감싼 요소(`w:sdt` 콘텐츠 컨트롤 · `w:customXml`)를 벗긴다
//! - 열 너비(`w:gridCol`)가 없거나 소수면 정수로, 빠진 너비는 칸 너비(`w:tcW`)나 평균으로 채운다
//! - 줄 앞뒤 빈 자리(`w:gridBefore` · `w:gridAfter`)와 칸이 모자란 줄은 빈 칸으로 채운다
//! - 칸이 넘치는 줄이 있으면 열을 늘린다
//! - 칸 합치기(`w:gridSpan`)가 0 · 소수 · 표 밖으로 넘치면 바로잡는다
//! - 세로 병합(`w:vMerge`)의 이어지는 칸 위에 같은 폭의 병합이 없으면 새 병합의 시작으로 바꾼다
//! - 표 들여쓰기(`w:tblInd`)가 소수면 정수로, 문단 없는 칸에는 빈 문단을 넣는다

use crate::xml::{Element, Node};

/// 고친 것(사람이 읽는 설명, 같은 것은 한 번).
#[derive(Default, Debug)]
pub struct Repair {
    pub fixes: Vec<&'static str>,
}

impl Repair {
    fn add(&mut self, what: &'static str) {
        if !self.fixes.contains(&what) {
            self.fixes.push(what);
        }
    }
}

pub fn has_rows(tbl: &Element) -> bool {
    let mut t = tbl.clone();
    unwrap_wrappers(&mut t, &mut Repair::default());
    t.elements().any(|e| e.name == "w:tr")
}

/// 정수 값(소수는 반올림). 못 읽으면 `None`.
fn int(v: Option<&str>) -> Option<i64> {
    let v = v?.trim();
    v.parse::<i64>().ok().or_else(|| {
        v.parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(|f| f.round() as i64)
    })
}

fn set_attr(el: &mut Element, key: &str, value: &str) {
    if let Some(slot) = el.attrs.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value.to_string();
    } else {
        el.attrs.push((key.to_string(), value.to_string()));
    }
}

fn child_mut<'a>(el: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    el.children.iter_mut().find_map(|n| match n {
        Node::El(e) if e.name == name => Some(e),
        _ => None,
    })
}

/// `w:tbl` · `w:tr` 바로 아래의 `w:sdt`(→ `w:sdtContent`) · `w:customXml` 를 벗긴다.
fn unwrap_wrappers(el: &mut Element, repair: &mut Repair) {
    loop {
        let mut changed = false;
        let old = std::mem::take(&mut el.children);
        for node in old {
            match node {
                Node::El(w) if w.name == "w:sdt" => {
                    if let Some(content) = w.first("w:sdtContent") {
                        el.children.extend(content.children.iter().cloned());
                    }
                    changed = true;
                }
                Node::El(w) if w.name == "w:customXml" => {
                    el.children.extend(
                        w.children
                            .into_iter()
                            .filter(|n| !matches!(n, Node::El(e) if e.name == "w:customXmlPr")),
                    );
                    changed = true;
                }
                other => el.children.push(other),
            }
        }
        if !changed {
            break;
        }
        repair.add("입력 양식(콘텐츠 컨트롤)으로 감싼 칸을 보통 칸으로");
    }
}

fn empty_cell(width: i64) -> Element {
    Element::new("w:tc")
        .child(
            Element::new("w:tcPr").child(
                Element::new("w:tcW")
                    .attr("w:w", &width.max(1).to_string())
                    .attr("w:type", "dxa"),
            ),
        )
        .child(Element::new("w:p"))
}

/// 칸 하나의 합친 열 수(1 이상).
fn span_of(tc: &Element, repair: &mut Repair) -> usize {
    let raw = tc
        .first("w:tcPr")
        .and_then(|p| p.prop("w:gridSpan", "w:val"));
    match raw {
        None => 1,
        Some(v) => match int(Some(v)) {
            Some(n) if n >= 1 && v.trim().parse::<i64>().is_ok() => n as usize,
            Some(n) if n >= 1 => {
                repair.add("셀 병합 값 정리");
                n as usize
            }
            _ => {
                repair.add("잘못된 셀 병합 풀기");
                1
            }
        },
    }
}

/// 세로 병합 상태: 없음 · 시작 · 이어짐.
#[derive(Clone, Copy, PartialEq)]
enum VMerge {
    None,
    Restart,
    Continue,
}

fn vmerge_of(tc: &Element) -> VMerge {
    match tc.first("w:tcPr").and_then(|p| p.first("w:vMerge")) {
        None => VMerge::None,
        Some(v)
            if v.get("w:val")
                .is_some_and(|x| x.eq_ignore_ascii_case("restart")) =>
        {
            VMerge::Restart
        }
        Some(_) => VMerge::Continue,
    }
}

/// 칸의 tcPr 에 값을 쓴다(없으면 만든다).
fn set_cell_prop(tc: &mut Element, name: &str, val: Option<&str>) {
    if tc.first("w:tcPr").is_none() {
        tc.children.insert(0, Node::El(Element::new("w:tcPr")));
    }
    let pr = child_mut(tc, "w:tcPr").expect("만들었다");
    pr.children
        .retain(|n| !matches!(n, Node::El(e) if e.name == name));
    if let Some(v) = val {
        pr.children
            .push(Node::El(Element::new(name).attr("w:val", v)));
    }
}

/// 표 하나를 바로잡는다. 행이 없는 표는 [`has_rows`] 로 먼저 걸러 지운다.
pub fn repair(tbl: &mut Element) -> Repair {
    let mut repair = Repair::default();
    unwrap_wrappers(tbl, &mut repair);
    for node in &mut tbl.children {
        if let Node::El(tr) = node
            && tr.name == "w:tr"
        {
            unwrap_wrappers(tr, &mut repair);
        }
    }

    // 표 들여쓰기
    if let Some(pr) = child_mut(tbl, "w:tblPr")
        && let Some(ind) = child_mut(pr, "w:tblInd")
    {
        let raw = ind.get("w:w").map(str::to_string);
        if let Some(raw) = raw
            && raw.trim().parse::<i32>().is_err()
        {
            let v = int(Some(&raw))
                .unwrap_or(0)
                .clamp(i32::MIN as i64, i32::MAX as i64);
            set_attr(ind, "w:w", &v.to_string());
            repair.add("표 들여쓰기 값 정리");
        }
    }

    // 줄마다 (앞 빈 자리, 칸들의 폭, 뒤 빈 자리) → 필요한 열 수
    struct Row {
        before: usize,
        spans: Vec<usize>,
        after: usize,
    }
    let mut rows = Vec::new();
    for tr in tbl.elements().filter(|e| e.name == "w:tr") {
        let trpr = tr.first("w:trPr");
        let grid = |k: &str| {
            trpr.and_then(|p| p.prop(k, "w:val"))
                .and_then(|v| int(Some(v)))
                .unwrap_or(0)
                .max(0) as usize
        };
        let spans = tr
            .elements()
            .filter(|e| e.name == "w:tc")
            .map(|tc| span_of(tc, &mut repair))
            .collect();
        rows.push(Row {
            before: grid("w:gridBefore"),
            spans,
            after: grid("w:gridAfter"),
        });
    }

    // 열 너비
    let declared: Vec<Option<i64>> = tbl
        .first("w:tblGrid")
        .map(|g| {
            g.elements()
                .filter(|e| e.name == "w:gridCol")
                .map(|c| int(c.get("w:w")).filter(|w| *w > 0))
                .collect()
        })
        .unwrap_or_default();
    let exact = tbl.first("w:tblGrid").is_some_and(|g| {
        g.elements().filter(|e| e.name == "w:gridCol").all(|c| {
            c.get("w:w")
                .is_some_and(|w| w.trim().parse::<u32>().is_ok())
        })
    });
    // 열 수: 칸이 넘치는 줄에 맞춰 늘린다. 줄 끝 칸의 넘치는 합치기만으로는 늘리지 않는다
    // (Word 는 그 칸을 남은 열까지로 그린다).
    let need = rows
        .iter()
        .map(|r| {
            let n = r.spans.len();
            r.before + r.spans.iter().take(n.saturating_sub(1)).sum::<usize>() + usize::from(n > 0)
        })
        .max()
        .unwrap_or(0)
        .max(1);
    let columns = need.max(declared.len());
    let mut widths: Vec<Option<i64>> = declared.clone();
    widths.resize(columns, None);
    if tbl.first("w:tblGrid").is_none() {
        repair.add("빠진 열 너비 정보를 새로 계산");
    } else if !exact {
        repair.add("열 너비 값 정리");
    }
    if columns > declared.len() && !declared.is_empty() {
        repair.add("칸이 많은 줄에 맞춰 열을 늘림");
    }
    // 빠진 너비: 한 열만 차지하는 칸의 tcW, 그래도 없으면 아는 너비의 평균(없으면 1500)
    for tr in tbl.elements().filter(|e| e.name == "w:tr") {
        let mut col = tr
            .first("w:trPr")
            .and_then(|p| p.prop("w:gridBefore", "w:val"))
            .and_then(|v| int(Some(v)))
            .unwrap_or(0)
            .max(0) as usize;
        for tc in tr.elements().filter(|e| e.name == "w:tc") {
            let span = span_of(tc, &mut Repair::default());
            if span == 1
                && col < columns
                && widths[col].is_none()
                && let Some(w) = tc
                    .first("w:tcPr")
                    .and_then(|p| p.first("w:tcW"))
                    .filter(|w| w.get("w:type").is_none_or(|t| t == "dxa"))
                    .and_then(|w| int(w.get("w:w")))
                    .filter(|w| *w > 0)
            {
                widths[col] = Some(w);
            }
            col += span;
        }
    }
    let known: Vec<i64> = widths.iter().flatten().copied().collect();
    let fallback = if known.is_empty() {
        1500
    } else {
        known.iter().sum::<i64>() / known.len() as i64
    };
    let widths: Vec<i64> = widths.into_iter().map(|w| w.unwrap_or(fallback)).collect();
    let grid = widths.iter().fold(Element::new("w:tblGrid"), |g, w| {
        g.child(Element::new("w:gridCol").attr("w:w", &w.to_string()))
    });
    match tbl
        .children
        .iter()
        .position(|n| matches!(n, Node::El(e) if e.name == "w:tblGrid"))
    {
        Some(at) => tbl.children[at] = Node::El(grid),
        None => {
            let at = tbl
                .children
                .iter()
                .position(|n| matches!(n, Node::El(e) if e.name == "w:tblPr"))
                .map_or(0, |i| i + 1);
            tbl.children.insert(at, Node::El(grid));
        }
    }

    // 줄을 다시 짠다: 앞 빈 자리 → 칸들 → 모자란 자리(뒤 빈 자리 포함)는 빈 칸.
    // 세로 병합: 열마다 위 줄에서 이어 온 병합(시작 열, 폭).
    let mut above: Vec<Option<(usize, usize)>> = vec![None; columns];
    let mut row_iter = rows.into_iter();
    for node in &mut tbl.children {
        let Node::El(tr) = node else { continue };
        if tr.name != "w:tr" {
            continue;
        }
        let row = row_iter.next().expect("같은 줄 수");
        if row.before > 0 || row.after > 0 {
            repair.add("줄 앞뒤의 빈 자리를 빈 칸으로 채움");
        }
        if let Some(trpr) = child_mut(tr, "w:trPr") {
            trpr.children.retain(|n| {
                !matches!(n, Node::El(e) if matches!(e.name.as_str(), "w:gridBefore" | "w:gridAfter" | "w:wBefore" | "w:wAfter"))
            });
        }
        let old = std::mem::take(&mut tr.children);
        let mut col = 0_usize;
        let mut next: Vec<Option<(usize, usize)>> = vec![None; columns];
        let mut cells_started = false;
        let lead = |tr: &mut Element, col: &mut usize, until: usize| {
            while *col < until {
                tr.children.push(Node::El(empty_cell(widths[*col])));
                *col += 1;
            }
        };
        for node in old {
            let Node::El(mut tc) = node else {
                tr.children.push(node);
                continue;
            };
            if tc.name != "w:tc" {
                tr.children.push(Node::El(tc));
                continue;
            }
            if !cells_started {
                lead(tr, &mut col, row.before.min(columns));
                cells_started = true;
            }
            let mut span = span_of(&tc, &mut Repair::default());
            if col + span > columns {
                span = columns.saturating_sub(col).max(1);
                repair.add("표 밖으로 넘친 셀 병합을 표 끝까지로 줄임");
            }
            // gridSpan 을 정수로 다시 쓴다(1 이면 지운다).
            let raw = tc
                .first("w:tcPr")
                .and_then(|p| p.prop("w:gridSpan", "w:val"))
                .map(str::to_string);
            if span == 1 {
                if raw.is_some() {
                    set_cell_prop(&mut tc, "w:gridSpan", None);
                }
            } else if raw.as_deref() != Some(span.to_string().as_str()) {
                set_cell_prop(&mut tc, "w:gridSpan", Some(&span.to_string()));
            }
            if col >= columns {
                // 열을 모두 채운 뒤의 칸 — 열 수를 need 로 잡았으니 생기지 않는다.
                tr.children.push(Node::El(tc));
                continue;
            }
            let mut merge = vmerge_of(&tc);
            if merge == VMerge::Continue
                && !(col..col + span).all(|c| above[c] == Some((col, span)))
            {
                set_cell_prop(&mut tc, "w:vMerge", Some("restart"));
                merge = VMerge::Restart;
                repair.add("어긋난 세로 병합을 다시 맞춤");
            }
            if merge != VMerge::None {
                for slot in &mut next[col..col + span] {
                    *slot = Some((col, span));
                }
            }
            if !tc.elements().any(|e| e.name == "w:p") {
                tc.children.push(Node::El(Element::new("w:p")));
                repair.add("비어 있는 칸 정리");
            }
            tr.children.push(Node::El(tc));
            col += span;
        }
        if !cells_started {
            lead(tr, &mut col, row.before.min(columns));
        }
        if col < columns {
            if row.after == 0 || col + row.after < columns {
                repair.add("칸 수가 모자란 줄에 빈 칸을 채움");
            }
            lead(tr, &mut col, columns);
        }
        above = next;
    }
    repair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn table(body: &str) -> Element {
        let src = format!(r#"<w:tbl xmlns:w="w"><w:tblPr/>{body}</w:tbl>"#);
        xml::parse(src.as_bytes()).unwrap()
    }

    /// 줄마다 (합친 열 수, 세로 병합) — hlt-core 가 보는 모양.
    type Rows = Vec<Vec<(usize, &'static str)>>;

    fn shape(t: &Element) -> (Vec<String>, Rows) {
        let grid = t
            .first("w:tblGrid")
            .unwrap()
            .elements()
            .map(|c| c.get("w:w").unwrap().to_string())
            .collect();
        let rows = t
            .elements()
            .filter(|e| e.name == "w:tr")
            .map(|tr| {
                tr.elements()
                    .filter(|e| e.name == "w:tc")
                    .map(|tc| {
                        let span = span_of(tc, &mut Repair::default());
                        let m = match vmerge_of(tc) {
                            VMerge::None => "",
                            VMerge::Restart => "R",
                            VMerge::Continue => "C",
                        };
                        (span, m)
                    })
                    .collect()
            })
            .collect();
        (grid, rows)
    }

    const TC: &str = "<w:tc><w:p/></w:tc>";

    #[test]
    fn short_rows_grid_before_after_and_overflow() {
        let mut t = table(&format!(
            r#"<w:tblGrid><w:gridCol w:w="1000.4"/><w:gridCol/><w:gridCol w:w="3000"/></w:tblGrid>
            <w:tr>{TC}<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p/></w:tc>{TC}</w:tr>
            <w:tr><w:trPr><w:gridBefore w:val="1"/></w:trPr>{TC}</w:tr>
            <w:tr>{TC}<w:tc><w:tcPr><w:gridSpan w:val="5"/></w:tcPr><w:p/></w:tc></w:tr>
            <w:tr>{TC}{TC}{TC}{TC}</w:tr>"#
        ));
        let r = repair(&mut t);
        assert!(!r.fixes.is_empty());
        let (grid, rows) = shape(&t);
        // 넘치는 줄(칸 4개)에 맞춰 열 4개, 빠진 너비는 tcW(2000), 넷째는 평균
        assert_eq!(grid, ["1000", "2000", "3000", "2000"]);
        for row in &rows {
            assert_eq!(row.iter().map(|(s, _)| s).sum::<usize>(), 4, "{rows:?}");
        }
        assert_eq!(rows[2][1].0, 3, "넘친 합치기는 남은 열까지");
    }

    #[test]
    fn orphan_and_mismatched_vertical_merges_restart() {
        let mut t = table(&format!(
            r#"<w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid>
            <w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/></w:tcPr><w:p/></w:tc></w:tr>
            <w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>{TC}</w:tr>"#
        ));
        repair(&mut t);
        let (_, rows) = shape(&t);
        assert_eq!(rows[0], [(1, "R"), (2, "R")], "고아 이어짐 → 시작");
        assert_eq!(
            rows[1],
            [(1, "C"), (1, "R"), (1, "")],
            "폭이 다른 이어짐 → 시작"
        );
    }

    #[test]
    fn content_controls_unwrap_and_good_tables_stay() {
        let good = format!(
            r#"<w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr>{TC}{TC}</w:tr>"#
        );
        let mut t = table(&good);
        let before = xml::write(&t);
        assert!(repair(&mut t).fixes.is_empty());
        assert_eq!(xml::write(&t), before, "멀쩡한 표는 그대로");

        let mut t = table(&format!(
            r#"<w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr>{TC}<w:sdt><w:sdtPr/><w:sdtContent>{TC}</w:sdtContent></w:sdt></w:tr>"#
        ));
        repair(&mut t);
        assert_eq!(shape(&t).1, [vec![(1, ""), (1, "")]]);
        assert!(!has_rows(&table("<w:tblGrid/>")));
    }
}
