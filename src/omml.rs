//! OMML(Word 수식, `m:oMath`) → Typst 수식 문법.
//! 글자는 하나씩 떼어 낸다 — Typst 는 붙은 글자(`ab`)를 변수 이름으로 읽는다.

use crate::xml::Element;

/// `m:oMathPara` 또는 `m:oMath` 하나.
pub fn to_typst(el: &Element) -> String {
    if el.name == "m:oMathPara" {
        let lines: Vec<String> = el
            .elements()
            .filter(|e| e.name == "m:oMath")
            .map(|e| children(e, false))
            .collect();
        return lines.join(" \\ ");
    }
    children(el, false)
}

/// `eq_arr` = 수식 배열 안 — `&` 가 맞춤점이다.
fn children(el: &Element, eq_arr: bool) -> String {
    let mut out = String::new();
    for child in el.elements() {
        let piece = node(child, eq_arr);
        if !piece.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&piece);
        }
    }
    out
}

/// 자식 `name` 의 내용. 없으면 빈 글.
fn part(el: &Element, name: &str) -> String {
    el.first(name)
        .map(|e| children(e, false))
        .unwrap_or_default()
}

/// 함수 인자 하나 — 비면 Typst 가 거부하므로 빈 자리(`#none`)를 둔다.
fn arg(s: String) -> String {
    if s.trim().is_empty() {
        "#none".into()
    } else {
        s
    }
}

fn props<'a>(el: &'a Element, pr: &str, name: &str) -> Option<&'a str> {
    el.first(pr)?.prop(name, "m:val")
}

fn node(el: &Element, eq_arr: bool) -> String {
    match el.name.as_str() {
        "m:r" => run(el, eq_arr),
        // 수식 안에 섞인 Word 글(드물다)
        "w:r" => text(&el.text(), Style::Upright, eq_arr),
        "m:e" | "m:oMath" | "m:box" | "m:borderBox" | "m:phant" | "m:argPr" => children(el, eq_arr),
        "m:f" => {
            let (num, den) = (arg(part(el, "m:num")), arg(part(el, "m:den")));
            match props(el, "m:fPr", "m:type") {
                Some("lin") => format!("{num} \\/ {den}"),
                Some("noBar") => format!("binom({num}, {den})"),
                _ => format!("frac({num}, {den})"),
            }
        }
        "m:sSup" => format!(
            "attach({}, t: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:sup"))
        ),
        "m:sSub" => format!(
            "attach({}, b: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:sub"))
        ),
        "m:sSubSup" => format!(
            "attach({}, b: {}, t: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:sub")),
            arg(part(el, "m:sup"))
        ),
        "m:sPre" => format!(
            "attach({}, bl: {}, tl: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:sub")),
            arg(part(el, "m:sup"))
        ),
        "m:rad" => {
            let degree = part(el, "m:deg");
            let hidden = props(el, "m:radPr", "m:degHide").is_some_and(on);
            if hidden || degree.trim().is_empty() {
                format!("sqrt({})", arg(part(el, "m:e")))
            } else {
                format!("root({degree}, {})", arg(part(el, "m:e")))
            }
        }
        "m:nary" => nary(el),
        "m:d" => delimited(el),
        "m:acc" => {
            let chr = props(el, "m:accPr", "m:chr").unwrap_or("\u{0302}");
            format!("{}({})", accent(chr), arg(part(el, "m:e")))
        }
        "m:bar" => {
            let under = props(el, "m:barPr", "m:pos") == Some("bot");
            format!(
                "{}({})",
                if under { "underline" } else { "overline" },
                arg(part(el, "m:e"))
            )
        }
        "m:func" => format!("op({}) {}", arg(part(el, "m:fName")), part(el, "m:e")),
        "m:limLow" => format!(
            "attach(limits({}), b: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:lim"))
        ),
        "m:limUpp" => format!(
            "attach(limits({}), t: {})",
            arg(part(el, "m:e")),
            arg(part(el, "m:lim"))
        ),
        "m:groupChr" => {
            let chr = props(el, "m:groupChrPr", "m:chr").unwrap_or("\u{23DF}");
            let top = props(el, "m:groupChrPr", "m:pos") == Some("top");
            let body = arg(part(el, "m:e"));
            match chr {
                "\u{23DE}" => format!("overbrace({body})"),
                "\u{23DF}" => format!("underbrace({body})"),
                _ if top => format!("attach(limits({body}), t: {})", symbol(chr)),
                _ => format!("attach(limits({body}), b: {})", symbol(chr)),
            }
        }
        "m:m" => {
            let rows: Vec<String> = el
                .elements()
                .filter(|e| e.name == "m:mr")
                .map(|row| {
                    row.elements()
                        .filter(|e| e.name == "m:e")
                        .map(|e| arg(children(e, false)))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect();
            format!("mat(delim: #none, {})", rows.join("; "))
        }
        "m:eqArr" => {
            let rows: Vec<String> = el
                .elements()
                .filter(|e| e.name == "m:e")
                .map(|e| children(e, true))
                .collect();
            rows.join(" \\ ")
        }
        // 속성 · 제어 글자 · 그 밖: 그리지 않는다
        _ => String::new(),
    }
}

fn on(value: &str) -> bool {
    !matches!(value, "0" | "off" | "false")
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Italic,
    Upright,
}

fn run(el: &Element, eq_arr: bool) -> String {
    let text_of: String = el
        .elements()
        .filter(|e| e.name == "m:t")
        .map(|e| e.text())
        .collect();
    let rpr = el.first("m:rPr");
    let normal = rpr.is_some_and(|p| {
        p.first("m:nor")
            .is_some_and(|n| n.get("m:val").is_none_or(on))
    });
    let sty = rpr.and_then(|p| p.prop("m:sty", "m:val"));
    let style = if normal || matches!(sty, Some("p" | "b")) {
        Style::Upright
    } else {
        Style::Italic
    };
    let mut body = text(&text_of, style, eq_arr);
    if body.is_empty() {
        return body;
    }
    if matches!(sty, Some("b" | "bi")) {
        body = format!("bold({body})");
    }
    let script = rpr.and_then(|p| p.prop("m:scr", "m:val"));
    if let Some(func) = match script {
        Some("double-struck") => Some("bb"),
        Some("fraktur") => Some("frak"),
        Some("script") => Some("cal"),
        Some("sans-serif") => Some("sans"),
        Some("monospace") => Some("mono"),
        _ => None,
    } {
        body = format!("{func}({body})");
    }
    body
}

/// 글 → Typst 수식 원자들(공백으로 떼어 놓는다).
fn text(s: &str, style: Style, eq_arr: bool) -> String {
    if style == Style::Upright {
        // 똑바로 쓴 글(sin, lim, 단위, 한글)은 글 덩어리로 — 띄어쓰기도 살린다.
        let mut parts = Vec::new();
        let mut word = String::new();
        for c in s.chars() {
            if c.is_alphanumeric() || c == ' ' {
                word.push(c);
            } else {
                if !word.is_empty() {
                    parts.push(quote(&std::mem::take(&mut word)));
                }
                parts.push(atom(c, eq_arr));
            }
        }
        if !word.is_empty() {
            parts.push(quote(&word));
        }
        return parts.join(" ");
    }
    let mut parts: Vec<String> = Vec::new();
    let mut number = String::new();
    for c in s.chars() {
        // 숫자는 붙여 둔다(3.14) — 떼면 Typst 가 사이를 띄운다.
        if c.is_ascii_digit() || (c == '.' && !number.is_empty()) {
            number.push(c);
            continue;
        }
        if !number.is_empty() {
            parts.push(std::mem::take(&mut number));
        }
        if c == ' ' {
            parts.push("space".into());
        } else if c.is_alphabetic() && !c.is_ascii() && !is_math_letter(c) {
            // 한글 같은 글자는 글 덩어리로
            match parts.last_mut() {
                Some(last) if last.starts_with('"') && last.ends_with('"') && last.len() > 1 => {
                    last.pop();
                    last.push(c);
                    last.push('"');
                }
                _ => parts.push(quote(&c.to_string())),
            }
        } else {
            parts.push(atom(c, eq_arr));
        }
    }
    if !number.is_empty() {
        parts.push(number);
    }
    parts.join(" ")
}

/// 그리스 문자처럼 수식 글꼴이 이탤릭으로 그릴 글자.
fn is_math_letter(c: char) -> bool {
    matches!(c, '\u{0370}'..='\u{03FF}' | '\u{2100}'..='\u{214F}' | '\u{1D400}'..='\u{1D7FF}')
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 글자 하나를 수식 원자로. ASCII 기호는 모두 `\` 로 막는다(`<=` 같은 줄임 기호가 되지 않게).
fn atom(c: char, eq_arr: bool) -> String {
    match c {
        '&' if eq_arr => "&".into(),
        '\t' | '\n' | '\r' => String::new(),
        // Word 수식의 `-` 는 빼기표로 그린다.
        '-' => "\\u{2212}".into(),
        c if c.is_ascii_punctuation() => format!("\\{c}"),
        c if c.is_ascii_alphanumeric() => c.to_string(),
        c if c.is_alphanumeric() => c.to_string(),
        // 결합 문자는 앞 글자에 붙어야 하니 그대로(드물다)
        c => symbol(&c.to_string()),
    }
}

/// 기호 글자(∑, ±, →). Typst 이름 대신 글자 그대로 — `\u{…}` 로 써서 줄임 기호 해석을 피한다.
fn symbol(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_punctuation() {
                format!("\\{c}")
            } else if c.is_ascii() {
                c.to_string()
            } else {
                format!("\\u{{{:x}}}", c as u32)
            }
        })
        .collect()
}

fn accent(chr: &str) -> &'static str {
    match chr {
        "\u{0302}" | "^" | "\u{02C6}" => "hat",
        "\u{0303}" | "~" | "\u{02DC}" => "tilde",
        "\u{0304}" | "\u{0305}" | "\u{00AF}" | "\u{203E}" => "macron",
        "\u{0307}" | "\u{02D9}" => "dot",
        "\u{0308}" | "\u{00A8}" => "dot.double",
        "\u{20DB}" => "dot.triple",
        "\u{0301}" | "\u{00B4}" => "acute",
        "\u{0300}" | "`" => "grave",
        "\u{030C}" | "\u{02C7}" => "caron",
        "\u{0306}" | "\u{02D8}" => "breve",
        "\u{030A}" | "\u{02DA}" => "circle",
        "\u{20D6}" => "arrow.l",
        "\u{20E1}" => "arrow.l.r",
        // 벡터 화살표(U+20D7)와 모르는 것
        _ => "arrow",
    }
}

fn nary(el: &Element) -> String {
    let chr = props(el, "m:naryPr", "m:chr").unwrap_or("\u{222B}");
    let integral = matches!(
        chr,
        "\u{222B}" | "\u{222C}" | "\u{222D}" | "\u{222E}" | "\u{222F}" | "\u{2230}"
    );
    let loc =
        props(el, "m:naryPr", "m:limLoc").unwrap_or(if integral { "subSup" } else { "undOvr" });
    let hide = |name| props(el, "m:naryPr", name).is_some_and(on);
    let operator = format!(
        "{}({})",
        if loc == "undOvr" { "limits" } else { "scripts" },
        symbol(chr)
    );
    let mut attach = Vec::new();
    let sub = part(el, "m:sub");
    let sup = part(el, "m:sup");
    if !hide("m:subHide") && !sub.trim().is_empty() {
        attach.push(format!("b: {sub}"));
    }
    if !hide("m:supHide") && !sup.trim().is_empty() {
        attach.push(format!("t: {sup}"));
    }
    let head = if attach.is_empty() {
        operator
    } else {
        format!("attach({operator}, {})", attach.join(", "))
    };
    format!("{head} {}", part(el, "m:e"))
}

fn delimited(el: &Element) -> String {
    let pr = |name| el.first("m:dPr").and_then(|p| p.prop(name, "m:val"));
    let open = pr("m:begChr").unwrap_or("(");
    let close = pr("m:endChr").unwrap_or(")");
    let separator = pr("m:sepChr").unwrap_or("|");
    let items: Vec<&Element> = el.elements().filter(|e| e.name == "m:e").collect();
    if open.is_empty() != close.is_empty() {
        // 한쪽 괄호(조건식의 큰 중괄호): Typst 는 `cases` 로 그린다. 수식 배열이면 줄마다 한 행.
        let rows: Vec<String> = match items.as_slice() {
            [one] if one.elements().count() == 1 && one.elements().all(|c| c.name == "m:eqArr") => {
                one.elements()
                    .next()
                    .unwrap()
                    .elements()
                    .filter(|e| e.name == "m:e")
                    .map(|r| arg(children(r, true)))
                    .collect()
            }
            _ => vec![arg(items
                .iter()
                .map(|e| children(e, false))
                .collect::<Vec<_>>()
                .join(" "))],
        };
        let (delim, reverse) = if open.is_empty() {
            (close, "true")
        } else {
            (open, "false")
        };
        return format!(
            "cases(delim: {}, reverse: #{reverse}, {})",
            quote(delim),
            rows.join(", ")
        );
    }
    let inner = items
        .iter()
        .map(|e| children(e, false))
        .collect::<Vec<_>>()
        .join(&format!(" mid({}) ", symbol(separator)));
    if open.is_empty() {
        return inner;
    }
    format!("lr({} {inner} {})", symbol(open), symbol(close))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(xml: &str) -> Element {
        let wrapped = format!(r#"<m:oMath xmlns:m="m">{xml}</m:oMath>"#);
        crate::xml::parse(wrapped.as_bytes()).unwrap()
    }

    #[test]
    fn fraction_script_and_letters() {
        let math = el(
            r#"<m:f><m:num><m:r><m:t>ab</m:t></m:r></m:num><m:den><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:den></m:f><m:r><m:t>=3.14</m:t></m:r>"#,
        );
        assert_eq!(to_typst(&math), r"frac(a b, attach(x, t: 2)) \= 3.14");
    }

    #[test]
    fn accent_and_delimiter_defaults() {
        let math = el(
            r#"<m:acc><m:e><m:r><m:t>y</m:t></m:r></m:e></m:acc><m:d><m:e><m:r><m:t>a,b</m:t></m:r></m:e></m:d>"#,
        );
        assert_eq!(to_typst(&math), r"hat(y) lr(\( a \, b \))");
    }
}
