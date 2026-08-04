//! Chat header + transcript: messages, work trace, empty state, thinking dots.

use crate::icons::{cos_mark, icon, Icon};
use crate::markdown::render_markdown;
use crate::state::AppModel;
use crate::theme::Theme;
use cos_core::{ChatMessage, GoalStatus, MessageRole, WorkTraceKind};
use gpui::{prelude::FluentBuilder, *};
use std::collections::HashMap;
use uuid::Uuid;

pub fn header(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> Div {
    let model_read = model.read(cx);
    let title = model_read
        .selected_thread()
        .map(|thread| thread.title.clone())
        .unwrap_or_else(|| "Cos".into());
    let path = model_read
        .selected_thread()
        .map(|thread| thread.workspace_path.clone())
        .map(|path| {
            let home = cos_core::dirs_home().to_string_lossy().into_owned();
            path.replace(&home, "~")
        });
    let goal = model_read.selected_thread().and_then(|thread| thread.goal.clone());
    let browser_enabled = model_read.is_betterwright_enabled();
    let browser_open = model_read.is_browser_panel_presented;

    div()
        .h(px(52.0))
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(title),
                )
                .when_some(path, |column, path| {
                    column.child(
                        div()
                            .text_size(px(10.5))
                            .text_color(theme.secondary)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(path),
                    )
                }),
        )
        .child(div().flex_1())
        .when_some(goal, |row, goal| {
            row.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5.0))
                    .child(icon("goal-icon", Icon::Scope, px(11.0), crate::theme::BLUE))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(crate::theme::BLUE)
                            .child(if goal.status == GoalStatus::Active {
                                "Goal active".to_string()
                            } else {
                                crate::state::title_case(goal.status.raw_value())
                            }),
                    )
                    .id("goal-label")
                    .tooltip(move |_, cx| {
                        cx.new(|_| TooltipText(goal.objective.clone())).into()
                    }),
            )
        })
        .child({
            let model = model.clone();
            div()
                .id("choose-workspace")
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme.fill_075))
                .child(icon("folder", Icon::Folder, px(14.0), theme.secondary))
                .on_click(move |_, _, cx| crate::views::choose_workspace(model.clone(), cx))
        })
        .when(browser_enabled, |row| {
            let model = model.clone();
            row.child(
                div()
                    .id("toggle-browser")
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .when(browser_open, |button| button.bg(theme.fill_10))
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon("sidebar-right", Icon::SidebarRight, px(13.0), theme.secondary))
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| {
                            model.is_browser_panel_presented = !model.is_browser_panel_presented;
                            cx.notify();
                        });
                    }),
            )
        })
}

pub fn transcript(
    model: &Entity<AppModel>,
    theme: &Theme,
    scroll: ScrollHandle,
    work_expanded: &mut HashMap<Uuid, bool>,
    cx: &mut App,
) -> AnyElement {
    let model_read = model.read(cx);
    let Some(thread) = model_read.selected_thread().cloned() else {
        return div().flex_1().child(empty_hint("No task selected", theme)).into_any_element();
    };
    if thread.messages.is_empty() {
        return div().flex_1().child(empty_task(model, theme)).into_any_element();
    }

    let mut content = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .max_w(px(760.0))
        .w_full()
        .mx_auto()
        .px(px(28.0))
        .py(px(18.0));
    for message in &thread.messages {
        let expanded = work_expanded
            .get(&message.id)
            .copied()
            .unwrap_or(message.is_streaming && message.work_items.as_ref().map(|items| !items.is_empty()).unwrap_or(false));
        work_expanded.insert(message.id, expanded);
        content = content.child(message_view(message, expanded, theme));
    }

    div()
        .id("transcript")
        .flex_1()
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .child(content)
        .into_any_element()
}

fn message_view(message: &ChatMessage, work_expanded: bool, theme: &Theme) -> AnyElement {
    let is_assistant = message.role == MessageRole::Assistant;
    let message_id = message.id;

    let mut bubble = div().flex().flex_col().gap(px(7.0));
    if is_assistant {
        bubble = bubble
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Cos"),
                    )
                    .when(message.is_streaming, |row| {
                        row.child(spinner(theme))
                    }),
            );
        if let Some(items) = &message.work_items {
            if !items.is_empty() {
                bubble = bubble.child(work_trace(items, work_expanded, message.is_streaming, message_id, theme));
            }
        }
        if message.content.is_empty()
            && message.is_streaming
            && message.work_items.as_ref().map(|items| items.is_empty()).unwrap_or(true)
        {
            bubble = bubble.child(thinking_indicator(theme));
        } else if !message.content.is_empty() {
            bubble = bubble.child(render_markdown(&message.content, &theme_colors(theme), 13.2, 18.7));
        }
    } else {
        bubble = bubble.child(
            div()
                .px(px(13.0))
                .py(px(9.0))
                .rounded(px(13.0))
                .bg(theme.fill_075)
                .text_size(px(13.0))
                .line_height(px(18.0))
                .whitespace_normal()
                .child(message.content.clone()),
        );
    }

    if is_assistant {
        div()
            .w_full()
            .flex()
            .flex_row()
            .gap(px(11.0))
            .child(cos_mark(true, *theme))
            .child(div().flex_1().child(bubble))
            .child(div().w(px(30.0)).flex_none())
            .into_any_element()
    } else {
        div()
            .w_full()
            .flex()
            .flex_row()
            .child(div().w(px(70.0)).flex_none())
            .child(div().flex_1().flex().justify_end().child(bubble))
            .into_any_element()
    }
}

pub(crate) fn theme_colors(theme: &Theme) -> crate::markdown::CosColors {
    crate::markdown::CosColors {
        text: theme.primary,
        text_muted: theme.secondary,
        accent: crate::theme::BLUE,
        surface_raised: theme.fill_045,
        surface_border: theme.divider,
    }
}

fn work_trace(
    items: &[cos_core::WorkTraceItem],
    expanded: bool,
    running: bool,
    message_id: Uuid,
    theme: &Theme,
) -> AnyElement {
    let count = items.len();
    let mut header = div()
        .id(ElementId::Uuid(message_id))
        .h(px(28.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(12.0))
                .flex()
                .justify_center()
                .child(icon(
                    ElementId::Uuid(message_id),
                    if expanded { Icon::ChevronDown } else { Icon::ChevronRight },
                    px(9.0),
                    theme.secondary,
                )),
        )
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.secondary)
                .child(if running { "Cos is working" } else { "Work" }),
        )
        .child(
            div()
                .px(px(5.0))
                .py(px(2.0))
                .rounded(px(8.0))
                .bg(theme.fill_06)
                .text_size(px(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.secondary)
                .child(format!("{count}")),
        )
        .child(div().flex_1())
        .when(running, |row| row.child(spinner(theme)));

    let mut container = div().flex().flex_col().w_full().child(header);
    if expanded {
        let mut list = div()
            .flex()
            .flex_col()
            .px(px(9.0))
            .py(px(8.0))
            .rounded(px(9.0))
            .bg(theme.fill_032);
        for (index, item) in items.iter().enumerate() {
            let color = match item.kind {
                WorkTraceKind::Status => crate::theme::BLUE,
                WorkTraceKind::Reasoning => theme.secondary,
                WorkTraceKind::Tool => crate::theme::ORANGE,
                WorkTraceKind::Subagent => crate::theme::INDIGO,
            };
            let icon_kind = match item.kind {
                WorkTraceKind::Status => Icon::Waveform,
                WorkTraceKind::Reasoning => Icon::Bubble,
                WorkTraceKind::Tool => Icon::Wrench,
                WorkTraceKind::Subagent => Icon::People,
            };
            let detail = item.detail.trim().to_string();
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(9.0))
                    .pb(px(if index + 1 < count { 7.0 } else { 2.0 }))
                    .child(
                        div()
                            .w(px(20.0))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .w(px(20.0))
                                    .h(px(20.0))
                                    .rounded_full()
                                    .bg(color.opacity(0.14))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(ElementId::Integer(index as u64), icon_kind, px(9.0), color)),
                            )
                            .when(index + 1 < count, |column| {
                                column.child(div().w(px(1.0)).h(px(16.0)).bg(theme.fill_10))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(item.title.clone()),
                            )
                            .when(!detail.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(theme.secondary)
                                        .whitespace_normal()
                                        .child(detail),
                                )
                            }),
                    ),
            );
        }
        container = container.child(list);
    }
    container.into_any_element()
}

fn empty_hint(text: &str, theme: &Theme) -> Div {
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .child(div().text_size(px(13.0)).text_color(theme.secondary).child(text.to_string()))
}

fn empty_task(model: &Entity<AppModel>, theme: &Theme) -> Div {
    let suggestions = [
        ("Inspect this project", "Find the architecture, risks, and the best first improvement."),
        ("Build a feature", "Implement the next high-impact feature and verify it."),
        ("Fix a bug", "Reproduce the current failure, find its cause, and fix it."),
    ];
    let mut cards = div().flex().flex_row().gap(px(9.0)).max_w(px(650.0)).w_full();
    for (title, prompt) in suggestions {
        let model = model.clone();
        cards = cards.child(
            div()
                .id(ElementId::Name(title.into()))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(12.0))
                .bg(theme.fill_045)
                .cursor_pointer()
                .hover(|style| style.bg(theme.fill_06))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(theme.secondary)
                        .line_height(px(14.0))
                        .child(prompt),
                )
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| model.send(prompt, cx));
                }),
        );
    }

    div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(22.0))
        .p(px(30.0))
        .child(cos_mark(false, *theme))
        .child(
            div()
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("What should Cos work on?"),
        )
        .child(cards)
}

fn thinking_indicator(theme: &Theme) -> Div {
    let theme = *theme;
    div()
        .flex()
        .flex_row()
        .gap(px(4.0))
        .py(px(8.0))
        .children((0..3).map(|index| {
            div()
                .w(px(4.0))
                .h(px(4.0))
                .rounded_full()
                .bg(theme.secondary.opacity(if index == 1 { 0.9 } else { 0.5 }))
                .with_animation(
                    ElementId::Integer(index),
                    Animation::new(std::time::Duration::from_millis(700))
                        .repeat()
                        .with_easing(gpui::bounce(ease_in_out)),
                    move |dot, delta| {
                        let opacity = if index == 1 {
                            0.4 + 0.6 * (1.0 - delta)
                        } else {
                            0.4 + 0.6 * delta
                        };
                        dot.bg(theme.secondary.opacity(opacity as f32))
                    },
                )
        }))
}

pub fn spinner(theme: &Theme) -> AnyElement {
    let theme = *theme;
    fn arc_element(theme: Theme, start: f32) -> impl IntoElement {
        canvas(
            move |_, _, _| (),
            move |bounds, _, window, _| {
                let center = bounds.center();
                let radius = bounds.size.width.min(bounds.size.height) / 2.0 - px(1.0);
                let mut builder = PathBuilder::stroke(px(1.4));
                for index in 0..=16 {
                    let angle = start + (index as f32 / 16.0) * std::f32::consts::PI * 1.5;
                    let p = point(center.x + radius * angle.cos(), center.y + radius * angle.sin());
                    if index == 0 {
                        builder.move_to(p);
                    } else {
                        builder.line_to(p);
                    }
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, theme.secondary);
                }
            },
        )
        .size(px(10.0))
        .flex_none()
    }
    arc_element(theme, 0.0)
        .with_animation(
            "cos-spinner",
            Animation::new(std::time::Duration::from_millis(900)).repeat(),
            move |_, delta| arc_element(theme, delta * std::f32::consts::TAU),
        )
        .into_any_element()
}

struct TooltipText(String);

impl Render for TooltipText {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .max_w(px(320.0))
            .rounded(px(6.0))
            .bg(gpui::hsla(0.0, 0.0, 0.12, 0.96))
            .text_color(gpui::white())
            .text_size(px(11.0))
            .whitespace_normal()
            .child(self.0.clone())
    }
}
