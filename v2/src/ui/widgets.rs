//! 通用界面组件：卡片、卡片头、chip、按钮，观感对齐设计稿（Tailwind 风格）。

use eframe::egui::{self, Button, Color32, CornerRadius, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use super::icons::{self, Icon};
use super::theme::Theme;

/// 圆角卡片容器（rounded-xl + neutral-200 描边 + 卡底）。
pub fn card(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui)) {
    egui::Frame::default()
        .fill(th.card_bg)
        .stroke(Stroke::new(1.0, th.card_border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
}

/// 卡片头：图标 + 标题 + 副标题 + 右侧内容。
pub fn section_header(
    ui: &mut Ui,
    th: &Theme,
    icon: Icon,
    icon_color: Color32,
    title: &str,
    subtitle: Option<&str>,
    right: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        icons::show(ui, icon, 14.0, icon_color, th.card_bg);
        ui.label(RichText::new(title).strong().size(12.0).color(th.text));
        if let Some(s) = subtitle {
            ui.label(RichText::new(s).size(11.0).color(th.text_faint));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// 小 chip 按钮（快捷项/开关）。
pub fn chip(ui: &mut Ui, th: &Theme, text: &str, selected: bool) -> Response {
    let text = RichText::new(text).size(10.0).color(if selected {
        th.text
    } else {
        th.text_muted
    });
    let btn = Button::new(text)
        .corner_radius(CornerRadius::same(4))
        .fill(if selected { th.inner_panel_bg } else { Color32::TRANSPARENT });
    ui.add(btn)
}

/// 带图标按钮样式。
#[allow(dead_code)] // 样式全集保留
pub enum Btn {
    /// 蓝色主按钮（开始下载/解析）
    Primary,
    /// 白底描边中性按钮
    Neutral,
    /// 琥珀色实心（暂停）
    Amber,
    /// 青色软底描边（直链下载）
    EmeraldSoft,
    /// 无边框幽灵按钮（清空 ×）
    Ghost,
}

fn btn_colors(th: &Theme, kind: &Btn) -> (Color32, Color32, Color32, Color32) {
    // (填充, 描边, 文字, 悬停)
    match kind {
        Btn::Primary => (th.accent, Color32::TRANSPARENT, Color32::WHITE, th.accent_hover),
        Btn::Neutral => (th.btn_bg, th.btn_border, th.btn_text, th.hover),
        Btn::Amber => (th.amber, Color32::TRANSPARENT, Color32::WHITE, th.amber),
        Btn::EmeraldSoft => (th.emerald_soft, th.emerald_border, th.emerald_text, th.hover),
        Btn::Ghost => (Color32::TRANSPARENT, Color32::TRANSPARENT, th.text_muted, th.hover),
    }
}

fn icon_tint(th: &Theme, kind: &Btn) -> Color32 {
    match kind {
        Btn::EmeraldSoft => th.emerald,
        Btn::Neutral | Btn::Ghost => th.text_muted,
        _ => Color32::WHITE,
    }
}

/// 带图标的按钮。
pub fn button(
    ui: &mut Ui,
    th: &Theme,
    icon: Option<Icon>,
    label: &str,
    kind: &Btn,
    enabled: bool,
    min_size: Vec2,
) -> Response {
    let (fill, stroke_c, fg, hover) = btn_colors(th, kind);
    let (fill, fg) = if enabled { (fill, fg) } else { (th.btn_bg, th.text_faint) };
    ui.style_mut().visuals.widgets.hovered.bg_fill = hover;
    ui.style_mut().visuals.widgets.active.bg_fill = hover;

    let mut rich = RichText::new(label).size(12.0).color(fg);
    if matches!(kind, Btn::Primary | Btn::Amber) {
        rich = rich.strong();
    }
    let mut btn = Button::new(rich)
        .corner_radius(CornerRadius::same(8))
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke_c))
        .min_size(min_size);
    let has_icon = icon.is_some();
    if has_icon {
        // 文字与图标之间留出图标宽度（图标单独绘制在按钮左侧）
        btn = Button::new(RichText::new(format!("    {label}")).size(12.0).color(fg))
            .corner_radius(CornerRadius::same(8))
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke_c))
            .min_size(min_size);
    }
    let resp = ui.add_enabled(enabled, btn);
    if let (true, Some(ic)) = (has_icon, icon) {
        let icon_color = if fg == Color32::WHITE { fg } else { icon_tint(th, kind) };
        let rect = resp.rect;
        let ir = egui::Rect::from_min_size(
            rect.left_top() + Vec2::new(8.0, (rect.height() - 13.0) / 2.0),
            Vec2::splat(13.0),
        );
        icons::paint(ui.painter(), ic, ir, icon_color, fill);
    }
    resp
}

/// 徽标文字（通道标签、扩展名、版本号）。
pub fn badge(ui: &mut Ui, th: &Theme, text: &str) -> Response {
    let galley = ui.painter().layout(
        text.to_string(),
        egui::FontId::new(9.0, egui::FontFamily::Monospace),
        th.badge_text,
        f32::INFINITY,
    );
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(8.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), th.badge_bg);
    ui.painter().galley(rect.left_top() + Vec2::new(4.0, 2.0), galley, th.badge_text);
    resp
}

/// 标题栏图标按钮（无边框，悬停浅底）。
#[allow(dead_code)] // 标题栏精简后暂未使用
pub fn titlebar_icon_btn(ui: &mut Ui, th: &Theme, icon: Icon, tip: &str, hover_fill: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, CornerRadius::same(6), hover_fill);
    }
    let r = if !tip.is_empty() { resp.on_hover_text(tip) } else { resp };
    // 图标取矩形短边做正方形，保证居中且不变形
    let side = rect.width().min(rect.height()) - 8.0;
    let ir = egui::Rect::from_center_size(rect.center(), Vec2::splat(side));
    icons::paint(ui.painter(), icon, ir, th.text_secondary, th.titlebar_bg);
    r
}

/// 窗口控制按钮（— □ ✕），关闭键悬停变红。
pub fn window_btn(ui: &mut Ui, th: &Theme, icon: Icon, tip: &str, close: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 26.0), Sense::click());
    if ui.rect_contains_pointer(rect) {
        let fill = if close { th.red_hover } else { th.hover };
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    }
    let r = if !tip.is_empty() { resp.on_hover_text(tip) } else { resp };
    let color = if close && ui.rect_contains_pointer(rect) {
        Color32::WHITE
    } else {
        th.text_muted
    };
    let side = rect.width().min(rect.height()) - 10.0;
    let ir = egui::Rect::from_center_size(rect.center(), Vec2::splat(side));
    icons::paint(ui.painter(), icon, ir, color, th.titlebar_bg);
    r
}
