//! NK-Editor(HLT) 글자 규칙. HLT 는 글을 CP949 로 저장하므로 그 밖의 글자는 살아남지 못한다.
//! 1) 첨자 모양 글자(², ₂, ⁿ …) → 보통 글자 + 위/아래첨자 서식
//! 2) 모양이 같은 대체 글자가 있으면 바꾼다(– → -, 공백류, 합자 … · hlt-core 대응표)
//! 3) 나머지(x̂ 같은 결합 문자, 악센트 글자, 낯선 기호) → 글자 그림

use encoding_rs::EUC_KR;

#[derive(Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// 바꾼 글자(원래 글자, 바꾼 글)
    Sub(char, String),
    /// (글자, 위첨자면 true)
    Script(String, bool),
    /// 그림으로 그릴 글자 묶음(바탕 글자 + 결합 문자)
    Image(String),
}

pub fn encodable(c: char) -> bool {
    let mut buf = [0_u8; 4];
    !EUC_KR.encode(c.encode_utf8(&mut buf)).2
}

/// 앞 글자에 붙어 한 글자 그림이 되는 것: 결합 부호, 옛한글 중성 · 종성 자모, 방점.
fn combining(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}'
        | '\u{1160}'..='\u{11FF}' | '\u{D7B0}'..='\u{D7FF}' | '\u{302E}' | '\u{302F}')
}

/// 이체 선택자(보이지 않는 모양 힌트, ✔︎ · 葛󠄀 의 뒷부분) — 지운다.
fn variation_selector(c: char) -> bool {
    matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

/// 풀어쓴 현대 한글 자모(macOS · 일부 PDF 복사에서 온 NFD) → 완성형 음절. 옛한글은 그대로 둔다.
pub fn compose_hangul(text: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        let last = out.last().copied();
        match (last, c) {
            (Some(l @ '\u{1100}'..='\u{1112}'), '\u{1161}'..='\u{1175}') => {
                let s = 0xAC00 + ((l as u32 - 0x1100) * 21 + (c as u32 - 0x1161)) * 28;
                *out.last_mut().unwrap() = char::from_u32(s).unwrap();
            }
            (Some(s @ '\u{AC00}'..='\u{D7A3}'), '\u{11A8}'..='\u{11C2}')
                if (s as u32 - 0xAC00).is_multiple_of(28) =>
            {
                let t = s as u32 + (c as u32 - 0x11A7);
                *out.last_mut().unwrap() = char::from_u32(t).unwrap();
            }
            _ => out.push(c),
        }
    }
    out.into_iter().collect()
}

const SUPERS: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱᵃᵇᶜᵈᵉᶠᵍʰʲᵏˡᵐᵒᵖʳˢᵗᵘᵛʷˣʸᶻᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁⱽᵂᵅᵝᵞᵟᵋᶿᵠᵡ";
const SUPER_BASE: &str = "0123456789+-=()niabcdefghjklmoprstuvwxyzABDEGHIJKLMNOPRTUVWαβγδεθφχ";
const SUBS: &str = "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₒₓₕₖₗₘₙₚₛₜᵢⱼᵣᵤᵥᵦᵧᵨᵩᵪ";
const SUB_BASE: &str = "0123456789+-=()aeoxhklmnpstijruvβγρφχ";

fn script(c: char) -> Option<(char, bool)> {
    let find = |set: &str, base: &str, up| {
        set.chars()
            .position(|v| v == c)
            .and_then(|i| base.chars().nth(i))
            .map(|b| (b, up))
    };
    find(SUPERS, SUPER_BASE, true).or_else(|| find(SUBS, SUB_BASE, false))
}

/// hlt-core 대응표에 없는, 모양이 같은 CP949 글자.
fn local_substitute(c: char) -> Option<&'static str> {
    Some(match c {
        // 가운뎃점 꼴(일본어 · 수학 · 하이픈 점) → ·
        '\u{30FB}' | '\u{FF65}' | '\u{2027}' | '\u{22C5}' | '\u{2E31}' => "\u{00B7}",
        // 분수 · 나눗셈 빗금 → /
        '\u{2044}' | '\u{2215}' => "/",
        _ => return None,
    })
}

/// 모양이 같은 CP949 글자. 악센트를 떼는 대응(é → e)은 뜻이 바뀌니 받지 않는다 — 그림으로 간다.
fn substitute(c: char) -> Option<String> {
    if let Some(text) = local_substitute(c) {
        return Some(text.to_string());
    }
    let (text, _, unresolved) = hlt_core::cp949_safe_text(&c.to_string());
    if !unresolved.is_empty() || text.chars().any(|v| !encodable(v)) {
        return None;
    }
    if c.is_alphabetic() && text.len() == 1 && text.is_ascii() {
        return None;
    }
    Some(text)
}

pub fn split(text: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let chars: Vec<char> = compose_hangul(text).chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut end = i + 1;
        while end < chars.len() && combining(chars[end]) {
            end += 1;
        }
        let cluster: String = chars[i..end].iter().collect();
        i = end;
        let piece = if variation_selector(c) {
            Piece::Sub(c, String::new())
        } else if cluster.chars().count() > 1 {
            Piece::Image(cluster)
        } else if let Some((base, up)) = script(c) {
            Piece::Script(base.to_string(), up)
        } else if encodable(c) {
            Piece::Text(cluster)
        } else if let Some(text) = substitute(c) {
            Piece::Sub(c, text)
        } else {
            Piece::Image(cluster)
        };
        // 이웃한 같은 종류는 합친다.
        match (out.last_mut(), &piece) {
            (Some(Piece::Text(a)), Piece::Text(b)) => a.push_str(b),
            (Some(Piece::Script(a, x)), Piece::Script(b, y)) if x == y => a.push_str(b),
            _ => out.push(piece),
        }
    }
    out
}

/// Symbol 글꼴(`w:sym w:font="Symbol"`)의 글자 자리 → 유니코드.
pub fn symbol_font(code: u32) -> Option<char> {
    let code = if code >= 0xF000 { code - 0xF000 } else { code };
    const GREEK_UPPER: &str = "ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ";
    const GREEK_LOWER: &str = "αβχδεφγηιϕκλμνοπθρστυϖωξψζ";
    let c = match code {
        0x41..=0x5A => GREEK_UPPER.chars().nth((code - 0x41) as usize)?,
        0x61..=0x7A => GREEK_LOWER.chars().nth((code - 0x61) as usize)?,
        0x22 => '∀',
        0x24 => '∃',
        0x27 => '∋',
        0x2A => '∗',
        0x2D => '−',
        0x40 => '≅',
        0x5C => '∴',
        0x5E => '⊥',
        0x7E => '∼',
        0x5B => '[',
        0x5D => ']',
        0x5F => '_',
        0x60 => '‾',
        0x7B => '{',
        0x7C => '|',
        0x7D => '}',
        0xA1 => 'ϒ',
        0xA4 => '⁄',
        0xA6 => 'ƒ',
        0xA7 => '♣',
        0xA8 => '♦',
        0xA9 => '♥',
        0xAA => '♠',
        0xBF => '↵',
        0xC0 => 'ℵ',
        0xC1 => 'ℑ',
        0xC2 => 'ℜ',
        0xC3 => '℘',
        0xCB => '⊄',
        // 세리프 · 산세리프 두 벌
        0xD2 | 0xE2 => '®',
        0xD3 | 0xE3 => '©',
        0xD4 | 0xE4 => '™',
        0xE0 => '◊',
        0xA2 => '′',
        0xA3 => '≤',
        0xA5 => '∞',
        0xAB => '↔',
        0xAC => '←',
        0xAD => '↑',
        0xAE => '→',
        0xAF => '↓',
        0xB0 => '°',
        0xB1 => '±',
        0xB2 => '″',
        0xB3 => '≥',
        0xB4 => '×',
        0xB5 => '∝',
        0xB6 => '∂',
        0xB7 => '•',
        0xB8 => '÷',
        0xB9 => '≠',
        0xBA => '≡',
        0xBB => '≈',
        0xBC => '…',
        0xC4 => '⊗',
        0xC5 => '⊕',
        0xC6 => '∅',
        0xC7 => '∩',
        0xC8 => '∪',
        0xC9 => '⊃',
        0xCA => '⊇',
        0xCC => '⊂',
        0xCD => '⊆',
        0xCE => '∈',
        0xCF => '∉',
        0xD0 => '∠',
        0xD1 => '∇',
        0xD5 => '∏',
        0xD6 => '√',
        0xD7 => '⋅',
        0xD8 => '¬',
        0xD9 => '∧',
        0xDA => '∨',
        0xDB => '⇔',
        0xDC => '⇐',
        0xDE => '⇒',
        0xE1 => '〈',
        0xE5 => '∑',
        0xF1 => '〉',
        0xF2 => '∫',
        0x20..=0x3F => char::from_u32(code)?,
        _ => return None,
    };
    Some(c)
}

/// Wingdings 글꼴의 글자 자리 → 유니코드(명세서에 자주 나오는 글머리표 · 확인 표시만).
pub fn wingdings(code: u32) -> Option<char> {
    let code = if code >= 0xF000 { code - 0xF000 } else { code };
    Some(match code {
        0x20 => ' ',
        0x6C => '●',
        0x6D => '○',
        0x6E => '■',
        0x6F => '□',
        0x71 => '❑',
        0x75 => '◆',
        0x76 => '❖',
        0x77 => '⬥',
        0x81..=0x8A => char::from_u32(0x2460 + code - 0x81)?, // ①–⑩
        0x9F => '•',
        0xA1 => '○',
        0xA4 => '◉',
        0xA7 => '▪',
        0xA8 => '◻',
        0xD8 => '➢',
        0xE8 => '➔',
        0xFB => '✗',
        0xFC => '✓',
        0xFD => '☒',
        0xFE => '☑',
        _ => return None,
    })
}

/// 글자 자리를 글자로 바꾸는 기호 글꼴(`w:rFonts` · `w:sym w:font`).
pub fn symbol_font_map(font: &str) -> Option<fn(u32) -> Option<char>> {
    if font.eq_ignore_ascii_case("Symbol") {
        Some(symbol_font)
    } else if font.eq_ignore_ascii_case("Wingdings") {
        Some(wingdings)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Piece::*;

    #[test]
    fn nk_policy() {
        assert_eq!(
            split("CO₂ 10⁻³ m – x̂ é 가"),
            vec![
                Text("CO".into()),
                Script("2".into(), false),
                Text(" 10".into()),
                Script("-3".into(), true),
                Text(" m ".into()),
                Sub('–', "-".into()),
                Text(" ".into()),
                Image("x\u{302}".into()),
                Text(" ".into()),
                Image("é".into()),
                Text(" 가".into()),
            ]
        );
        assert_eq!(split("ŷ"), vec![Image("ŷ".into())]);
        assert_eq!(split("ﬁ"), vec![Sub('ﬁ', "fi".into())]);
        assert_eq!(symbol_font(0xF062), Some('β'));
        assert_eq!(symbol_font(0xD2), Some('®'));
        assert_eq!(symbol_font(0xE4), Some('™'));
        assert_eq!(wingdings(0xF0FC), Some('✓'));
        assert_eq!(wingdings(0x83), Some('③'));
    }

    #[test]
    fn hangul_and_lookalikes() {
        // 풀어쓴 한글 → 완성형 글
        assert_eq!(
            split("\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}"),
            vec![Text("한글".into())]
        );
        // 옛한글은 한 글자 그림 하나
        assert_eq!(
            split("\u{1112}\u{119E}\u{11AB}"),
            vec![Image("\u{1112}\u{119E}\u{11AB}".into())]
        );
        // 가운뎃점 꼴 · 빗금
        assert_eq!(
            split("・‧⋅"),
            vec![
                Sub('・', "·".into()),
                Sub('‧', "·".into()),
                Sub('⋅', "·".into())
            ]
        );
        assert_eq!(
            split("1⁄2"),
            vec![Text("1".into()), Sub('⁄', "/".into()), Text("2".into())]
        );
        // 이체 선택자는 지우고 앞 글자는 살린다
        assert_eq!(
            split("★\u{FE0E}"),
            vec![Text("★".into()), Sub('\u{FE0E}', String::new())]
        );
    }
}
