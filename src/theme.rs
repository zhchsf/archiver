//! 清新明亮主题 + macOS 中文字体。

use egui::{
    Button, Color32, FontData, FontDefinitions, FontFamily, FontTweak, RichText, Rounding, Shadow,
    Stroke, Vec2, Visuals,
};

/// 主操作：明快的青绿色
pub const ACCENT: Color32 = Color32::from_rgb(16, 185, 129);
const ACCENT_HOVER: Color32 = Color32::from_rgb(5, 150, 105);
const ACCENT_STROKE: Color32 = Color32::from_rgb(52, 211, 153);
const ACCENT_SOFT: Color32 = Color32::from_rgb(236, 253, 245);

/// 正文
pub const INK: Color32 = Color32::from_rgb(30, 41, 59);

const WINDOW_BG: Color32 = Color32::from_rgb(248, 250, 252);
const CARD_STROKE: Color32 = Color32::from_rgb(226, 232, 240);

/// 模式切换条背景
const TAB_TRACK: Color32 = Color32::from_rgb(230, 236, 244);
/// 未选中模式块
const TAB_INACTIVE: Color32 = Color32::from_rgb(252, 252, 254);

/// 「选择文件」：天蓝系次要按钮
const FILE_BTN_FILL: Color32 = Color32::from_rgb(224, 242, 254);
const FILE_BTN_TEXT: Color32 = Color32::from_rgb(3, 105, 161);
const FILE_BTN_STROKE: Color32 = Color32::from_rgb(125, 211, 252);

/// 「输出目录」：薄荷绿系次要按钮（与文件按钮区分）
const FOLDER_BTN_FILL: Color32 = Color32::from_rgb(209, 250, 229);
const FOLDER_BTN_TEXT: Color32 = Color32::from_rgb(6, 95, 70);
const FOLDER_BTN_STROKE: Color32 = Color32::from_rgb(110, 231, 183);

/// 线框次要操作（预览、统计等）
const OUTLINE_STROKE: Color32 = Color32::from_rgb(203, 213, 225);

/// 取消任务：柔和警示
const DANGER_TEXT: Color32 = Color32::from_rgb(185, 28, 28);
const DANGER_FILL: Color32 = Color32::from_rgb(254, 242, 242);
const DANGER_STROKE: Color32 = Color32::from_rgb(252, 165, 165);

/// 列表内「移除」
pub const REMOVE_FILL: Color32 = Color32::from_rgb(254, 242, 242);
pub const REMOVE_FILL_HOVER: Color32 = Color32::from_rgb(252, 231, 231);
pub const REMOVE_STROKE: Color32 = Color32::from_rgb(252, 165, 165);
pub const REMOVE_LABEL: Color32 = Color32::from_rgb(185, 28, 28);

const R_MAIN: f32 = 12.0;
const R_SECONDARY: f32 = 10.0;
const R_TAB: f32 = 10.0;

pub fn text_muted() -> Color32 {
    Color32::from_rgb(100, 116, 139)
}

pub fn drop_zone_fill(hover: bool) -> Color32 {
    if hover {
        ACCENT_SOFT
    } else {
        Color32::from_rgb(248, 252, 250)
    }
}

pub fn drop_zone_stroke(hover: bool) -> Stroke {
    if hover {
        Stroke::new(1.5, ACCENT)
    } else {
        Stroke::new(1.0, Color32::from_rgb(167, 243, 208))
    }
}

/// 主操作按钮（解压 / 压缩等）
pub fn primary_action_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(16.0)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(ACCENT)
    .stroke(Stroke::new(1.0, ACCENT_STROKE))
    .rounding(Rounding::same(R_MAIN))
}

/// 选择与压缩包相关的文件（天蓝）
pub fn secondary_file_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(14.0)
            .strong()
            .color(FILE_BTN_TEXT),
    )
    .fill(FILE_BTN_FILL)
    .stroke(Stroke::new(1.0, FILE_BTN_STROKE))
    .rounding(Rounding::same(R_SECONDARY))
}

/// 选择输出目录（薄荷绿）
pub fn secondary_folder_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(14.0)
            .strong()
            .color(FOLDER_BTN_TEXT),
    )
    .fill(FOLDER_BTN_FILL)
    .stroke(Stroke::new(1.0, FOLDER_BTN_STROKE))
    .rounding(Rounding::same(R_SECONDARY))
}

/// 线框次要按钮：预览、统计等
pub fn outline_secondary_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(13.0)
            .strong()
            .color(INK),
    )
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, OUTLINE_STROKE))
    .rounding(Rounding::same(R_SECONDARY))
}

/// 取消当前任务（柔和红框，不与主操作抢视觉）
pub fn danger_outline_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(14.0)
            .strong()
            .color(DANGER_TEXT),
    )
    .fill(DANGER_FILL)
    .stroke(Stroke::new(1.0, DANGER_STROKE))
    .rounding(Rounding::same(R_SECONDARY))
}

/// 选择压缩输出路径
pub fn save_destination_button() -> Button<'static> {
    Button::new(
        RichText::new("选择保存位置…")
            .size(14.0)
            .strong()
            .color(INK),
    )
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, OUTLINE_STROKE))
    .rounding(Rounding::same(R_SECONDARY))
}

/// 完成后操作区的小按钮（打开 Finder 等）
pub fn subtle_tertiary_button(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(12.5)
            .color(Color32::from_rgb(2, 132, 199)),
    )
    .fill(Color32::from_rgb(240, 249, 255))
    .stroke(Stroke::new(1.0, Color32::from_rgb(186, 230, 253)))
    .rounding(Rounding::same(8.0))
}

/// 解压 / 压缩 模式切换：选中态
pub fn mode_tab_button_selected(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(ACCENT)
    .stroke(Stroke::new(1.0, ACCENT_STROKE))
    .rounding(Rounding::same(R_TAB))
}

/// 解压 / 压缩 模式切换：未选中态
pub fn mode_tab_button_inactive(label: &str) -> Button<'static> {
    Button::new(
        RichText::new(label.to_owned())
            .size(15.0)
            .strong()
            .color(text_muted()),
    )
    .fill(TAB_INACTIVE)
    .stroke(Stroke::new(1.0, Color32::TRANSPARENT))
    .rounding(Rounding::same(R_TAB))
}

/// 列表右上角等弱操作（清空列表）
pub fn small_muted_action(label: &str) -> Button<'static> {
    Button::new(RichText::new(label.to_owned()).size(12.0).color(text_muted()))
        .fill(Color32::from_rgb(250, 250, 252))
        .stroke(Stroke::new(1.0, Color32::from_rgb(226, 232, 240)))
        .rounding(Rounding::same(8.0))
}

#[cfg(target_os = "macos")]
fn load_macos_cjk_font() -> Option<(Vec<u8>, u32)> {
    const CANDIDATES: &[(&str, u32)] = &[
        ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
        ("/System/Library/Fonts/STHeiti Medium.ttc", 0),
        ("/System/Library/Fonts/PingFang.ttc", 0),
    ];
    for &(path, index) in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            if !bytes.is_empty() {
                return Some((bytes, index));
            }
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn load_macos_cjk_font() -> Option<(Vec<u8>, u32)> {
    None
}

pub fn install_fonts(ctx: &egui::Context) {
    let Some((bytes, index)) = load_macos_cjk_font() else {
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "system_cjk".to_owned(),
        FontData {
            font: std::borrow::Cow::Owned(bytes),
            index,
            tweak: FontTweak {
                scale: 0.94,
                y_offset_factor: -0.02,
                ..Default::default()
            },
        },
    );

    if let Some(v) = fonts.families.get_mut(&FontFamily::Proportional) {
        v.insert(0, "system_cjk".to_owned());
    }
    if let Some(v) = fonts.families.get_mut(&FontFamily::Monospace) {
        v.insert(0, "system_cjk".to_owned());
    }

    ctx.set_fonts(fonts);
}

pub fn install_visuals(ctx: &egui::Context) {
    let mut visuals = Visuals::light();
    visuals.dark_mode = false;
    visuals.override_text_color = Some(INK);
    visuals.window_fill = WINDOW_BG;
    visuals.panel_fill = WINDOW_BG;
    visuals.extreme_bg_color = Color32::from_rgb(241, 245, 249);
    visuals.faint_bg_color = Color32::WHITE;
    visuals.window_stroke = Stroke::NONE;

    visuals.widgets.noninteractive.bg_fill = Color32::WHITE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, CARD_STROKE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(248, 250, 252);
    visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, CARD_STROKE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(241, 245, 249);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);

    visuals.widgets.active.bg_fill = ACCENT_HOVER;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);

    visuals.widgets.open.bg_fill = Color32::from_rgb(248, 250, 252);

    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(16, 185, 129, 45);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = Color32::from_rgb(2, 132, 199);

    ctx.set_visuals(visuals);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(12.0, 12.0);
        s.spacing.button_padding = Vec2::new(16.0, 10.0);
        s.spacing.window_margin = egui::Margin::same(16.0);
        s.spacing.menu_margin = egui::Margin::same(8.0);
        let r = Rounding::same(11.0);
        s.visuals.widgets.noninteractive.rounding = r;
        s.visuals.widgets.inactive.rounding = r;
        s.visuals.widgets.hovered.rounding = r;
        s.visuals.widgets.active.rounding = r;
        s.visuals.widgets.open.rounding = r;
    });
}

pub fn main_panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::WHITE)
        .rounding(Rounding::same(16.0))
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .shadow(Shadow {
            offset: Vec2::new(0.0, 6.0),
            blur: 28.0,
            spread: 0.0,
            color: Color32::from_rgba_unmultiplied(15, 23, 42, 10),
        })
        .inner_margin(egui::Margin::same(24.0))
}

/// 顶部「解压 / 压缩」切换条容器
pub fn mode_tab_bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(TAB_TRACK)
        .rounding(Rounding::same(14.0))
        .inner_margin(egui::Margin::symmetric(6.0, 5.0))
}

pub fn section_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_rgb(250, 251, 253))
        .rounding(Rounding::same(12.0))
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .shadow(Shadow {
            offset: Vec2::new(0.0, 1.0),
            blur: 8.0,
            spread: 0.0,
            color: Color32::from_rgba_unmultiplied(15, 23, 42, 5),
        })
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
}
