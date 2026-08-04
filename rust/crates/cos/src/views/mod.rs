//! Root window view: sidebar + chat + composer + overlays (browser panel,
//! plugin library modal, model picker, plus menu, error banner).

mod browser;
mod chat;
mod composer;
mod plugins;
mod settings;
mod sidebar;

pub use browser::BrowserPanel;
pub use settings::SettingsView;

use crate::editor::{Editor, EditorEvent};
use crate::state::AppModel;
use crate::theme::{self, Theme};
use crate::views::plugins::PluginLibraryView;
use crate::views::settings::open_settings_window;
use cos_core::ComposerReferenceResolver;
use gpui::{prelude::FluentBuilder, *};
use uuid::Uuid;

actions!(
    cos,
    [
        NewTask,
        CancelRun,
        ChooseWorkspace,
        ToggleBrowserPanel,
        OpenSettings,
        OpenPluginLibrary,
        FocusComposer,
    ]
);

pub struct MainView {
    pub model: Entity<AppModel>,
    pub composer: Entity<Editor>,
    pub browser: Entity<BrowserPanel>,
    plugin_library: Entity<PluginLibraryView>,
    transcript_scroll: ScrollHandle,
    focus_handle: FocusHandle,
    pub plus_menu_open: bool,
    pub model_picker_open: bool,
    pub work_expanded: std::collections::HashMap<Uuid, bool>,
    last_message_count: usize,
    last_content_len: usize,
    pub suggestion_index: usize,
    dismissed_suggestion_signature: Option<String>,
    pub effort_drag: Option<f32>,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    pub fn new(model: Entity<AppModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = current_theme(&model, window, cx);
        let composer = cx.new(|cx| {
            Editor::new(
                "Ask Cos to build, inspect, fix, or run anything…",
                true,
                13.2,
                theme.primary,
                theme.tertiary,
                cx,
            )
        });
        let browser = cx.new(|cx| BrowserPanel::new(cx));
        let plugin_library = cx.new(|cx| PluginLibraryView::new(model.clone(), theme, cx));
        let mut subscriptions = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.observe(&browser, |_, _, cx| cx.notify()),
            cx.observe(&plugin_library, |_, _, cx| cx.notify()),
        ];
        subscriptions.push(cx.subscribe(
            &composer,
            |this: &mut MainView, editor, event: &EditorEvent, cx| match event {
                EditorEvent::Submit => this.submit(cx),
                EditorEvent::Changed => {
                    let query = ComposerReferenceResolver::query(&editor.read(cx).text(), 0);
                    this.suggestion_index = 0;
                    if let Some(query) = query {
                        if Some(query.signature()) != this.dismissed_suggestion_signature {
                            this.dismissed_suggestion_signature = None;
                        }
                    }
                    cx.notify();
                }
                EditorEvent::SuggestionMove(offset) => {
                    let count = this.reference_suggestions(cx).len();
                    if count > 0 {
                        let next = (this.suggestion_index as i32 + offset).rem_euclid(count as i32);
                        this.suggestion_index = next as usize;
                        cx.notify();
                    }
                }
                EditorEvent::SuggestionAccept => this.accept_suggestion(cx),
                EditorEvent::SuggestionDismiss => {
                    if let Some(query) = this.reference_query(cx) {
                        this.dismissed_suggestion_signature = Some(query.signature());
                        cx.notify();
                    }
                }
            },
        ));
        let view = Self {
            model,
            composer,
            browser,
            plugin_library,
            transcript_scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            plus_menu_open: false,
            model_picker_open: false,
            work_expanded: std::collections::HashMap::new(),
            last_message_count: 0,
            last_content_len: 0,
            suggestion_index: 0,
            dismissed_suggestion_signature: None,
            effort_drag: None,
            _subscriptions: subscriptions,
        };
        window.focus(&view.composer.read(cx).focus_handle(cx));
        view
    }

    pub fn reference_query(&self, cx: &App) -> Option<cos_core::ComposerReferenceQuery> {
        let text = self.composer.read(cx).text();
        ComposerReferenceResolver::query(&text, usize::MAX)
    }

    pub fn reference_suggestions(&self, cx: &App) -> Vec<cos_core::ComposerReferenceSuggestion> {
        let Some(query) = self.reference_query(cx) else {
            return Vec::new();
        };
        if Some(query.signature()) == self.dismissed_suggestion_signature {
            return Vec::new();
        }
        let model = self.model.read(cx);
        let plugins: Vec<cos_core::InstalledPlugin> = model
            .plugins
            .iter()
            .map(|plugin| {
                let mut visible = plugin.clone();
                visible.manifest.skills = plugin
                    .manifest
                    .skills
                    .iter()
                    .filter(|skill| model.is_skill_enabled(skill, plugin))
                    .cloned()
                    .collect();
                visible
            })
            .collect();
        ComposerReferenceResolver::suggestions(&query, &plugins, 6)
    }

    pub fn accept_suggestion(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.reference_query(cx) else { return };
        let suggestions = self.reference_suggestions(cx);
        let Some(suggestion) = suggestions.get(self.suggestion_index) else { return };
        let replacement = ComposerReferenceResolver::replacing_query(
            &self.composer.read(cx).text(),
            &query,
            &suggestion.insertion,
        );
        self.suggestion_index = 0;
        self.dismissed_suggestion_signature = None;
        self.composer.update(cx, |editor, cx| {
            editor.set_text(&replacement.0, cx);
        });
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let prompt = self.composer.read(cx).text().trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let model = self.model.clone();
        let (running, can_steer) = {
            let model = model.read(cx);
            (model.is_running, model.can_steer_selected_thread())
        };
        if running {
            if !can_steer {
                return;
            }
            model.update(cx, |model, cx| model.steer(&prompt, cx));
        } else {
            model.update(cx, |model, cx| model.send(&prompt, cx));
        }
        self.composer.update(cx, |editor, cx| editor.set_text("", cx));
        self.dismissed_suggestion_signature = None;
        cx.notify();
    }

    fn new_task(&mut self, _: &NewTask, _: &mut Window, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| model.new_thread(cx));
    }

    fn cancel_run(&mut self, _: &CancelRun, _: &mut Window, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| model.cancel(cx));
    }

    fn choose_workspace(&mut self, _: &ChooseWorkspace, _: &mut Window, cx: &mut Context<Self>) {
        choose_workspace(self.model.clone(), cx);
    }

    fn toggle_browser(&mut self, _: &ToggleBrowserPanel, _: &mut Window, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| {
            if model.is_betterwright_enabled() {
                model.is_browser_panel_presented = !model.is_browser_panel_presented;
                cx.notify();
            }
        });
    }

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        open_settings_window(self.model.clone(), cx);
    }

    fn open_plugin_library(&mut self, _: &OpenPluginLibrary, _: &mut Window, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| {
            model.is_plugin_library_presented = true;
            model.load_marketplace(false, cx);
            cx.notify();
        });
    }

    fn focus_composer(&mut self, _: &FocusComposer, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.composer.read(cx).focus_handle(cx));
    }
}

pub fn current_theme(model: &Entity<AppModel>, window: &Window, cx: &App) -> Theme {
    let mode = model.read(cx).preferences.appearance;
    Theme::resolve(mode, window.appearance())
}

pub fn choose_workspace(model: Entity<AppModel>, cx: &mut App) {
    let Some(model_entity) = Some(model) else { return };
    let task = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Choose workspace".into()),
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = task.await;
        let _ = cx.update(|cx| {
            if let Ok(Ok(Some(paths))) = result {
                if let Some(path) = paths.first() {
                    model_entity.update(cx, |model, cx| {
                        model.set_workspace(path.to_string_lossy().into_owned(), cx);
                    });
                }
            }
        });
    })
    .detach();
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = current_theme(&self.model, window, cx);

        // Auto-scroll the transcript on new content.
        let (message_count, content_len, is_running, browser_visible, session, last_error) = {
            let model = self.model.read(cx);
            (
                model.selected_thread().map(|thread| thread.messages.len()).unwrap_or(0),
                model
                    .selected_thread()
                    .and_then(|thread| thread.messages.last())
                    .map(|message| message.content.len())
                    .unwrap_or(0),
                model.is_running,
                model.is_browser_panel_presented && model.is_betterwright_enabled(),
                model
                    .selected_thread_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "default".into()),
                model.last_error.clone(),
            )
        };
        if message_count != self.last_message_count || (is_running && content_len != self.last_content_len) {
            self.transcript_scroll.scroll_to_bottom();
            self.last_message_count = message_count;
            self.last_content_len = content_len;
        }

        if browser_visible {
            self.browser.update(cx, |panel, cx| panel.open(&session, cx));
        } else {
            self.browser.update(cx, |panel, cx| panel.close(cx));
        }

        let suggestions = self.reference_suggestions(cx);
        self.composer.update(cx, |editor, _| {
            editor.suggestions_active = !suggestions.is_empty();
        });
        if self.suggestion_index >= suggestions.len() {
            self.suggestion_index = 0;
        }

        let model_entity = self.model.clone();
        let composer = self.composer.clone();
        let browser_panel = self.browser.clone();
        let plugin_library = self.plugin_library.clone();

        div()
            .key_context("CosMain")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::new_task))
            .on_action(cx.listener(Self::cancel_run))
            .on_action(cx.listener(Self::choose_workspace))
            .on_action(cx.listener(Self::toggle_browser))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::open_plugin_library))
            .on_action(cx.listener(Self::focus_composer))
            .flex()
            .flex_row()
            .w_full()
            .h_full()
            .bg(theme.window_background)
            .text_color(theme.primary)
            .child(sidebar::sidebar(&model_entity, &theme, cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(chat::header(&model_entity, &theme, cx))
                    .child(divider(&theme))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(chat::transcript(
                                &model_entity,
                                &theme,
                                self.transcript_scroll.clone(),
                                &mut self.work_expanded,
                                cx,
                            ))
                            .child(composer::composer(
                                &model_entity,
                                &composer,
                                &theme,
                                self.suggestion_index,
                                &suggestions,
                                cx,
                            )),
                    ),
            )
            .when(browser_visible, |container| {
                container.child(browser::browser_panel(&model_entity, &browser_panel, &theme, cx))
            })
            // In-window popovers + modal + error banner, layered on top.
            .children(self.plus_menu_open.then(|| {
                composer::plus_menu(&model_entity, &theme, cx)
            }))
            .children(self.model_picker_open.then(|| {
                composer::model_picker(&model_entity, &theme, cx)
            }))
            .child(plugin_library)
            .when_some(last_error, |container, error| {
                container.child(error_banner(&error, &model_entity, &theme))
            })
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.plus_menu_open || this.model_picker_open {
                    this.plus_menu_open = false;
                    this.model_picker_open = false;
                    cx.notify();
                }
            }))
    }
}

fn divider(theme: &Theme) -> Div {
    div().w_full().h(px(1.0)).bg(theme.divider.opacity(0.55)).flex_none()
}

fn error_banner(error: &str, model: &Entity<AppModel>, theme: &Theme) -> Div {
    let model = model.clone();
    let error = error.to_string();
    div()
        .absolute()
        .bottom(px(16.0))
        .left_1_2()
        .ml(px(-260.0))
        .w(px(520.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(14.0))
        .rounded(px(12.0))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme::ORANGE.opacity(0.5))
        .shadow_lg()
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Cos needs attention"),
        )
        .child(div().text_size(px(11.5)).text_color(theme.secondary).child(error))
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    div()
                        .id("error-ok")
                        .px(px(12.0))
                        .h(px(26.0))
                        .flex()
                        .items_center()
                        .rounded(px(7.0))
                        .bg(theme.fill_10)
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child("OK")
                        .on_click(move |_, _, cx| {
                            model.update(cx, |model, cx| {
                                model.last_error = None;
                                cx.notify();
                            });
                        }),
                ),
        )
}

impl Focusable for MainView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
