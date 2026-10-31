mod chars;
mod hwpx;
mod normalize;
mod omml;
mod render;
mod xml;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

const USAGE: &str = "\
사용법: docx2hwpx-hlt <입력.docx> --to <hwpx|hlt> [-o 출력] [--pct] [--overwrite] [--json] [--font-dir 폴더]...

  --to hwpx     MOIP-Editor 용 HWPX
  --to hlt      NK-Editor 용 HLT (CP949 밖 글자는 바꾸거나 그림으로)
  -o            출력 경로(기본: 입력 이름 + 확장자)
  --overwrite   출력이 이미 있으면 바꿔 쓴다(없으면 멈춘다). 실패하면 원래 파일은 그대로
  --pct         HLT 를 PCT 국내진입 서식으로(기본: 국내 출원 명세서)
  --json        결과를 JSON 한 줄로 표준 출력에(사람용 글은 내지 않는다)
  --font-dir    수식 · 글자 그림에 쓸 글꼴 폴더 더하기(시스템 글꼴은 자동)
  --version     판 번호

종료 코드: 0 성공(경고가 있어도) · 1 변환 실패 · 2 인자 잘못";

struct Args {
    input: PathBuf,
    hlt: bool,
    output: PathBuf,
    pct: bool,
    overwrite: bool,
    json: bool,
    font_dirs: Vec<PathBuf>,
}

/// 실패 하나. `code` 는 에이전트가 갈래를 탈 수 있게 고정된 이름이다(AGENTS.md).
struct Failure {
    code: &'static str,
    message: String,
}

fn fail(code: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}

enum Parsed {
    Run(Args),
    Help,
    Version,
}

fn parse_args(raw: &[String]) -> Result<Parsed, String> {
    let mut input = None;
    let mut to = None;
    let mut output = None;
    let (mut pct, mut overwrite, mut json) = (false, false, false);
    let mut font_dirs = Vec::new();
    let mut args = raw.iter();
    while let Some(a) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{a} 뒤에 값이 없습니다"))
        };
        match a.as_str() {
            "--to" | "-t" => to = Some(value()?),
            "-o" | "--output" => output = Some(PathBuf::from(value()?)),
            "--font-dir" => font_dirs.push(PathBuf::from(value()?)),
            "--pct" => pct = true,
            "--overwrite" => overwrite = true,
            "--json" => json = true,
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            _ if input.is_none() && !a.starts_with('-') => input = Some(PathBuf::from(a)),
            _ => return Err(format!("모르는 인자: {a}")),
        }
    }
    let input: PathBuf = input.ok_or("입력 파일을 주세요")?;
    // --to 가 없으면 출력 확장자로 고른다.
    let to = to
        .or_else(|| {
            output
                .as_ref()
                .and_then(|o: &PathBuf| o.extension())
                .map(|e| e.to_string_lossy().into_owned())
        })
        .ok_or("--to hwpx 또는 --to hlt 를 주세요")?
        .to_ascii_lowercase();
    let hlt = match to.as_str() {
        "hwpx" => false,
        "hlt" => true,
        _ => return Err(format!("--to 는 hwpx 또는 hlt: {to}")),
    };
    if pct && !hlt {
        return Err("--pct 는 --to hlt 에만 씁니다".into());
    }
    let output = output.unwrap_or_else(|| input.with_extension(if hlt { "hlt" } else { "hwpx" }));
    Ok(Parsed::Run(Args {
        input,
        hlt,
        output,
        pct,
        overwrite,
        json,
        font_dirs,
    }))
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let wants_json = raw.iter().any(|a| a == "--json");
    let args = match parse_args(&raw) {
        Ok(Parsed::Run(a)) => a,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Parsed::Version) => {
            println!("docx2hwpx-hlt {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            if wants_json {
                println!(
                    "{}",
                    json!({"ok": false, "error": {"code": "usage", "message": e}})
                );
            } else {
                eprintln!("{e}\n\n{USAGE}");
            }
            return ExitCode::from(2);
        }
    };
    let mut result = json!({
        "ok": false,
        "input": args.input,
        "output": args.output,
        "format": if args.hlt { "hlt" } else { "hwpx" },
        "dialect": if !args.hlt { Value::Null } else if args.pct { "pct".into() } else { "domestic".into() },
    });
    let outcome = run(&args, &mut result);
    if let Err(f) = &outcome {
        result["error"] = json!({"code": f.code, "message": f.message});
    } else {
        result["ok"] = true.into();
    }
    if args.json {
        println!("{result}");
    } else {
        print_human(&result);
    }
    if outcome.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(args: &Args, result: &mut Value) -> Result<(), Failure> {
    if args.output.exists() && !args.overwrite {
        return Err(fail(
            "output_exists",
            format!(
                "{} 이(가) 이미 있습니다. --overwrite 를 주거나 -o 로 다른 이름을 주세요",
                args.output.display()
            ),
        ));
    }
    let docx = std::fs::read(&args.input)
        .map_err(|e| fail("input_unreadable", format!("{}: {e}", args.input.display())))?;
    let renderer = render::Renderer::new(&args.font_dirs);
    let (normalized, report) =
        normalize::normalize(&docx, &renderer, args.hlt).map_err(|e| fail("invalid_docx", e))?;

    let mut substitutions: Vec<Value> = report
        .substitutions
        .iter()
        .map(|((from, to), n)| json!({"from": from.to_string(), "codepoints": codepoints(&from.to_string()), "to": to, "count": n}))
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

    // 출력은 옆의 임시 이름에 다 쓴 뒤 옮긴다 — 실패해도 원래 파일이 남고, 반쯤 쓴 파일이 생기지 않는다.
    let temp = args.output.with_file_name(format!(
        ".{}.d2h-{}.tmp",
        args.output
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        std::process::id()
    ));
    let written = if args.hlt {
        write_hlt(&normalized, args, &temp, &mut substitutions)
    } else {
        hwpx::convert(&normalized)
            .map_err(|e| fail("convert_failed", e))
            .and_then(|(bytes, w)| {
                warnings.extend(
                    w.iter()
                        .map(|(code, message)| json!({"code": code, "message": message})),
                );
                std::fs::write(&temp, bytes)
                    .map_err(|e| fail("write_failed", format!("{}: {e}", temp.display())))
            })
    };
    let written = written.and_then(|()| {
        std::fs::rename(&temp, &args.output)
            .map_err(|e| fail("write_failed", format!("{}: {e}", args.output.display())))
    });
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    substitutions.sort_by_key(|v| v["from"].as_str().unwrap_or_default().to_string());

    result["equations"] = report.equations.into();
    result["metafiles"] = report.metafiles.into();
    result["scripts"] = report.scripts.into();
    result["substitutions"] = substitutions.into();
    result["glyph_images"] = glyphs.into();
    result["warnings"] = warnings.into();
    written
}

fn codepoints(s: &str) -> String {
    s.chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_human(r: &Value) {
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    if r.get("equations").is_some() {
        eprintln!(
            "수식 그림 {}개 · WMF/EMF 그림 변환 {}개",
            n("equations"),
            n("metafiles")
        );
    }
    if r["format"] == "hlt" && r.get("scripts").is_some() {
        eprintln!("첨자 글자 → 첨자 서식 {}곳", n("scripts"));
        for s in r["substitutions"].as_array().into_iter().flatten() {
            eprintln!(
                "  바꾼 글자: {} ({}) → {} × {}",
                s["from"], s["codepoints"], s["to"], s["count"]
            );
        }
        for g in r["glyph_images"].as_array().into_iter().flatten() {
            eprintln!(
                "  글자 그림: {} ({}) × {}",
                g["text"], g["codepoints"], g["count"]
            );
        }
    }
    for w in r["warnings"].as_array().into_iter().flatten() {
        eprintln!(
            "경고[{}]: {}",
            w["code"].as_str().unwrap_or_default(),
            w["message"].as_str().unwrap_or_default()
        );
    }
    match r["error"]["message"].as_str() {
        Some(m) => eprintln!(
            "실패[{}]: {m}",
            r["error"]["code"].as_str().unwrap_or_default()
        ),
        None => eprintln!("완료: {}", r["output"].as_str().unwrap_or_default()),
    }
}

/// hlt-core 는 경로로 읽으므로 고쳐 쓴 DOCX 를 임시 파일로 둔다.
fn write_hlt(
    normalized: &[u8],
    args: &Args,
    output: &Path,
    substitutions: &mut Vec<Value>,
) -> Result<(), Failure> {
    let stem = args.input.file_stem().unwrap_or_default().to_string_lossy();
    let docx = std::env::temp_dir().join(format!("{stem}.d2h-{}.docx", std::process::id()));
    std::fs::write(&docx, normalized)
        .map_err(|e| fail("write_failed", format!("{}: {e}", docx.display())))?;
    let result = hlt_from_docx(&docx, args, output, substitutions);
    let _ = std::fs::remove_file(&docx);
    result
}

fn hlt_from_docx(
    docx: &Path,
    args: &Args,
    output: &Path,
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
    substitutions.extend(subs.iter().map(|s| {
        json!({"from": s.from.to_string(), "codepoints": codepoints(&s.from.to_string()), "to": s.to, "count": s.count})
    }));
    let dialect = if args.pct {
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
