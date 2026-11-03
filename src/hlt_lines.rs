//! HLT 저장 줄나눔 — 첨자 · 글 속 그림이 든 문단도 NK-Editor(통특기) 줄 규칙으로 끊는다.
//!
//! hlt-core 는 첨자나 그림이 섞인 문단을 「폭 미확정」으로 보고 39칸마다 기계적으로 끊는다(`wrap` 의
//! 옛 규칙). 그러면 `x_max` 가 `x_m | ax` 로, `confidence` 가 `c | onfidence` 로 저장 줄에서 갈린다.
//! 통특기는 들어가는 저장 줄을 다시 흘리지 않으므로 그 자리가 그대로 남고, 긴 첨자 단어를 고칠 수 없다
//! (고객 시험 2026-11 「약간 긴 텍스트 형태의 윗/아래 첨자만 편집이 안 된다」).
//!
//! 그래서 그런 문단의 줄 길이(`source_line_bytes`, EUC-KR 바이트)를 여기서 미리 정해 넘긴다.
//! 끊는 규칙과 글자 폭(첨자 ASCII 31 · 한글 62)은 hlt-core `reflow.rs` 에서 옮겼다
//! (seoan-editor, Copyright (c) 2026 tornado1014, MIT License).

use hlt_core::{CharacterFormat, HmlDocument, HmlInline};

/// 저장 본문 폭(twip) — hlt-core 생성 서식의 A4 여백(왼 0.984in · 오 0.7868in)을 뺀 폭.
const STORED_BODY_TWIPS: f64 = (210.0 / 25.4 - 0.984 - 0.7868) * 1440.0;
/// twip → 글자 폭 단위(한글 100 = 12pt = 240twip).
const SCALE: f64 = 5.0 / 12.0;
const FULL: f64 = 100.0;
const HALF: f64 = 50.0;
const SUB: f64 = 31.0;
const TAB: f64 = 300.0;
/// 글자 폭 단위로 1pt.
const PT: f64 = 100.0 / 12.0;
const CLOSE: &[u8] = b",.)]}:;!?%'\"";
const OPEN: &[u8] = b"([{";
/// 토큰 안에서 끊을 수 있는 자리(그 글자 뒤) — 저장 줄나눔은 하이픈만.
const SUBTOKEN_END: &[u8] = b"-";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Item {
    /// 1바이트 글자 · 첨자인가.
    Ascii(u8, bool),
    /// 2바이트 글자 · 첨자인가.
    Wide(bool),
    /// 글 속 그림 · 폭.
    Image(f64),
}

impl Item {
    fn byte(self) -> Option<u8> {
        match self {
            Item::Ascii(b, _) => Some(b),
            _ => None,
        }
    }
    fn is_ascii(self) -> bool {
        matches!(self, Item::Ascii(b, _) if b != b' ')
    }
    fn is_wide(self) -> bool {
        matches!(self, Item::Wide(_))
    }
    fn is_open(self) -> bool {
        self.byte().is_some_and(|b| OPEN.contains(&b))
    }
    /// EUC-KR 바이트(그림 자리표 `_` 는 1).
    fn bytes(self) -> u32 {
        if self.is_wide() { 2 } else { 1 }
    }
    fn width(self) -> f64 {
        match self {
            Item::Image(w) => w,
            Item::Ascii(b, _) if b < 0x20 && b != b'\t' => 0.0,
            Item::Ascii(_, sub) => {
                if sub {
                    SUB
                } else {
                    HALF
                }
            }
            Item::Wide(sub) => {
                if sub {
                    2.0 * SUB
                } else {
                    FULL
                }
            }
        }
    }
}

/// 첨자 · 글 속 그림이 든 본문 문단에 저장 줄 길이를 채운다. 그 밖의 문단은 hlt-core 가 같은 규칙으로 흘린다.
/// 바꾼 문단 수를 낸다.
pub fn fill(document: &mut HmlDocument) -> usize {
    let mut filled = 0;
    for index in 0..document.paragraphs.len() {
        let paragraph = &document.paragraphs[index];
        // 식별항목(표제) · 표 문단 · 원본 줄나눔이 있는 문단은 손대지 않는다.
        if paragraph.marker.is_some()
            || paragraph.table_id.is_some()
            || !paragraph.source_line_bytes.is_empty()
        {
            continue;
        }
        let Some(items) = items(document, &paragraph.inlines) else {
            continue;
        };
        if !items
            .iter()
            .any(|i| matches!(i, Item::Image(_) | Item::Ascii(_, true) | Item::Wide(true)))
        {
            continue;
        }
        let width = (STORED_BODY_TWIPS
            - f64::from(paragraph.lmargin_raw)
            - f64::from(paragraph.rmargin_raw))
            * SCALE;
        let ends = wrap(&items, f64::from(paragraph.indent_raw) * SCALE, width);
        let mut from = 0;
        let lines = ends
            .into_iter()
            .map(|end| {
                let bytes = items[from..end].iter().map(|i| i.bytes()).sum();
                from = end;
                bytes
            })
            .collect();
        document.paragraphs[index].source_line_bytes = lines;
        filled += 1;
    }
    filled
}

/// 문단 글 → 줄 칸들. 12pt 아닌 글자가 섞이면 폭을 모르니 `None`(hlt-core 에 맡긴다).
fn items(document: &HmlDocument, inlines: &[HmlInline]) -> Option<Vec<Item>> {
    let mut out = Vec::new();
    for inline in inlines {
        match inline {
            HmlInline::Text {
                text,
                formats,
                size,
            } => {
                if size.is_some_and(|s| s != 12) {
                    return None;
                }
                let script = formats.iter().any(|f| {
                    matches!(f, CharacterFormat::Superscript | CharacterFormat::Subscript)
                });
                out.extend(text.chars().map(|c| {
                    if c.is_ascii() {
                        Item::Ascii(c as u8, script)
                    } else {
                        Item::Wide(script)
                    }
                }));
            }
            HmlInline::Image { image_id } => {
                // DOCX 에서 온 그림 크기 단위: 1 = 0.282mm (hlt-core docx 판독).
                let units = document.images.iter().find(|i| &i.id == image_id)?.width?;
                out.push(Item::Image(f64::from(units) * 0.282 / 25.4 * 72.0 * PT));
            }
        }
    }
    Some(out)
}

/// `items[b]` 를 품는 ASCII 런의 시작(토큰 안 하이픈 뒤에서는 끊긴다).
fn run_start(items: &[Item], mut b: usize, start: usize) -> usize {
    while b > start && items[b - 1].is_ascii() {
        let c = items[b - 1].byte().unwrap();
        if SUBTOKEN_END.contains(&c)
            && b >= 2
            && items[b - 2].byte() != Some(b' ')
            && !(c == b'-' && items[b - 2].byte() == Some(b' '))
        {
            break;
        }
        b -= 1;
    }
    b
}

fn next_tab(width: f64) -> f64 {
    ((width / TAB).floor() + 1.0) * TAB
}

/// 끊을 자리. 다 들어가면 `None`.
fn cut(items: &[Item], start: usize, indent: f64, limit: f64) -> Option<usize> {
    let mut width = indent;
    let mut overflow = None;
    for (i, item) in items.iter().enumerate().skip(start) {
        if item.byte() == Some(b'\t') {
            width = next_tab(width);
            continue;
        }
        let w = item.width();
        if item.byte() != Some(b' ') && width + w > limit && i > start {
            overflow = Some(i);
            break;
        }
        width += w;
    }
    let f = overflow?;
    let item = items[f];
    let mut b = f;
    if item.byte() == Some(b'%')
        && f >= 2
        && items[f - 1].byte() == Some(b' ')
        && items[f - 2].is_ascii()
    {
        b = run_start(items, f - 2, start);
    } else if item.is_ascii() {
        b = run_start(items, f, start);
        let close = item.byte().is_some_and(|byte| CLOSE.contains(&byte));
        if close
            && b == f
            && f > start + 1
            && (items[f - 1].is_wide() || matches!(items[f - 1], Item::Image(_)))
        {
            b = f - 1;
        } else if items[b..=f].iter().any(|item| item.is_open())
            && b > start
            && items[b - 1].is_wide()
        {
            b -= 1;
        }
    } else if (item.is_wide() || matches!(item, Item::Image(_)))
        && f > start + 1
        && items[f - 1].is_open()
    {
        b = run_start(items, f - 1, start);
    }
    while b > start + 1 && items[b - 1].is_open() {
        b = run_start(items, b - 1, start);
    }
    if b <= start {
        b = if f > start { f } else { start + 1 };
    }
    Some(b)
}

/// 줄 끝 자리들. 공백은 넘침으로 치지 않으니 앞 줄 끝에 남는다.
fn wrap(items: &[Item], indent: f64, limit: f64) -> Vec<usize> {
    let (mut ends, mut pos, mut indent) = (Vec::new(), 0, indent);
    while let Some(b) = cut(items, pos, indent, limit) {
        pos = b;
        ends.push(pos);
        indent = 0.0;
    }
    ends.push(items.len());
    ends
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_items(s: &str, sub: &str) -> Vec<Item> {
        // `sub` 에 든 글자 자리(같은 길이 · '^' 표시)는 아래첨자.
        s.chars()
            .zip(sub.chars().chain(std::iter::repeat(' ')))
            .map(|(c, m)| {
                if c.is_ascii() {
                    Item::Ascii(c as u8, m == '^')
                } else {
                    Item::Wide(m == '^')
                }
            })
            .collect()
    }

    #[test]
    fn never_cuts_inside_an_ascii_word() {
        // 줄 폭 20칸(2,000)에 「가나다라마바사아자차카타 x_max 와 confidence 값」 — 영문 낱말은 통째로 다음 줄로.
        let text = "가나다라마바사아자차카타파하 xmax 와 confidence 값";
        let marks = "                 ^^^                      ";
        let items = line_items(text, marks);
        let ends = wrap(&items, 0.0, 1600.0);
        let chars: Vec<char> = text.chars().collect();
        for &e in &ends[..ends.len() - 1] {
            let (a, b) = (chars[e - 1], chars[e]);
            assert!(
                !(a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric()),
                "{a}|{b} at {e}: {ends:?}"
            );
        }
    }
}
