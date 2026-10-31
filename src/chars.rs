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

fn combining(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
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

/// 모양이 같은 CP949 글자. 악센트를 떼는 대응(é → e)은 뜻이 바뀌니 받지 않는다 — 그림으로 간다.
fn substitute(c: char) -> Option<String> {
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
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut end = i + 1;
        while end < chars.len() && combining(chars[end]) {
            end += 1;
        }
        let cluster: String = chars[i..end].iter().collect();
        i = end;
        let piece = if cluster.chars().count() > 1 {
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
    }
}
