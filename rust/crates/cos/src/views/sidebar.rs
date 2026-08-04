//! Sidebar: Cos mark, update button, New task, thread list, bottom rows.

use crate::icons::{cos_mark, icon, Icon};
use crate::state::AppModel;
use crate::theme::Theme;
use cos_core::GoalStatus;
use gpui::{prelude::FluentBuilder, *};
use uuid::Uuid;

pub fn sidebar(model: &Entity<AppModel>, theme: &Theme, cx: &mut Context<crate::views::MainView>) -> Div {
    let model_read = model.read(cx);
    let threads = model_read.threads.clone();
    let selected = model_read.selected_thread_id;
    let update = model_read.available_update.clone();
    let installing = model_read.is_installing_update;
    let true_dark = theme.true_dark;

    let mut list = div().id("thread-list").flex_1().flex().flex_col().overflow_y_scroll().px(px(8.0));
    list = list.child(
        div()
            .px(px(8.0))
            .pt(px(2.0))
            .pb(px(6.0))
            .text_size(px(10.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.tertiary)
            .child("Tasks"),
    );
    for thread in &threads {
        list = list.child(thread_row(model, thread.id, selected == Some(thread.id), theme, cx));
    }

    div()
        .w(px(crate::theme::SIDEBAR_WIDTH))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .bg(theme.sidebar_background)
        .border_r_1()
        .border_color(theme.divider.opacity(if true_dark { 0.5 } else { 0.8 }))
        .child(
            // Top row: Cos mark + optional update button + new-task button
            div()
                .flex()
                .flex_row()
                .items_center()
                .px(px(13.0))
                .pt(px(12.0))
                .pb(px(10.0))
                .child(cos_mark(false, *theme))
                .child(div().flex_1())
                .when_some(update, |row, update| {
                    let model = model.clone();
                    let version = update.version.clone();
                    row.child(
                        div()
                            .id("install-update")
                            .w(px(24.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .hover(|style| style.bg(theme.fill_075))
                            .child(icon("update-icon", Icon::UpdateAvailable, px(15.0), crate::theme::BLUE))
                            .on_click(move |_, _, cx| {
                                if !installing {
                                    model.update(cx, |model, cx| model.install_available_update(cx));
                                }
                            })
                            .tooltip(move |_, cx| {
                                cx.new(|_| SimpleTooltip(format!("Install Cos {version} and restart")))
                                    .into()
                            }),
                    )
                })
                .child({
                    let model = model.clone();
                    div()
                        .id("new-task-top")
                        .w(px(24.0))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded(px(6.0))
                        .hover(|style| style.bg(theme.fill_075))
                        .child(icon("new-task-icon", Icon::SquarePencil, px(13.0), theme.secondary))
                        .on_click(move |_, _, cx| {
                            model.update(cx, |model, cx| model.new_thread(cx));
                        })
                }),
        )
        .child(
            // New task button
            div().px(px(10.0)).pb(px(9.0)).child({
                let model = model.clone();
                div()
                    .id("new-task")
                    .h(px(34.0))
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(11.0))
                    .rounded(px(9.0))
                    .bg(theme.fill_075)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_10))
                    .child(icon("new-task-plus", Icon::Plus, px(12.0), theme.secondary))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("New task"),
                    )
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| model.new_thread(cx));
                    })
            }),
        )
        .child(list)
        .child(
            // Bottom rows
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(8.0))
                .py(px(8.0))
                .border_t_1()
                .border_color(theme.divider.opacity(0.55))
                .child(bottom_row(
                    "open-plugins",
                    "Plugins & skills",
                    Icon::Box,
                    theme,
                    {
                        let model = model.clone();
                        move |cx| {
                            model.update(cx, |model, cx| {
                                model.is_plugin_library_presented = true;
                                model.load_marketplace(false, cx);
                                cx.notify();
                            });
                        }
                    },
                ))
                .child(bottom_row("open-settings", "Settings", Icon::Gear, theme, {
                    let model = model.clone();
                    move |cx| open_settings_from_sidebar(model.clone(), cx)
                })),
        )
}

fn open_settings_from_sidebar(model: Entity<AppModel>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(780.0), px(620.0)), cx);
    let model_for_window = model.clone();
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            cx.new(|cx| crate::views::SettingsView::new(model_for_window, window, cx))
        },
    )
    .ok();
}

fn thread_row(
    model: &Entity<AppModel>,
    thread_id: Uuid,
    selected: bool,
    theme: &Theme,
    cx: &mut Context<crate::views::MainView>,
) -> impl IntoElement {
    let (title, workspace, goal_active) = {
        let model_read = model.read(cx);
        let thread = model_read.threads.iter().find(|thread| thread.id == thread_id);
        (
            thread.map(|thread| thread.title.clone()).unwrap_or_default(),
            thread
                .map(|thread| {
                    std::path::Path::new(&thread.workspace_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default(),
            thread
                .and_then(|thread| thread.goal.as_ref())
                .map(|goal| goal.status == GoalStatus::Active)
                .unwrap_or(false),
        )
    };
    let model_select = model.clone();
    let model_delete = model.clone();
    div()
        .id(ElementId::Uuid(thread_id))
        .mx(px(2.0))
        .px(px(8.0))
        .py(px(6.0))
        .mb(px(1.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.fill_10))
        .when(!selected, |row| row.hover(|style| style.bg(theme.fill_045)))
        .group("thread-row")
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .child(icon(
            ElementId::Uuid(thread_id),
            if goal_active { Icon::Scope } else { Icon::Bubble },
            px(12.0),
            if goal_active { crate::theme::BLUE } else { theme.secondary },
        ))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .overflow_hidden()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(theme.secondary)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(workspace),
                ),
        )
        .child(
            div()
                .id(ElementId::Uuid(thread_id))
                .invisible()
                .group_hover("thread-row", |style| style.visible())
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .hover(|style| style.bg(theme.fill_10))
                .child(icon(ElementId::Uuid(thread_id), Icon::Xmark, px(9.0), theme.secondary))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    model_delete.update(cx, |model, cx| model.delete_thread(thread_id, cx));
                }),
        )
        .on_click(move |_, _, cx| {
            model_select.update(cx, |model, cx| model.select_thread(thread_id, cx));
        })
}

fn bottom_row(
    id: &'static str,
    label: &'static str,
    icon_kind: Icon,
    theme: &Theme,
    action: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(30.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .px(px(10.0))
        .rounded(px(7.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme.fill_06))
        .child(icon(id, icon_kind, px(13.0), theme.secondary))
        .child(div().text_size(px(12.5)).child(label))
        .on_click(move |_, _, cx| action(cx))
}

struct SimpleTooltip(String);

impl Render for SimpleTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(gpui::hsla(0.0, 0.0, 0.12, 0.96))
            .text_color(gpui::white())
            .text_size(px(11.0))
            .child(self.0.clone())
    }
}
