//! 免费macos解压缩软件 — 解压（ZIP、7z、RAR、TAR…）与压缩（ZIP、tar.gz）。

mod compress;
mod extract;
mod theme;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Vec2};
use compress::{CompressFormat, compress};
use extract::{ArchiveKind, extract};

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 700.0])
            .with_min_inner_size([500.0, 520.0])
            .with_title("免费macos解压缩软件"),
        ..Default::default()
    };
    eframe::run_native(
        "免费macos解压缩软件",
        native_options,
        Box::new(|cc| Ok(Box::new(ArchiverApp::new(cc)))),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum AppMode {
    #[default]
    Extract,
    Compress,
}

enum WorkerMsg {
    Done(Result<(), String>),
}

struct ArchiverApp {
    mode: AppMode,
    archive_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    compress_sources: Vec<PathBuf>,
    compress_output: Option<PathBuf>,
    log: String,
    busy: bool,
    worker_rx: Option<Receiver<WorkerMsg>>,
    _worker_tx: Option<Sender<WorkerMsg>>,
}

impl ArchiverApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::install_visuals(&cc.egui_ctx);
        Self {
            mode: AppMode::Extract,
            archive_path: None,
            output_dir: None,
            compress_sources: Vec::new(),
            compress_output: None,
            log: String::new(),
            busy: false,
            worker_rx: None,
            _worker_tx: None,
        }
    }

    fn append_log(&mut self, line: impl AsRef<str>) {
        self.log.push_str(line.as_ref());
        self.log.push('\n');
    }

    fn pick_archive(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "压缩包",
                &[
                    "zip", "7z", "rar", "cbr", "tar", "tgz", "gz", "bz2", "xz", "zst", "tbz",
                    "tbz2", "txz", "tzst",
                ],
            )
            .pick_file()
        {
            self.archive_path = Some(path);
            self.log.clear();
        }
    }

    fn pick_output(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.output_dir = Some(dir);
        }
    }

    fn default_output_for(archive: &Path) -> PathBuf {
        let stem = archive
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted");
        let parent = archive.parent().unwrap_or(Path::new("."));
        parent.join(format!("{stem}_extracted"))
    }

    /// 压缩列表里是否已有同一路径（含 canonical 后相同）。
    fn compress_sources_has(&self, p: &Path) -> bool {
        self.compress_sources
            .iter()
            .any(|q| Self::compress_path_same(q, p))
    }

    fn compress_path_same(a: &Path, b: &Path) -> bool {
        if a == b {
            return true;
        }
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }

    fn add_compress_source(&mut self, p: PathBuf) {
        if self.compress_sources_has(&p) {
            return;
        }
        self.compress_sources.push(p);
    }

    fn pick_compress_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_file() {
            self.add_compress_source(p);
        }
    }

    fn pick_compress_folder(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.add_compress_source(p);
        }
    }

    fn pick_compress_save(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("ZIP", &["zip"])
            .add_filter("tar.gz", &["tar.gz", "tgz"])
            .set_file_name("archive.zip")
            .save_file()
        {
            self.compress_output = Some(p);
        }
    }

    fn start_extract(&mut self) {
        let Some(archive) = self.archive_path.clone() else {
            self.append_log("请先选择压缩包。");
            return;
        };
        let Some(kind) = ArchiveKind::from_path(&archive) else {
            self.append_log(format!(
                "不支持的扩展名: {}。支持: .zip .7z .rar/.cbr .tar .tar.gz/.tgz .tar.bz2/.tbz2 .tar.xz/.txz .tar.zst/.tzst",
                archive.display()
            ));
            return;
        };
        let out = self
            .output_dir
            .clone()
            .unwrap_or_else(|| Self::default_output_for(&archive));

        self.busy = true;
        self.log.clear();
        self.append_log(format!("解压 {} → {}", kind.label(), out.display()));

        let (tx, rx) = mpsc::channel();
        self.worker_rx = Some(rx);
        self._worker_tx = Some(tx.clone());

        std::thread::spawn(move || {
            let res = extract(&archive, &out, kind).map_err(|e| e.to_string());
            let _ = tx.send(WorkerMsg::Done(res));
        });
    }

    fn start_compress(&mut self) {
        if self.compress_sources.is_empty() {
            self.append_log("请添加至少一个文件或文件夹。");
            return;
        }
        let Some(out) = self.compress_output.clone() else {
            self.append_log("请选择生成的压缩包路径（.zip 或 .tar.gz）。");
            return;
        };
        let Some(fmt) = CompressFormat::from_output_path(&out) else {
            self.append_log("输出文件扩展名需为 .zip、.tar.gz 或 .tgz。");
            return;
        };

        let sources = self.compress_sources.clone();
        self.busy = true;
        self.log.clear();
        self.append_log(format!(
            "压缩 {} → {}",
            fmt.label(),
            out.display()
        ));

        let (tx, rx) = mpsc::channel();
        self.worker_rx = Some(rx);
        self._worker_tx = Some(tx.clone());

        std::thread::spawn(move || {
            let res = compress(&sources, &out, fmt).map_err(|e| e.to_string());
            let _ = tx.send(WorkerMsg::Done(res));
        });
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.worker_rx else {
            return;
        };
        if let Ok(WorkerMsg::Done(res)) = rx.try_recv() {
            self.busy = false;
            self.worker_rx = None;
            self._worker_tx = None;
            match res {
                Ok(()) => self.append_log("完成。"),
                Err(e) => self.append_log(format!("错误: {e}")),
            }
            ctx.request_repaint();
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    match self.mode {
                        AppMode::Extract => {
                            if path.is_file() {
                                self.archive_path = Some(path.clone());
                                self.log.clear();
                            }
                        }
                        AppMode::Compress => {
                            if path.is_file() || path.is_dir() {
                                self.add_compress_source(path.clone());
                            }
                        }
                    }
                }
            }
        });
    }

    fn path_lines_extract(&self) -> (String, String) {
        let file_line = self
            .archive_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未选择".to_string());
        let out_line = self
            .output_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                self.archive_path
                    .as_ref()
                    .map(|a| Self::default_output_for(a).display().to_string())
                    .unwrap_or_else(|| "默认：同目录 · 子文件夹「原名_extracted」".to_string())
            });
        (file_line, out_line)
    }

    fn render_mode_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let extract = self.mode == AppMode::Extract;
            let w = (ui.available_width() - 4.0) / 2.0;
            if ui
                .add_sized(
                    [w, 36.0],
                    egui::SelectableLabel::new(extract, RichText::new("解压").size(15.0).strong()),
                )
                .clicked()
            {
                self.mode = AppMode::Extract;
            }
            if ui
                .add_sized(
                    [w, 36.0],
                    egui::SelectableLabel::new(!extract, RichText::new("压缩").size(15.0).strong()),
                )
                .clicked()
            {
                self.mode = AppMode::Compress;
            }
        });
    }

    fn render_extract_panel(&mut self, ui: &mut egui::Ui, drag_from_os: bool) {
        let w = ui.available_width();
        let h = 88.0;
        let response = ui.allocate_response(Vec2::new(w, h), Sense::click());
        let rect = response.rect;
        let hover = response.hovered() || drag_from_os;
        ui.painter().rect(
            rect,
            12.0,
            theme::drop_zone_fill(hover),
            theme::drop_zone_stroke(hover),
        );
        if response.clicked() {
            self.pick_archive();
        }
        let c = rect.center();
        ui.painter().text(
            c - Vec2::new(0.0, 6.0),
            Align2::CENTER_CENTER,
            "拖放压缩包",
            FontId::proportional(15.0),
            theme::INK,
        );
        ui.painter().text(
            c + Vec2::new(0.0, 12.0),
            Align2::CENTER_CENTER,
            "点击选择",
            FontId::proportional(12.5),
            theme::text_muted(),
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let gap = 10.0;
            let each = (ui.available_width() - gap) / 2.0;
            if ui
                .add_sized([each, 40.0], theme::secondary_file_button("选择文件"))
                .clicked()
            {
                self.pick_archive();
            }
            if ui
                .add_sized([each, 40.0], theme::secondary_folder_button("输出目录"))
                .clicked()
            {
                self.pick_output();
            }
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        let (f_line, o_line) = self.path_lines_extract();
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ui.label(RichText::new("压缩包").size(11.0).color(theme::text_muted()));
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&f_line)
                            .size(12.5)
                            .family(egui::FontFamily::Monospace)
                            .color(theme::INK),
                    )
                    .wrap(),
                );
            });
            cols[1].vertical(|ui| {
                ui.label(RichText::new("解压到").size(11.0).color(theme::text_muted()));
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&o_line)
                            .size(12.5)
                            .family(egui::FontFamily::Monospace)
                            .color(theme::INK),
                    )
                    .wrap(),
                );
            });
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        let can_run = self.archive_path.is_some() && !self.busy;
        if ui
            .add_enabled_ui(can_run, |ui| {
                ui.add_sized(
                    [ui.available_width(), 46.0],
                    theme::primary_action_button("解压"),
                )
            })
            .inner
            .clicked()
        {
            self.start_extract();
        }
    }

    fn render_compress_panel(&mut self, ui: &mut egui::Ui, drag_from_os: bool) {
        let w = ui.available_width();
        let h = 72.0;
        let response = ui.allocate_response(Vec2::new(w, h), Sense::click());
        let rect = response.rect;
        let hover = response.hovered() || drag_from_os;
        ui.painter().rect(
            rect,
            12.0,
            theme::drop_zone_fill(hover),
            theme::drop_zone_stroke(hover),
        );
        if response.clicked() {
            self.pick_compress_file();
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "拖入文件或文件夹 · 点击添加文件",
            FontId::proportional(14.0),
            theme::INK,
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let gap = 10.0;
            let each = (ui.available_width() - gap) / 2.0;
            if ui
                .add_sized([each, 40.0], theme::secondary_file_button("添加文件"))
                .clicked()
            {
                self.pick_compress_file();
            }
            if ui
                .add_sized([each, 40.0], theme::secondary_folder_button("添加文件夹"))
                .clicked()
            {
                self.pick_compress_folder();
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui
                .small_button(RichText::new("清空列表").size(12.0).color(theme::text_muted()))
                .clicked()
            {
                self.compress_sources.clear();
            }
        });

        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .max_height(100.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if self.compress_sources.is_empty() {
                    ui.label(
                        RichText::new("列表为空")
                            .size(12.5)
                            .color(theme::text_muted()),
                    );
                } else {
                    let mut remove_idx: Option<usize> = None;
                    for (i, p) in self.compress_sources.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 20.0;
                            let avail = ui.available_width();
                            let row_h = 36.0;
                            let btn_w = 72.0;
                            let gap = 20.0;
                            let label_w = f32::max(avail - btn_w - gap, 48.0);
                            ui.add_sized(
                                [label_w, row_h],
                                egui::Label::new(
                                    RichText::new(p.display().to_string())
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(theme::INK),
                                )
                                .wrap(),
                            );
                            // 自绘：标准 Button 用 visuals.text_color 画字，RichText 白色不生效。
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(btn_w, row_h),
                                Sense::click(),
                            );
                            let fill = if response.hovered() || response.highlighted() {
                                Color32::from_rgb(239, 68, 68)
                            } else {
                                Color32::from_rgb(220, 38, 38)
                            };
                            let stroke = egui::Stroke::new(1.0, Color32::from_rgb(153, 27, 27));
                            ui.painter().rect(rect, 6.0, fill, stroke);
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "移除",
                                FontId::proportional(13.0),
                                Color32::WHITE,
                            );
                            if response.clicked() {
                                remove_idx = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove_idx {
                        self.compress_sources.remove(i);
                    }
                }
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new(RichText::new("选择保存位置…").size(14.0).color(theme::INK))
                        .fill(egui::Color32::from_rgb(252, 252, 253))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(220, 222, 228),
                        )),
                )
                .clicked()
            {
                self.pick_compress_save();
            }
        });

        ui.add_space(4.0);
        let out_label = self
            .compress_output
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "未选择 · 扩展名需为 .zip 或 .tar.gz".to_string());
        ui.label(RichText::new("输出").size(11.0).color(theme::text_muted()));
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(&out_label)
                    .size(12.5)
                    .family(egui::FontFamily::Monospace)
                    .color(theme::INK),
            )
            .wrap(),
        );

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        let can_run = !self.compress_sources.is_empty() && self.compress_output.is_some() && !self.busy;
        if ui
            .add_enabled_ui(can_run, |ui| {
                ui.add_sized(
                    [ui.available_width(), 46.0],
                    theme::primary_action_button("压缩"),
                )
            })
            .inner
            .clicked()
        {
            self.start_compress();
        }
    }

    fn render_log_and_formats(&mut self, ui: &mut egui::Ui) {
        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("处理中…")
                        .size(13.0)
                        .color(theme::text_muted()),
                );
            });
            ui.add_space(4.0);
        }

        egui::CollapsingHeader::new(RichText::new("日志").size(13.0).color(theme::text_muted()))
            .id_salt("archiver_log")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let body = if self.log.is_empty() {
                            "暂无记录".to_string()
                        } else {
                            self.log.clone()
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(body)
                                    .family(egui::FontFamily::Monospace)
                                    .size(12.0)
                                    .line_height(Some(17.0))
                                    .color(theme::INK),
                            )
                            .wrap(),
                        );
                    });
            });

        egui::CollapsingHeader::new(
            RichText::new("支持格式").size(13.0).color(theme::text_muted()),
        )
        .id_salt("archiver_formats")
        .default_open(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
            ui.label(
                RichText::new("解压：ZIP、7z、RAR / CBR、TAR 及 .tar.gz / .bz2 / .xz / .zst")
                    .size(12.5)
                    .color(theme::INK),
            );
            ui.label(
                RichText::new("压缩：ZIP（deflate）、tar.gz（gzip）")
                    .size(12.5)
                    .color(theme::INK),
            );
        });
    }
}

impl eframe::App for ArchiverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_drops(ctx);
        self.poll_worker(ctx);

        let drag_from_os = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(28.0, 24.0)),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    theme::main_panel_frame().show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 14.0);
                        self.render_mode_tabs(ui);
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(10.0);

                        match self.mode {
                            AppMode::Extract => self.render_extract_panel(ui, drag_from_os),
                            AppMode::Compress => self.render_compress_panel(ui, drag_from_os),
                        }

                        ui.add_space(4.0);
                        self.render_log_and_formats(ui);
                    });
                });
            });

        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
