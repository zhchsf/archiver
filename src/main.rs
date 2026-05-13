//! 免费macos解压缩软件 — 解压（ZIP、7z、RAR、TAR…）与压缩（ZIP、tar.gz）。

mod compress;
mod extract;
mod theme;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Vec2};
use compress::{
    CompressFormat, CompressOptions, CompressStats, CompressionLevel, compress_with_options,
    estimate_sources,
};
use extract::{
    ArchiveEntryPreview, ArchiveKind, ExtractOptions, ExtractProgress, OverwritePolicy,
    extract_with_options, list_archive,
};

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
    Progress(ExtractProgress),
    Done(Result<WorkerOutcome, String>),
}

struct WorkerOutcome {
    message: String,
    outputs: Vec<PathBuf>,
}

struct PreviewOutcome {
    status: String,
    entries: Vec<ArchiveEntryPreview>,
}

enum PreviewMsg {
    Done(Result<PreviewOutcome, String>),
}

struct ArchiverApp {
    mode: AppMode,
    archive_paths: Vec<PathBuf>,
    output_dir: Option<PathBuf>,
    overwrite_policy: OverwritePolicy,
    password: String,
    preview_entries: Vec<ArchiveEntryPreview>,
    preview_status: String,
    preview_busy: bool,
    preview_rx: Option<Receiver<PreviewMsg>>,
    _preview_tx: Option<Sender<PreviewMsg>>,
    last_output: Option<PathBuf>,
    last_outputs: Vec<PathBuf>,
    progress_current: usize,
    progress_total: Option<usize>,
    progress_file: String,
    compress_sources: Vec<PathBuf>,
    compress_output: Option<PathBuf>,
    compress_level: CompressionLevel,
    compress_keep_top_level: bool,
    compress_include_hidden: bool,
    compress_exclude_mac_metadata: bool,
    compress_exclude_common_dev_dirs: bool,
    compress_stats: Option<CompressStats>,
    compress_stats_status: String,
    log: String,
    busy: bool,
    cancel_flag: Option<Arc<AtomicBool>>,
    worker_rx: Option<Receiver<WorkerMsg>>,
    _worker_tx: Option<Sender<WorkerMsg>>,
}

impl ArchiverApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::install_visuals(&cc.egui_ctx);
        Self {
            mode: AppMode::Extract,
            archive_paths: Vec::new(),
            output_dir: None,
            overwrite_policy: OverwritePolicy::Rename,
            password: String::new(),
            preview_entries: Vec::new(),
            preview_status: String::new(),
            preview_busy: false,
            preview_rx: None,
            _preview_tx: None,
            last_output: None,
            last_outputs: Vec::new(),
            progress_current: 0,
            progress_total: None,
            progress_file: String::new(),
            compress_sources: Vec::new(),
            compress_output: None,
            compress_level: CompressionLevel::Balanced,
            compress_keep_top_level: true,
            compress_include_hidden: false,
            compress_exclude_mac_metadata: true,
            compress_exclude_common_dev_dirs: true,
            compress_stats: None,
            compress_stats_status: String::new(),
            log: String::new(),
            busy: false,
            cancel_flag: None,
            worker_rx: None,
            _worker_tx: None,
        }
    }

    fn append_log(&mut self, line: impl AsRef<str>) {
        self.log.push_str(line.as_ref());
        self.log.push('\n');
    }

    fn pick_archive(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter(
                "压缩包",
                &[
                    "zip", "7z", "rar", "cbr", "tar", "tgz", "gz", "bz2", "xz", "zst", "tbz",
                    "tbz2", "txz", "tzst",
                ],
            )
            .pick_files()
        {
            self.archive_paths.clear();
            for path in paths {
                self.add_archive_path(path);
            }
            self.preview_entries.clear();
            self.preview_status.clear();
            self.log.clear();
        }
    }

    fn pick_output(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.output_dir = Some(dir);
        }
    }

    fn archive_base_name(archive: &Path) -> String {
        let name = archive
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted");
        for suffix in [
            ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tgz", ".tbz2", ".tbz", ".txz",
            ".tzst", ".zip", ".7z", ".rar", ".cbr", ".tar",
        ] {
            if name.to_lowercase().ends_with(suffix) {
                let keep = name.len() - suffix.len();
                return name[..keep].to_string();
            }
        }
        archive
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted")
            .to_string()
    }

    fn unique_path(path: PathBuf) -> PathBuf {
        if !path.exists() {
            return path;
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("output");
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        for n in 2u32.. {
            let candidate = parent.join(format!("{stem} {n}{ext}"));
            if !candidate.exists() {
                return candidate;
            }
        }
        unreachable!()
    }

    fn default_output_for(archive: &Path) -> PathBuf {
        let stem = Self::archive_base_name(archive);
        let parent = archive.parent().unwrap_or(Path::new("."));
        Self::unique_path(parent.join(stem))
    }

    fn add_archive_path(&mut self, path: PathBuf) {
        if ArchiveKind::from_path(&path).is_none() {
            return;
        }
        let exists = self.archive_paths.iter().any(|p| Self::compress_path_same(p, &path));
        if !exists {
            self.archive_paths.push(path);
        }
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
        self.compress_stats = None;
        self.compress_stats_status.clear();
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
            .add_filter("tar.bz2", &["tar.bz2", "tbz2", "tbz"])
            .add_filter("tar.xz", &["tar.xz", "txz"])
            .add_filter("tar.zst", &["tar.zst", "tzst"])
            .set_file_name("archive.zip")
            .save_file()
        {
            self.compress_output = Some(p);
        }
    }

    fn prepare_worker_task(
        &mut self,
        initial_log: impl AsRef<str>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Sender<WorkerMsg> {
        self.busy = true;
        self.log.clear();
        self.last_output = None;
        self.last_outputs.clear();
        self.progress_current = 0;
        self.progress_total = None;
        self.progress_file.clear();
        self.cancel_flag = Some(cancel_flag);
        self.append_log(initial_log);

        let (tx, rx) = mpsc::channel();
        self.worker_rx = Some(rx);
        self._worker_tx = Some(tx.clone());
        tx
    }

    fn start_extract(&mut self) {
        if self.archive_paths.is_empty() {
            self.append_log("请先选择压缩包。");
            return;
        };
        let archives = self.archive_paths.clone();
        let output_dir = self.output_dir.clone();
        let overwrite = self.overwrite_policy;
        let password = if self.password.trim().is_empty() {
            None
        } else {
            Some(self.password.clone())
        };
        let batch = archives.len() > 1;
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let tx = self.prepare_worker_task(
            format!("开始解压 {} 个压缩包。", archives.len()),
            cancel_flag.clone(),
        );

        std::thread::spawn(move || {
            let mut outputs = Vec::new();
            let mut completed = 0usize;
            for archive in archives {
                if cancel_flag.load(Ordering::Relaxed) {
                    let _ = tx.send(WorkerMsg::Done(Err("任务已取消".to_string())));
                    return;
                }
                let Some(kind) = ArchiveKind::from_path(&archive) else {
                    let _ = tx.send(WorkerMsg::Done(Err(format!(
                        "不支持的扩展名: {}",
                        archive.display()
                    ))));
                    return;
                };
                let out = if let Some(base) = &output_dir {
                    if batch {
                        Self::unique_path(base.join(Self::archive_base_name(&archive)))
                    } else {
                        base.clone()
                    }
                } else {
                    Self::default_output_for(&archive)
                };
                let _ = tx.send(WorkerMsg::Progress(ExtractProgress {
                    current: 0,
                    total: None,
                    file: format!("{} → {}", archive.display(), out.display()),
                }));
                let options = ExtractOptions {
                    overwrite,
                    password: password.clone(),
                    cancel: Some(cancel_flag.clone()),
                };
                let progress_tx = tx.clone();
                let res = extract_with_options(&archive, &out, kind, options, |progress| {
                    let _ = progress_tx.send(WorkerMsg::Progress(progress));
                });
                if let Err(e) = res {
                    let _ = tx.send(WorkerMsg::Done(Err(e.to_string())));
                    return;
                }
                completed += 1;
                outputs.push(out);
            }
            let message = format!("完成。已解压 {completed} 个压缩包。");
            let _ = tx.send(WorkerMsg::Done(Ok(WorkerOutcome {
                message,
                outputs,
            })));
        });
    }

    fn start_compress(&mut self) {
        if self.compress_sources.is_empty() {
            self.append_log("请添加至少一个文件或文件夹。");
            return;
        }
        let Some(out) = self.compress_output.clone() else {
            self.append_log("请选择生成的压缩包路径。");
            return;
        };
        let Some(fmt) = CompressFormat::from_output_path(&out) else {
            self.append_log("输出文件扩展名需为 .zip、.tar.gz、.tar.bz2、.tar.xz 或 .tar.zst。");
            return;
        };

        let sources = self.compress_sources.clone();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let options = CompressOptions {
            level: self.compress_level,
            keep_top_level: self.compress_keep_top_level,
            include_hidden: self.compress_include_hidden,
            exclude_mac_metadata: self.compress_exclude_mac_metadata,
            exclude_common_dev_dirs: self.compress_exclude_common_dev_dirs,
            cancel: Some(cancel_flag.clone()),
        };
        let tx = self.prepare_worker_task(
            format!("压缩 {} → {}", fmt.label(), out.display()),
            cancel_flag,
        );

        std::thread::spawn(move || {
            let res = compress_with_options(&sources, &out, fmt, options)
                .map(|_| WorkerOutcome {
                    message: "完成。压缩包已生成。".to_string(),
                    outputs: vec![out],
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerMsg::Done(res));
        });
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.worker_rx else {
            return;
        };
        let mut done = None;
        let mut log_lines = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMsg::Progress(progress) => {
                    self.progress_current = progress.current;
                    self.progress_total = progress.total;
                    self.progress_file = progress.file.clone();
                    if progress.current == 0 {
                        log_lines.push(progress.file);
                    }
                }
                WorkerMsg::Done(res) => done = Some(res),
            }
        }
        for line in log_lines {
            self.append_log(line);
        }
        if let Some(res) = done {
            self.busy = false;
            self.worker_rx = None;
            self._worker_tx = None;
            self.cancel_flag = None;
            match res {
                Ok(outcome) => {
                    self.last_output = outcome.outputs.last().cloned();
                    self.last_outputs = outcome.outputs;
                    self.append_log(outcome.message);
                }
                Err(e) => self.append_log(format!("错误: {}", Self::friendly_error(&e))),
            }
            ctx.request_repaint();
        }
    }

    fn cancel_current_task(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
            self.append_log("正在取消任务…");
        }
    }

    fn poll_preview(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.preview_rx else {
            return;
        };
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                PreviewMsg::Done(res) => done = Some(res),
            }
        }
        if let Some(res) = done {
            self.preview_busy = false;
            self.preview_rx = None;
            self._preview_tx = None;
            match res {
                Ok(outcome) => {
                    self.preview_status = outcome.status;
                    self.preview_entries = outcome.entries;
                }
                Err(e) => {
                    self.preview_entries.clear();
                    self.preview_status = format!("预览失败: {}", Self::friendly_error(&e));
                }
            }
            ctx.request_repaint();
        }
    }

    fn friendly_error(error: &str) -> String {
        let lower = error.to_lowercase();
        if lower.contains("password") || error.contains("密码") {
            return "密码缺失或不正确，请填写密码后重试。".to_string();
        }
        if lower.contains("permission denied") || error.contains("权限") {
            return "没有权限访问该文件或目录，请检查文件权限。".to_string();
        }
        if lower.contains("no space") || error.contains("空间") {
            return "磁盘空间不足，请清理空间后重试。".to_string();
        }
        if error.contains("任务已取消") || lower.contains("cancel") {
            return "任务已取消。".to_string();
        }
        if lower.contains("invalid") || error.contains("损坏") || error.contains("读取") {
            return "压缩包可能已损坏，或格式与扩展名不匹配。".to_string();
        }
        error.to_string()
    }

    fn preview_selected_archive(&mut self) {
        let Some(archive) = self.archive_paths.first().cloned() else {
            self.preview_status = "请先选择压缩包。".to_string();
            return;
        };
        let Some(kind) = ArchiveKind::from_path(&archive) else {
            self.preview_status = "当前文件格式不支持预览。".to_string();
            return;
        };
        let password = if self.password.trim().is_empty() {
            None
        } else {
            Some(self.password.clone())
        };
        self.preview_busy = true;
        self.preview_entries.clear();
        self.preview_status = "正在读取目录…".to_string();
        let (tx, rx) = mpsc::channel();
        self.preview_rx = Some(rx);
        self._preview_tx = Some(tx.clone());
        std::thread::spawn(move || {
            let res = list_archive(&archive, kind, password.as_deref())
                .map(|entries| {
                    let total_size: u64 = entries.iter().filter_map(|e| e.size).sum();
                    PreviewOutcome {
                        status: format!(
                            "{} · {} 个条目 · 约 {}",
                            kind.label(),
                            entries.len(),
                            Self::human_size(total_size)
                        ),
                        entries,
                    }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(PreviewMsg::Done(res));
        });
    }

    fn refresh_compress_stats(&mut self) {
        if self.compress_sources.is_empty() {
            self.compress_stats = None;
            self.compress_stats_status = "暂无待压缩内容".to_string();
            return;
        }
        let options = self.current_compress_options(None);
        match estimate_sources(&self.compress_sources, options) {
            Ok(stats) => {
                self.compress_stats = Some(stats);
                self.compress_stats_status = format!(
                    "{} 个文件 · 约 {}",
                    stats.file_count,
                    Self::human_size(stats.total_bytes)
                );
            }
            Err(e) => {
                self.compress_stats = None;
                self.compress_stats_status = format!("统计失败: {}", Self::friendly_error(&e.to_string()));
            }
        }
    }

    fn current_compress_options(&self, cancel: Option<Arc<AtomicBool>>) -> CompressOptions {
        CompressOptions {
            level: self.compress_level,
            keep_top_level: self.compress_keep_top_level,
            include_hidden: self.compress_include_hidden,
            exclude_mac_metadata: self.compress_exclude_mac_metadata,
            exclude_common_dev_dirs: self.compress_exclude_common_dev_dirs,
            cancel,
        }
    }

    fn human_size(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit = 0usize;
        while size >= 1024.0 && unit + 1 < UNITS.len() {
            size /= 1024.0;
            unit += 1;
        }
        if unit == 0 {
            format!("{bytes} B")
        } else {
            format!("{size:.1} {}", UNITS[unit])
        }
    }

    fn open_in_finder(path: &Path) {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(path).spawn();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
        }
    }

    fn reveal_in_finder(path: &Path) {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg("-R").arg(path).spawn();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    match self.mode {
                        AppMode::Extract => {
                            if path.is_file() {
                                self.add_archive_path(path.clone());
                                self.preview_entries.clear();
                                self.preview_status.clear();
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
        let file_line = match self.archive_paths.as_slice() {
            [] => "未选择".to_string(),
            [one] => one.display().to_string(),
            many => format!(
                "已选择 {} 个压缩包，首个：{}",
                many.len(),
                many[0].display()
            ),
        };
        let out_line = self
            .output_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                self.archive_paths
                    .first()
                    .map(|a| Self::default_output_for(a).display().to_string())
                    .unwrap_or_else(|| "默认：同目录 · 子文件夹「压缩包原名」".to_string())
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

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let gap = 12.0;
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

        ui.add_space(14.0);

        let (f_line, o_line) = self.path_lines_extract();
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            ui.columns(2, |cols| {
                cols[0].vertical(|ui| {
                    ui.label(RichText::new("压缩包").size(11.0).color(theme::text_muted()));
                    ui.add_space(3.0);
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
                    ui.add_space(3.0);
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
        });

        ui.add_space(12.0);
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(14.0, 12.0);
            ui.label(RichText::new("解压选项").size(12.5).strong().color(theme::INK));
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("覆盖策略").size(12.0).color(theme::text_muted()));
                ui.radio_value(&mut self.overwrite_policy, OverwritePolicy::Rename, "自动重命名");
                ui.radio_value(&mut self.overwrite_policy, OverwritePolicy::Skip, "跳过");
                ui.radio_value(&mut self.overwrite_policy, OverwritePolicy::Overwrite, "覆盖");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("密码").size(12.0).color(theme::text_muted()));
                ui.add_sized(
                    [ui.available_width(), 34.0],
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .hint_text("加密 ZIP / RAR / 7z 可填写"),
                );
            });
        });

        ui.add_space(12.0);
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            ui.horizontal(|ui| {
                let can_preview = !self.archive_paths.is_empty() && !self.busy && !self.preview_busy;
                if ui
                    .add_enabled(
                        can_preview,
                        egui::Button::new(RichText::new("预览内容").size(13.0)),
                    )
                    .clicked()
                {
                    self.preview_selected_archive();
                }
                if self.preview_busy {
                    ui.spinner();
                }
                if !self.preview_status.is_empty() {
                    ui.label(
                        RichText::new(&self.preview_status)
                            .size(12.0)
                            .color(theme::text_muted()),
                    );
                }
            });
            if !self.preview_entries.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(130.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 7.0;
                        for entry in self.preview_entries.iter().take(80) {
                            let suffix = if entry.is_dir {
                                "目录".to_string()
                            } else {
                                entry
                                    .size
                                    .map(Self::human_size)
                                    .unwrap_or_else(|| "未知大小".to_string())
                            };
                            let lock = if entry.encrypted { " · 加密" } else { "" };
                            ui.label(
                                RichText::new(format!("{}  ·  {}{}", entry.name, suffix, lock))
                                    .size(12.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(theme::INK),
                            );
                        }
                        if self.preview_entries.len() > 80 {
                            ui.label(
                                RichText::new(format!(
                                    "另有 {} 个条目未显示",
                                    self.preview_entries.len() - 80
                                ))
                                .size(12.0)
                                .color(theme::text_muted()),
                            );
                        }
                    });
            }
        });

        if !self.last_outputs.is_empty() {
            ui.add_space(12.0);
            theme::section_frame().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("完成后操作").size(12.0).color(theme::text_muted()));
                    if ui.button("打开输出位置").clicked() {
                        if let Some(first) = self.last_outputs.first() {
                            Self::open_in_finder(first);
                        }
                    }
                    if ui.button("打开父目录").clicked() {
                        if let Some(parent) = self.last_outputs.first().and_then(|p| p.parent()) {
                            Self::open_in_finder(parent);
                        }
                    }
                    if ui.button("复制路径").clicked() {
                        ui.ctx().copy_text(
                            self.last_outputs
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                    }
                });
                if self.last_outputs.len() > 1 {
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(90.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for output in &self.last_outputs {
                                ui.label(
                                    RichText::new(output.display().to_string())
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(theme::INK),
                                );
                            }
                        });
                }
            });
        }

        ui.add_space(16.0);

        if self.busy {
            if ui
                .add_sized([ui.available_width(), 42.0], egui::Button::new("取消当前任务"))
                .clicked()
            {
                self.cancel_current_task();
            }
        } else {
            let can_run = !self.archive_paths.is_empty();
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

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let gap = 12.0;
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

        ui.add_space(12.0);
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("待压缩内容").size(12.5).strong().color(theme::INK));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(
                            RichText::new("清空列表")
                                .size(12.0)
                                .color(theme::text_muted()),
                        )
                        .clicked()
                    {
                        self.compress_sources.clear();
                        self.compress_stats = None;
                        self.compress_stats_status.clear();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .max_height(130.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 9.0;
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
                                ui.spacing_mut().item_spacing.x = 18.0;
                                let avail = ui.available_width();
                                let row_h = 38.0;
                                let btn_w = 72.0;
                                let gap = 18.0;
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
                                let stroke =
                                    egui::Stroke::new(1.0, Color32::from_rgb(153, 27, 27));
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
                            self.compress_stats = None;
                            self.compress_stats_status.clear();
                        }
                    }
                });
        });

        ui.add_space(12.0);
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.compress_sources.is_empty() && !self.busy,
                        egui::Button::new(RichText::new("统计待压缩内容").size(13.0)),
                    )
                    .clicked()
                {
                    self.refresh_compress_stats();
                }
                let status = if self.compress_stats_status.is_empty() {
                    "压缩前可先统计文件数量和原始大小"
                } else {
                    &self.compress_stats_status
                };
                ui.label(RichText::new(status).size(12.0).color(theme::text_muted()));
            });
            if self
                .compress_stats
                .is_some_and(|stats| stats.total_bytes > 1024 * 1024 * 1024)
            {
                ui.label(
                    RichText::new("提示：待压缩内容超过 1 GB，可能耗时较久。")
                        .size(12.0)
                        .color(Color32::from_rgb(180, 83, 9)),
                );
            }
        });

        ui.add_space(12.0);
        let out_label = self
            .compress_output
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                "未选择 · 支持 .zip / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst".to_string()
            });
        theme::section_frame().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new(
                        RichText::new("选择保存位置…")
                            .size(14.0)
                            .color(theme::INK),
                    )
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
            ui.label(RichText::new("输出").size(11.0).color(theme::text_muted()));
            ui.add(
                egui::Label::new(
                    RichText::new(&out_label)
                        .size(12.5)
                        .family(egui::FontFamily::Monospace)
                        .color(theme::INK),
                )
                .wrap(),
            );
        });

        ui.add_space(12.0);
        egui::CollapsingHeader::new(
            RichText::new("压缩选项").size(13.0).color(theme::text_muted()),
        )
        .id_salt("compress_options")
        .default_open(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("压缩级别").size(12.0).color(theme::text_muted()));
                ui.radio_value(&mut self.compress_level, CompressionLevel::Fast, "快速");
                ui.radio_value(&mut self.compress_level, CompressionLevel::Balanced, "均衡");
                ui.radio_value(&mut self.compress_level, CompressionLevel::Best, "最高");
            });
            ui.checkbox(&mut self.compress_keep_top_level, "保留文件夹顶层目录");
            ui.checkbox(&mut self.compress_include_hidden, "包含隐藏文件");
            ui.checkbox(&mut self.compress_exclude_mac_metadata, "排除 .DS_Store / __MACOSX");
            ui.checkbox(
                &mut self.compress_exclude_common_dev_dirs,
                "排除常见开发目录（.git / target / node_modules 等）",
            );
        });

        if !self.last_outputs.is_empty() {
            ui.add_space(12.0);
            theme::section_frame().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("完成后操作").size(12.0).color(theme::text_muted()));
                    if ui.button("打开输出位置").clicked() {
                        if let Some(parent) = self.last_outputs.first().and_then(|p| p.parent()) {
                            Self::open_in_finder(parent);
                        }
                    }
                    if ui.button("在 Finder 中显示").clicked() {
                        if let Some(output) = self.last_outputs.first() {
                            Self::reveal_in_finder(output);
                        }
                    }
                    if ui.button("复制路径").clicked() {
                        ui.ctx().copy_text(
                            self.last_outputs
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                    }
                });
            });
        }

        ui.add_space(16.0);

        if self.busy {
            if ui
                .add_sized([ui.available_width(), 42.0], egui::Button::new("取消当前任务"))
                .clicked()
            {
                self.cancel_current_task();
            }
        } else {
            let can_run = !self.compress_sources.is_empty() && self.compress_output.is_some();
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
            if let Some(total) = self.progress_total {
                if total > 0 {
                    let progress = self.progress_current as f32 / total as f32;
                    ui.add(
                        egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                            .show_percentage()
                            .text(format!("{}/{}", self.progress_current, total)),
                    );
                }
            } else if !self.progress_file.is_empty() {
                ui.add(egui::ProgressBar::new(0.0).animate(true).text("正在处理"));
            }
            if !self.progress_file.is_empty() {
                ui.label(
                    RichText::new(&self.progress_file)
                        .size(12.0)
                        .family(egui::FontFamily::Monospace)
                        .color(theme::text_muted()),
                );
            }
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
                RichText::new("压缩：ZIP、tar.gz、tar.bz2、tar.xz、tar.zst")
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
        self.poll_preview(ctx);

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
                        ui.add_space(16.0);

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 16.0);

                                match self.mode {
                                    AppMode::Extract => self.render_extract_panel(ui, drag_from_os),
                                    AppMode::Compress => {
                                        self.render_compress_panel(ui, drag_from_os)
                                    }
                                }

                                ui.add_space(2.0);
                                self.render_log_and_formats(ui);
                            });
                    });
                });
            });

        if self.busy || self.preview_busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
