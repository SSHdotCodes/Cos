//! Composer: trust banner, editor, suggestion menu, control row with the
//! plus menu, full-access pill, model pill + picker, effort slider, send/stop.

use crate::editor::Editor;
use crate::icons::{icon, Icon};
use crate::state::AppModel;
use crate::theme::{self, Theme};
use cos_core::{ComposerReferenceKind, ComposerReferenceSuggestion, ReasoningEffort};
use gpui::{prelude::FluentBuilder, *};

use super::MainView;

pub fn composer(
    model: &Entity<AppModel>,
    editor: &Entity<Editor>,
    theme: &Theme,
    suggestion_index: usize,
    suggestions: &[ComposerReferenceSuggestion],
    cx: &mut Context<MainView>,
) -> Div {
    let (
        trust,
        selected_thread_id,
        is_running,
        can_steer,
        full_access,
        model_name,
        provider_id,
        effort_short,
        fast_badge,
    ) = {
        let model_read = model.read(cx);
        (
            model_read.pending_directory_trust.clone(),
            model_read.selected_thread_id,
            model_read.is_running,
            model_read.can_steer_selected_thread(),
            model_read.preferences.full_access,
            model_read.selected_model().name.clone(),
            model_read.selected_model().provider_id.clone(),
            model_read
                .selected_thread()
                .map(|thread| thread.effort.short_title())
                .unwrap_or(model_read.preferences.default_effort.short_title()),
            model_read.selected_model().supports_fast_mode() && model_read.preferences.fast_mode,
        )
    };
    let text_empty = editor.read(cx).is_empty();
    let shows_stop = is_running && (text_empty || !can_steer);
    let send_active = is_running || !text_empty;

    let placeholder = if can_steer {
        "Steer the active run…"
    } else if is_running {
        "Another task is running…"
    } else {
        "Ask Cos to build, inspect, fix, or run anything…"
    };
    let _ = placeholder; // Placeholder baked into the editor entity.

    let mut column = div()
        .max_w(px(820.0))
        .w_full()
        .mx_auto()
        .px(px(18.0))
        .pb(px(14.0))
        .flex()
        .flex_col();

    // Trust banner
    if let Some(trust) = &trust {
        if Some(trust.thread_id) == selected_thread_id {
            let workspace_name = std::path::Path::new(&trust.workspace_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&trust.workspace_path)
                .to_string();
            let model_decline = model.clone();
            let model_trust = model.clone();
            column = column.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .mb(px(10.0))
                    .rounded(px(14.0))
                    .bg(theme.composer_background)
                    .border_1()
                    .border_color(theme.divider)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(9.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(icon("trust-folder", Icon::FolderQuestion, px(15.0), theme::ORANGE))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("Trust {workspace_name}?")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(theme.secondary)
                                            .child("Allow Codex to work in this directory from now on."),
                                    ),
                            )
                            .child(
                                div()
                                    .id("trust-decline")
                                    .px(px(9.0))
                                    .h(px(27.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(7.0))
                                    .cursor_pointer()
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.secondary)
                                    .hover(|style| style.bg(theme.fill_06))
                                    .child("Not now")
                                    .on_click(move |_, _, cx| {
                                        model_decline.update(cx, |model, cx| {
                                            model.decline_pending_workspace_trust(cx);
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .id("trust-accept")
                                    .px(px(11.0))
                                    .h(px(27.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(7.0))
                                    .cursor_pointer()
                                    .bg(theme::BLUE)
                                    .text_color(gpui::white())
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .hover(|style| style.bg(theme::BLUE.opacity(0.85)))
                                    .child("Trust & continue")
                                    .on_click(move |_, _, cx| {
                                        model_trust.update(cx, |model, cx| {
                                            model.trust_pending_workspace_and_continue(cx);
                                        });
                                    }),
                            ),
                    ),
            );
        }
    }

    // Suggestion menu above the card
    let suggestion_menu = if !suggestions.is_empty() {
        Some(suggestion_list(suggestions, suggestion_index, theme, cx))
    } else {
        None
    };

    // The composer card
    let mut card = div()
        .relative()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(theme::COMPOSER_RADIUS))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme.divider)
        .shadow_sm();

    if let Some(menu) = suggestion_menu {
        card = card.child(
            div()
                .absolute()
                .bottom(px(64.0))
                .left(px(8.0))
                .child(menu)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
        );
    }

    card = card.child(
        div().w_full().px(px(14.0)).pt(px(6.0)).pb(px(2.0)).child(editor.clone()),
    );

    // Control row
    card = card.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(7.0))
            .px(px(8.0))
            .pb(px(7.0))
            // Plus button
            .child(
                div()
                    .id("composer-plus")
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon("plus", Icon::Plus, px(13.0), theme.secondary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.plus_menu_open = !this.plus_menu_open;
                        this.model_picker_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            // Full access / workspace pill
            .child({
                let model = model.clone();
                div()
                    .id("full-access")
                    .h(px(28.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .rounded_full()
                    .bg(theme.fill_05)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon(
                        "access-icon",
                        if full_access { Icon::Shield } else { Icon::FolderQuestion },
                        px(11.0),
                        if full_access { theme::ORANGE } else { theme.secondary },
                    ))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if full_access { theme::ORANGE } else { theme.secondary })
                            .child(if full_access { "Full access" } else { "Workspace" }),
                    )
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| {
                            model.preferences.full_access = !model.preferences.full_access;
                            model.persist_preferences();
                            cx.notify();
                        });
                    })
            })
            .child(div().flex_1())
            // Model pill
            .child(
                div()
                    .id("model-pill")
                    .h(px(28.0))
                    .w(px(194.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(9.0))
                    .rounded_full()
                    .bg(theme.fill_055)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .when(fast_badge, |row| {
                        row.child(icon("fast-badge", Icon::Bolt, px(10.0), theme::BLUE))
                    })
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(model_name),
                    )
                    .child(crate::views::settings::provider_mark(&provider_id, 13.0, theme))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(theme.secondary)
                            .whitespace_nowrap()
                            .child(effort_short.to_string()),
                    )
                    .child(icon("chevron-down", Icon::ChevronDown, px(9.0), theme.secondary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model_picker_open = !this.model_picker_open;
                        this.plus_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            // Mic (focuses editor for macOS dictation)
            .child({
                let editor = editor.clone();
                div()
                    .id("composer-mic")
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon("mic", Icon::Mic, px(12.0), theme.secondary))
                    .on_click(move |_, window, cx| {
                        window.focus(&editor.read(cx).focus_handle(cx));
                    })
            })
            // Send / stop
            .child(
                div()
                    .id("composer-send")
                    .w(px(29.0))
                    .h(px(29.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .when(send_active, |button| button.bg(theme.primary))
                    .when(!send_active, |button| button.bg(theme.fill_011))
                    .child(icon(
                        "send-icon",
                        if shows_stop { Icon::Stop } else { Icon::ArrowUp },
                        px(12.0),
                        if send_active { theme.window_background } else { theme.secondary },
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let (running, can_steer) = {
                            let model = this.model.read(cx);
                            (model.is_running, model.can_steer_selected_thread())
                        };
                        let empty = this.composer.read(cx).is_empty();
                        if running && (empty || !can_steer) {
                            this.model.update(cx, |model, cx| model.cancel(cx));
                        } else {
                            this.submit(cx);
                        }
                    })),
            ),
    );

    column.child(card)
}

fn suggestion_list(
    suggestions: &[ComposerReferenceSuggestion],
    selected: usize,
    theme: &Theme,
    cx: &mut Context<MainView>,
) -> Div {
    let mut list = div()
        .w(px(520.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(6.0))
        .rounded(px(12.0))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme.fill_10)
        .shadow_lg();

    for (index, suggestion) in suggestions.iter().enumerate() {
        let icon_kind = match suggestion.kind {
            ComposerReferenceKind::Plugin => Icon::Box,
            ComposerReferenceKind::Skill => Icon::WandStars,
            ComposerReferenceKind::Command => Icon::ChevronRight,
        };
        let is_selected = index == selected;
        list = list.child(
            div()
                .id(ElementId::Integer(index as u64))
                .h(px(38.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(9.0))
                .px(px(10.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .when(is_selected, |row| row.bg(theme::BLUE.opacity(0.12)))
                .child(icon(
                    ElementId::Integer(index as u64),
                    icon_kind,
                    px(11.0),
                    if is_selected { theme::BLUE } else { theme.secondary },
                ))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(suggestion.title.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(10.5))
                        .text_color(theme.secondary)
                        .child(suggestion.detail.clone()),
                )
                .child(
                    div()
                        .text_size(px(8.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.tertiary)
                        .child(suggestion.kind.title().to_uppercase()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.suggestion_index = index;
                    this.accept_suggestion(cx);
                })),
        );
    }
    list
}

pub fn plus_menu(model: &Entity<AppModel>, theme: &Theme, cx: &mut Context<MainView>) -> Div {
    let routes = model.read(cx).subagent_routes();
    let mut menu = div()
        .absolute()
        .left(px(crate::theme::SIDEBAR_WIDTH + 26.0))
        .bottom(px(120.0))
        .w(px(240.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(6.0))
        .rounded(px(12.0))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme.fill_10)
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

    let model_ws = model.clone();
    menu = menu.child(menu_row(
        "choose-workspace",
        "Choose workspace…",
        Icon::Folder,
        theme,
        move |cx| crate::views::choose_workspace(model_ws.clone(), cx),
    ));
    let model_new = model.clone();
    menu = menu.child(menu_row("new-task-menu", "New task", Icon::Plus, theme, move |cx| {
        model_new.update(cx, |model, cx| model.new_thread(cx));
    }));

    if !routes.is_empty() {
        menu = menu.child(
            div()
                .px(px(10.0))
                .pt(px(6.0))
                .pb(px(2.0))
                .text_size(px(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.tertiary)
                .child("ASK A SUBAGENT"),
        );
        for route in routes.iter().take(4) {
            let label = format!("{} · {}", route.model.name, route.model.effort_options().last().map(|e| e.title()).unwrap_or("High"));
            let insertion = format!(
                "/subagent Ask {} [{}] at {} reasoning to ",
                route.model.name,
                route.model.id,
                route.model.effort_options().last().map(|e| e.raw_value()).unwrap_or("high")
            );
            menu = menu.child(
                div()
                    .id(ElementId::Name(label.clone().into()))
                    .h(px(28.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_06))
                    .child(icon("subagent", Icon::People, px(11.0), theme.secondary))
                    .child(div().text_size(px(12.0)).child(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.plus_menu_open = false;
                        this.composer.update(cx, |editor, cx| {
                            editor.set_text(&insertion, cx);
                        });
                        cx.notify();
                    })),
            );
        }
    }

    menu = menu.child(div().h(px(1.0)).w_full().my(px(4.0)).bg(theme.divider));
    menu = menu.child(
        div()
            .id("plugin-library-menu")
            .h(px(30.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .rounded(px(7.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme.fill_06))
            .child(icon("box", Icon::Box, px(11.0), theme.secondary))
            .child(div().text_size(px(12.0)).child("Plugin library…"))
            .on_click({
                let model = model.clone();
                cx.listener(move |this, _, _, cx| {
                    this.plus_menu_open = false;
                    model.update(cx, |model, cx| {
                        model.is_plugin_library_presented = true;
                        model.load_marketplace(false, cx);
                        cx.notify();
                    });
                })
            }),
    );
    menu
}

fn menu_row(
    id: &'static str,
    label: impl Into<String>,
    icon_kind: Icon,
    theme: &Theme,
    action: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(30.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .rounded(px(7.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme.fill_06))
        .child(icon(id, icon_kind, px(11.0), theme.secondary))
        .child(div().text_size(px(12.0)).child(label.into()))
        .on_click(move |_, _, cx| action(cx))
}

pub fn model_picker(model: &Entity<AppModel>, theme: &Theme, cx: &mut Context<MainView>) -> Div {
    let model_read = model.read(cx);
    let providers: Vec<_> = model_read.providers.iter().filter(|provider| provider.is_enabled).cloned().collect();
    let models = model_read.models.clone();
    let selected_model = model_read.selected_model();
    let provider_name = model_read.selected_provider().name.clone();
    let supports_fast = selected_model.supports_fast_mode();
    let fast_mode = model_read.preferences.fast_mode;
    let effort = model_read
        .selected_thread()
        .map(|thread| thread.effort)
        .unwrap_or(model_read.preferences.default_effort);
    let effort_options = selected_model.effort_options().to_vec();

    let mut picker = div()
        .absolute()
        .right(px(20.0))
        .bottom(px(120.0))
        .w(px(330.0))
        .flex()
        .flex_col()
        .rounded(px(14.0))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme.fill_10)
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

    // Header
    picker = picker.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .p(px(13.0))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Model & reasoning"),
                    )
                    .child(div().text_size(px(10.5)).text_color(theme.secondary).child(provider_name)),
            )
            .when(supports_fast, |row| {
                let model = model.clone();
                row.child(
                    div()
                        .id("fast-toggle")
                        .w(px(27.0))
                        .h(px(27.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(theme.fill_055)
                        .cursor_pointer()
                        .child(icon(
                            "fast",
                            Icon::Bolt,
                            px(12.0),
                            if fast_mode { theme::BLUE } else { theme.secondary },
                        ))
                        .on_click(move |_, _, cx| {
                            model.update(cx, |model, cx| {
                                model.preferences.fast_mode = !model.preferences.fast_mode;
                                model.persist_preferences();
                                cx.notify();
                            });
                        }),
                )
            }),
    );
    picker = picker.child(div().h(px(1.0)).w_full().bg(theme.divider));

    // Model list
    let mut list = div().id("model-list").h(px(238.0)).overflow_y_scroll().p(px(7.0)).flex().flex_col().gap(px(4.0));
    for provider in &providers {
        let provider_models: Vec<_> = models.iter().filter(|model| model.provider_id == provider.id).collect();
        if provider_models.is_empty() {
            continue;
        }
        list = list.child(
            div()
                .px(px(10.0))
                .pt(px(8.0))
                .text_size(px(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.tertiary)
                .child(provider.name.to_uppercase()),
        );
        for item in provider_models {
            let is_selected = selected_model.id == item.id;
            let model = model.clone();
            let item_id = item.id.clone();
            list = list.child(
                div()
                    .id(ElementId::Name(item.id.clone().into()))
                    .h(px(38.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .when(is_selected, |row| row.bg(theme::BLUE.opacity(0.09)))
                    .when(!is_selected, |row| row.hover(|style| style.bg(theme.fill_045)))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .child(item.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(theme.secondary)
                                    .whitespace_nowrap()
                                    .child(item.model.clone()),
                            ),
                    )
                    .when(is_selected, |row| {
                        row.child(icon("check", Icon::Checkmark, px(10.0), theme::BLUE))
                    })
                    .child(crate::views::settings::provider_mark(&item.provider_id, 15.0, theme))
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| model.select_model(&item_id, cx));
                    }),
            );
        }
    }
    picker = picker.child(list);
    picker = picker.child(div().h(px(1.0)).w_full().bg(theme.divider));

    // Effort section
    let mut effort_section = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(13.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Reasoning effort"),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(px(11.5))
                        .text_color(theme.secondary)
                        .child(effort.title().to_string()),
                ),
        );
    effort_section = effort_section.child(effort_slider(model, effort, &effort_options, theme, cx));
    effort_section = effort_section.child(fast_mode_row(model, supports_fast, fast_mode, theme));
    picker = picker.child(effort_section);

    picker
}

fn fast_mode_row(model: &Entity<AppModel>, supports_fast: bool, fast_mode: bool, theme: &Theme) -> Div {
    let (title, detail, icon_kind) = if supports_fast {
        ("Fast mode", "Prefer the provider’s lower-latency route", Icon::Bolt)
    } else {
        ("Standard latency", "Fast mode isn’t offered for this model", Icon::Clock)
    };
    let mut row = div()
        .h(px(30.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(5.0))
                        .child(icon("fast-row-icon", icon_kind, px(11.0), theme.secondary))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        ),
                )
                .child(div().text_size(px(9.5)).text_color(theme.secondary).child(detail)),
        );
    if supports_fast {
        let model = model.clone();
        row = row.child(
            div()
                .id("fast-mode-switch")
                .w(px(34.0))
                .h(px(20.0))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .px(px(2.0))
                .when(fast_mode, |switch| switch.bg(theme::BLUE).justify_end())
                .when(!fast_mode, |switch| switch.bg(theme.fill_12).justify_start())
                .child(div().size(px(16.0)).rounded_full().bg(gpui::white()))
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.preferences.fast_mode = !model.preferences.fast_mode;
                        model.persist_preferences();
                        cx.notify();
                    });
                }),
        );
    }
    row
}

/// The effort slider: track with gradient fill + stop dots + glowing sun thumb.
/// Inner width is fixed at 304 (330 popover − 13×2 padding), inset 14.
fn effort_slider(
    model: &Entity<AppModel>,
    selection: ReasoningEffort,
    options: &[ReasoningEffort],
    theme: &Theme,
    cx: &mut Context<MainView>,
) -> AnyElement {
    const WIDTH: f32 = 304.0;
    const INSET: f32 = 14.0;
    let count = options.len().max(2);
    let usable = WIDTH - INSET * 2.0;
    let step = usable / (count - 1) as f32;
    let selected_index = options.iter().position(|effort| *effort == selection).unwrap_or(0);
    let thumb_position = INSET + selected_index as f32 * step;
    let normalized = ((thumb_position - INSET) / usable).clamp(0.0, 1.0);
    let intensity = ((normalized - 0.2) / 0.8).max(0.0);
    let options_vec = options.to_vec();
    let model_for_click = model.clone();

    div()
        .id("effort-slider")
        .w(px(WIDTH))
        .h(px(28.0))
        .relative()
        .cursor_pointer()
        // Track
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(2.0))
                .w(px(WIDTH))
                .h(px(24.0))
                .rounded_full()
                .bg(theme.fill_12),
        )
        // Fill
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(2.0))
                .w(px(thumb_position))
                .h(px(24.0))
                .rounded_full()
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(theme::BLUE, 0.0),
                    linear_color_stop(theme::VIOLET, 1.0),
                )),
        )
        // Stop dots
        .children((0..count).map(|index| {
            let x = INSET + index as f32 * step;
            div()
                .absolute()
                .left(px(x - 2.0))
                .top(px(12.0))
                .size(px(4.0))
                .rounded_full()
                .bg(if index <= selected_index {
                    gpui::white().opacity(0.65)
                } else {
                    theme.secondary.opacity(0.55)
                })
        }))
        // Sun thumb
        .child(sun_thumb(thumb_position - 14.0, intensity))
        .on_mouse_down(MouseButton::Left, cx.listener(move |_this, event: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            // Popover: right 20px, width 330, padding 13 → slider left edge.
            let slider_left = window.viewport_size().width - px(20.0 + 330.0) + px(13.0);
            let local_x = event.position.x - slider_left;
            let clamped = local_x.clamp(px(INSET), px(WIDTH - INSET));
            let raw = ((clamped - px(INSET)) / px(step.max(1.0))).round() as usize;
            let index = raw.min(options_vec.len().saturating_sub(1));
            if let Some(effort) = options_vec.get(index).copied() {
                model_for_click.update(cx, |model, cx| model.set_effort(effort, cx));
            }
        }))
        .into_any_element()
}

fn sun_thumb(x: f32, intensity: f32) -> AnyElement {
    canvas(
        move |_, _, _| intensity,
        move |bounds, intensity, window, _| {
            let center = point(bounds.origin.x + px(14.0), bounds.origin.y + px(14.0));
            // Rays
            if intensity > 0.02 {
                let mut builder = PathBuilder::stroke(px(1.5));
                for index in 0..8 {
                    let angle = index as f32 * std::f32::consts::FRAC_PI_4;
                    let inner = point(center.x + px(15.0) * angle.cos(), center.y + px(15.0) * angle.sin());
                    let outer = point(center.x + px(19.0) * angle.cos(), center.y + px(19.0) * angle.sin());
                    builder.move_to(inner);
                    builder.line_to(outer);
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, gpui::white().opacity(0.88 * intensity));
                }
            }
            // Warm glow
            if intensity > 0.02 {
                let mut builder = PathBuilder::fill();
                builder.move_to(point(center.x + px(17.0), center.y));
                for index in 1..=24 {
                    let angle = index as f32 / 24.0 * std::f32::consts::TAU;
                    builder.line_to(point(center.x + px(17.0) * angle.cos(), center.y + px(17.0) * angle.sin()));
                }
                builder.close();
                if let Ok(path) = builder.build() {
                    window.paint_path(path, hsla(0.1, 1.0, 0.7, 0.30 * intensity));
                }
            }
            // Thumb circle
            let mut builder = PathBuilder::fill();
            builder.move_to(point(center.x + px(13.0), center.y));
            for index in 1..=28 {
                let angle = index as f32 / 28.0 * std::f32::consts::TAU;
                builder.line_to(point(center.x + px(13.0) * angle.cos(), center.y + px(13.0) * angle.sin()));
            }
            builder.close();
            if let Ok(path) = builder.build() {
                window.paint_path(path, gpui::white());
            }
        },
    )
    .absolute()
    .left(px(x))
    .top(px(0.0))
    .size(px(28.0))
    .into_any_element()
}
