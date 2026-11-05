//! 사용자에게 보이는 문구. 개발자가 아닌 사람(변리사 · 명세서 작성자)이 읽고 바로 할 일을 알 수 있게
//! 「어디서 — 무엇이 — 어떻게」 순서로 쓰고, 개발자에게 넘길 기술 정보는 맨 뒤 괄호에 둔다.
//! 경고 · 실패 코드는 계약(AGENTS.md)이라 그대로 두고, 이 문구만 사람 말로 다듬는다.

/// 기술 정보 머리. 화면 앱은 이 앞까지만 크게 보여 준다([`split`]).
const TECH: &str = " (기술 정보: ";

/// 사람용 문장 + 맨 뒤 기술 정보.
pub fn with_tech(plain: &str, tech: &str) -> String {
    if tech.trim().is_empty() {
        plain.to_string()
    } else {
        format!("{plain}{TECH}{tech})")
    }
}

/// (사람용 문장, 기술 정보).
pub fn split(message: &str) -> (&str, Option<&str>) {
    match message.find(TECH) {
        Some(at) => (
            &message[..at],
            Some(message[at + TECH.len()..].trim_end_matches(')')),
        ),
        None => (message, None),
    }
}

/// 문서에서의 자리: 가장 가까운 앞의 식별 번호(【0012】 · 【청구항 3】 · 【도 2】).
pub fn place(marker: &str) -> String {
    if marker.is_empty() {
        "문서 앞부분".to_string()
    } else {
        format!("{marker} 부근")
    }
}

/// 문단 글이 【…】 로 시작하면 그 식별 번호.
pub fn marker_of(text: &str) -> Option<String> {
    let t = text.trim_start();
    if !t.starts_with('【') {
        return None;
    }
    let end = t.find('】')?;
    Some(t[..end + '】'.len_utf8()].to_string())
}

/// 그림 관계 Target 을 사람이 알아볼 꼴로: `file:///C:/figs/a.png` → `C:\figs\a.png`, `media/x.png` → `x.png`.
pub fn picture_name(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("file:///") {
        return percent_decode(rest).replace('/', "\\");
    }
    if target.starts_with("http://") || target.starts_with("https://") || target.starts_with("\\\\")
    {
        return percent_decode(target);
    }
    target.rsplit('/').next().unwrap_or(target).to_string()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 그림을 「그림 없음」 표시로 바꾼 까닭.
#[derive(Clone, Copy)]
pub enum MissingPicture {
    Linked,
    NotInFile,
    Unreadable,
    Svg,
}

pub fn picture_missing(at: &str, target: &str, why: MissingPicture) -> String {
    let name = picture_name(target);
    let (what, todo) = match why {
        MissingPicture::Linked => (
            format!("문서 안에 그림이 없고 다른 파일({name})에 연결만 되어 있어"),
            "Word 에서 그 그림을 지우고 「삽입 → 그림」으로 다시 넣거나(「파일에 연결」이 아닌 그냥 「삽입」), 「파일 → 정보 → 파일에 대한 연결 편집 → 연결 끊기」를 한 뒤 다시 변환해 주세요",
        ),
        MissingPicture::NotInFile => (
            format!("그림 파일({name})이 문서에서 빠져 있어"),
            "원본 그림을 Word 에서 다시 넣고 저장한 뒤 다시 변환해 주세요",
        ),
        MissingPicture::Unreadable => (
            format!("그림 파일({name})이 손상되었거나 읽을 수 없는 형식이라"),
            "원본 그림을 PNG 나 JPG 로 저장해 Word 에서 다시 넣은 뒤 다시 변환해 주세요",
        ),
        MissingPicture::Svg => (
            format!("SVG 그림({name})을 그리지 못해"),
            "그 그림을 PNG 로 저장해 Word 에서 다시 넣은 뒤 다시 변환해 주세요",
        ),
    };
    format!(
        "{} 그림: {what} 그 자리에 「그림 없음」 표시를 넣었습니다. 이대로 제출하면 그림이 빠집니다 — {todo}.",
        place(at)
    )
}

pub fn pictures_dangling(count: usize) -> String {
    format!(
        "그림 {count}개를 문서 안에서 찾을 수 없어 그 자리에 「그림 없음」 표시를 넣었습니다. 이대로 제출하면 그림이 빠집니다 — 「그림 없음」이 보이는 자리에 원본 그림을 Word 에서 다시 넣은 뒤 다시 변환해 주세요."
    )
}

pub fn metafile_placeholder(at: &str, target: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "{} 그림: 옛 형식(WMF/EMF) 그림 {}을 그리지 못해 「그림 변환 실패」 표시를 넣었습니다. 원본 그림을 PNG 로 저장해 Word 에서 다시 넣은 뒤 다시 변환해 주세요.",
            place(at),
            picture_name(target)
        ),
        tech,
    )
}

pub fn metafile_failed(at: &str, target: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "{} 그림: 옛 형식(WMF/EMF) 그림 {}을 넣지 못했습니다. 그 자리의 그림이 빠지니, 원본 그림을 PNG 로 저장해 Word 에서 다시 넣은 뒤 다시 변환해 주세요.",
            place(at),
            picture_name(target)
        ),
        tech,
    )
}

pub fn images_downscaled(before_mb: f64, limit_mb: f64, dpi: u32, after_mb: f64) -> String {
    format!(
        "그림이 많아 문서가 {before_mb:.0}MB 로 NK-Editor(HLT) 파일 한도 {limit_mb:.0}MB 를 넘어서, 그림 선명도를 {dpi}dpi 로 낮춰 {after_mb:.0}MB 로 맞췄습니다. 작은 글씨가 든 도면 · 구조식이 잘 읽히는지 편집기에서 확인해 주세요."
    )
}

pub fn too_large(size_mb: f64, limit_mb: f64) -> String {
    format!(
        "그림이 너무 많거나 커서, 그림 선명도를 가장 낮게(72dpi) 줄여도 문서가 {size_mb:.0}MB 로 NK-Editor(HLT) 파일 한도 {limit_mb:.0}MB 를 넘습니다. 문서를 둘 이상으로 나누어 변환하거나, 큰 사진 · 도면을 줄여 다시 넣어 주세요."
    )
}

pub fn table_empty(at: &str, n: usize) -> String {
    format!(
        "{} 표(문서의 {n}번째 표): 내용이 없는 빈 표라서 뺐습니다.",
        place(at)
    )
}

pub fn table_repaired(at: &str, n: usize, fixes: &[&str]) -> String {
    format!(
        "{} 표(문서의 {n}번째 표): 칸 배치가 NK-Editor 형식에 맞지 않아 고쳐 넣었습니다({}). 편집기에서 표 모양과 칸 내용이 원본과 같은지 확인해 주세요.",
        place(at),
        fixes.join(" · ")
    )
}

pub fn nested_table(at: &str) -> String {
    format!(
        "{} 표: 표 칸 안에 든 표는 NK-Editor 가 지원하지 않아 글(「칸 | 칸 / 다음 줄」)로 풀어 넣었습니다. 편집기에서 내용을 확인해 주세요.",
        place(at)
    )
}

pub fn equation_as_text(at: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "{} 수식: 수식을 그림으로 그리지 못해 글자로 남겼습니다. 편집기에서 수식이 맞는지 확인하고, 틀리면 편집기에서 다시 입력해 주세요.",
            place(at)
        ),
        tech,
    )
}

pub fn symbol_unmapped(at: &str, font: &str, code: u32) -> String {
    with_tech(
        &format!(
            "{} 기호: {font} 글꼴의 기호 하나를 바꿀 글자가 없어 그대로 두었습니다. 편집기에서 깨져 보이면 Word 에서 「삽입 → 기호」의 일반 글꼴 기호로 바꿔 다시 변환해 주세요.",
            place(at)
        ),
        &format!("{font} U+{code:04X}"),
    )
}

pub fn glyph_failed(at: &str, text: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "{} 글자 「{text}」: NK-Editor 에 없는 글자라 그림으로 넣으려 했지만 그리지 못했습니다. 변환이 실패할 수 있으니 Word 에서 그 글자를 다른 표현으로 바꿔 주세요.",
            place(at)
        ),
        tech,
    )
}

pub fn shape_partial(at: &str, what: &str) -> String {
    format!(
        "{} 도형: 도형 묶음을 그림 한 장으로 넣었지만 일부({what})는 그리지 못했습니다. 그림에서 빠진 선 · 화살표 · 글자가 없는지 확인해 주세요.",
        place(at)
    )
}

pub fn shape_failed(at: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "{} 도형: 도형 묶음을 그림으로 만들지 못해 그 안의 선 · 화살표가 빠집니다. Word 에서 그 도형 묶음을 복사해 「선택하여 붙여넣기 → 그림(PNG)」으로 바꾼 뒤 다시 변환해 주세요.",
            place(at)
        ),
        tech,
    )
}

/// hlt-core 실패(코드 · 단계) → 사람용 문장.
pub fn hlt_failure(code: &str, phase: &str, tech: &str) -> String {
    let plain = match (code, phase) {
        (_, "docx_relationships" | "docx_image") => {
            "문서의 그림 하나를 읽지 못했습니다. 손상되었거나 「파일에 연결」로 넣은 그림이 있는지 확인하고, 그 그림을 Word 에서 다시 넣은 뒤 다시 변환해 주세요."
        }
        (_, "docx_table" | "table_rowspan" | "generate_tail") | ("geometry_invalid", _) => {
            "문서의 표 하나를 NK-Editor 형식으로 바꾸지 못했습니다. 셀 병합이 복잡한 표가 원인인 경우가 많습니다. 문제가 되는 표의 병합을 풀었다가 다시 하거나, 표를 새로 만들어 내용을 붙여 넣은 뒤 다시 변환해 주세요."
        }
        ("size_limit_exceeded", _) => {
            "문서가 너무 커서 NK-Editor(HLT) 파일로 바꿀 수 없습니다. 문서를 둘 이상으로 나누어 변환하거나 큰 그림을 줄여 주세요."
        }
        ("cp949_invalid", _) => {
            "NK-Editor 에 넣을 수 없는 글자가 남아 있습니다. HWPX 로 변환해 보거나, 특수 글자를 일반 글자로 바꾼 뒤 다시 변환해 주세요."
        }
        (_, "docx_document" | "docx_package" | "docx_read") => {
            "Word 문서의 구조를 읽지 못했습니다. Word 에서 문서를 열어 「다른 이름으로 저장 → Word 문서(.docx)」로 다시 저장한 뒤 변환해 주세요."
        }
        _ => {
            "NK-Editor(HLT) 파일을 만들지 못했습니다. HWPX 로 변환해 보거나, 이 메시지를 그대로 개발자에게 전해 주세요."
        }
    };
    with_tech(plain, tech)
}

pub fn hwpx_failure(tech: &str) -> String {
    with_tech(
        "MOIP-Editor(HWPX) 파일을 만들지 못했습니다. HLT 로 변환해 보거나, 이 메시지를 그대로 개발자에게 전해 주세요.",
        tech,
    )
}

pub fn invalid_docx(tech: &str) -> String {
    with_tech(
        "Word 문서(.docx)가 아니거나 파일이 손상되었습니다. Word 에서 열어 「다른 이름으로 저장 → Word 문서(.docx)」로 다시 저장한 뒤 변환해 주세요. 옛 .doc 파일도 이렇게 .docx 로 저장하면 됩니다.",
        tech,
    )
}

pub fn input_unreadable(path: &str, tech: &str) -> String {
    with_tech(
        &format!("파일({path})을 열지 못했습니다. 파일이 옮겨졌거나 지워졌는지 확인해 주세요."),
        tech,
    )
}

pub fn write_failed(path: &str, tech: &str) -> String {
    with_tech(
        &format!(
            "파일({path})을 저장하지 못했습니다. 저장할 폴더가 있는지, 같은 이름의 파일이 다른 프로그램에서 열려 있지 않은지, 디스크 공간이 남아 있는지 확인해 주세요."
        ),
        tech,
    )
}

pub fn output_exists(path: &str) -> String {
    format!(
        "같은 이름의 파일({path})이 이미 있어 멈췄습니다. 덮어쓰려면 --overwrite 를, 다른 이름으로 저장하려면 -o 로 새 이름을 주세요."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_tech_split() {
        let m = with_tech("쉬운 말.", "HLT: geometry_invalid (docx_table) {}");
        assert_eq!(
            split(&m),
            ("쉬운 말.", Some("HLT: geometry_invalid (docx_table) {}"))
        );
        assert_eq!(split("쉬운 말."), ("쉬운 말.", None));
    }

    #[test]
    fn places_and_names() {
        assert_eq!(
            marker_of("  【0012】 상기 화합물").as_deref(),
            Some("【0012】")
        );
        assert_eq!(marker_of("상기 【0012】"), None);
        assert_eq!(place(""), "문서 앞부분");
        assert_eq!(
            picture_name("file:///C:/My%20figs/fig%201.png"),
            r"C:\My figs\fig 1.png"
        );
        assert_eq!(picture_name("media/image3.png"), "image3.png");
    }
}
