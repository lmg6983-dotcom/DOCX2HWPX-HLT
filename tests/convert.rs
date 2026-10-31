//! 시험용 DOCX(tests/make_fixture.py 로 만든다)를 두 형식으로 바꾸고 다시 읽어 본다.

use std::path::{Path, PathBuf};
use std::process::Command;

fn convert(to: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("d2h-test-{}-{to}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join(format!("sample.{to}"));
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.docx");
    let run = Command::new(env!("CARGO_BIN_EXE_docx2hwpx-hlt"))
        .arg(&fixture)
        .args(["--to", to, "-o"])
        .arg(&out)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{log}");
    assert!(log.contains("수식 그림 6개"), "{log}");
    assert!(!log.contains("수식을 그리지 못해"), "{log}");
    assert!(log.contains("WMF/EMF 그림 변환 2개"), "{log}");
    (out, log)
}

#[test]
fn hlt_keeps_equations_tables_and_nk_characters() {
    let (out, log) = convert("hlt");
    assert!(log.contains("U+0078+0302"), "x̂ 는 그림으로: {log}");
    let (doc, payloads) = hlt_core::hml_document_from_hlt(&out).unwrap();
    // 수식 6 · WMF/EMF 2 · OLE 수식 1 · PNG 2 · 글자 그림 3(x̂ ŷ ü)
    assert_eq!(payloads.len(), 14);
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
    let cell = |t: &str| doc.cells.iter().any(|c| c.text.trim() == t);
    assert!(cell("첫 줄 둘째 줄") && cell("안1 | 안2"));
    let merged = doc.cells.iter().find(|c| c.text == "세로 합침").unwrap();
    assert_eq!((merged.rowspan, merged.colspan), (2, 1));
    assert!(doc.paragraphs.iter().any(|p| p.text.contains("넣은 글")));
    assert!(!doc.paragraphs.iter().any(|p| p.text.contains("지운 글")));
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
    assert!(
        text.contains("CO₂")
            && text.contains("x̂")
            && text.contains("넣은 글")
            && !text.contains("지운 글"),
        "{text}"
    );
    assert_eq!(tables, 2, "표 + 칸 안의 표");
    assert_eq!(images, 11, "수식 6 · WMF/EMF 2 · OLE 수식 1 · PNG 2");
}
