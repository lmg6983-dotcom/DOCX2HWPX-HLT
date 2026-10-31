//! Typst 로 수식·글자를 PNG 로 굽는다. 글꼴은 Typst 내장(New Computer Modern 등) + 시스템 글꼴(한글).

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

/// 300 dpi.
const PIXEL_PER_PT: f64 = 300.0 / 72.0;
/// 한글 글꼴 후보(윈도 · 맥 · 리눅스). 없는 이름은 Typst 가 건너뛴다.
const KOREAN: &str = r#""Batang", "BatangChe", "AppleMyungjo", "Nanum Myeongjo", "Noto Serif CJK KR", "Malgun Gothic", "Apple SD Gothic Neo", "NanumGothic", "Noto Sans CJK KR""#;

pub struct Png {
    pub bytes: Vec<u8>,
    /// 그릴 크기(pt).
    pub width_pt: f64,
    pub height_pt: f64,
}

pub struct Renderer {
    library: LazyHash<Library>,
    fonts: FontStore,
}

impl Renderer {
    pub fn new(font_dirs: &[std::path::PathBuf]) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());
        for dir in font_dirs {
            fonts.extend(typst_kit::fonts::scan(dir));
        }
        Self {
            library: LazyHash::new(Library::default()),
            fonts,
        }
    }

    /// OMML 에서 옮긴 Typst 수식 몸통. `display` = 별행 수식(문단 수식).
    pub fn math(&self, body: &str, display: bool, size_pt: f64) -> Result<Png, String> {
        let (open, close) = if display { ("$ ", " $") } else { ("$", "$") };
        let source = format!(
            "#set page(width: auto, height: auto, margin: 1.5pt, fill: white)\n\
             #set text(size: {size_pt}pt, top-edge: \"bounds\", bottom-edge: \"bounds\", font: (\"New Computer Modern\", {KOREAN}))\n\
             #show math.equation: set text(font: (\"New Computer Modern Math\", {KOREAN}))\n\
             {open}{body}{close}\n"
        );
        self.compile(source)
    }

    /// 글자 하나(+ 결합 문자)를 본문 글자처럼 — 위아래는 글꼴의 ascender·descender 까지.
    pub fn glyph(&self, text: &str, size_pt: f64) -> Result<Png, String> {
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let source = format!(
            "#set page(width: auto, height: auto, margin: 0pt, fill: white)\n\
             #set text(size: {size_pt}pt, top-edge: \"ascender\", bottom-edge: \"descender\", \
             font: (\"Libertinus Serif\", \"New Computer Modern\", \"New Computer Modern Math\", {KOREAN}))\n\
             #box(inset: (x: 0.5pt))[#\"{escaped}\"]\n"
        );
        self.compile(source)
    }

    fn compile(&self, text: String) -> Result<Png, String> {
        let world = OneShot {
            renderer: self,
            source: Source::detached(text),
        };
        let document: PagedDocument = typst::compile(&world).output.map_err(|errors| {
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let page = document.pages().first().ok_or("빈 수식")?;
        let size = page.frame.size();
        let options = typst_render::RenderOptions {
            pixel_per_pt: typst::utils::Scalar::new(PIXEL_PER_PT),
            ..Default::default()
        };
        let pixmap = typst_render::render(page, &options);
        Ok(Png {
            bytes: pixmap.encode_png().map_err(|e| e.to_string())?,
            width_pt: size.x.to_pt(),
            height_pt: size.y.to_pt(),
        })
    }
}

struct OneShot<'a> {
    renderer: &'a Renderer,
    source: Source,
}

impl World for OneShot<'_> {
    fn library(&self) -> &LazyHash<Library> {
        &self.renderer.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.renderer.fonts.book()
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, _: FileId) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.renderer.fonts.font(index)
    }
    fn today(&self, _: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

/// WMF · EMF(OLE 수식 미리보기, 도면) → PNG. 크기는 SVG 가 말하는 크기를 쓴다.
pub fn metafile_to_png(bytes: &[u8], is_emf: bool) -> Result<Vec<u8>, String> {
    let svg = if is_emf {
        emf_core::converter::convert_to_svg(bytes).map_err(|e| format!("EMF: {e:?}"))?
    } else {
        wmf_core::converter::convert_to_svg(bytes).map_err(|e| format!("WMF: {e:?}"))?
    };
    svg_to_png(&svg)
}

fn svg_to_png(svg: &[u8]) -> Result<Vec<u8>, String> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts(); // 그림 속 글자
    let tree = resvg::usvg::Tree::from_data(svg, &options).map_err(|e| format!("SVG: {e}"))?;
    let size = tree.size();
    // 미리보기는 대개 96dpi 크기라 3배로 그려 300dpi 쯤에 맞추되, 긴 변은 2400px 까지.
    let scale = (3.0_f32).min(2400.0 / size.width().max(size.height()));
    let width = ((size.width() * scale).ceil() as u32).max(1);
    let height = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("SVG 크기")?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| e.to_string())
}
