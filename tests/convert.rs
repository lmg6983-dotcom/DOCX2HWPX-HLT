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
        ["U+0078 U+0302", "U+00FC", "U+0177"],
        "x̂ · ü · ŷ 는 그림으로(글자 순)"
    );
    assert_eq!(r["substitutions"][0]["to"], "-", "– → -");
    assert!(
        r["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "nested_table_flattened")
    );
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
