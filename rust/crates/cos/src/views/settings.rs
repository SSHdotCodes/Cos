//! Settings window (SettingsRootView.swift): 8 sections, provider cards,
//! catalog management, import, security, advanced.

use crate::editor::Editor;
use crate::icons::{icon, Icon};
use crate::state::{AppModel, ExternalSkillSource};
use crate::theme::{self, Theme};
use cos_core::{AppearanceMode, AuthenticationMode, ReasoningEffort};
use gpui::{prelude::FluentBuilder, *};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<WindowHandle<SettingsView>>> = const { RefCell::new(None) };
}

pub fn open_settings_window(model: Entity<AppModel>, cx: &mut App) {
    let existing = SETTINGS_WINDOW.with(|slot| slot.borrow().clone());
    if let Some(handle) = existing {
        if handle
            .update(cx, |_, window, _| {
                window.activate_window();
            })
            .is_ok()
        {
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(780.0), px(620.0)), cx);
    let model_for_window = model;
    let Ok(handle) = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| SettingsView::new(model_for_window, window, cx)),
    ) else {
        return;
    };
    handle
        .update(cx, |_, window, cx| {
            window.on_window_should_close(cx, |_window, _cx| {
                SETTINGS_WINDOW.with(|slot| {
                    if slot.borrow().is_some() {
                        slot.replace(None);
                    }
                });
                true
            });
        })
        .ok();
    SETTINGS_WINDOW.with(|slot| slot.replace(Some(handle)));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Models,
    Providers,
    Agent,
    Plugins,
    Import,
    Security,
    Advanced,
}

impl Section {
    const ALL: [Section; 8] = [
        Self::General,
        Self::Models,
        Self::Providers,
        Self::Agent,
        Self::Plugins,
        Self::Import,
        Self::Security,
        Self::Advanced,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Models => "Models",
            Self::Providers => "Providers",
            Self::Agent => "Agent",
            Self::Plugins => "Plugins",
            Self::Import => "Import",
            Self::Security => "Security",
            Self::Advanced => "Advanced",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::General => "Make Cos feel right for the way you work.",
            Self::Models => "Every connected provider appears in the main model selector.",
            Self::Providers => "Use official subscription sign-in flows or bring your own API key.",
            Self::Agent => "Control access, compaction, and persistent execution behavior.",
            Self::Plugins => "Add capabilities without making the app core heavier.",
            Self::Import => "Bring your existing agent skills into Cos without changing the originals.",
            Self::Security => "Cos keeps credentials local and makes authority visible.",
            Self::Advanced => "Diagnostics and catalog controls for experienced users.",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Sliders,
            Self::Models => Icon::Sparkles,
            Self::Providers => Icon::Person,
            Self::Agent => Icon::Terminal,
            Self::Plugins => Icon::Box,
            Self::Import => Icon::Import,
            Self::Security => Icon::LockShield,
            Self::Advanced => Icon::Gear,
        }
    }
}

pub struct SettingsView {
    model: Entity<AppModel>,
    section: Section,
    theme: Theme,
    show_add_provider: bool,
    add_name: Entity<Editor>,
    add_base_url: Entity<Editor>,
    add_model_name: Entity<Editor>,
    add_model_id: Entity<Editor>,
    add_key: Entity<Editor>,
    add_error: Option<String>,
    api_key_editors: HashMap<String, Entity<Editor>>,
    _subscription: Subscription,
}

impl SettingsView {
    pub fn new(model: Entity<AppModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = crate::views::current_theme(&model, window, cx);
        let editor = |placeholder: &str, cx: &mut Context<Self>| {
            let placeholder = placeholder.to_string();
            cx.new(|cx| Editor::new(placeholder, false, 12.0, theme.primary, theme.secondary, cx))
        };
        let subscription = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            section: Section::General,
            theme,
            show_add_provider: false,
            add_name: editor("OpenAI-compatible name", cx),
            add_base_url: editor("https://api.example.com/v1", cx),
            add_model_name: editor("Model display name", cx),
            add_model_id: editor("model-id", cx),
            add_key: editor("API key", cx),
            add_error: None,
            api_key_editors: HashMap::new(),
            _subscription: subscription,
        }
    }

    fn api_key_editor(&mut self, provider_id: &str, cx: &mut Context<Self>) -> Entity<Editor> {
        self.api_key_editors
            .entry(provider_id.to_string())
            .or_insert_with(|| {
                cx.new(|cx| Editor::new("API key", false, 12.0, self.theme.primary, self.theme.secondary, cx))
            })
            .clone()
    }

    fn save_new_provider(&mut self, cx: &mut Context<Self>) {
        let name = self.add_name.read(cx).text().trim().to_string();
        let base_url = self.add_base_url.read(cx).text().trim().to_string();
        let model_name = self.add_model_name.read(cx).text().trim().to_string();
        let model_id = self.add_model_id.read(cx).text().trim().to_string();
        let key = self.add_key.read(cx).text().trim().to_string();
        if name.is_empty() || model_id.is_empty() || key.is_empty() {
            self.add_error = Some("Name, model ID, and API key are required.".into());
            cx.notify();
            return;
        }
        let Ok(url) = url::Url::parse(&base_url) else {
            self.add_error = Some("Enter a valid base URL.".into());
            cx.notify();
            return;
        };
        let result = self
            .model
            .update(cx, |model, _cx| model.add_provider(&name, url, &model_name, &model_id, &key));
        match result {
            Ok(()) => {
                self.show_add_provider = false;
                self.add_error = None;
                for editor in [
                    &self.add_name,
                    &self.add_base_url,
                    &self.add_model_name,
                    &self.add_model_id,
                    &self.add_key,
                ] {
                    editor.update(cx, |editor, cx| editor.set_text("", cx));
                }
            }
            Err(error) => self.add_error = Some(error),
        }
        cx.notify();
    }
}

pub fn provider_mark(provider_id: &str, size: f32, theme: &Theme) -> AnyElement {
    let logo = match provider_id {
        "chatgpt" | "openai-api" => Some("openai.svg"),
        "anthropic" => Some("claude.svg"),
        "xai" => Some("grok.svg"),
        "opencode-go" => Some("opencode.svg"),
        "qwen" => Some("qwen.svg"),
        "pi" => Some("pi.svg"),
        _ => None,
    };
    if let Some(logo) = logo {
        let path = crate::embedded::provider_logos_root().join(logo);
        svg()
            .path(path.to_string_lossy().into_owned())
            .size(px(size))
            .text_color(theme.secondary)
            .flex_none()
            .into_any_element()
    } else {
        icon(
            ElementId::Name(format!("pm-{provider_id}").into()),
            Icon::Sparkles,
            px(size),
            theme.secondary,
        )
        .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.theme = crate::views::current_theme(&self.model, window, cx);
        let theme = self.theme;

        let mut nav = div()
            .w(px(200.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(10.0))
            .bg(theme.sidebar_background)
            .border_r_1()
            .border_color(theme.divider);
        for section in Section::ALL {
            let active = self.section == section;
            nav = nav.child(
                div()
                    .id(ElementId::Name(section.title().into()))
                    .h(px(32.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(9.0))
                    .px(px(10.0))
                    .rounded(px(7.0))
                    .cursor_pointer()
                    .when(active, |row| row.bg(theme.fill_10))
                    .when(!active, |row| row.hover(|style| style.bg(theme.fill_045)))
                    .child(icon(
                        ElementId::Name(section.title().into()),
                        section.icon(),
                        px(13.0),
                        if active { theme::BLUE } else { theme.secondary },
                    ))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(if active { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                            .child(section.title()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    })),
            );
        }

        let detail: AnyElement = match self.section {
            Section::General => general_page(&self.model, &theme, cx),
            Section::Models => models_page(self, &theme, cx),
            Section::Providers => providers_page(self, &theme, cx),
            Section::Agent => agent_page(&self.model, &theme, cx),
            Section::Plugins => plugins_page(&self.model, &theme, cx),
            Section::Import => import_page(&self.model, &theme, cx),
            Section::Security => security_page(&theme),
            Section::Advanced => advanced_page(&self.model, &theme, cx),
        };

        let mut root = div()
            .flex()
            .flex_row()
            .w_full()
            .h_full()
            .bg(theme.window_background)
            .text_color(theme.primary)
            .child(nav)
            .child(div().id("settings-detail").flex_1().h_full().overflow_y_scroll().child(detail));

        if self.show_add_provider {
            root = root.child(add_provider_sheet(self, &theme, cx));
        }
        root
    }
}

fn page(section: Section, content: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(18.0))
        .max_w(px(560.0))
        .w_full()
        .p(px(28.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(section.title()),
                )
                .child(div().text_size(px(11.5)).text_color(hsla(0.0, 0.0, 0.5, 1.0)).child(section.subtitle())),
        )
        .children(content)
}

fn group(title: &str, rows: Vec<AnyElement>, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.secondary)
                .child(title.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .rounded(px(11.0))
                .bg(theme.fill_045)
                .children(rows.into_iter().enumerate().map(|(index, row)| {
                    let wrapper = div()
                        .flex()
                        .flex_col()
                        .child(row)
                        .into_any_element();
                    let _ = index;
                    wrapper
                })),
        )
        .into_any_element()
}

fn row(label: &str, detail: Option<String>, control: AnyElement, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(9.0))
        .border_b_1()
        .border_color(theme.divider.opacity(0.3))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(div().text_size(px(12.0)).child(label.to_string()))
                .when_some(detail, |column, detail| {
                    column.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.secondary)
                            .whitespace_normal()
                            .child(detail),
                    )
                }),
        )
        .child(control)
        .into_any_element()
}

// Toggle helper with a live read performed by the caller.
fn pref_toggle(
    id: &'static str,
    on: bool,
    theme: &Theme,
    action: impl Fn(&mut AppModel, &mut App) + 'static,
    model: &Entity<AppModel>,
) -> AnyElement {
    let model = model.clone();
    div()
        .id(id)
        .w(px(38.0))
        .h(px(22.0))
        .rounded_full()
        .cursor_pointer()
        .flex()
        .items_center()
        .px(px(2.0))
        .flex_none()
        .when(on, |switch| switch.bg(theme::GREEN).justify_end())
        .when(!on, |switch| switch.bg(theme.fill_12).justify_start())
        .child(div().size(px(18.0)).rounded_full().bg(gpui::white()))
        .on_click(move |_, _, cx| {
            model.update(cx, |model, cx| {
                action(model, cx);
                model.persist_preferences();
                cx.notify();
            });
        })
        .into_any_element()
}

fn general_page(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> AnyElement {
    let m = model.read(cx);
    let appearance = m.preferences.appearance;
    let fast = m.preferences.fast_mode;
    let supports_fast = m.selected_model().supports_fast_mode();
    let animate = m.preferences.animate_streaming;
    let tokens = m.preferences.show_token_usage;
    let effort = m.preferences.default_effort;
    let workspace = m.preferences.default_workspace.clone();
    let update_status = m.update_status.clone();
    let checking = m.is_checking_for_update || m.is_installing_update;
    let available = m.available_update.clone();
    let version = m.current_version.clone();
    drop(m);

    let appearance_options: Vec<(AppearanceMode, &'static str)> = vec![
        (AppearanceMode::System, "System"),
        (AppearanceMode::Light, "Light"),
        (AppearanceMode::Dark, "Dark"),
        (AppearanceMode::TrueDark, "True Dark"),
    ];
    let mut appearance_row = div().flex().flex_row().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(theme.fill_06);
    for (value, label) in appearance_options {
        let active = appearance == value;
        let model = model.clone();
        appearance_row = appearance_row.child(
            div()
                .id(ElementId::Name(format!("appearance-{label}").into()))
                .px(px(8.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .when(active, |segment| segment.bg(theme.composer_background).shadow_sm())
                .text_size(px(10.5))
                .text_color(if active { theme.primary } else { theme.secondary })
                .child(label)
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.preferences.appearance = value;
                        model.persist_preferences();
                        cx.notify();
                    });
                }),
        );
    }

    let mut effort_row = div().flex().flex_row().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(theme.fill_06);
    for value in ReasoningEffort::ALL {
        let active = effort == value;
        let model = model.clone();
        effort_row = effort_row.child(
            div()
                .id(ElementId::Name(format!("effort-{}", value.title()).into()))
                .px(px(8.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .when(active, |segment| segment.bg(theme.composer_background).shadow_sm())
                .text_size(px(10.5))
                .text_color(if active { theme.primary } else { theme.secondary })
                .child(value.title())
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.preferences.default_effort = value;
                        model.persist_preferences();
                        cx.notify();
                    });
                }),
        );
    }

    let update_control: AnyElement = if checking {
        div().text_size(px(11.0)).text_color(theme.secondary).child("Checking…").into_any_element()
    } else if let Some(update) = &available {
        let model = model.clone();
        let version = update.version.clone();
        div()
            .id("install-update-settings")
            .px(px(10.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .rounded(px(7.0))
            .bg(theme::BLUE)
            .text_color(gpui::white())
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .child(format!("Install {version} & Restart"))
            .on_click(move |_, _, cx| {
                model.update(cx, |model, cx| model.install_available_update(cx));
            })
            .into_any_element()
    } else {
        let model = model.clone();
        div()
            .id("check-update-settings")
            .px(px(10.0))
            .h(px(26.0))
            .flex()
            .items_center()
            .rounded(px(7.0))
            .bg(theme.fill_075)
            .text_size(px(11.0))
            .cursor_pointer()
            .child("Check for Updates")
            .on_click(move |_, _, cx| {
                model.update(cx, |model, cx| model.check_for_updates(true, cx));
            })
            .into_any_element()
    };

    let workspace_model = model.clone();
    page(
        Section::General,
        vec![
            group(
                "Experience",
                vec![
                    row(
                        "Appearance",
                        (appearance == AppearanceMode::TrueDark)
                            .then(|| "Pure black surfaces for OLED displays".to_string()),
                        appearance_row.into_any_element(),
                        theme,
                    ),
                    row(
                        "Fast mode",
                        Some(if supports_fast {
                            "Prefer the selected model’s lower-latency route".into()
                        } else {
                            "Unavailable for the selected model".into()
                        }),
                        pref_toggle(
                            "fast-mode",
                            fast && supports_fast,
                            theme,
                            |model, _| {
                                model.preferences.fast_mode = !model.preferences.fast_mode;
                            },
                            model,
                        ),
                        theme,
                    ),
                    row(
                        "Streaming animation",
                        Some("Animate new response content".into()),
                        pref_toggle(
                            "animate-streaming",
                            animate,
                            theme,
                            |model, _| {
                                model.preferences.animate_streaming = !model.preferences.animate_streaming;
                            },
                            model,
                        ),
                        theme,
                    ),
                    row(
                        "Show token usage",
                        None,
                        pref_toggle(
                            "show-tokens",
                            tokens,
                            theme,
                            |model, _| {
                                model.preferences.show_token_usage = !model.preferences.show_token_usage;
                            },
                            model,
                        ),
                        theme,
                    ),
                ],
                theme,
            ),
            group(
                "Defaults",
                vec![
                    row("Reasoning effort", None, effort_row.into_any_element(), theme),
                    row(
                        "Workspace",
                        Some(workspace),
                        div()
                            .id("choose-default-workspace")
                            .px(px(10.0))
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .rounded(px(7.0))
                            .bg(theme.fill_075)
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child("Choose…")
                            .on_click(move |_, _, cx| {
                                let model = workspace_model.clone();
                                let task = cx.prompt_for_paths(PathPromptOptions {
                                    files: false,
                                    directories: true,
                                    multiple: false,
                                    prompt: Some("Choose default workspace".into()),
                                });
                                cx.spawn(async move |cx: &mut AsyncApp| {
                                    if let Ok(Ok(Some(paths))) = task.await {
                                        if let Some(path) = paths.first() {
                                            let path = path.to_string_lossy().into_owned();
                                            let _ = cx.update(|cx| {
                                                model.update(cx, |model, cx| {
                                                    model.preferences.default_workspace = path;
                                                    model.persist_preferences();
                                                    cx.notify();
                                                });
                                            });
                                        }
                                    }
                                })
                                .detach();
                            })
                            .into_any_element(),
                        theme,
                    ),
                ],
                theme,
            ),
            group(
                "Updates",
                vec![row(
                    "Check for Updates",
                    Some(update_status.unwrap_or(format!("Cos {version}"))),
                    update_control,
                    theme,
                )],
                theme,
            ),
        ],
    )
    .into_any_element()
}

fn models_page(view: &mut SettingsView, theme: &Theme, cx: &mut Context<SettingsView>) -> AnyElement {
    let model = view.model.clone();
    let m = model.read(cx);
    let title_models = m.title_models();
    let selected_title = m.selected_title_model().map(|model| model.id.clone());
    let models = m.models.clone();
    let selected_id = m.preferences.selected_model_id.clone();
    drop(m);

    let mut title_row = div().flex().flex_row().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(theme.fill_06);
    for item in &title_models {
        let active = selected_title.as_deref() == Some(item.id.as_str());
        let model = model.clone();
        let id = item.id.clone();
        title_row = title_row.child(
            div()
                .id(ElementId::Name(format!("title-{}", item.id).into()))
                .px(px(8.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .when(active, |segment| segment.bg(theme.composer_background).shadow_sm())
                .text_size(px(10.5))
                .text_color(if active { theme.primary } else { theme.secondary })
                .child(format!("{} · Low", item.name))
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.preferences.title_model_id = Some(id.clone());
                        model.persist_preferences();
                        cx.notify();
                    });
                }),
        );
    }

    let mut model_rows: Vec<AnyElement> = Vec::new();
    for item in &models {
        let is_default = selected_id == item.id;
        let model = model.clone();
        let id = item.id.clone();
        let control: AnyElement = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(if is_default {
                div()
                    .text_size(px(10.5))
                    .text_color(theme::BLUE)
                    .child("Default")
                    .into_any_element()
            } else {
                div()
                    .id(ElementId::Name(format!("default-{}", item.id).into()))
                    .px(px(8.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(10.5))
                    .hover(|style| style.bg(theme.fill_075))
                    .child("Make default")
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| {
                            model.preferences.selected_model_id = id.clone();
                            model.persist_preferences();
                            cx.notify();
                        });
                    })
                    .into_any_element()
            })
            .child(provider_mark(&item.provider_id, 15.0, theme))
            .into_any_element();
        model_rows.push(row(
            &item.name,
            Some(format!("{} · {}K context", item.model, item.context_window / 1_000)),
            control,
            theme,
        ));
    }

    page(
        Section::Models,
        vec![
            group("Task naming", vec![row("Title model", Some("Generates concise task names at Low reasoning".into()), title_row.into_any_element(), theme)], theme),
            group("Available models", model_rows, theme),
            div()
                .id("show-add-provider")
                .h(px(30.0))
                .px(px(12.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .rounded(px(8.0))
                .bg(theme::BLUE)
                .text_color(gpui::white())
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child(icon("plus", Icon::Plus, px(11.0), gpui::white()))
                .child("Add custom provider & model…")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_add_provider = true;
                    cx.notify();
                }))
                .into_any_element(),
        ],
    )
    .into_any_element()
}

fn providers_page(view: &mut SettingsView, theme: &Theme, cx: &mut Context<SettingsView>) -> AnyElement {
    let model = view.model.clone();
    let m = model.read(cx);
    let providers = m.providers.clone();
    let sessions = m.provider_sessions.clone();
    let login_status = m.login_status.clone();
    let has_key: HashMap<String, bool> = providers
        .iter()
        .map(|provider| (provider.id.clone(), m.has_api_key(provider)))
        .collect();
    drop(m);

    let mut cards: Vec<AnyElement> = Vec::new();
    for provider in &providers {
        let session = sessions.get(&provider.id);
        let status = login_status.get(&provider.id);
        let subtitle = match provider.auth_mode {
            AuthenticationMode::Subscription => "Subscription credential · native Cos transport",
            AuthenticationMode::ApiKey => "Keychain-protected · native Cos transport",
            AuthenticationMode::Local => "Cos smart route",
        };
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(px(12.0))
            .bg(theme.fill_045);
        if let Some(session) = session {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(5.0))
                    .py(px(3.0))
                    .child(icon("connected", Icon::CheckmarkCircle, px(17.0), theme::GREEN))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(session.display_name()),
                    )
                    .child(div().text_size(px(9.5)).font_weight(FontWeight::MEDIUM).text_color(theme::GREEN).child("Connected")),
            );
        }
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(provider.name.clone()),
                    )
                    .child(div().text_size(px(10.5)).text_color(theme.secondary).child(subtitle)),
            );
        match provider.auth_mode {
            AuthenticationMode::Subscription => {
                let model = model.clone();
                let provider_for_sign_in = provider.clone();
                let label = if session.is_some() { "Refresh Token…" } else { "Sign In…" };
                header = header.child(
                    div()
                        .id(ElementId::Name(format!("sign-in-{}", provider.id).into()))
                        .px(px(10.0))
                        .h(px(26.0))
                        .flex()
                        .items_center()
                        .rounded(px(7.0))
                        .bg(theme::BLUE)
                        .text_color(gpui::white())
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .child(label)
                        .on_click(move |_, _, cx| {
                            model.update(cx, |model, cx| model.sign_in(&provider_for_sign_in, cx));
                        }),
                );
            }
            AuthenticationMode::Local => {
                header = header.child(div().text_size(px(10.5)).text_color(theme.secondary).child("Local"));
            }
            _ => {}
        }
        card = card.child(header);

        if provider.auth_mode == AuthenticationMode::ApiKey {
            let editor = view.api_key_editor(&provider.id, cx);
            let stored = has_key.get(&provider.id).copied().unwrap_or(false);
            let model = model.clone();
            let provider_save = provider.clone();
            let editor_save = editor.clone();
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .h(px(28.0))
                            .px(px(8.0))
                            .rounded(px(7.0))
                            .bg(theme.composer_background)
                            .border_1()
                            .border_color(theme.divider)
                            .child(editor),
                    )
                    .child(
                        div()
                            .id(ElementId::Name(format!("save-key-{}", provider.id).into()))
                            .px(px(10.0))
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .rounded(px(7.0))
                            .bg(theme.fill_075)
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child("Save")
                            .on_click(move |_, _, cx| {
                                let value = editor_save.read(cx).text().trim().to_string();
                                if value.is_empty() {
                                    return;
                                }
                                model.update(cx, |model, cx| {
                                    if let Err(error) = model.set_api_key(&value, &provider_save) {
                                        model.last_error = Some(error);
                                    }
                                    cx.notify();
                                });
                                editor_save.update(cx, |editor, cx| editor.set_text("", cx));
                            }),
                    ),
            );
            if stored {
                card = card.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::GREEN)
                        .child("Key stored in this Mac’s Keychain — enter to replace"),
                );
            }
        }
        if let Some(status) = status {
            let good = status.to_lowercase().contains("signed") || status.to_lowercase().contains("stored");
            card = card.child(
                div()
                    .text_size(px(10.5))
                    .text_color(if good { theme::GREEN } else { theme.secondary })
                    .whitespace_normal()
                    .child(status.clone()),
            );
        }
        cards.push(card.into_any_element());
    }

    page(Section::Providers, cards).into_any_element()
}

fn agent_page(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> AnyElement {
    let m = model.read(cx);
    let full_access = m.preferences.full_access;
    let auto_compact = m.preferences.auto_compact;
    let compact_at = m.preferences.compact_at_percent;
    let keep_recent = m.preferences.keep_recent_tokens;
    drop(m);

    let mut keep_row = div().flex().flex_row().gap(px(2.0)).p(px(2.0)).rounded(px(7.0)).bg(theme.fill_06);
    for value in [10_000i64, 20_000, 40_000] {
        let active = keep_recent == value;
        let model = model.clone();
        keep_row = keep_row.child(
            div()
                .id(ElementId::Name(format!("keep-{value}").into()))
                .px(px(8.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .cursor_pointer()
                .when(active, |segment| segment.bg(theme.composer_background).shadow_sm())
                .text_size(px(10.5))
                .text_color(if active { theme.primary } else { theme.secondary })
                .child(format!("{}K tokens", value / 1_000))
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.preferences.keep_recent_tokens = value;
                        model.persist_preferences();
                        cx.notify();
                    });
                }),
        );
    }

    let compact_at_label = format!("{}%", compact_at as i64);
    let model_minus = model.clone();
    let model_plus = model.clone();
    let compact_control = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(stepper_button("compact-minus", "−", theme, move |cx| {
            model_minus.update(cx, |model, cx| {
                model.preferences.compact_at_percent = (model.preferences.compact_at_percent - 1.0).max(55.0);
                model.persist_preferences();
                cx.notify();
            });
        }))
        .child(
            div()
                .w(px(36.0))
                .text_size(px(11.0))
                .child(compact_at_label),
        )
        .child(stepper_button("compact-plus", "+", theme, move |cx| {
            model_plus.update(cx, |model, cx| {
                model.preferences.compact_at_percent = (model.preferences.compact_at_percent + 1.0).min(92.0);
                model.persist_preferences();
                cx.notify();
            });
        }));

    page(
        Section::Agent,
        vec![
            group(
                "Access",
                vec![row(
                    "Full access",
                    Some("Allow Cos tools outside the workspace and enable commands".into()),
                    pref_toggle(
                        "agent-full-access",
                        full_access,
                        theme,
                        |model, _| {
                            model.preferences.full_access = !model.preferences.full_access;
                        },
                        model,
                    ),
                    theme,
                )],
                theme,
            ),
            group(
                "Compaction",
                vec![
                    row(
                        "Automatic compaction",
                        Some("Preserve a checkpoint plus recent verbatim context".into()),
                        pref_toggle(
                            "auto-compact",
                            auto_compact,
                            theme,
                            |model, _| {
                                model.preferences.auto_compact = !model.preferences.auto_compact;
                            },
                            model,
                        ),
                        theme,
                    ),
                    row("Compact at", None, compact_control.into_any_element(), theme),
                    row("Keep recent context", None, keep_row.into_any_element(), theme),
                ],
                theme,
            ),
        ],
    )
    .into_any_element()
}

fn stepper_button(id: &'static str, label: &'static str, theme: &Theme, action: impl Fn(&mut App) + 'static) -> impl IntoElement {
    div()
        .id(id)
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(theme.fill_075)
        .cursor_pointer()
        .text_size(px(13.0))
        .child(label)
        .on_click(move |_, _, cx| action(cx))
}

fn plugins_page(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> AnyElement {
    let m = model.read(cx);
    let plugins = m.plugins.clone();
    drop(m);
    let mut rows: Vec<AnyElement> = Vec::new();
    for plugin in &plugins {
        let built_in = plugin.manifest.built_in == Some(true);
        rows.push(row(
            &plugin.manifest.name,
            Some(format!("v{} · {}", plugin.manifest.version, plugin.manifest.author)),
            div()
                .text_size(px(10.5))
                .text_color(if built_in { theme::BLUE } else { theme.secondary })
                .child(if built_in { "Built in" } else if plugin.is_enabled { "Enabled" } else { "Disabled" })
                .into_any_element(),
            theme,
        ));
    }
    let model_library = model.clone();
    let model_disk = model.clone();
    let buttons = div()
        .flex()
        .flex_row()
        .gap(px(8.0))
        .child(
            div()
                .id("open-library")
                .px(px(12.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .bg(theme::BLUE)
                .text_color(gpui::white())
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child("Open library")
                .on_click(move |_, _, cx| {
                    model_library.update(cx, |model, cx| {
                        model.is_plugin_library_presented = true;
                        model.load_marketplace(false, cx);
                        cx.notify();
                    });
                }),
        )
        .child(
            div()
                .id("install-from-disk-settings")
                .px(px(12.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .bg(theme.fill_075)
                .text_size(px(11.5))
                .cursor_pointer()
                .child("Install from disk…")
                .on_click(move |_, _, cx| {
                    let model = model_disk.clone();
                    let task = cx.prompt_for_paths(PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: false,
                        prompt: Some("Choose cos.plugin.json".into()),
                    });
                    cx.spawn(async move |cx: &mut AsyncApp| {
                        if let Ok(Ok(Some(paths))) = task.await {
                            if let Some(path) = paths.first() {
                                let path = path.clone();
                                let _ = cx.update(|cx| {
                                    model.update(cx, |model, cx| {
                                        model.install_plugin_from_disk(&path, cx);
                                    });
                                });
                            }
                        }
                    })
                    .detach();
                }),
        );
    page(
        Section::Plugins,
        vec![group("Installed", rows, theme), buttons.into_any_element()],
    )
    .into_any_element()
}

fn import_page(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> AnyElement {
    let m = model.read(cx);
    let counts = m.skill_import_counts.clone();
    let statuses = m.skill_import_status.clone();
    drop(m);

    let mut rows: Vec<AnyElement> = Vec::new();
    for source in ExternalSkillSource::ALL {
        let count = counts.get(&source).copied().unwrap_or(0);
        let detail = if let Some(status) = statuses.get(&source) {
            status.clone()
        } else if source == ExternalSkillSource::Folder {
            source.detail().to_string()
        } else if count == 0 {
            "No skills found in the default folder".into()
        } else {
            format!("{count} available · {}", source.detail())
        };
        let button_title = if source == ExternalSkillSource::Folder {
            "Choose…".to_string()
        } else if count > 0 {
            format!("Import {count}")
        } else {
            "Import".into()
        };
        let model = model.clone();
        let disabled = source != ExternalSkillSource::Folder && count == 0;
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9.0))
            .child(icon(
                ElementId::Name(format!("import-{}", source.title()).into()),
                if source == ExternalSkillSource::Folder { Icon::Folder } else { Icon::Import },
                px(14.0),
                if source == ExternalSkillSource::Folder { theme.secondary } else { theme::BLUE },
            ))
            .child(
                div()
                    .id(ElementId::Name(format!("import-btn-{}", source.title()).into()))
                    .px(px(10.0))
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .rounded(px(7.0))
                    .bg(theme.fill_075)
                    .text_size(px(11.0))
                    .when(!disabled, |button| button.cursor_pointer())
                    .when(disabled, |button| button.opacity(0.5))
                    .child(button_title)
                    .on_click(move |_, _, cx| {
                        if disabled {
                            return;
                        }
                        let model = model.clone();
                        if source == ExternalSkillSource::Folder {
                            let task = cx.prompt_for_paths(PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: true,
                                prompt: Some("Choose folders containing SKILL.md bundles".into()),
                            });
                            cx.spawn(async move |cx: &mut AsyncApp| {
                                if let Ok(Ok(Some(paths))) = task.await {
                                    let _ = cx.update(|cx| {
                                        model.update(cx, |model, cx| {
                                            model.import_skills_from(paths, source, cx);
                                        });
                                    });
                                }
                            })
                            .detach();
                        } else {
                            model.update(cx, |model, cx| model.import_skills(source, cx));
                        }
                    }),
            )
            .into_any_element();
        rows.push(row(source.title(), Some(detail), control, theme));
    }

    let rescan = {
        let model = model.clone();
        div()
            .id("rescan-skills")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.secondary)
            .child(icon("rescan", Icon::Refresh, px(11.0), theme.secondary))
            .child("Rescan skill libraries")
            .on_click(move |_, _, cx| {
                model.update(cx, |model, cx| {
                    model.refresh_skill_import_counts();
                    cx.notify();
                });
            })
            .into_any_element()
    };

    page(
        Section::Import,
        vec![
            group("Skill libraries", rows, theme),
            group(
                "How importing works",
                vec![
                    row(
                        "Portable bundles",
                        Some("Copies SKILL.md plus scripts, references, and assets up to 10 MB per skill".into()),
                        icon("doc", Icon::DocOnDoc, px(14.0), theme::BLUE).into_any_element(),
                        theme,
                    ),
                    row(
                        "Local and recoverable",
                        Some("Original folders stay untouched; imported skills can be disabled or moved to Trash as plugins".into()),
                        icon("shield", Icon::CheckmarkShield, px(14.0), theme::GREEN).into_any_element(),
                        theme,
                    ),
                ],
                theme,
            ),
            rescan,
        ],
    )
    .into_any_element()
}

fn security_page(theme: &Theme) -> AnyElement {
    page(
        Section::Security,
        vec![
            group(
                "Credentials",
                vec![
                    row(
                        "BYOK secrets",
                        Some("Stored in macOS Keychain with device-only access".into()),
                        icon("seal1", Icon::CheckmarkCircle, px(14.0), theme::GREEN).into_any_element(),
                        theme,
                    ),
                    row(
                        "Subscription sessions",
                        Some("Read locally into memory by the native Cos transport".into()),
                        icon("seal2", Icon::CheckmarkCircle, px(14.0), theme::GREEN).into_any_element(),
                        theme,
                    ),
                    row(
                        "Plugin trust",
                        Some("Managed actions are scoped, validated, and recoverable".into()),
                        icon("seal3", Icon::Shield, px(14.0), theme::BLUE).into_any_element(),
                        theme,
                    ),
                ],
                theme,
            ),
            div()
                .text_size(px(11.5))
                .text_color(theme.secondary)
                .whitespace_normal()
                .line_height(px(16.0))
                .child("Cos does not send subscription tokens to another agent harness. Only the selected native provider transport receives the credential. Full Access is shown in the composer whenever it is enabled.")
                .into_any_element(),
        ],
    )
    .into_any_element()
}

fn advanced_page(model: &Entity<AppModel>, theme: &Theme, cx: &mut App) -> AnyElement {
    let m = model.read(cx);
    let activity = m.activity.clone();
    let running = m.is_running;
    drop(m);
    let model_reset = model.clone();
    page(
        Section::Advanced,
        vec![group(
            "Runtime",
            vec![
                row(
                    "Harness activity",
                    Some(activity),
                    div()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(if running { theme::GREEN } else { theme.secondary })
                        .into_any_element(),
                    theme,
                ),
                row(
                    "Marketplace",
                    None,
                    div()
                        .id("open-marketplace-site")
                        .text_size(px(11.5))
                        .text_color(theme::BLUE)
                        .cursor_pointer()
                        .child("cos.ssh.codes")
                        .on_click(|_, _, _| {
                            let _ = std::process::Command::new("/usr/bin/open")
                                .arg("https://cos.ssh.codes")
                                .spawn();
                        })
                        .into_any_element(),
                    theme,
                ),
                row(
                    "Reset provider catalog",
                    Some("Restore Cos defaults without deleting Keychain secrets".into()),
                    div()
                        .id("reset-catalog")
                        .px(px(10.0))
                        .h(px(26.0))
                        .flex()
                        .items_center()
                        .rounded(px(7.0))
                        .bg(theme.fill_075)
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child("Reset")
                        .on_click(move |_, _, cx| {
                            model_reset.update(cx, |model, cx| model.reset_catalog(cx));
                        })
                        .into_any_element(),
                    theme,
                ),
            ],
            theme,
        )],
    )
    .into_any_element()
}

fn add_provider_sheet(view: &mut SettingsView, theme: &Theme, cx: &mut Context<SettingsView>) -> Div {
    let mut form = div()
        .w(px(360.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(18.0))
        .rounded(px(14.0))
        .bg(theme.composer_background)
        .border_1()
        .border_color(theme.divider)
        .shadow_xl()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Add custom provider"),
        );

    for (label, editor) in [
        ("Provider name", &view.add_name),
        ("Base URL", &view.add_base_url),
        ("Model name", &view.add_model_name),
        ("Model ID", &view.add_model_id),
        ("API key", &view.add_key),
    ] {
        form = form.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_size(px(10.5)).text_color(theme.secondary).child(label))
                .child(
                    div()
                        .h(px(30.0))
                        .px(px(8.0))
                        .rounded(px(7.0))
                        .bg(theme.window_background)
                        .border_1()
                        .border_color(theme.divider)
                        .flex()
                        .items_center()
                        .child(editor.clone()),
                ),
        );
    }
    if let Some(error) = &view.add_error {
        form = form.child(
            div()
                .text_size(px(10.5))
                .text_color(theme::RED)
                .whitespace_normal()
                .child(error.clone()),
        );
    }
    form = form.child(
        div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(8.0))
            .child(
                div()
                    .id("cancel-add-provider")
                    .px(px(12.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .hover(|style| style.bg(theme.fill_075))
                    .child("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_add_provider = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("save-add-provider")
                    .px(px(12.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .bg(theme::BLUE)
                    .text_color(gpui::white())
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .child("Save provider")
                    .on_click(cx.listener(|this, _, _, cx| this.save_new_provider(cx))),
            ),
    );

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.35))
        .child(form)
        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
            this.show_add_provider = false;
            cx.notify();
        }))
}
