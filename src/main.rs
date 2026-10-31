mod chars;
mod hwpx;
mod normalize;
mod omml;
mod render;
mod xml;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
사용법: docx2hwpx-hlt <입력.docx> --to <hwpx|hlt> [-o 출력] [--pct] [--font-dir 폴더]...

  --to hwpx    MOIP-Editor 용 HWPX
  --to hlt     NK-Editor 용 HLT (CP949 밖 글자는 바꾸거나 그림으로)
  -o           출력 경로(기본: 입력 이름 + 확장자). 이미 있으면 멈춘다
  --pct        HLT 를 PCT 국내진입 서식으로(기본: 국내 출원 명세서)
  --font-dir   수식 · 글자 그림에 쓸 글꼴 폴더 더하기(시스템 글꼴은 자동)";

struct Args {
    input: PathBuf,
    hlt: bool,
    output: PathBuf,
    pct: bool,
    font_dirs: Vec<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut input = None;
    let mut to = None;
    let mut output = None;
    let mut pct = false;
    let mut font_dirs = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--to" | "-t" => to = args.next(),
            "-o" | "--output" => output = args.next().map(PathBuf::from),
            "--pct" => pct = true,
            "--font-dir" => font_dirs.extend(args.next().map(PathBuf::from)),
            "-h" | "--help" => return Err(String::new()),
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
    let output = output.unwrap_or_else(|| input.with_extension(if hlt { "hlt" } else { "hwpx" }));
    Ok(Args {
        input,
        hlt,
        output,
        pct,
        font_dirs,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("실패: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    if args.output.exists() {
        return Err(format!(
            "{} 이(가) 이미 있습니다. 지우거나 -o 로 다른 이름을 주세요",
            args.output.display()
        ));
    }
    let docx = std::fs::read(&args.input).map_err(|e| format!("{}: {e}", args.input.display()))?;
    let renderer = render::Renderer::new(&args.font_dirs);
    let (normalized, report) = normalize::normalize(&docx, &renderer, args.hlt)?;

    eprintln!(
        "수식 그림 {}개 · WMF/EMF 그림 변환 {}개",
        report.equations, report.metafiles
    );
    if args.hlt {
        eprintln!("첨자 글자 → 첨자 서식 {}곳", report.scripts);
        let mut subs: Vec<_> = report.substitutions.iter().collect();
        subs.sort();
        for ((from, to), n) in subs {
            eprintln!(
                "  바꾼 글자: {from} (U+{:04X}) → {to:?} × {n}",
                *from as u32
            );
        }
        let mut glyphs: Vec<_> = report.glyph_images.iter().collect();
        glyphs.sort();
        for (g, n) in glyphs {
            eprintln!(
                "  글자 그림: {g} (U+{}) × {n}",
                g.chars()
                    .map(|c| format!("{:04X}", c as u32))
                    .collect::<Vec<_>>()
                    .join("+")
            );
        }
    }
    for f in &report.equation_failures {
        eprintln!("경고: 수식을 그리지 못해 글로 남김: {f}");
    }
    for w in &report.warnings {
        eprintln!("경고: {w}");
    }

    if args.hlt {
        write_hlt(&normalized, args)?;
    } else {
        let (bytes, warnings) = hwpx::convert(&normalized)?;
        for w in warnings {
            eprintln!("경고: {w}");
        }
        std::fs::write(&args.output, bytes)
            .map_err(|e| format!("{}: {e}", args.output.display()))?;
    }
    eprintln!("완료: {}", args.output.display());
    Ok(())
}

/// hlt-core 는 경로로 읽으므로 고쳐 쓴 DOCX 를 임시 파일로 둔다.
fn write_hlt(normalized: &[u8], args: &Args) -> Result<(), String> {
    let stem = args.input.file_stem().unwrap_or_default().to_string_lossy();
    let temp = std::env::temp_dir().join(format!("{stem}.d2h-{}.docx", std::process::id()));
    std::fs::write(&temp, normalized).map_err(|e| format!("{}: {e}", temp.display()))?;
    let result = hlt_from_docx(&temp, args);
    let _ = std::fs::remove_file(&temp);
    result
}

fn hlt_from_docx(docx: &Path, args: &Args) -> Result<(), String> {
    let core = |e: hlt_core::CoreError| format!("HLT: {} ({}) {:?}", e.message, e.phase, e.details);
    let (document, payloads, substitutions) =
        hlt_core::load_generation_input(docx).map_err(core)?;
    for s in substitutions {
        eprintln!("  바꾼 글자: {} → {} × {}", s.from, s.to, s.count);
    }
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
        &args.output,
        hlt_core::WriteOptions::default(),
    )
    .map_err(core)?;
    Ok(())
}
