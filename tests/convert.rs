//! 시험용 DOCX(tests/make_fixture.py 로 만든다)를 두 형식으로 바꾸고 다시 읽어 본다.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin(args: &[&std::ffi::OsStr]) -> (i32, serde_json::Value) {
    let run = Command::new(env!("CARGO_BIN_EXE_docx2hwpx-hlt"))
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let json = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("{e}: {stdout:?}"));
    (run.status.code().unwrap(), json)
}

fn convert(to: &str) -> (PathBuf, serde_json::Value) {
    let dir = std::env::temp_dir().join(format!("d2h-test-{}-{to}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join(format!("sample.{to}"));
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.docx");
    let (code, r) = bin(&[
        fixture.as_os_str(),
        "--to".as_ref(),
        to.as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(code, 0, "{r}");
    assert_eq!(r["ok"], true);
    assert_eq!(r["equations"], 6, "{r}");
    assert_eq!(r["metafiles"], 2, "{r}");
    // 그룹 도형(그림 · 고리 경로 · 화살표 · 글상자) 하나 → 그림 한 장
    assert_eq!(r["shapes"], 1, "{r}");
    // 넣기 · 지우기 · 지운 줄 · 서식 변경 · 지운 문단 표시 · 옮겨 간 글 · 옮겨 온 글
    assert_eq!(r["revisions_accepted"], 7, "{r}");
    assert_eq!(r["comments_removed"], 1, "{r}");
    assert!(
        !r["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "equation_as_text"),
        "{r}"
    );

    // 다시 돌리면 멈추고(output_exists · 원본 그대로), --overwrite 면 바꿔 쓴다.
    let before = std::fs::read(&out).unwrap();
    let (code, again) = bin(&[
        fixture.as_os_str(),
        "--to".as_ref(),
        to.as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(
        (code, again["error"]["code"].as_str()),
        (1, Some("output_exists"))
    );
    assert_eq!(std::fs::read(&out).unwrap(), before);
    let (code, _) = bin(&[
        fixture.as_os_str(),
        "--to".as_ref(),
        to.as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
        "--overwrite".as_ref(),
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        1,
        "임시 파일이 남지 않는다"
    );
    (out, r)
}

#[test]
fn usage_errors_are_json_with_exit_2() {
    let (code, r) = bin(&["a.docx".as_ref(), "--to".as_ref()]);
    assert_eq!((code, r["error"]["code"].as_str()), (2, Some("usage")));
    let (code, r) = bin(&["없는.docx".as_ref(), "--to".as_ref(), "hlt".as_ref()]);
    assert_eq!(
        (code, r["error"]["code"].as_str()),
        (1, Some("input_unreadable"))
    );
}

#[test]
fn hlt_keeps_equations_tables_and_nk_characters() {
    let (out, r) = convert("hlt");
    assert_eq!(r["dialect"], "domestic");
    assert_eq!(r["scripts"], 4);
    let glyphs: Vec<&str> = r["glyph_images"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["codepoints"].as_str().unwrap())
        .collect();
    assert_eq!(
        glyphs,
        ["U+0078 U+0302", "U+00FC", "U+0177", "U+210F"],
        "x̂ · ü · ŷ · ℏ(MT Extra) 는 그림으로(글자 순)"
    );
    assert_eq!(r["substitutions"][0]["to"], "-", "– → -");
    assert!(
        r["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "nested_table_flattened")
    );
    // 링크로만 연결된 그림은 「그림 없음」 자리표로(SVG 는 PNG 로, 파일이 없는 안 쓰는 관계는 조용히 지운다).
    let missing: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "image_missing")
        .map(|w| w["message"].as_str().unwrap())
        .collect();
    assert!(
        missing.len() == 1
            && missing[0].contains("연결")
            && missing[0].contains(r"C:\figs\fig1.png")
            && missing[0].contains("【0009】 부근"),
        "{missing:?}"
    );
    // hlt-core 가 거부하던 표는 고쳐 넘기고 무엇을 고쳤는지 알린다.
    let repaired: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "table_repaired")
        .map(|w| w["message"].as_str().unwrap())
        .collect();
    assert!(
        repaired.len() == 1
            && [
                "3번째 표",
                "빈 자리",
                "세로 병합",
                "모자란 줄",
                "콘텐츠 컨트롤",
                "열 너비",
                "부근"
            ]
            .iter()
            .all(|k| repaired[0].contains(k)),
        "{repaired:?}"
    );
    let (doc, payloads) = hlt_core::hml_document_from_hlt(&out).unwrap();
    // 수식 6 · WMF/EMF 2 · OLE 수식 1 · PNG 2 · 글자 그림 4(x̂ ŷ ü ℏ) · 그룹 도형 1 · SVG 1 · 「그림 없음」 1
    assert_eq!(payloads.len(), 18);
    // OLE 수식은 VML 크기(60pt ≈ 21mm)대로 — 쪽 폭으로 늘지 않는다(그림 단위 0.282mm).
    let ole = doc
        .paragraphs
        .iter()
        .find(|p| p.text.starts_with("【0005】"))
        .unwrap();
    let image = doc
        .images
        .iter()
        .find(|i| i.id == ole.image_ids[0])
        .unwrap();
    assert!(image.width.unwrap() < 100, "{image:?}");
    let first = doc
        .paragraphs
        .iter()
        .find(|p| p.text.starts_with("【0001】"))
        .unwrap();
    let has = |text: &str, format| {
        first.inlines.iter().any(|i| matches!(i, hlt_core::HmlInline::Text { text: t, formats, .. } if t == text && formats.contains(&format)))
    };
    assert!(has("2", hlt_core::CharacterFormat::Subscript), "CO₂");
    assert!(has("-3", hlt_core::CharacterFormat::Superscript), "10⁻³");
    assert!(first.text.contains("25 °C - 범위"), "{}", first.text);
    // Symbol 글꼴 글 · 줄 안 나뉘는 하이픈 · 풀어쓴 한글 · 일본어 가운뎃점 · 스타일로 받은 Symbol 글꼴 · ≈ → ≒
    assert!(
        first.text.contains("α®×1-2 한·끝 β") && first.text.contains("≒ 1"),
        "{}",
        first.text
    );
    let cell = |t: &str| doc.cells.iter().any(|c| c.text.trim() == t);
    assert!(
        cell("화합물 A") && cell("화합물 B") && cell("92%"),
        "고친 표의 칸 글"
    );
    assert!(cell("첫 줄 둘째 줄") && cell("안1 | 안2"));
    // 긴 첨자 문단: 저장 줄 경계가 영문 낱말(첨자 포함) 가운데에 오지 않는다(통특기 줄 규칙).
    let long = doc
        .paragraphs
        .iter()
        .find(|p| p.text.contains("confidencescore"))
        .unwrap();
    assert!(
        long.source_line_bytes.len() > 3,
        "{:?}",
        long.source_line_bytes
    );
    let chars: Vec<char> = long.text.chars().collect();
    let (mut at, mut bytes) = (0, 0);
    for &line in &long.source_line_bytes[..long.source_line_bytes.len() - 1] {
        while bytes < line {
            bytes += if chars[at].is_ascii() { 1 } else { 2 };
            at += 1;
        }
        bytes = 0;
        let (a, b) = (chars[at - 1], chars[at]);
        assert!(
            !(a.is_ascii_alphanumeric() && b.is_ascii_alphanumeric()),
            "{a}|{b}: {}",
            long.text
        );
    }
    let merged = doc.cells.iter().find(|c| c.text == "세로 합침").unwrap();
    assert_eq!((merged.rowspan, merged.colspan), (2, 1));
    let texts: Vec<&str> = doc.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("넣은 글")));
    for gone in ["지운 글", "옮겨 간 글", "지워야 할 메모"] {
        assert!(!texts.iter().any(|t| t.contains(gone)), "{gone}: {texts:?}");
    }
    assert!(!doc.cells.iter().any(|c| c.text.contains("지운 줄")));
    assert!(
        texts.contains(&"【0006】 메모 달린 글 굵게 바뀐 글"),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"【0007】 앞 문단 뒷 문단"),
        "지운 문단 표시 → 합침: {texts:?}"
    );
    assert!(texts.contains(&"【0008】 가운데 옮겨 온 글"), "{texts:?}");
    let memo = doc
        .paragraphs
        .iter()
        .find(|p| p.text.starts_with("【0006】"))
        .unwrap();
    assert!(
        memo.inlines.iter().any(
            |i| matches!(i, hlt_core::HmlInline::Text { text, formats, .. }
        if text.contains("굵게 바뀐 글") && formats.contains(&hlt_core::CharacterFormat::Bold))
        ),
        "새 서식(굵게)이 남는다"
    );
}

#[test]
fn hwpx_keeps_equations_tables_and_text() {
    let (out, _) = convert("hwpx");
    let bytes = std::fs::read(&out).unwrap();
    let decoded = hwpforge_smithy_hwpx::HwpxDecoder::decode(&bytes).unwrap();
    let mut text = String::new();
    let (mut tables, mut images) = (0, 0);
    for section in decoded.document.sections() {
        for p in &section.paragraphs {
            text.push_str(&p.text_content());
            p.for_each_table(|_| tables += 1);
            p.for_each_image(|_| images += 1);
        }
    }
    for gone in ["지운 글", "지운 줄", "옮겨 간 글", "지워야 할 메모"] {
        assert!(!text.contains(gone), "{gone}: {text}");
    }
    assert!(
        text.contains("【0007】 앞 문단 뒷 문단") && text.contains("【0008】 가운데 옮겨 온 글"),
        "{text}"
    );
    assert!(
        text.contains("CO₂")
            && text.contains("x̂")
            && text.contains("넣은 글")
            && !text.contains("지운 글"),
        "{text}"
    );
    assert_eq!(
        tables, 3,
        "표 + 칸 안의 표 + 구조를 고친 표(HWPX 는 그대로)"
    );
    assert_eq!(
        images, 14,
        "수식 6 · WMF/EMF 2 · OLE 수식 1 · PNG 2 · 그룹 도형 1 · SVG 1 · 「그림 없음」 1"
    );

    // 지식재산처가 고친 HWPX 에 맞춘 서식: 쪽 여백 · 바탕체 · 줄 간격 280% · 식별항목 규칙 · 런 합치기
    let section = &decoded.document.sections()[0];
    let page = &section.page_settings;
    let mm = |v: hwpforge_foundation::HwpUnit| v.as_i32();
    assert_eq!(
        (
            mm(page.margin_left),
            mm(page.margin_right),
            mm(page.margin_top),
            mm(page.margin_bottom)
        ),
        (7100, 5670, 11340, 5670)
    );
    assert_eq!((mm(page.header_margin), mm(page.footer_margin)), (0, 0));
    let store = &decoded.style_store;
    assert_eq!(
        store
            .font(hwpforge_foundation::FontIndex::new(0))
            .unwrap()
            .face_name,
        "바탕체"
    );
    let para = |p: &hwpforge_core::paragraph::Paragraph| store.para_shape(p.para_shape_id).unwrap();
    for p in &section.paragraphs {
        assert_eq!(para(p).line_spacing, 280);
        let t = p.text_content();
        if ["【청구범위】", "【요약서】", "【도면】"].contains(&t.trim()) {
            assert!(p.page_break, "{t} 는 새 쪽");
        }
        if t.trim() == "【발명의 명칭】" {
            assert_ne!(
                para(p).alignment,
                hwpforge_foundation::Alignment::Justify,
                "식별항목은 왼쪽"
            );
        }
        let texts: Vec<_> = p
            .runs
            .iter()
            .filter(|r| r.content.as_text().is_some())
            .collect();
        assert!(
            texts
                .windows(2)
                .all(|w| w[0].char_shape_id != w[1].char_shape_id)
                || p.runs.iter().any(|r| r.content.as_text().is_none()),
            "같은 모양 런은 합친다: {t}"
        );
    }
}

/// 화면 앱 게이지가 기대는 진행 알림: 줄지 않고, 수식마다 오고, 1.0 으로 끝난다.
#[test]
fn progress_rises_to_one() {
    let docx =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.docx"))
            .unwrap();
    let mut seen: Vec<(f32, String)> = Vec::new();
    let outcome = docx2hwpx_hlt::convert(&docx, docx2hwpx_hlt::Target::Hwpx, &[], &mut |f, s| {
        seen.push((f, s.to_string()))
    });
    assert!(outcome.result.is_ok());
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    assert_eq!(seen.last().unwrap().0, 1.0);
    assert!(
        seen.iter().any(|(_, s)| s == "수식 그리는 중 (6/6)"),
        "{seen:?}"
    );
}
