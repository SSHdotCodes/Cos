//! SF Symbols replacements: small stroke-based icons on a 24×24 grid, painted
//! through gpui's PathBuilder so they tint and scale like the SwiftUI glyphs.

use gpui::{
    canvas, point, px, Bounds, Canvas, Hsla, Path, PathBuilder, Pixels, Point, Radians, Size,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    SquarePencil,
    UpdateAvailable,
    Xmark,
    XmarkSquare,
    Folder,
    FolderQuestion,
    Shield,
    ChevronRight,
    ChevronDown,
    ArrowUp,
    Stop,
    Bolt,
    Scope,
    Globe,
    SidebarRight,
    Box,
    Gear,
    WandStars,
    Ellipsis,
    Search,
    Mic,
    Waveform,
    Bubble,
    Wrench,
    People,
    Checkmark,
    CheckmarkCircle,
    CheckmarkShield,
    ExclamationShield,
    Refresh,
    Display,
    Trash,
    Pause,
    Play,
    DocOnDoc,
    Import,
    LockShield,
    Sliders,
    Sparkles,
    Person,
    Terminal,
    ArrowUpRight,
    HandRaised,
    Warning,
    Wifi,
    Clock,
    Code,
    Storefront,
    Chat,
    Question,
}

type P = (f32, f32);

fn circle(cx: f32, cy: f32, r: f32) -> Vec<P> {
    let segments = 20;
    (0..=segments)
        .map(|i| {
            let angle = (i as f32) / (segments as f32) * std::f32::consts::TAU;
            (cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

fn arc(cx: f32, cy: f32, r: f32, start_deg: f32, end_deg: f32) -> Vec<P> {
    let segments = 10;
    (0..=segments)
        .map(|i| {
            let deg = start_deg + (end_deg - start_deg) * (i as f32) / (segments as f32);
            let angle = deg * std::f32::consts::PI / 180.0;
            (cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<P> {
    let mut points = Vec::new();
    points.extend(arc(x + w - r, y + r, r, -90.0, 0.0));
    points.extend(arc(x + w - r, y + h - r, r, 0.0, 90.0));
    points.extend(arc(x + r, y + h - r, r, 90.0, 180.0));
    points.extend(arc(x + r, y + r, r, 180.0, 270.0));
    points.push(points[0]);
    points
}

fn star(cx: f32, cy: f32, r: f32) -> Vec<P> {
    // Four-point sparkle like SF Symbols' sparkles.
    vec![
        (cx, cy - r),
        (cx + r * 0.28, cy - r * 0.28),
        (cx + r, cy),
        (cx + r * 0.28, cy + r * 0.28),
        (cx, cy + r),
        (cx - r * 0.28, cy + r * 0.28),
        (cx - r, cy),
        (cx - r * 0.28, cy - r * 0.28),
        (cx, cy - r),
    ]
}

fn shield(cx: f32, cy: f32, r: f32) -> Vec<P> {
    vec![
        (cx, cy - r),
        (cx + r * 0.85, cy - r * 0.55),
        (cx + r * 0.85, cy + r * 0.1),
        (cx + r * 0.5, cy + r * 0.62),
        (cx, cy + r),
        (cx - r * 0.5, cy + r * 0.62),
        (cx - r * 0.85, cy + r * 0.1),
        (cx - r * 0.85, cy - r * 0.55),
        (cx, cy - r),
    ]
}

fn strokes(icon: Icon) -> (Vec<Vec<P>>, Vec<Vec<P>>) {
    // (strokes, fills)
    match icon {
        Icon::Plus => (vec![vec![(12.0, 5.0), (12.0, 19.0)], vec![(5.0, 12.0), (19.0, 12.0)]], vec![]),
        Icon::SquarePencil => (
            vec![
                vec![(13.5, 5.5), (6.5, 5.5), (5.5, 6.5), (5.5, 17.5), (6.5, 18.5), (17.5, 18.5), (18.5, 17.5), (18.5, 10.5)],
                vec![(14.0, 15.5), (19.0, 10.5), (16.5, 8.0), (11.5, 13.0), (11.0, 16.0)],
                vec![(16.5, 8.0), (19.0, 10.5)],
            ],
            vec![],
        ),
        Icon::UpdateAvailable => (
            vec![circle(12.0, 12.0, 8.5), vec![(12.0, 7.5), (12.0, 14.5)], vec![(8.5, 11.5), (12.0, 15.0), (15.5, 11.5)]],
            vec![],
        ),
        Icon::Xmark => (vec![vec![(6.0, 6.0), (18.0, 18.0)], vec![(18.0, 6.0), (6.0, 18.0)]], vec![]),
        Icon::XmarkSquare => (
            vec![rounded_rect(5.0, 5.0, 14.0, 14.0, 3.0), vec![(9.0, 9.0), (15.0, 15.0)], vec![(15.0, 9.0), (9.0, 15.0)]],
            vec![],
        ),
        Icon::Folder | Icon::FolderQuestion => {
            let mut strokes = vec![vec![
                (3.5, 6.5), (3.5, 17.5), (4.5, 18.5), (19.5, 18.5), (20.5, 17.5), (20.5, 8.5),
                (19.5, 7.5), (12.5, 7.5), (10.5, 5.5), (4.5, 5.5), (3.5, 6.5),
            ]];
            if icon == Icon::FolderQuestion {
                strokes.push(vec![(11.0, 11.0), (11.6, 10.2), (12.4, 10.2), (13.0, 11.0), (12.0, 12.4), (12.0, 13.2)]);
                strokes.push(vec![(12.0, 15.4), (12.0, 15.5)]);
            }
            (strokes, vec![])
        }
        Icon::Shield => (
            vec![shield(12.0, 12.0, 8.0), vec![(12.0, 4.0), (12.0, 20.0)], arc(12.0, 12.0, 8.0, -30.0, 90.0)],
            vec![],
        ),
        Icon::ChevronRight => (vec![vec![(9.0, 5.5), (15.5, 12.0), (9.0, 18.5)]], vec![]),
        Icon::ChevronDown => (vec![vec![(5.5, 9.0), (12.0, 15.5), (18.5, 9.0)]], vec![]),
        Icon::ArrowUp => (
            vec![vec![(12.0, 18.5), (12.0, 5.5)], vec![(6.0, 11.5), (12.0, 5.5), (18.0, 11.5)]],
            vec![],
        ),
        Icon::Stop => (vec![], vec![rounded_rect(6.5, 6.5, 11.0, 11.0, 2.5)]),
        Icon::Bolt => (
            vec![],
            vec![vec![
                (13.6, 3.0), (6.5, 13.5), (11.3, 13.5), (10.4, 21.0), (17.5, 10.5), (12.7, 10.5),
                (13.6, 3.0),
            ]],
        ),
        Icon::Scope => (
            vec![
                circle(12.0, 12.0, 7.5),
                vec![(12.0, 1.5), (12.0, 5.5)],
                vec![(12.0, 18.5), (12.0, 22.5)],
                vec![(1.5, 12.0), (5.5, 12.0)],
                vec![(18.5, 12.0), (22.5, 12.0)],
            ],
            vec![circle(12.0, 12.0, 1.6)],
        ),
        Icon::Globe => (
            vec![
                circle(12.0, 12.0, 8.5),
                vec![(3.5, 12.0), (20.5, 12.0)],
                arc(12.0, 12.0, 4.5, -90.0, 90.0),
                arc(12.0, 12.0, 4.5, 90.0, 270.0),
            ],
            vec![],
        ),
        Icon::SidebarRight => (
            vec![
                rounded_rect(3.5, 5.0, 17.0, 14.0, 2.5),
                vec![(14.5, 5.0), (14.5, 19.0)],
                vec![(17.0, 9.5), (18.5, 9.5)],
                vec![(17.0, 12.0), (18.5, 12.0)],
                vec![(17.0, 14.5), (18.5, 14.5)],
            ],
            vec![],
        ),
        Icon::Box => (
            vec![
                vec![
                    (4.0, 7.5), (12.0, 3.5), (20.0, 7.5), (20.0, 16.5), (12.0, 20.5), (4.0, 16.5),
                    (4.0, 7.5),
                ],
                vec![(4.0, 7.5), (12.0, 11.5), (20.0, 7.5)],
                vec![(12.0, 11.5), (12.0, 20.5)],
            ],
            vec![],
        ),
        Icon::Gear => (
            vec![
                circle(12.0, 12.0, 3.2),
                circle(12.0, 12.0, 7.0),
                vec![(12.0, 2.5), (12.0, 5.0)],
                vec![(12.0, 19.0), (12.0, 21.5)],
                vec![(2.5, 12.0), (5.0, 12.0)],
                vec![(19.0, 12.0), (21.5, 12.0)],
                vec![(5.2, 5.2), (7.0, 7.0)],
                vec![(17.0, 17.0), (18.8, 18.8)],
                vec![(18.8, 5.2), (17.0, 7.0)],
                vec![(7.0, 17.0), (5.2, 18.8)],
            ],
            vec![],
        ),
        Icon::WandStars => (
            vec![
                vec![(3.5, 20.5), (13.5, 10.5)],
                star(16.5, 6.5, 3.4),
                star(20.0, 13.0, 2.0),
            ],
            vec![],
        ),
        Icon::Ellipsis => (
            vec![],
            vec![circle(5.5, 12.0, 1.5), circle(12.0, 12.0, 1.5), circle(18.5, 12.0, 1.5)],
        ),
        Icon::Search => (vec![circle(10.5, 10.5, 6.0), vec![(15.0, 15.0), (20.5, 20.5)]], vec![]),
        Icon::Mic => (
            vec![
                rounded_rect(9.0, 3.5, 6.0, 11.0, 3.0),
                arc(12.0, 14.5, 6.0, 0.0, 180.0),
                vec![(12.0, 20.5), (12.0, 22.5)],
            ],
            vec![],
        ),
        Icon::Waveform => (
            vec![vec![
                (3.0, 12.0), (6.0, 12.0), (8.0, 5.0), (11.0, 19.0), (13.5, 8.0), (16.0, 15.0),
                (18.0, 12.0), (21.0, 12.0),
            ]],
            vec![],
        ),
        Icon::Bubble => (
            vec![{
                let mut bubble = arc(12.0, 11.0, 8.0, 160.0, -160.0);
                bubble.extend([(6.5, 19.5), (7.8, 15.8)]);
                bubble
            }],
            vec![],
        ),
        Icon::Wrench => (
            vec![{
                let mut jaw = arc(16.0, 8.0, 4.5, 100.0, 320.0);
                jaw.extend([(12.5, 11.5), (4.5, 19.5), (6.5, 21.0), (8.0, 19.5), (14.5, 13.0)]);
                jaw
            }],
            vec![],
        ),
        Icon::People => (
            vec![
                circle(9.0, 8.0, 3.5),
                arc(9.0, 21.0, 6.0, 200.0, 340.0),
                circle(16.5, 9.5, 2.8),
                arc(16.5, 20.5, 4.5, 210.0, 330.0),
            ],
            vec![],
        ),
        Icon::Checkmark => (vec![vec![(5.0, 12.5), (10.0, 17.5), (19.0, 6.5)]], vec![]),
        Icon::CheckmarkCircle => (
            vec![circle(12.0, 12.0, 8.5), vec![(8.0, 12.5), (11.0, 15.5), (16.5, 8.5)]],
            vec![],
        ),
        Icon::CheckmarkShield => (
            vec![shield(12.0, 12.0, 8.0), vec![(8.5, 12.0), (11.0, 14.5), (15.5, 9.0)]],
            vec![],
        ),
        Icon::ExclamationShield => (
            vec![shield(12.0, 12.0, 8.0), vec![(12.0, 8.0), (12.0, 13.5)], vec![(12.0, 16.2), (12.0, 16.3)]],
            vec![],
        ),
        Icon::Refresh => (
            vec![arc(12.0, 12.0, 7.0, 40.0, 350.0), vec![(16.5, 2.5), (19.2, 5.3), (16.4, 8.0)]],
            vec![],
        ),
        Icon::Display => (
            vec![
                rounded_rect(3.0, 4.5, 18.0, 12.5, 2.0),
                vec![(9.0, 20.5), (15.0, 20.5)],
                vec![(12.0, 17.0), (12.0, 20.5)],
            ],
            vec![],
        ),
        Icon::Trash => (
            vec![
                vec![(5.0, 7.0), (19.0, 7.0)],
                vec![(10.0, 7.0), (10.0, 5.0), (14.0, 5.0), (14.0, 7.0)],
                vec![(6.5, 7.0), (7.5, 19.5), (16.5, 19.5), (17.5, 7.0)],
                vec![(10.0, 10.0), (10.0, 16.5)],
                vec![(14.0, 10.0), (14.0, 16.5)],
            ],
            vec![],
        ),
        Icon::Pause => (vec![circle(12.0, 12.0, 8.5), vec![(9.5, 8.5), (9.5, 15.5)], vec![(14.5, 8.5), (14.5, 15.5)]], vec![]),
        Icon::Play => (vec![circle(12.0, 12.0, 8.5), vec![(9.5, 8.0), (16.5, 12.0), (9.5, 16.0), (9.5, 8.0)]], vec![]),
        Icon::DocOnDoc => (
            vec![
                rounded_rect(7.0, 7.0, 12.0, 14.0, 2.0),
                vec![(5.0, 17.0), (5.0, 3.0), (15.0, 3.0)],
            ],
            vec![],
        ),
        Icon::Import => (
            vec![
                vec![(12.0, 3.5), (12.0, 14.5)],
                vec![(7.5, 10.0), (12.0, 14.5), (16.5, 10.0)],
                vec![(4.5, 17.5), (4.5, 20.5), (19.5, 20.5), (19.5, 17.5)],
            ],
            vec![],
        ),
        Icon::LockShield => (
            vec![shield(12.0, 12.5, 8.0), rounded_rect(9.0, 9.5, 6.0, 5.5, 1.2), arc(12.0, 9.5, 2.6, 180.0, 360.0)],
            vec![],
        ),
        Icon::Sliders => (
            vec![
                vec![(4.0, 7.0), (20.0, 7.0)],
                vec![(4.0, 12.0), (20.0, 12.0)],
                vec![(4.0, 17.0), (20.0, 17.0)],
            ],
            vec![circle(15.0, 7.0, 2.0), circle(8.5, 12.0, 2.0), circle(13.0, 17.0, 2.0)],
        ),
        Icon::Sparkles => (vec![star(9.5, 10.0, 6.0), star(17.5, 16.0, 3.2)], vec![]),
        Icon::Person => (vec![circle(12.0, 8.0, 4.5), arc(12.0, 21.0, 7.0, 200.0, 340.0)], vec![]),
        Icon::Terminal => (
            vec![
                rounded_rect(3.0, 4.5, 18.0, 15.0, 2.5),
                vec![(6.5, 8.5), (10.5, 12.0), (6.5, 15.5)],
                vec![(12.5, 15.5), (17.5, 15.5)],
            ],
            vec![],
        ),
        Icon::ArrowUpRight => (
            vec![vec![(6.0, 18.0), (18.0, 6.0)], vec![(9.0, 6.0), (18.0, 6.0), (18.0, 15.0)]],
            vec![],
        ),
        Icon::HandRaised => (
            vec![{
                let mut palm = arc(12.0, 14.0, 5.0, 170.0, 350.0);
                palm.extend([(7.5, 18.5), (16.5, 18.5)]);
                palm
            }],
            vec![],
        ),
        Icon::Warning => (
            vec![
                vec![(12.0, 4.0), (21.0, 19.5), (3.0, 19.5), (12.0, 4.0)],
                vec![(12.0, 9.5), (12.0, 14.5)],
                vec![(12.0, 17.0), (12.0, 17.1)],
            ],
            vec![],
        ),
        Icon::Wifi => (
            vec![
                arc(12.0, 16.5, 9.5, 220.0, 320.0),
                arc(12.0, 16.5, 5.5, 220.0, 320.0),
            ],
            vec![circle(12.0, 17.0, 1.6)],
        ),
        Icon::Clock => (vec![circle(12.0, 12.0, 8.5), vec![(12.0, 6.5), (12.0, 12.0), (15.5, 13.5)]], vec![]),
        Icon::Code => (
            vec![
                vec![(8.5, 7.0), (3.5, 12.0), (8.5, 17.0)],
                vec![(15.5, 7.0), (20.5, 12.0), (15.5, 17.0)],
                vec![(13.5, 5.0), (10.5, 19.0)],
            ],
            vec![],
        ),
        Icon::Storefront => (
            vec![
                vec![(5.0, 9.5), (4.5, 5.0), (19.5, 5.0), (19.0, 9.5)],
                arc(7.25, 9.5, 2.25, 0.0, 180.0),
                arc(12.0, 9.5, 2.5, 0.0, 180.0),
                arc(16.75, 9.5, 2.25, 0.0, 180.0),
                vec![(6.0, 13.0), (6.0, 19.5), (18.0, 19.5), (18.0, 13.0)],
            ],
            vec![],
        ),
        Icon::Chat => (
            vec![
                {
                    let mut bubble = arc(9.0, 10.0, 6.0, 150.0, -140.0);
                    bubble.extend([(5.0, 16.5), (6.0, 13.6)]);
                    bubble
                },
                {
                    let mut bubble = arc(15.0, 14.0, 5.0, 30.0, 320.0);
                    bubble.extend([(18.5, 19.5), (17.6, 16.4)]);
                    bubble
                },
            ],
            vec![],
        ),
        Icon::Question => (
            vec![
                circle(12.0, 12.0, 8.5),
                vec![(10.0, 9.5), (10.6, 8.2), (12.0, 7.8), (13.4, 8.2), (14.0, 9.5), (12.0, 11.6), (12.0, 13.0)],
                vec![(12.0, 16.0), (12.0, 16.1)],
            ],
            vec![],
        ),
    }
}

/// Paint an icon inside the current element bounds.
pub fn icon(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    size: Pixels,
    color: Hsla,
) -> impl gpui::IntoElement {
    let (lines, fills) = strokes(icon);
    let inner = canvas(
        move |_, _, _| (lines, fills),
        move |bounds: Bounds<Pixels>, (lines, fills), window, _| {
            let side = bounds.size.width.min(bounds.size.height);
            let origin = point(
                bounds.origin.x + (bounds.size.width - side) / 2.0,
                bounds.origin.y + (bounds.size.height - side) / 2.0,
            );
            let scale = f32::from(side) / 24.0;
            let map_point = |(x, y): P| point(origin.x + px(x * scale), origin.y + px(y * scale));
            for fill in fills {
                let mut builder = PathBuilder::fill();
                let mut iter = fill.into_iter();
                if let Some(first) = iter.next() {
                    builder.move_to(map_point(first));
                    for point in iter {
                        builder.line_to(map_point(point));
                    }
                    builder.close();
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
                }
            }
            let stroke_width = (size * 0.075).max(px(0.8));
            for line in lines {
                let mut builder = PathBuilder::stroke(stroke_width);
                let mut iter = line.into_iter();
                if let Some(first) = iter.next() {
                    builder.move_to(map_point(first));
                    for point in iter {
                        builder.line_to(map_point(point));
                    }
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .w(size)
    .h(size);
    use gpui::prelude::*;
    gpui::div().id(id).size(size).flex_none().child(inner)
}

/// The Cos wordmark: rounded black tile with "cos θ" in serif, matching the
/// SwiftUI `CosMark`.
pub fn cos_mark(compact: bool, _theme: crate::theme::Theme) -> impl gpui::IntoElement {
    use gpui::prelude::*;
    let (w, h, radius, font_size) = if compact {
        (px(38.0), px(28.0), px(7.0), px(9.5))
    } else {
        (px(46.0), px(32.0), px(9.0), px(12.0))
    };
    gpui::div()
        .id("cos-mark")
        .w(w)
        .h(h)
        .flex_none()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 1.0))
        .rounded(radius)
        .border_1()
        .border_color(gpui::hsla(0.0, 0.0, 1.0, 0.14))
        .flex()
        .items_center()
        .justify_center()
        .child(
            gpui::div()
                .text_color(gpui::hsla(0.0, 0.0, 1.0, 1.0))
                .text_size(font_size)
                .font_family("New York")
                .child("cos θ"),
        )
}

#[allow(unused)]
fn _unused(_: Size<Pixels>, _: Point<Pixels>, _: Radians, _: Path<Pixels>) {}
