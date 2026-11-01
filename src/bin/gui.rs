//! 화면 앱: DOCX 고르기(끌어다 놓기 · 누르기 · exe 에 끌어다 놓기) → HLT/HWPX 고르기 → 변환(게이지) → 다른 이름으로 저장.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use docx2hwpx_hlt::Target;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, RichText, Sense, Stroke, StrokeKind,
};
use serde_json::Value;

const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xeb);

fn main() -> eframe::Result {
    // exe 아이콘에 파일을 끌어다 놓거나 「연결 프로그램」으로 열면 경로가 인자로 온다.
    let input = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DOCX → HLT · HWPX")
            .with_inner_size([460.0, 540.0])
            .with_min_inner_size([400.0, 380.0])
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
    Finished(Result<Vec<u8>, String>, Value),
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
    /// 변환은 끝났다. `saved` 가 없으면 저장 창을 닫은 것 — 「저장」 단추로 다시 연다.
    Done {
        bytes: Vec<u8>,
        report: Value,
        saved: Option<PathBuf>,
        save_error: Option<String>,
    },
    Failed(String),
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

impl App {
    fn pick(&mut self, path: PathBuf) {
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            self.input = Some(path);
            self.notice = None;
            self.state = State::Idle;
        } else {
            self.notice = Some(format!("DOCX 파일만 받습니다: {}", file_name(&path)));
        }
    }

    fn target(&self) -> Target {
        if self.hlt {
            Target::Hlt { pct: false }
        } else {
            Target::Hwpx
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
                    let _ = tx.send(Msg::Finished(
                        Err(format!("{}: {e}", input.display())),
                        Value::Null,
                    ));
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
            let result = outcome.result.map_err(|f| f.message);
            let _ = tx.send(Msg::Finished(result, outcome.report));
            ctx.request_repaint();
        });
        self.shown = 0.0;
        self.save_pending = false;
        self.state = State::Working {
            rx,
            target: 0.0,
            stage: "시작".into(),
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
                Err(f) => *save_error = Some(f.message),
            }
        }
    }

    fn poll(&mut self) {
        let State::Working { rx, target, stage } = &mut self.state else {
            return;
        };
        let mut finished = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Progress(f, s) => {
                    *target = f;
                    *stage = s;
                }
                Msg::Finished(result, report) => finished = Some((result, report)),
            }
        }
        // 게이지가 끝까지 차오른 걸 보여 준 뒤 저장 창을 연다.
        if let Some((result, report)) = finished {
            self.state = match result {
                Ok(bytes) => State::Done {
                    bytes,
                    report,
                    saved: None,
                    save_error: None,
                },
                Err(e) => State::Failed(e),
            };
            // 게이지가 끝까지 차오른 뒤 저장 창을 연다(ui 에서).
            self.save_pending = matches!(self.state, State::Done { .. });
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
        }) && !matches!(self.state, State::Working { .. })
        {
            self.pick(path);
        }
        let working = matches!(self.state, State::Working { .. });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.add_space(4.0);
            ui.label(RichText::new("특허 명세서 변환").size(20.0).strong());

            // 1. 파일
            let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 110.0), Sense::click());
            let lit = hovering || response.hovered();
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                CornerRadius::same(10),
                if lit {
                    Color32::from_rgb(0xe8, 0xf0, 0xfe)
                } else {
                    Color32::from_gray(248)
                },
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(10),
                Stroke::new(
                    if lit { 2.0 } else { 1.0 },
                    if lit { ACCENT } else { Color32::from_gray(190) },
                ),
                StrokeKind::Inside,
            );
            let (title, sub) = match &self.input {
                Some(p) => (
                    file_name(p),
                    "다른 파일은 끌어다 놓거나 눌러서 바꿉니다".to_string(),
                ),
                None => (
                    "DOCX 파일을 여기에 끌어다 놓으세요".to_string(),
                    "또는 눌러서 고르기".to_string(),
                ),
            };
            painter.text(
                rect.center() - egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(16.0),
                Color32::from_gray(40),
            );
            painter.text(
                rect.center() + egui::vec2(0.0, 16.0),
                egui::Align2::CENTER_CENTER,
                sub,
                egui::FontId::proportional(12.5),
                Color32::from_gray(120),
            );
            if response.clicked()
                && !working
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Word 문서", &["docx"])
                    .pick_file()
            {
                self.pick(path);
            }
            if let Some(n) = &self.notice {
                ui.colored_label(Color32::from_rgb(0xc0, 0x30, 0x30), n);
            }

            // 2. 형식
            ui.horizontal(|ui| {
                ui.label("만들 형식");
                ui.add_enabled_ui(!working, |ui| {
                    let before = self.hlt;
                    ui.selectable_value(&mut self.hlt, true, "HLT  (NK-Editor)");
                    ui.selectable_value(&mut self.hlt, false, "HWPX  (MOIP-Editor)");
                    if before != self.hlt
                        && matches!(self.state, State::Done { .. } | State::Failed(_))
                    {
                        self.state = State::Idle; // 형식을 바꾸면 이전 결과는 맞지 않는다
                    }
                });
            });

            // 3. 변환
            let can_start = self.input.is_some() && !working;
            let button = egui::Button::new(RichText::new("변환").size(16.0).color(Color32::WHITE))
                .fill(ACCENT)
                .min_size(egui::vec2(0.0, 36.0));
            // 가로로 꽉 채우고 글은 가운데
            if ui
                .vertical_centered_justified(|ui| ui.add_enabled(can_start, button))
                .inner
                .clicked()
            {
                self.start(&ctx);
            }

            // 게이지
            let (goal, stage) = match &self.state {
                State::Working { target, stage, .. } => (*target, stage.clone()),
                State::Done { .. } => (1.0, "변환 끝".into()),
                _ => (0.0, String::new()),
            };
            if working || matches!(self.state, State::Done { .. }) {
                self.shown += (goal - self.shown) * 0.12;
                if (goal - self.shown).abs() < 0.002 {
                    self.shown = goal;
                }
                ui.add(
                    egui::ProgressBar::new(self.shown)
                        .desired_height(14.0)
                        .fill(ACCENT)
                        .corner_radius(7.0)
                        .animate(working)
                        .show_percentage(),
                );
                ui.label(RichText::new(stage).color(Color32::from_gray(110)));
                if working || self.shown < goal {
                    ctx.request_repaint();
                }
            }

            // 4. 결과
            let mut save_again = self.save_pending && self.shown >= 1.0;
            if save_again {
                self.save_pending = false;
            }
            match &self.state {
                State::Done {
                    report,
                    saved,
                    save_error,
                    ..
                } => {
                    match saved {
                        Some(p) => {
                            ui.colored_label(
                                Color32::from_rgb(0x1e, 0x8e, 0x3e),
                                format!("저장했습니다: {}", p.display()),
                            );
                        }
                        None => {
                            ui.horizontal(|ui| {
                                ui.label("아직 저장하지 않았습니다.");
                                save_again = ui.button("저장…").clicked();
                            });
                        }
                    }
                    if let Some(e) = save_error {
                        ui.colored_label(
                            Color32::from_rgb(0xc0, 0x30, 0x30),
                            format!("저장 실패: {e}"),
                        );
                    }
                    summary(ui, report);
                }
                State::Failed(e) => {
                    ui.colored_label(
                        Color32::from_rgb(0xc0, 0x30, 0x30),
                        format!("변환 실패: {e}"),
                    );
                }
                _ => {}
            }
            if save_again {
                self.save();
            }
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.label(
                    RichText::new(
                        "만든 파일은 NK-Editor · MOIP-Editor 에서 열어 확인한 뒤 제출하세요.",
                    )
                    .size(11.5)
                    .color(Color32::from_gray(130)),
                );
            });
        });
    }
}

/// 변환 보고 — 사람이 편집기에서 확인할 곳.
fn summary(ui: &mut egui::Ui, report: &Value) {
    let n = |k: &str| report[k].as_u64().unwrap_or(0);
    let list = |k: &str| report[k].as_array().cloned().unwrap_or_default();
    let mut lines = vec![format!(
        "변경 추적 {}곳 받아들임 · 메모 {}개 지움",
        n("revisions_accepted"),
        n("comments_removed")
    )];
    lines.push(format!(
        "수식 그림 {}개 · WMF/EMF 그림 {}개",
        n("equations"),
        n("metafiles")
    ));
    if n("scripts") > 0 {
        lines.push(format!("첨자 글자 → 첨자 서식 {}곳", n("scripts")));
    }
    for s in list("substitutions") {
        lines.push(format!(
            "바꾼 글자: {} → {} ({}곳)",
            s["from"].as_str().unwrap_or(""),
            s["to"].as_str().unwrap_or(""),
            s["count"]
        ));
    }
    for g in list("glyph_images") {
        lines.push(format!(
            "글자 그림: {} ({}) {}곳",
            g["text"].as_str().unwrap_or(""),
            g["codepoints"].as_str().unwrap_or(""),
            g["count"]
        ));
    }
    let warnings = list("warnings");
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - 28.0).max(48.0))
        .show(ui, |ui| {
            for l in &lines {
                ui.label(RichText::new(l).size(12.5));
            }
            for w in &warnings {
                ui.label(
                    RichText::new(format!("⚠ {}", w["message"].as_str().unwrap_or("")))
                        .size(12.5)
                        .color(Color32::from_rgb(0xb0, 0x6a, 0x00)),
                );
            }
        });
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
