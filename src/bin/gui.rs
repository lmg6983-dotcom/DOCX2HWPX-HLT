//! 화면 앱: DOCX 고르기(끌어다 놓기 · 누르기 · Ctrl+O · exe 에 끌어다 놓기) → HLT/HWPX 고르기 →
//! 변환(게이지) → 다 차면 다른 이름으로 저장(이름은 입력과 같고 확장자만 바뀐다).
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use docx2hwpx_hlt::{Failure, Target};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Key, KeyboardShortcut, Layout, Modifiers, RichText, Sense,
    Stroke, StrokeKind,
};
use serde_json::Value;

// 밝은 바탕(#F8F8F8 안팎) 위에서 본문 대비 4.5:1 이상을 맞춘 색.
const ACCENT: Color32 = Color32::from_rgb(0x25, 0x5f, 0xd6);
const ACCENT_SOFT: Color32 = Color32::from_rgb(0xe8, 0xf0, 0xfe);
const INK: Color32 = Color32::from_gray(34);
const MUTED: Color32 = Color32::from_gray(92);
const LINE: Color32 = Color32::from_gray(190);
const OK: Color32 = Color32::from_rgb(0x15, 0x6b, 0x35);
const WARN: Color32 = Color32::from_rgb(0x85, 0x4d, 0x00);
const ERR: Color32 = Color32::from_rgb(0xb4, 0x23, 0x18);

fn main() -> eframe::Result {
    // exe 아이콘에 파일을 끌어다 놓거나 「연결 프로그램」으로 열면 경로가 인자로 온다.
    let input = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("특허 명세서 변환 — DOCX → HLT · HWPX")
            .with_inner_size([460.0, 560.0])
            .with_min_inner_size([400.0, 420.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "docx2hwpx-hlt",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            let mut app = App::default();
            if let Some(path) = input {
                app.pick(path);
            }
            Ok(Box::new(app))
        }),
    )
}

enum Msg {
    Progress(f32, String),
    Finished(Result<Vec<u8>, Failure>, Value),
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Working {
        rx: Receiver<Msg>,
        target: f32,
        stage: String,
    },
    /// 변환은 끝났다. `saved` 가 없으면 저장 창을 닫은 것 — 큰 단추가 「저장…」이 된다.
    Done {
        bytes: Vec<u8>,
        report: Value,
        saved: Option<PathBuf>,
        save_error: Option<String>,
    },
    Failed(Failure),
}

struct App {
    input: Option<PathBuf>,
    hlt: bool,
    state: State,
    /// 게이지가 그리는 값 — 목표값을 부드럽게 따라간다.
    shown: f32,
    notice: Option<String>,
    /// 변환이 끝나 게이지가 다 차면 저장 창을 연다.
    save_pending: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: None,
            hlt: true,
            state: State::Idle,
            shown: 0.0,
            notice: None,
            save_pending: false,
        }
    }
}

/// 큰 단추가 지금 할 일.
#[derive(PartialEq)]
enum Primary {
    Convert,
    Busy,
    Save,
}

impl App {
    fn pick(&mut self, path: PathBuf) {
        if matches!(self.state, State::Working { .. }) {
            self.notice =
                Some("변환 중에는 파일을 바꿀 수 없습니다. 끝난 뒤 다시 놓아 주세요.".into());
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            self.input = Some(path);
            self.notice = None;
            self.state = State::Idle;
            self.shown = 0.0;
        } else {
            self.notice = Some(format!(
                "「{}」은(는) DOCX 가 아닙니다. Word 문서(.docx)를 놓아 주세요.",
                file_name(&path)
            ));
        }
    }

    fn browse(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("DOCX 고르기")
            .add_filter("Word 문서", &["docx"])
            .pick_file()
        {
            self.pick(path);
        }
    }

    fn target(&self) -> Target {
        if self.hlt {
            Target::Hlt { pct: false }
        } else {
            Target::Hwpx
        }
    }

    fn primary(&self) -> Primary {
        match self.state {
            State::Working { .. } => Primary::Busy,
            State::Done { .. } => Primary::Save,
            _ => Primary::Convert,
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let target = self.target();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let docx = match std::fs::read(&input) {
                Ok(b) => b,
                Err(e) => {
                    let failure = docx2hwpx_hlt::fail(
                        "input_unreadable",
                        format!("{}: {e}", input.display()),
                    );
                    let _ = tx.send(Msg::Finished(Err(failure), Value::Null));
                    ctx.request_repaint();
                    return;
                }
            };
            let progress_tx = tx.clone();
            let progress_ctx = ctx.clone();
            let mut progress = |fraction: f32, stage: &str| {
                let _ = progress_tx.send(Msg::Progress(fraction, stage.to_string()));
                progress_ctx.request_repaint();
            };
            let outcome = docx2hwpx_hlt::convert(&docx, target, &[], &mut progress);
            let _ = tx.send(Msg::Finished(outcome.result, outcome.report));
            ctx.request_repaint();
        });
        self.shown = 0.0;
        self.save_pending = false;
        self.notice = None;
        self.state = State::Working {
            rx,
            target: 0.0,
            stage: "준비하는 중".into(),
        };
    }

    /// 다른 이름으로 저장 — 이름은 입력과 같고 확장자만 바뀐다. 기본 폴더는 입력 파일 폴더.
    fn save(&mut self) {
        let ext = self.target().extension();
        let State::Done {
            bytes,
            saved,
            save_error,
            ..
        } = &mut self.state
        else {
            return;
        };
        let Some(input) = &self.input else { return };
        let name = format!(
            "{}.{ext}",
            input.file_stem().unwrap_or_default().to_string_lossy()
        );
        let mut dialog = rfd::FileDialog::new()
            .set_title("다른 이름으로 저장")
            .set_file_name(&name)
            .add_filter(ext.to_uppercase(), &[ext]);
        if let Some(dir) = input.parent().filter(|d| !d.as_os_str().is_empty()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.save_file() {
            match docx2hwpx_hlt::write_atomic(&path, bytes) {
                Ok(()) => {
                    *saved = Some(path);
                    *save_error = None;
                }
                Err(f) => {
                    *save_error = Some(format!(
                        "저장하지 못했습니다. 폴더 권한이나 남은 공간을 확인한 뒤 다시 저장해 주세요. ({})",
                        f.message
                    ))
                }
            }
        }
    }

    fn poll(&mut self) {
        let State::Working { rx, target, stage } = &mut self.state else {
            return;
        };
        let mut finished = None;
        loop {
            match rx.try_recv() {
                Ok(Msg::Progress(f, s)) => {
                    *target = f;
                    *stage = s;
                }
                Ok(Msg::Finished(result, report)) => finished = Some((result, report)),
                Err(TryRecvError::Empty) => break,
                // 변환 스레드가 결과 없이 끝났다(예기치 않은 오류) — 「변환 중」에 멈춰 있지 않게.
                Err(TryRecvError::Disconnected) => {
                    if finished.is_none() {
                        let failure = docx2hwpx_hlt::fail(
                            "convert_failed",
                            "변환 중 예기치 않은 오류로 멈췄습니다",
                        );
                        finished = Some((Err(failure), Value::Null));
                    }
                    break;
                }
            }
        }
        if let Some((result, report)) = finished {
            self.state = match result {
                Ok(bytes) => State::Done {
                    bytes,
                    report,
                    saved: None,
                    save_error: None,
                },
                Err(f) => State::Failed(f),
            };
            // 게이지가 끝까지 차오른 뒤 저장 창을 연다(ui 에서).
            self.save_pending = matches!(self.state, State::Done { .. });
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let open = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
        let save = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        let (open, save) =
            ctx.input_mut(|i| (i.consume_shortcut(&open), i.consume_shortcut(&save)));
        // Enter = 큰 단추. 단추에 초점이 있으면 그 단추가 Enter 를 받으니 겹치지 않게.
        let enter = ctx.memory(|m| m.focused().is_none())
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if open && self.primary() != Primary::Busy {
            self.browse();
        }
        if save && self.primary() == Primary::Save {
            self.save();
        }
        if enter {
            self.press_primary(ctx);
        }
    }

    fn press_primary(&mut self, ctx: &egui::Context) {
        match self.primary() {
            Primary::Convert if self.input.is_some() => self.start(ctx),
            Primary::Save => self.save(),
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        if let Some(path) = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .find(|p| !p.as_os_str().is_empty())
        }) {
            self.pick(path);
        }
        self.shortcuts(&ctx);
        let working = matches!(self.state, State::Working { .. });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(6.0);
            ui.label(
                RichText::new("특허 명세서 변환")
                    .size(20.0)
                    .strong()
                    .color(INK),
            );
            ui.add_space(4.0);

            // 1. 파일 — 누르기 · Enter/Space · 끌어다 놓기
            let hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
            let sense = if working {
                Sense::hover()
            } else {
                Sense::click()
            };
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 112.0), sense);
            let (title, sub) = match &self.input {
                Some(p) => (
                    file_name(p),
                    "다른 파일은 끌어다 놓거나 눌러서 바꿉니다 (Ctrl+O)".to_string(),
                ),
                None => (
                    "DOCX 파일을 여기에 끌어다 놓으세요".to_string(),
                    "또는 눌러서 고르기 (Ctrl+O)".to_string(),
                ),
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, !working, format!("{title}. {sub}"))
            });
            let lit = !working && (hovering_file || response.hovered() || response.has_focus());
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                CornerRadius::same(10),
                if lit {
                    ACCENT_SOFT
                } else {
                    Color32::from_gray(250)
                },
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(10),
                Stroke::new(if lit { 2.0 } else { 1.0 }, if lit { ACCENT } else { LINE }),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center() - egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(16.0),
                INK,
            );
            painter.text(
                rect.center() + egui::vec2(0.0, 15.0),
                egui::Align2::CENTER_CENTER,
                sub,
                egui::FontId::proportional(13.0),
                MUTED,
            );
            if response.clicked() {
                self.browse();
            }
            if let Some(n) = &self.notice {
                ui.colored_label(ERR, n);
            }

            // 2. 형식
            ui.horizontal(|ui| {
                ui.label(RichText::new("만들 형식").color(INK));
                ui.add_enabled_ui(!working, |ui| {
                    let before = self.hlt;
                    ui.selectable_value(&mut self.hlt, true, "HLT · NK-Editor");
                    ui.selectable_value(&mut self.hlt, false, "HWPX · MOIP-Editor");
                    if before != self.hlt
                        && matches!(self.state, State::Done { .. } | State::Failed(_))
                    {
                        // 형식을 바꾸면 이전 결과는 맞지 않는다 — 다시 변환한다.
                        self.state = State::Idle;
                        self.shown = 0.0;
                    }
                });
            });
            ui.add_space(2.0);

            // 3. 큰 단추 — 흐름의 다음 할 일(변환 → 저장)
            let primary = self.primary();
            let (label, enabled) = match primary {
                Primary::Convert => ("변환", self.input.is_some()),
                Primary::Busy => ("변환 중…", false),
                Primary::Save => ("저장…  (Ctrl+S)", true),
            };
            let button = egui::Button::new(RichText::new(label).size(16.0).color(Color32::WHITE))
                .fill(ACCENT)
                .min_size(egui::vec2(0.0, 38.0));
            if ui
                .vertical_centered_justified(|ui| ui.add_enabled(enabled, button))
                .inner
                .clicked()
            {
                self.press_primary(&ctx);
            }

            // 게이지 — 숫자는 막대 위가 아니라 아래 글에(막대 위 글은 대비가 깨진다)
            let (goal, stage) = match &self.state {
                State::Working { target, stage, .. } => (*target, stage.clone()),
                State::Done { .. } => (1.0, "변환 끝".into()),
                _ => (0.0, String::new()),
            };
            if working || matches!(self.state, State::Done { .. }) {
                // 화면 갱신 빈도와 무관하게 0.25초쯤에 따라붙는다.
                let dt = ctx.input(|i| i.stable_dt).min(0.1);
                self.shown += (goal - self.shown) * (1.0 - (-dt * 12.0).exp());
                if (goal - self.shown).abs() < 0.002 {
                    self.shown = goal;
                }
                ui.add(
                    egui::ProgressBar::new(self.shown)
                        .desired_height(10.0)
                        .fill(ACCENT)
                        .corner_radius(5.0)
                        .animate(working),
                );
                ui.label(
                    RichText::new(format!("{stage} · {:.0}%", self.shown * 100.0)).color(MUTED),
                );
                if working || self.shown < goal {
                    ctx.request_repaint();
                }
            }

            // 4. 결과
            let mut save_now = self.save_pending && self.shown >= 1.0;
            if save_now {
                self.save_pending = false;
            }
            let mut reveal_path = None;
            match &self.state {
                State::Done {
                    report,
                    saved,
                    save_error,
                    ..
                } => {
                    match saved {
                        Some(p) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(OK, format!("저장했습니다: {}", file_name(p)));
                                if ui.link("폴더 열기").clicked() {
                                    reveal_path = Some(p.clone());
                                }
                            });
                        }
                        None => {
                            ui.label(
                                RichText::new(
                                    "변환이 끝났습니다. 「저장…」을 눌러 파일로 남기세요.",
                                )
                                .color(INK),
                            );
                        }
                    }
                    if let Some(e) = save_error {
                        ui.colored_label(ERR, e);
                    }
                    ui.separator();
                    summary(ui, report);
                }
                State::Failed(f) => {
                    let (what, how) = explain(f);
                    ui.colored_label(ERR, RichText::new(what).strong());
                    ui.label(RichText::new(how).color(INK));
                    ui.label(RichText::new(&f.message).size(12.0).color(MUTED));
                    save_now = false;
                }
                _ => {}
            }
            if save_now {
                self.save();
            }
            if let Some(p) = reveal_path {
                reveal(&p);
            }
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.label(
                    RichText::new(format!(
                        "만든 파일은 NK-Editor · MOIP-Editor 에서 열어 확인한 뒤 제출하세요.   v{}",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
            });
        });
    }
}

/// 실패 코드 → (무엇이 잘못됐나, 어떻게 하나). 원래 메시지는 그 아래 작게.
fn explain(f: &Failure) -> (&'static str, &'static str) {
    match f.code {
        "input_unreadable" => (
            "파일을 읽지 못했습니다",
            "파일이 옮겨졌거나 다른 프로그램(Word 등)이 열고 있는지 확인한 뒤 다시 변환해 주세요.",
        ),
        "invalid_docx" => (
            "DOCX 파일이 아니거나 손상되었습니다",
            "Word 에서 열어 「다른 이름으로 저장 → Word 문서(.docx)」로 다시 저장한 뒤 변환해 주세요.",
        ),
        _ => (
            "변환하지 못했습니다",
            "문서 구조(표 · 그림 배치)에서 막혔습니다. 아래 내용을 확인하거나, 다른 형식으로 바꿔 시도해 보세요.",
        ),
    }
}

/// 변환 보고 — 무엇을 바꿨는지, 사람이 편집기에서 확인할 곳은 어디인지.
fn summary(ui: &mut egui::Ui, report: &Value) {
    let n = |k: &str| report[k].as_u64().unwrap_or(0);
    let list = |k: &str| report[k].as_array().cloned().unwrap_or_default();
    let mut done = vec![
        format!(
            "변경 추적 {}곳 받아들임 · 메모 {}개 지움",
            n("revisions_accepted"),
            n("comments_removed")
        ),
        format!(
            "수식 그림 {}개 · WMF/EMF 그림 {}개",
            n("equations"),
            n("metafiles")
        ),
    ];
    if n("scripts") > 0 {
        done.push(format!("첨자 글자를 첨자 서식으로 {}곳", n("scripts")));
    }
    for s in list("substitutions") {
        done.push(format!(
            "「{}」 → 「{}」 {}곳",
            s["from"].as_str().unwrap_or(""),
            s["to"].as_str().unwrap_or(""),
            s["count"]
        ));
    }
    // 확인할 곳: 경고 + 글자 그림(편집기에서 글로 고칠 수 없다)
    let mut check: Vec<String> = list("warnings")
        .iter()
        .map(|w| w["message"].as_str().unwrap_or("").to_string())
        .collect();
    for g in list("glyph_images") {
        check.push(format!(
            "글자 그림으로 넣음: {} ({}) {}곳",
            g["text"].as_str().unwrap_or(""),
            g["codepoints"].as_str().unwrap_or(""),
            g["count"]
        ));
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, true]) // 창 폭에 맞춰 긴 줄을 접는다(스크롤바 밑으로 숨지 않게)
        .max_height((ui.available_height() - 30.0).max(60.0))
        .show(ui, |ui| {
            ui.label(RichText::new("바꾼 것").strong().color(INK));
            for l in &done {
                ui.label(RichText::new(l).size(13.0).color(INK));
            }
            if !check.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("편집기에서 확인할 곳 {}", check.len()))
                        .strong()
                        .color(WARN),
                );
                for l in &check {
                    ui.label(RichText::new(l).size(13.0).color(WARN));
                }
            }
        });
}

/// 저장한 파일을 파일 탐색기에서 보여 준다.
fn reveal(path: &Path) {
    #[cfg(windows)]
    let result = std::process::Command::new("explorer")
        .arg("/select,")
        .arg(path)
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(Path::new(".")))
        .spawn();
    let _ = result;
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
