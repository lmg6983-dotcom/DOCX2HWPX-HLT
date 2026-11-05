use std::path::PathBuf;
use std::process::ExitCode;

use docx2hwpx_hlt::{Failure, Target, fail};
use serde_json::{Value, json};

const USAGE: &str = "\
사용법: docx2hwpx-hlt <입력.docx> --to <hwpx|hlt> [-o 출력] [--pct] [--image-dpi N] [--overwrite] [--json] [--font-dir 폴더]...

  --to hwpx     MOIP-Editor 용 HWPX
  --to hlt      NK-Editor 용 HLT (CP949 밖 글자는 바꾸거나 그림으로)
  -o            출력 경로(기본: 입력 이름 + 확장자)
  --overwrite   출력이 이미 있으면 바꿔 쓴다(없으면 멈춘다). 실패하면 원래 파일은 그대로
  --pct         HLT 를 PCT 국내진입 서식으로(기본: 국내 출원 명세서)
  --image-dpi   HLT 그림 해상도 상한(기본 300, 0 이면 그림을 다듬지 않음)
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
    max_dpi: u32,
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
    let mut max_dpi = docx2hwpx_hlt::image_opt::DEFAULT_MAX_DPI;
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
            "--image-dpi" => {
                max_dpi = value()?
                    .parse()
                    .map_err(|_| "--image-dpi 는 정수입니다".to_string())?;
            }
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
        max_dpi,
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
            docx2hwpx_hlt::messages::output_exists(&args.output.display().to_string()),
        ));
    }
    let docx = std::fs::read(&args.input).map_err(|e| {
        fail(
            "input_unreadable",
            docx2hwpx_hlt::messages::input_unreadable(
                &args.input.display().to_string(),
                &e.to_string(),
            ),
        )
    })?;
    let target = if args.hlt {
        Target::Hlt {
            pct: args.pct,
            max_dpi: args.max_dpi,
        }
    } else {
        Target::Hwpx
    };
    let outcome = docx2hwpx_hlt::convert(&docx, target, &args.font_dirs, &mut |_, _| {});
    if let (Value::Object(into), Value::Object(report)) = (result, outcome.report) {
        into.extend(report);
    }
    docx2hwpx_hlt::write_atomic(&args.output, &outcome.result?)
}

fn print_human(r: &Value) {
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    if r.get("equations").is_some() {
        eprintln!(
            "변경 내용 추적 {}곳 반영 · 메모 {}개 지움",
            n("revisions_accepted"),
            n("comments_removed")
        );
        eprintln!(
            "그림으로 넣음: 수식 {}개 · 옛 형식(WMF/EMF) 그림 {}개 · 도형 묶음 {}개",
            n("equations"),
            n("metafiles"),
            n("shapes")
        );
    }
    if r["format"] == "hlt" && r.get("scripts").is_some() {
        eprintln!("첨자 모양 글자 → 위/아래첨자 서식 {}곳", n("scripts"));
        for s in r["substitutions"].as_array().into_iter().flatten() {
            eprintln!(
                "  비슷한 글자로 바꿈: 「{}」 → 「{}」 {}곳",
                s["from"].as_str().unwrap_or_default(),
                s["to"].as_str().unwrap_or_default(),
                s["count"]
            );
        }
        for g in r["glyph_images"].as_array().into_iter().flatten() {
            eprintln!(
                "  NK-Editor 에 없어 그림으로 넣은 글자: 「{}」 ({}) {}곳",
                g["text"].as_str().unwrap_or_default(),
                g["codepoints"].as_str().unwrap_or_default(),
                g["count"]
            );
        }
    }
    let warnings: Vec<&Value> = r["warnings"].as_array().into_iter().flatten().collect();
    if !warnings.is_empty() {
        eprintln!("편집기에서 확인할 곳 {}:", warnings.len());
    }
    for w in warnings {
        eprintln!(
            "  - {} [{}]",
            w["message"].as_str().unwrap_or_default(),
            w["code"].as_str().unwrap_or_default()
        );
    }
    match r["error"]["message"].as_str() {
        Some(m) => eprintln!(
            "실패: {m} [{}]",
            r["error"]["code"].as_str().unwrap_or_default()
        ),
        None => eprintln!(
            "완료: {} — NK-Editor · MOIP-Editor 에서 열어 확인한 뒤 제출하세요",
            r["output"].as_str().unwrap_or_default()
        ),
    }
}
