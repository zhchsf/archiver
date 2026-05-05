//! 清新明亮主题 + macOS 中文字体。

use egui::{
    Button, Color32, FontData, FontDefinitions, FontFamily, FontTweak, RichText, Rounding, Shadow,
    Stroke, Vec2, Visuals,
};

/// 主操作：明快的青绿色
pub const ACCENT: Color32 = Color32::from_rgb(16, 185, 129);
const ACCENT_HOVER: Color32 = Color32::from_rgb(5, 150, 105);
const ACCENT_STROKE: Color32 = Color32::from_rgb(52, 211, 153);

/// 正文
pub const INK: Color32 = Color32::from_rgb(30, 41, 59);

const WINDOW_BG: Color32 = Color32::from_rgb(248, 250, 252);
const CARD_STROKE: Color32 = Color32::from_rgb(226, 232, 240);

/// 「选择文件」：天蓝系次要按钮
const FILE_BTN_FILL: Color32 = Color32::from_rgb(224, 242, 254);
const FILE_BTN_TEXT: Color32 = Color32::from_rgb(3, 105, 161);
const FILE_BTN_STROKE: Color32 = Color32::from_rgb(125, 211, 252);

/// 「输出目录」：薄荷绿系次要按钮（与文件按钮区分）
const FOLDER_BTN_FILL: Color32 = Color32::from_rgb(209, 250, 229);
const FOLDER_BTN_TEXT: Color32 = Color32::from_rgb(6, 95, 70);
const FOLDER_BTN_STROKE: Color32 = Color32::from_rgb(110, 231, 183);

pub fn text_muted() -> Color32 {
    Color32::from_rgb(100, 116, 139)
}

pub fn drop_zone_fill(hover: bool) -> Color32 {
    if hover {
        Color32::from_rgba_unmultiplied(16, 185, 129, 24)
    } else {
        Color32::from_rgb(240, 253, 250)
    }
}

pub fn drop_zone_stroke(hover: bool) -> Stroke {
    if hover {
        Stroke::new(2.0, ACCENT)
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
    .stroke(Stroke::new(1.5, FILE_BTN_STROKE))
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
    .stroke(Stroke::new(1.5, FOLDER_BTN_STROKE))
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
        s.spacing.button_padding = Vec2::new(14.0, 9.0);
        s.spacing.window_margin = egui::Margin::same(16.0);
        s.spacing.menu_margin = egui::Margin::same(8.0);
        let r = Rounding::same(10.0);
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
            offset: Vec2::new(0.0, 8.0),
            blur: 32.0,
            spread: 0.0,
            color: Color32::from_rgba_unmultiplied(15, 23, 42, 12),
        })
        .inner_margin(egui::Margin::same(22.0))
}
