//! DOCX → HWPX(MOIP-Editor) · HLT(NK-Editor). 명령줄(`src/main.rs`)과 화면 앱(`src/bin/gui.rs`)이 함께 쓴다.

mod chars;
mod hwpx;
mod normalize;
mod omml;
mod render;
mod xml;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

/// 실패 하나. `code` 는 고정된 이름이다(AGENTS.md 의 표).
#[derive(Debug)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

pub fn fail(code: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Hwpx,
    /// `pct` = PCT 국내진입 서식(아니면 국내 출원 명세서)
    Hlt {
        pct: bool,
    },
}

impl Target {
    pub fn extension(self) -> &'static str {
        match self {
            Target::Hwpx => "hwpx",
            Target::Hlt { .. } => "hlt",
        }
    }
}

pub struct Outcome {
    /// 출력 파일 내용
    pub result: Result<Vec<u8>, Failure>,
    /// equations · metafiles · scripts · substitutions · glyph_images · warnings. 고쳐 쓰기 전에 실패하면 빈 객체.
    pub report: Value,
}

/// 변환 단계 알림: (0.0 ~ 1.0, 지금 하는 일).
pub type Progress<'a> = &'a mut dyn FnMut(f32, &str);

/// DOCX 내용 → 출력 파일 내용. 파일은 쓰지 않는다(HLT 는 hlt-core 가 경로로만 일해서 임시 파일을 거친다).
pub fn convert(docx: &[u8], target: Target, font_dirs: &[PathBuf], progress: Progress) -> Outcome {
    progress(0.02, "글꼴을 읽는 중");
    let renderer = render::Renderer::new(font_dirs);
    progress(0.15, "수식 · 그림을 바꾸는 중");
    let hlt = matches!(target, Target::Hlt { .. });
    let mut on_math = |done: usize, total: usize| {
        let share = (done as f32 / total.max(1) as f32).min(1.0);
        progress(
            0.15 + 0.7 * share,
            &format!("수식 그리는 중 ({done}/{total})"),
        );
    };
    let (normalized, report) = match normalize::normalize(docx, &renderer, hlt, &mut on_math) {
        Ok(v) => v,
        Err(e) => {
            return Outcome {
                result: Err(fail("invalid_docx", e)),
                report: json!({}),
            };
        }
    };
    progress(
        0.87,
        if hlt {
            "HLT 쓰는 중"
        } else {
            "HWPX 쓰는 중"
        },
    );

    let mut substitutions: Vec<Value> = report
        .substitutions
        .iter()
        .map(|((from, to), n)| substitution(*from, to, *n))
        .collect();
    let mut glyphs: Vec<Value> = report
        .glyph_images
        .iter()
        .map(|(g, n)| json!({"text": g, "codepoints": codepoints(g), "count": n}))
        .collect();
    glyphs.sort_by_key(|v| v["text"].as_str().unwrap_or_default().to_string());
    let mut warnings: Vec<Value> = report
        .warnings
        .iter()
        .map(|(code, message)| json!({"code": code, "message": message}))
        .collect();

    let result = match target {
        Target::Hlt { pct } => hlt_bytes(&normalized, pct, &mut substitutions),
        Target::Hwpx => hwpx::convert(&normalized)
            .map(|(bytes, w)| {
                warnings.extend(
                    w.iter()
                        .map(|(code, message)| json!({"code": code, "message": message})),
                );
                bytes
            })
            .map_err(|e| fail("convert_failed", e)),
    };
    substitutions.sort_by_key(|v| v["from"].as_str().unwrap_or_default().to_string());
    if result.is_ok() {
        progress(1.0, "끝");
    }
    Outcome {
        result,
        report: json!({
            "revisions_accepted": report.revisions_accepted,
            "comments_removed": report.comments_removed,
            "equations": report.equations,
            "metafiles": report.metafiles,
            "scripts": report.scripts,
            "substitutions": substitutions,
            "glyph_images": glyphs,
            "warnings": warnings,
        }),
    }
}

/// 옆의 임시 이름에 다 쓴 뒤 옮긴다 — 실패해도 원래 파일이 남고, 반쯤 쓴 파일이 생기지 않는다.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let temp = path.with_file_name(format!(
        ".{}.d2h-{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let written = std::fs::write(&temp, bytes)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| fail("write_failed", format!("{}: {e}", path.display())));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

fn codepoints(s: &str) -> String {
    s.chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

fn substitution(from: char, to: &str, count: usize) -> Value {
    json!({"from": from.to_string(), "codepoints": codepoints(&from.to_string()), "to": to, "count": count})
}

/// hlt-core 는 경로로 읽고 쓰므로 임시 폴더를 거친다. 같은 프로세스에서 여러 번 불려도 이름이 겹치지 않게 번호를 단다.
fn hlt_bytes(
    normalized: &[u8],
    pct: bool,
    substitutions: &mut Vec<Value>,
) -> Result<Vec<u8>, Failure> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let base = format!(
        "d2h-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let docx = std::env::temp_dir().join(format!("{base}.docx"));
    let output = std::env::temp_dir().join(format!("{base}.hlt"));
    let result = std::fs::write(&docx, normalized)
        .map_err(|e| fail("write_failed", format!("{}: {e}", docx.display())))
        .and_then(|()| hlt_from_docx(&docx, &output, pct, substitutions))
        .and_then(|()| {
            std::fs::read(&output)
                .map_err(|e| fail("write_failed", format!("{}: {e}", output.display())))
        });
    let _ = std::fs::remove_file(&docx);
    let _ = std::fs::remove_file(&output);
    result
}

fn hlt_from_docx(
    docx: &Path,
    output: &Path,
    pct: bool,
    substitutions: &mut Vec<Value>,
) -> Result<(), Failure> {
    let core = |e: hlt_core::CoreError| {
        fail(
            "convert_failed",
            format!("HLT: {} ({}) {:?}", e.message, e.phase, e.details),
        )
    };
    let (document, payloads, subs) = hlt_core::load_generation_input(docx).map_err(core)?;
    // 우리 글자 규칙이 먼저 거의 다 바꾸지만, hlt-core 가 마저 바꾼 것도 함께 알린다.
    substitutions.extend(subs.iter().map(|s| substitution(s.from, &s.to, s.count)));
    let dialect = if pct {
        hlt_core::GenerationDialect::PctPatent
    } else {
        hlt_core::GenerationDialect::PatentSpecificationKo
    };
    hlt_core::save_generated_hlt_as_new(
        docx,
        &document,
        dialect,
        &payloads,
        output,
        hlt_core::WriteOptions::default(),
    )
    .map_err(core)?;
    Ok(())
}
