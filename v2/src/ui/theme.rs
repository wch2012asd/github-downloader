//! 界面主题：亮 / 暗两套配色令牌，色值对齐设计稿（Tailwind neutral/blue/emerald/amber/indigo/rose）。

use eframe::egui::Color32;

use crate::events::LogLevel;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // 令牌按设计全集保留
pub struct Theme {
    pub dark: bool,
    // 窗口层级
    /// 窗口底色：neutral-100 / #121417
    pub window_bg: Color32,
    pub titlebar_bg: Color32,
    pub footer_bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    /// 卡片内嵌面板（代理端口区、速度曲线底）
    pub inner_panel_bg: Color32,
    pub inner_panel_border: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    pub input_text: Color32,
    pub placeholder: Color32,
    // 文字
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub sep: Color32,
    // 徽标
    pub badge_bg: Color32,
    pub badge_text: Color32,
    // 主色
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub accent_border: Color32,
    // 状态色
    pub emerald: Color32,
    pub emerald_text: Color32,
    pub emerald_soft: Color32,
    pub emerald_border: Color32,
    pub amber: Color32,
    pub amber_text: Color32,
    pub amber_soft: Color32,
    pub indigo_text: Color32,
    pub rose: Color32,
    pub rose_soft: Color32,
    pub red_hover: Color32,
    // 日志终端（两种主题恒为深色）
    pub log_bg: Color32,
    pub log_border: Color32,
    pub log_fg: Color32,
    pub log_ts: Color32,
    pub log_info: Color32,
    pub log_success: Color32,
    pub log_warn: Color32,
    pub log_error: Color32,
    pub log_proxy: Color32,
    // 交互
    pub hover: Color32,
    pub btn_bg: Color32,
    pub btn_border: Color32,
    pub btn_text: Color32,
}

pub fn log_color(theme: &Theme, level: LogLevel) -> Color32 {
    match level {
        LogLevel::Info => theme.log_info,
        LogLevel::Warn => theme.log_warn,
        LogLevel::Success => theme.log_success,
        LogLevel::Error => theme.log_error,
        LogLevel::Proxy => theme.log_proxy,
    }
}

pub fn light() -> Theme {
    Theme {
        dark: false,
        window_bg: rgb(0xf5, 0xf5, 0xf5),      // neutral-100
        titlebar_bg: rgb(0xff, 0xff, 0xff),
        footer_bg: rgb(0xff, 0xff, 0xff),
        card_bg: rgb(0xff, 0xff, 0xff),
        card_border: rgb(0xe5, 0xe5, 0xe5),    // neutral-200
        inner_panel_bg: rgb(0xfa, 0xfa, 0xfa), // neutral-50/70
        inner_panel_border: rgb(0xe5, 0xe5, 0xe5),
        input_bg: rgb(0xfa, 0xfa, 0xfa),       // neutral-50
        input_border: rgb(0xd4, 0xd4, 0xd4),   // neutral-300
        input_text: rgb(0x17, 0x17, 0x17),     // neutral-900
        placeholder: rgb(0xa3, 0xa3, 0xa3),    // neutral-400
        text: rgb(0x17, 0x17, 0x17),
        text_secondary: rgb(0x40, 0x40, 0x40), // neutral-700
        text_muted: rgb(0x73, 0x73, 0x73),     // neutral-500
        text_faint: rgb(0xa3, 0xa3, 0xa3),     // neutral-400
        sep: rgb(0xd4, 0xd4, 0xd4),
        badge_bg: rgb(0xf5, 0xf5, 0xf5),
        badge_text: rgb(0x52, 0x52, 0x52),     // neutral-600
        accent: rgb(0x25, 0x63, 0xeb),         // blue-600
        accent_hover: rgb(0x3b, 0x82, 0xf6),   // blue-500
        accent_text: rgb(0x25, 0x63, 0xeb),
        accent_soft: rgb(0xef, 0xf6, 0xff),    // blue-50
        accent_border: rgb(0xbf, 0xdb, 0xfe),  // blue-200
        emerald: rgb(0x10, 0xb9, 0x81),        // emerald-500
        emerald_text: rgb(0x05, 0x96, 0x69),   // emerald-600
        emerald_soft: rgb(0xec, 0xfd, 0xf5),   // emerald-50
        emerald_border: rgb(0xa7, 0xf3, 0xd0), // emerald-200
        amber: rgb(0xf5, 0x9e, 0x0b),          // amber-500
        amber_text: rgb(0xd9, 0x77, 0x06),     // amber-600
        amber_soft: rgb(0xff, 0xfb, 0xeb),     // amber-50
        indigo_text: rgb(0x63, 0x66, 0xf1),    // indigo-500
        rose: rgb(0xf4, 0x3f, 0x5e),           // rose-500
        rose_soft: rgb(0xff, 0xf1, 0xf2),      // rose-50
        red_hover: rgb(0xef, 0x44, 0x44),      // red-500
        log_bg: rgb(0x0a, 0x0a, 0x0a),         // neutral-950
        log_border: rgb(0x26, 0x26, 0x26),     // neutral-800
        log_fg: rgb(0xd4, 0xd4, 0xd4),         // neutral-300
        log_ts: rgb(0x73, 0x73, 0x73),
        log_info: rgb(0x60, 0xa5, 0xfa),       // blue-400
        log_success: rgb(0x34, 0xd3, 0x99),    // emerald-400
        log_warn: rgb(0xfb, 0xbf, 0x24),       // amber-400
        log_error: rgb(0xfb, 0x71, 0x85),      // rose-400
        log_proxy: rgb(0x81, 0x8c, 0xf8),      // indigo-400
        hover: rgb(0xf5, 0xf5, 0xf5),
        btn_bg: rgb(0xf5, 0xf5, 0xf5),
        btn_border: rgb(0xd4, 0xd4, 0xd4),
        btn_text: rgb(0x40, 0x40, 0x40),
    }
}

pub fn dark() -> Theme {
    Theme {
        dark: true,
        window_bg: rgb(0x12, 0x14, 0x17),      // 设计稿石板黑
        titlebar_bg: rgb(0x17, 0x17, 0x17),    // neutral-900
        footer_bg: rgb(0x17, 0x17, 0x17),
        card_bg: rgb(0x17, 0x17, 0x17),
        card_border: rgb(0x26, 0x26, 0x26),    // neutral-800
        inner_panel_bg: rgb(0x1d, 0x1d, 0x1d), // neutral-800/40
        inner_panel_border: rgb(0x26, 0x26, 0x26),
        input_bg: rgb(0x20, 0x20, 0x20),       // neutral-800/80
        input_border: rgb(0x40, 0x40, 0x40),   // neutral-700
        input_text: rgb(0xf5, 0xf5, 0xf5),
        placeholder: rgb(0x73, 0x73, 0x73),
        text: rgb(0xf5, 0xf5, 0xf5),
        text_secondary: rgb(0xd4, 0xd4, 0xd4),
        text_muted: rgb(0xa3, 0xa3, 0xa3),
        text_faint: rgb(0x73, 0x73, 0x73),
        sep: rgb(0x40, 0x40, 0x40),
        badge_bg: rgb(0x26, 0x26, 0x26),
        badge_text: rgb(0xa3, 0xa3, 0xa3),
        accent: rgb(0x25, 0x63, 0xeb),
        accent_hover: rgb(0x3b, 0x82, 0xf6),
        accent_text: rgb(0x60, 0xa5, 0xfa),    // blue-400
        accent_soft: rgb(0x14, 0x24, 0x3d),    // blue-950/40
        accent_border: rgb(0x1e, 0x40, 0xaf),  // blue-800
        emerald: rgb(0x10, 0xb9, 0x81),
        emerald_text: rgb(0x34, 0xd3, 0x99),   // emerald-400
        emerald_soft: rgb(0x06, 0x2c, 0x22),   // emerald-950/40
        emerald_border: rgb(0x06, 0x5f, 0x46), // emerald-800
        amber: rgb(0xf5, 0x9e, 0x0b),
        amber_text: rgb(0xfb, 0xbf, 0x24),
        amber_soft: rgb(0x2c, 0x22, 0x08),
        indigo_text: rgb(0x81, 0x8c, 0xf8),
        rose: rgb(0xf4, 0x3f, 0x5e),
        rose_soft: rgb(0x2a, 0x0a, 0x10),
        red_hover: rgb(0xdc, 0x26, 0x26),
        log_bg: rgb(0x0a, 0x0a, 0x0a),
        log_border: rgb(0x26, 0x26, 0x26),
        log_fg: rgb(0xd4, 0xd4, 0xd4),
        log_ts: rgb(0x73, 0x73, 0x73),
        log_info: rgb(0x60, 0xa5, 0xfa),
        log_success: rgb(0x34, 0xd3, 0x99),
        log_warn: rgb(0xfb, 0xbf, 0x24),
        log_error: rgb(0xfb, 0x71, 0x85),
        log_proxy: rgb(0x81, 0x8c, 0xf8),
        hover: rgb(0x26, 0x26, 0x26),
        btn_bg: rgb(0x26, 0x26, 0x26),
        btn_border: rgb(0x40, 0x40, 0x40),
        btn_text: rgb(0xd4, 0xd4, 0xd4),
    }
}
