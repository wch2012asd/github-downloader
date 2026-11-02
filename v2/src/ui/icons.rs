//! 矢量图标库：24 网格描边图标，风格对齐 lucide（设计稿所用图标集）。

use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Sense, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // 图标集按设计全集保留
pub enum Icon {
    Github,
    Zap,
    History,
    Settings,
    Sun,
    Moon,
    Minus,
    Maximize,
    Close,
    Link,
    Clipboard,
    ArrowRight,
    Package,
    Folder,
    FileCode,
    FileText,
    HardDrive,
    Network,
    Refresh,
    Activity,
    CheckCircle,
    XCircle,
    Gauge,
    Play,
    Pause,
    Stop,
    FolderCheck,
    Sparkles,
    RotateCcw,
    Terminal,
    Trash,
    Copy,
    Download,
    ArrowDown,
    Search,
    Fullscreen,
    Unfullscreen,
}

/// 在指定矩形内绘制图标（24 单位网格缩放到 rect）。
pub fn paint(p: &Painter, icon: Icon, rect: Rect, color: Color32, bg: Color32) {
    let s = rect.width() / 24.0;
    let pt = |x: f32, y: f32| -> Pos2 { rect.left_top() + Vec2::new(x * s, y * s) };
    let st = |w: f32| Stroke::new(w * s, color);
    let line = |x1: f32, y1: f32, x2: f32, y2: f32, w: f32| {
        p.add(Shape::line_segment([pt(x1, y1), pt(x2, y2)], st(w)));
    };
    let circle = |cx: f32, cy: f32, r: f32, w: f32| {
        p.circle_stroke(pt(cx, cy), r * s, st(w));
    };
    let dot = |cx: f32, cy: f32, r: f32| {
        p.circle_filled(pt(cx, cy), r * s, color);
    };
    let poly = |pts: &[(f32, f32)], w: f32, closed: bool| {
        let mut v: Vec<Pos2> = pts.iter().map(|(x, y)| pt(*x, *y)).collect();
        if closed {
            v.push(v[0]);
        }
        p.add(Shape::line(v, st(w)));
    };
    let fill_poly = |pts: &[(f32, f32)]| {
        p.add(Shape::convex_polygon(
            pts.iter().map(|(x, y)| pt(*x, *y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32, w: f32| {
        // 角度制，0° = 12 点方向，顺时针
        let steps = 24;
        let mut v = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let a = (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians();
            v.push(pt(cx + r * a.sin(), cy - r * a.cos()));
        }
        p.add(Shape::line(v, st(w)));
    };
    let w2 = 2.0;

    match icon {
        Icon::Github => {
            // 简化 octocat 轮廓：圆头 + 双耳
            circle(12.0, 13.0, 8.0, w2);
            poly(&[(6.5, 7.5), (5.5, 2.5), (10.5, 5.0)], w2, false);
            poly(&[(17.5, 7.5), (18.5, 2.5), (13.5, 5.0)], w2, false);
            line(9.5, 20.5, 9.5, 16.0, w2);
            line(14.5, 20.5, 14.5, 16.0, w2);
        }
        Icon::Zap => fill_poly(&[(13.0, 2.0), (3.0, 14.0), (11.0, 14.0), (10.5, 22.0), (21.0, 10.0), (12.5, 10.0)]),
        Icon::History => {
            arc(12.0, 12.0, 9.0, 130.0, 410.0, w2);
            poly(&[(3.0, 3.0), (3.0, 8.0), (8.0, 8.0)], w2, false);
            poly(&[(12.0, 7.0), (12.0, 12.0), (15.5, 14.0)], w2, false);
        }
        Icon::Settings => {
            line(20.0, 7.0, 11.0, 7.0, w2);
            circle(8.0, 7.0, 3.0, w2);
            line(4.0, 17.0, 13.0, 17.0, w2);
            circle(16.0, 17.0, 3.0, w2);
        }
        Icon::Sun => {
            circle(12.0, 12.0, 4.0, w2);
            for i in 0..8 {
                let a = i as f32 * 45.0f32.to_radians();
                let (dx, dy) = (a.sin(), a.cos());
                line(12.0 + dx * 6.5, 12.0 + dy * 6.5, 12.0 + dx * 9.5, 12.0 + dy * 9.5, w2);
            }
        }
        Icon::Moon => {
            // 月牙：整圆 + 底色圆叠加出干净的新月
            p.circle_filled(pt(12.0, 12.0), 8.5 * s, color);
            p.circle_filled(pt(16.5, 7.5), 7.5 * s, bg);
        }
        Icon::Minus => line(5.0, 12.0, 19.0, 12.0, w2),
        Icon::Maximize => {
            let r = Rect::from_min_max(pt(5.0, 5.0), pt(19.0, 19.0));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
        }
        Icon::Close => {
            line(6.0, 6.0, 18.0, 18.0, w2);
            line(18.0, 6.0, 6.0, 18.0, w2);
        }
        Icon::Link => {
            arc(8.0, 8.5, 4.5, 180.0, 360.0, w2);
            line(3.5, 8.5, 3.5, 15.5, w2);
            arc(8.0, 15.5, 4.5, 0.0, 180.0, w2);
            arc(16.0, 8.5, 4.5, 180.0, 360.0, w2);
            line(20.5, 8.5, 20.5, 15.5, w2);
            arc(16.0, 15.5, 4.5, 0.0, 180.0, w2);
            line(8.0, 12.0, 16.0, 12.0, w2);
        }
        Icon::Clipboard => {
            let r = Rect::from_min_max(pt(5.0, 4.0), pt(19.0, 21.0));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            let r2 = Rect::from_min_max(pt(9.0, 2.0), pt(15.0, 6.5));
            p.rect_filled(r2, CornerRadius::same(1), bg);
            p.rect_stroke(r2, CornerRadius::same(1), st(w2), eframe::egui::StrokeKind::Middle);
            line(9.0, 12.0, 15.0, 12.0, w2);
            line(9.0, 16.0, 13.0, 16.0, w2);
        }
        Icon::ArrowRight => {
            line(4.5, 12.0, 19.0, 12.0, w2);
            poly(&[(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)], w2, false);
        }
        Icon::Package => {
            poly(&[(12.0, 2.5), (21.0, 7.0), (21.0, 17.0), (12.0, 21.5), (3.0, 17.0), (3.0, 7.0)], w2, true);
            poly(&[(3.0, 7.0), (12.0, 12.0), (21.0, 7.0)], w2, false);
            line(12.0, 12.0, 12.0, 21.5, w2);
        }
        Icon::Folder | Icon::FolderCheck => {
            poly(&[(3.0, 19.5), (3.0, 5.0), (9.0, 5.0), (11.5, 8.0), (21.0, 8.0), (21.0, 19.5)], w2, true);
            if icon == Icon::FolderCheck {
                poly(&[(9.0, 14.0), (11.5, 16.5), (15.5, 12.0)], w2, false);
            }
        }
        Icon::FileCode | Icon::FileText => {
            poly(&[(13.0, 2.5), (5.5, 2.5), (5.5, 21.5), (18.5, 21.5), (18.5, 8.0), (13.0, 2.5)], w2, true);
            poly(&[(13.0, 2.5), (13.0, 8.0), (18.5, 8.0)], w2, false);
            if icon == Icon::FileCode {
                poly(&[(10.5, 11.5), (8.5, 14.0), (10.5, 16.5)], w2, false);
                poly(&[(13.5, 11.5), (15.5, 14.0), (13.5, 16.5)], w2, false);
            } else {
                line(9.0, 12.5, 15.0, 12.5, w2);
                line(9.0, 15.5, 15.0, 15.5, w2);
                line(9.0, 18.5, 13.0, 18.5, w2);
            }
        }
        Icon::HardDrive => {
            poly(&[(4.5, 4.5), (2.5, 11.0), (2.5, 18.0), (21.5, 18.0), (21.5, 11.0), (19.5, 4.5)], w2, true);
            line(2.5, 11.5, 21.5, 11.5, w2);
            dot(6.0, 15.0, 0.9);
            dot(10.0, 15.0, 0.9);
        }
        Icon::Network => {
            let r = Rect::from_min_max(pt(9.0, 2.5), pt(15.0, 8.0));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            let r = Rect::from_min_max(pt(2.5, 16.0), pt(8.5, 21.5));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            let r = Rect::from_min_max(pt(15.5, 16.0), pt(21.5, 21.5));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            line(12.0, 8.0, 12.0, 12.0, w2);
            poly(&[(5.5, 16.0), (5.5, 12.0), (18.5, 12.0), (18.5, 16.0)], w2, false);
        }
        Icon::Refresh => {
            arc(12.0, 12.0, 8.5, 150.0, 340.0, w2);
            poly(&[(20.5, 8.0), (21.0, 13.5), (16.0, 12.0)], w2, false);
            arc(12.0, 12.0, 8.5, -30.0, 160.0, w2);
            poly(&[(3.5, 16.0), (3.0, 10.5), (8.0, 12.0)], w2, false);
        }
        Icon::Activity => {
            poly(&[(2.5, 12.0), (7.0, 12.0), (10.0, 4.5), (14.5, 19.5), (17.5, 12.0), (21.5, 12.0)], w2, false);
        }
        Icon::CheckCircle => {
            circle(12.0, 12.0, 9.0, w2);
            poly(&[(8.0, 12.0), (11.0, 15.0), (16.0, 9.5)], w2, false);
        }
        Icon::XCircle => {
            circle(12.0, 12.0, 9.0, w2);
            line(9.0, 9.0, 15.0, 15.0, w2);
            line(15.0, 9.0, 9.0, 15.0, w2);
        }
        Icon::Gauge => {
            arc(12.0, 13.0, 9.0, 200.0, 340.0, w2);
            line(12.0, 13.0, 15.8, 9.2, w2);
            dot(12.0, 13.0, 1.3);
        }
        Icon::Play => fill_poly(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]),
        Icon::Pause => {
            let r = Rect::from_min_max(pt(6.5, 5.0), pt(10.0, 19.0));
            p.rect_filled(r, CornerRadius::same(1), color);
            let r = Rect::from_min_max(pt(14.0, 5.0), pt(17.5, 19.0));
            p.rect_filled(r, CornerRadius::same(1), color);
        }
        Icon::Stop => {
            let r = Rect::from_min_max(pt(6.5, 6.5), pt(17.5, 17.5));
            p.rect_filled(r, CornerRadius::same(1), color);
        }
        Icon::Sparkles => {
            fill_poly(&[(11.0, 2.5), (13.2, 9.0), (19.5, 11.0), (13.2, 13.0), (11.0, 19.5), (8.8, 13.0), (2.5, 11.0), (8.8, 9.0)]);
            fill_poly(&[(18.5, 15.5), (19.3, 17.7), (21.5, 18.5), (19.3, 19.3), (18.5, 21.5), (17.7, 19.3), (15.5, 18.5), (17.7, 17.7)]);
        }
        Icon::RotateCcw => {
            arc(12.0, 12.0, 8.5, 210.0, 500.0, w2);
            poly(&[(4.5, 6.5), (3.8, 11.5), (8.8, 10.0)], w2, false);
        }
        Icon::Terminal => {
            let r = Rect::from_min_max(pt(3.0, 4.0), pt(21.0, 20.0));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            poly(&[(7.0, 9.0), (11.0, 12.5), (7.0, 16.0)], w2, false);
            line(12.5, 16.0, 16.5, 16.0, w2);
        }
        Icon::Trash => {
            line(3.5, 6.0, 20.5, 6.0, w2);
            poly(&[(6.5, 6.0), (7.5, 20.5), (16.5, 20.5), (17.5, 6.0)], w2, false);
            poly(&[(9.0, 6.0), (9.5, 3.0), (14.5, 3.0), (15.0, 6.0)], w2, false);
            line(10.0, 10.0, 10.0, 16.5, w2);
            line(14.0, 10.0, 14.0, 16.5, w2);
        }
        Icon::Copy => {
            let r = Rect::from_min_max(pt(9.0, 9.0), pt(20.5, 20.5));
            p.rect_stroke(r, CornerRadius::same(2), st(w2), eframe::egui::StrokeKind::Middle);
            poly(&[(15.0, 5.5), (15.0, 3.5), (3.5, 3.5), (3.5, 15.0), (5.5, 15.0)], w2, false);
        }
        Icon::Download => {
            line(12.0, 3.0, 12.0, 15.0, w2);
            poly(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)], w2, false);
            poly(&[(4.0, 16.5), (4.0, 20.5), (20.0, 20.5), (20.0, 16.5)], w2, false);
        }
        Icon::ArrowDown => {
            line(12.0, 3.0, 12.0, 16.0, w2);
            poly(&[(7.0, 11.5), (12.0, 16.5), (17.0, 11.5)], w2, false);
            line(5.0, 20.5, 19.0, 20.5, w2);
        }
        Icon::Search => {
            circle(11.0, 11.0, 7.0, w2);
            line(16.2, 16.2, 21.0, 21.0, w2);
        }
        Icon::Fullscreen | Icon::Unfullscreen => {
            if icon == Icon::Fullscreen {
                poly(&[(14.5, 3.5), (20.5, 3.5), (20.5, 9.5)], w2, false);
                line(20.5, 3.5, 14.0, 10.0, w2);
                poly(&[(9.5, 20.5), (3.5, 20.5), (3.5, 14.5)], w2, false);
                line(3.5, 20.5, 10.0, 14.0, w2);
            } else {
                poly(&[(10.5, 3.5), (10.5, 9.5), (4.5, 9.5)], w2, false);
                poly(&[(13.5, 20.5), (13.5, 14.5), (19.5, 14.5)], w2, false);
                line(10.5, 9.5, 3.5, 3.5, w2);
                line(13.5, 14.5, 20.5, 20.5, w2);
            }
        }
    }
}

/// 便捷方法：在 ui 上留出 size×size 的空间并绘制图标，返回右下角位置。
pub fn show(ui: &mut eframe::egui::Ui, icon: Icon, size: f32, color: Color32, bg: Color32) -> Pos2 {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let inner = Rect::from_min_size(rect.left_top(), Vec2::splat(size));
    paint(ui.painter(), icon, inner, color, bg);
    rect.right_bottom()
}
