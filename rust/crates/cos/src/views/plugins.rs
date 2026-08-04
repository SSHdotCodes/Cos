//! Plugin library modal (PluginLibraryView.swift): installed + marketplace
//! sections, plugin details, skill management, import, confirmations.

use crate::editor::Editor;
use crate::icons::{cos_mark, icon, Icon};
use crate::state::AppModel;
use crate::theme::{self, Theme};
use cos_core::{CosMarketplaceListing, InstalledPlugin};
use gpui::{prelude::FluentBuilder, *};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySection {
    Installed,
    Marketplace,
}

pub struct PluginLibraryView {
    model: Entity<AppModel>,
    section: LibrarySection,
    selection: Option<String>,
    search: Entity<Editor>,
    pending_plugin_removal: Option<String>,
    skill_pending_removal: Option<(String, String)>, // (skill, plugin_id)
    theme: Theme,
    _subscription: Subscription,
}

impl PluginLibraryView {
    pub fn new(model: Entity<AppModel>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            Editor::new("Search plugins", false, 12.0, theme.primary, theme.secondary, cx)
        });
        let subscription = cx.observe(&model, |_, _, cx| cx.notify());
        let search_subscription = cx.subscribe(&search, |_, _, _: &crate::editor::EditorEvent, cx| cx.notify());
        let _ = search_subscription;
        Self {
            model,
            section: LibrarySection::Installed,
            selection: None,
            search,
            pending_plugin_removal: None,
            skill_pending_removal: None,
            theme,
            _subscription: subscription,
        }
    }

    fn filtered_marketplace(&self, cx: &App) -> Vec<CosMarketplaceListing> {
        let model = self.model.read(cx);
        let query = self.search.read(cx).text().trim().to_lowercase();
        if query.is_empty() {
            return model.marketplace_plugins.clone();
        }
        model
            .marketplace_plugins
            .iter()
            .filter(|listing| {
                let mut haystack = vec![listing.name.clone(), listing.author.clone(), listing.description.clone()];
                if let Some(tags) = &listing.tags {
                    haystack.extend(tags.clone());
                }
                haystack.join(" ").to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }
}

impl Render for PluginLibraryView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.read(cx);
        if !model.is_plugin_library_presented {
            return div().into_any_element();
        }
        let theme = self.theme;
        let plugins = model.plugins.clone();
        let marketplace = self.filtered_marketplace(cx);
        let loading = model.is_loading_marketplace;
        let marketplace_error = model.marketplace_error.clone();
        let installing_id = model.installing_marketplace_plugin_id.clone();
        let cu_granted = model.computer_use_access_granted;
        let cu_status = model.computer_use_access_status.clone();
        let is_skill_enabled = |skill: &str, plugin: &InstalledPlugin| model.is_skill_enabled(skill, plugin);
        let _ = &is_skill_enabled;
        drop(model);

        // Left column list selection defaults
        if self.selection.is_none() {
            self.selection = plugins.first().map(|plugin| plugin.id().to_string());
        }

        let mut left_list = div().id("plugin-list").flex_1().flex().flex_col().overflow_y_scroll().px(px(6.0));
        match self.section {
            LibrarySection::Installed => {
                for plugin in &plugins {
                    left_list = left_list.child(installed_row(
                        &self.model,
                        plugin,
                        self.selection.as_deref() == Some(plugin.id()),
                        &theme,
                        cx,
                    ));
                }
            }
            LibrarySection::Marketplace => {
                if loading && marketplace.is_empty() {
                    left_list = left_list.child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_col()
                            .gap(px(8.0))
                            .child(super::chat::spinner(&theme))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.secondary)
                                    .child("Loading marketplace…"),
                            ),
                    );
                } else if let Some(error) = &marketplace_error {
                    if marketplace.is_empty() {
                        left_list = left_list.child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(8.0))
                                .p(px(16.0))
                                .child(icon("wifi-off", Icon::Warning, px(18.0), theme.secondary))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Marketplace unavailable"),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(theme.secondary)
                                        .whitespace_normal()
                                        .child(error.clone()),
                                ),
                        );
                    }
                } else {
                    for listing in &marketplace {
                        left_list = left_list.child(marketplace_row(
                            &self.model,
                            listing,
                            self.selection.as_deref() == Some(listing.id.as_str()),
                            &theme,
                            cx,
                        ));
                    }
                }
            }
        }

        // Detail pane
        let detail: AnyElement = match self.section {
            LibrarySection::Installed => {
                let selected = self.selection.clone().or_else(|| plugins.first().map(|plugin| plugin.id().to_string()));
                plugins
                    .iter()
                    .find(|plugin| Some(plugin.id()) == selected.as_deref())
                    .map(|plugin| {
                        plugin_detail(
                            &self.model,
                            plugin,
                            cu_granted,
                            cu_status.clone(),
                            &theme,
                            cx,
                        )
                    })
                    .unwrap_or_else(|| empty_detail("No plugins installed", Icon::Box, &theme))
            }
            LibrarySection::Marketplace => {
                let selected = self
                    .selection
                    .clone()
                    .or_else(|| marketplace.first().map(|listing| listing.id.clone()));
                marketplace
                    .iter()
                    .find(|listing| Some(&listing.id) == selected.as_ref())
                    .map(|listing| {
                        marketplace_detail(
                            &self.model,
                            listing,
                            installing_id.as_deref() == Some(listing.id.as_str()),
                            &theme,
                            cx,
                        )
                    })
                    .unwrap_or_else(|| empty_detail("Choose a marketplace plugin", Icon::Storefront, &theme))
            }
        };

        let close: AnyElement = {
            let model = self.model.clone();
            div()
                .id("close-plugin-library")
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .size(px(28.0))
                .rounded_full()
                .bg(theme.fill_06)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|style| style.bg(theme.fill_10))
                .child(icon("xmark", Icon::Xmark, px(11.0), theme.primary))
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.is_plugin_library_presented = false;
                        cx.notify();
                    });
                })
                .into_any_element()
        };

        let mut panel = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.32))
            .on_mouse_down(MouseButton::Left, {
                let model = self.model.clone();
                move |_, _, cx| {
                    model.update(cx, |model, cx| {
                        model.is_plugin_library_presented = false;
                        cx.notify();
                    });
                }
            });

        let mut body = div()
            .w(px(760.0))
            .h(px(560.0))
            .rounded(px(16.0))
            .bg(theme.window_background)
            .border_1()
            .border_color(theme.divider)
            .shadow_xl()
            .flex()
            .flex_row()
            .overflow_hidden()
            .relative()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // Left column
            .child(
                div()
                    .w(px(230.0))
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .bg(theme.sidebar_background)
                    .border_r_1()
                    .border_color(theme.divider)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .p(px(12.0))
                            .child(cos_mark(true, theme)),
                    )
                    .child(
                        // Segmented picker
                        div()
                            .mx(px(10.0))
                            .mb(px(8.0))
                            .h(px(26.0))
                            .rounded(px(7.0))
                            .bg(theme.fill_06)
                            .flex()
                            .flex_row()
                            .p(px(2.0))
                            .gap(px(2.0))
                            .child(segment(
                                "seg-installed",
                                "Installed",
                                self.section == LibrarySection::Installed,
                                &theme,
                                cx.listener(|this, _, _, cx| {
                                    this.section = LibrarySection::Installed;
                                    this.selection = None;
                                    cx.notify();
                                }),
                            ))
                            .child(segment(
                                "seg-marketplace",
                                "Marketplace",
                                self.section == LibrarySection::Marketplace,
                                &theme,
                                cx.listener(|this, _, _, cx| {
                                    this.section = LibrarySection::Marketplace;
                                    this.selection = None;
                                    this.model.update(cx, |model, cx| model.load_marketplace(false, cx));
                                    cx.notify();
                                }),
                            )),
                    )
                    .when(self.section == LibrarySection::Marketplace, |column| {
                        column.child(
                            div()
                                .mx(px(10.0))
                                .mb(px(5.0))
                                .h(px(30.0))
                                .px(px(9.0))
                                .rounded(px(8.0))
                                .bg(theme.fill_055)
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.0))
                                .child(icon("search", Icon::Search, px(11.0), theme.secondary))
                                .child(div().flex_1().child(self.search.clone())),
                        )
                    })
                    .child(left_list)
                    .child(
                        // Footer
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .p(px(10.0))
                            .child(
                                if self.section == LibrarySection::Installed {
                                    let model = self.model.clone();
                                    div()
                                        .id("install-from-disk")
                                        .px(px(8.0))
                                        .h(px(26.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.0))
                                        .cursor_pointer()
                                        .text_size(px(11.0))
                                        .hover(|style| style.bg(theme.fill_06))
                                        .child("Install from disk…")
                                        .on_click(move |_, _, cx| {
                                            let model = model.clone();
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
                                        })
                                        .into_any_element()
                                } else {
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme.secondary)
                                        .child("cos.ssh.codes")
                                        .into_any_element()
                                },
                            )
                            .child(div().flex_1())
                            .child({
                                let model = self.model.clone();
                                let section = self.section;
                                div()
                                    .id("refresh-plugins")
                                    .size(px(26.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.fill_06))
                                    .child(icon("refresh", Icon::Refresh, px(12.0), theme.secondary))
                                    .on_click(move |_, _, cx| {
                                        model.update(cx, |model, cx| match section {
                                            LibrarySection::Installed => model.reload_plugins(cx),
                                            LibrarySection::Marketplace => model.load_marketplace(true, cx),
                                        });
                                    })
                            }),
                    ),
            )
            // Detail
            .child(div().id("plugin-detail").flex_1().h_full().overflow_y_scroll().child(detail))
            .child(close);

        // Confirmation overlays
        if let Some(plugin_id) = &self.pending_plugin_removal {
            let name = plugins
                .iter()
                .find(|plugin| &plugin.id() == plugin_id)
                .map(|plugin| plugin.manifest.name.clone())
                .unwrap_or_else(|| plugin_id.clone());
            let model = self.model.clone();
            let plugin_id = plugin_id.clone();
            body = body.child(confirm_overlay(
                &format!("Move {name} to Trash?"),
                "The plugin can be recovered from the Trash.",
                &theme,
                cx.listener(|this, _, _, cx| {
                    this.pending_plugin_removal = None;
                    cx.notify();
                }),
                cx.listener(move |this, _, _, cx| {
                    this.pending_plugin_removal = None;
                    if let Some(plugin) = model.read(cx).plugins.iter().find(|plugin| plugin.id() == plugin_id).cloned() {
                        model.update(cx, |model, cx| model.remove_plugin(&plugin, cx));
                    }
                    cx.notify();
                }),
            ));
        }
        if let Some((skill, plugin_id)) = &self.skill_pending_removal {
            let model = self.model.clone();
            let skill_name = skill.clone();
            let plugin_id = plugin_id.clone();
            body = body.child(confirm_overlay(
                &format!("Delete {skill_name}?"),
                "The skill can be recovered from the Trash. Its plugin will remain installed.",
                &theme,
                cx.listener(|this, _, _, cx| {
                    this.skill_pending_removal = None;
                    cx.notify();
                }),
                cx.listener(move |this, _, _, cx| {
                    this.skill_pending_removal = None;
                    if let Some(plugin) = model.read(cx).plugins.iter().find(|plugin| plugin.id() == plugin_id).cloned() {
                        model.update(cx, |model, cx| model.remove_skill(&skill_name, &plugin, cx));
                    }
                    cx.notify();
                }),
            ));
        }

        panel = panel.child(body);
        panel.into_any_element()
    }
}

fn segment(
    id: &'static str,
    label: &'static str,
    active: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .when(active, |segment| segment.bg(theme.composer_background).shadow_sm())
        .text_size(px(11.0))
        .font_weight(if active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
        .text_color(if active { theme.primary } else { theme.secondary })
        .child(label)
        .on_click(on_click)
        .into_any_element()
}

fn installed_row(
    model: &Entity<AppModel>,
    plugin: &InstalledPlugin,
    selected: bool,
    theme: &Theme,
    cx: &mut Context<PluginLibraryView>,
) -> impl IntoElement {
    let id = plugin.id().to_string();
    let is_settings = id == "codes.ssh.cos.settings";
    let built_in = plugin.manifest.built_in == Some(true);
    let icon_kind = if id == "codes.ssh.cos.computer-use" {
        Icon::Display
    } else if built_in {
        Icon::Gear
    } else {
        Icon::Box
    };
    let subtitle = if plugin.is_enabled {
        plugin.manifest.author.clone()
    } else {
        "Disabled".into()
    };
    let subtitle_color = if plugin.is_enabled { theme.secondary } else { theme::ORANGE };
    let plugin_for_toggle = plugin.clone();
    let model_toggle = model.clone();
    let plugin_menu = plugin.clone();
    let model_menu = model.clone();

    div()
        .id(ElementId::Name(id.clone().into()))
        .mx(px(2.0))
        .px(px(8.0))
        .py(px(7.0))
        .mb(px(1.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.fill_10))
        .when(!selected, |row| row.hover(|style| style.bg(theme.fill_045)))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .child(icon(
            ElementId::Name(id.clone().into()),
            icon_kind,
            px(14.0),
            if built_in { theme::BLUE } else { theme.secondary },
        ))
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
                        .child(plugin.manifest.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(subtitle_color)
                        .whitespace_nowrap()
                        .child(subtitle),
                ),
        )
        .when(!is_settings, |row| {
            row.child(switch(
                ElementId::Name(format!("{id}-switch").into()),
                plugin.is_enabled,
                theme,
                move |enabled, cx| {
                    model_toggle.update(cx, |model, cx| model.set_plugin(&plugin_for_toggle, enabled, cx));
                },
            ))
        })
        .child(plugin_menu_button(
            &plugin_menu,
            &model_menu,
            is_settings,
            built_in,
            theme,
            cx,
        ))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selection = Some(id.to_string());
            cx.notify();
        }))
}

fn marketplace_row(
    model: &Entity<AppModel>,
    listing: &CosMarketplaceListing,
    selected: bool,
    theme: &Theme,
    cx: &mut Context<PluginLibraryView>,
) -> impl IntoElement {
    let id = listing.id.clone();
    let installed = model.read(cx).plugins.iter().any(|plugin| plugin.id() == id);
    let featured = listing.featured == Some(true);
    div()
        .id(ElementId::Name(format!("mp-{id}").into()))
        .mx(px(2.0))
        .px(px(8.0))
        .py(px(7.0))
        .mb(px(1.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.fill_10))
        .when(!selected, |row| row.hover(|style| style.bg(theme.fill_045)))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .child(icon(
            ElementId::Name(format!("mpi-{id}").into()),
            if id == "codes.ssh.cos.computer-use" { Icon::Display } else { Icon::Box },
            px(14.0),
            if featured { theme::BLUE } else { theme.secondary },
        ))
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
                        .child(listing.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(if installed { theme::GREEN } else { theme.secondary })
                        .whitespace_nowrap()
                        .child(if installed { "Installed".to_string() } else { listing.author.clone() }),
                ),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selection = Some(id.to_string());
            cx.notify();
        }))
}

fn plugin_menu_button(
    plugin: &InstalledPlugin,
    model: &Entity<AppModel>,
    is_settings: bool,
    built_in: bool,
    theme: &Theme,
    cx: &mut Context<PluginLibraryView>,
) -> AnyElement {
    if is_settings {
        return div().size(px(22.0)).into_any_element();
    }
    let enabled = plugin.is_enabled;
    let plugin_toggle = plugin.clone();
    let model_toggle = model.clone();
    let plugin_id = plugin.id().to_string();
    div()
        .id(ElementId::Name(format!("menu-{}", plugin.id()).into()))
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme.fill_075))
        .child(icon("ellipsis", Icon::Ellipsis, px(11.0), theme.secondary))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if !built_in {
                // Second action: uninstall; enable/disable lives on the switch.
                if enabled {
                    model_toggle.update(cx, |model, cx| model.set_plugin(&plugin_toggle, false, cx));
                } else {
                    this.pending_plugin_removal = Some(plugin_id.clone());
                }
            } else {
                model_toggle.update(cx, |model, cx| {
                    let mut copy = plugin_toggle.clone();
                    copy.is_enabled = enabled;
                    model.set_plugin(&copy, !enabled, cx);
                });
            }
            cx.notify();
        }))
        .into_any_element()
}

fn plugin_detail(
    model: &Entity<AppModel>,
    plugin: &InstalledPlugin,
    cu_granted: bool,
    cu_status: Option<String>,
    theme: &Theme,
    cx: &mut Context<PluginLibraryView>,
) -> AnyElement {
    let built_in = plugin.manifest.built_in == Some(true);
    let mut detail = div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .max_w(px(560.0))
        .w_full()
        .p(px(30.0));

    // Header
    let mut title_row = div().flex().flex_row().items_center().gap(px(8.0)).child(
        div()
            .text_size(px(22.0))
            .font_weight(FontWeight::SEMIBOLD)
            .child(plugin.manifest.name.clone()),
    );
    if built_in {
        title_row = title_row.child(badge("BUILT IN", theme::BLUE, theme));
    }
    detail = detail.child(
        div()
            .flex()
            .flex_row()
            .gap(px(14.0))
            .child(
                div()
                    .size(px(58.0))
                    .rounded(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(linear_gradient(
                        135.0,
                        linear_color_stop(theme::BLUE, 0.0),
                        linear_color_stop(theme::VIOLET, 1.0),
                    ))
                    .child(icon(
                        "plugin-icon",
                        if built_in { Icon::Gear } else { Icon::Box },
                        px(24.0),
                        gpui::white(),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(title_row)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.secondary)
                            .child(format!("{} · version {}", plugin.manifest.author, plugin.manifest.version)),
                    ),
            ),
    );
    detail = detail.child(
        div()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .whitespace_normal()
            .child(plugin.manifest.description.clone()),
    );
    detail = detail.child(div().h(px(1.0)).w_full().bg(theme.divider));

    // Capabilities
    detail = detail.child(section_label("CAPABILITIES", theme));
    for capability in &plugin.manifest.capabilities {
        let safe = capability.risk == "safe";
        detail = detail.child(
            div()
                .flex()
                .flex_row()
                .gap(px(9.0))
                .child(icon(
                    ElementId::Name(capability.id.clone().into()),
                    if safe { Icon::CheckmarkShield } else { Icon::ExclamationShield },
                    px(14.0),
                    if safe { theme::GREEN } else { theme::ORANGE },
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .font_family("Menlo")
                                .child(capability.id.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.secondary)
                                .whitespace_normal()
                                .child(capability.description.clone()),
                        ),
                ),
        );
    }

    // Computer Use permission
    if plugin.id() == "codes.ssh.cos.computer-use" {
        let model_allow = model.clone();
        let model_open = model.clone();
        detail = detail.child(div().h(px(1.0)).w_full().bg(theme.divider));
        detail = detail.child(section_label("MACOS PERMISSION", theme));
        detail = detail.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(11.0))
                .p(px(12.0))
                .rounded(px(11.0))
                .bg(theme.fill_045)
                .child(icon(
                    "cu-status",
                    if cu_granted { Icon::CheckmarkShield } else { Icon::HandRaised },
                    px(16.0),
                    if cu_granted { theme::GREEN } else { theme::ORANGE },
                ))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(if cu_granted {
                                    "Accessibility access granted"
                                } else {
                                    "Accessibility access required"
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(theme.secondary)
                                .whitespace_normal()
                                .child(cu_status.unwrap_or_else(|| {
                                    "Cos needs this permission to read and operate visible Mac apps.".into()
                                })),
                        ),
                )
                .when(cu_granted, |row| {
                    row.child(div().text_size(px(11.0)).text_color(theme::GREEN).child("Ready"))
                })
                .when(!cu_granted, |row| {
                    row.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .id("cu-allow")
                                    .px(px(10.0))
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.0))
                                    .bg(theme::BLUE)
                                    .text_color(gpui::white())
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .child("Allow Access…")
                                    .on_click(move |_, _, cx| {
                                        model_allow.update(cx, |model, cx| model.request_computer_use_access(cx));
                                    }),
                            )
                            .child(
                                div()
                                    .id("cu-open-settings")
                                    .text_size(px(10.5))
                                    .text_color(theme.secondary)
                                    .cursor_pointer()
                                    .child("Open Settings")
                                    .on_click(move |_, _, cx| {
                                        model_open.update(cx, |model, _cx| model.open_accessibility_settings());
                                    }),
                            ),
                    )
                }),
        );
    }

    // Skills
    if !plugin.manifest.skills.is_empty() {
        detail = detail.child(div().h(px(1.0)).w_full().bg(theme.divider));
        detail = detail.child(section_label("SKILLS", theme));
        let mut skills_list = div().flex().flex_col().rounded(px(10.0)).bg(theme.fill_045);
        for skill in &plugin.manifest.skills {
            let enabled = model.read(cx).is_skill_enabled(skill, plugin);
            let model_toggle = model.clone();
            let skill_toggle = skill.clone();
            let plugin_toggle = plugin.clone();
            let skill_menu = skill.clone();
            let plugin_id_menu = plugin.id().to_string();
            skills_list = skills_list.child(
                div()
                    .h(px(42.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(10.0))
                    .border_b_1()
                    .border_color(theme.divider.opacity(0.35))
                    .child(icon(
                        ElementId::Name(skill.clone().into()),
                        Icon::WandStars,
                        px(11.0),
                        if enabled { theme::BLUE } else { theme.secondary },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(skill.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(theme.secondary)
                                    .child(if enabled { "Enabled" } else { "Disabled" }),
                            ),
                    )
                    .child(switch(
                        ElementId::Name(format!("skill-{skill}").into()),
                        enabled,
                        theme,
                        move |enabled, cx| {
                            model_toggle.update(cx, |model, cx| {
                                model.set_skill(&skill_toggle, &plugin_toggle, enabled, cx);
                            });
                        },
                    ))
                    .child(
                        div()
                            .id(ElementId::Name(format!("skill-menu-{skill}").into()))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme.fill_075))
                            .child(icon("skill-ellipsis", Icon::Ellipsis, px(10.5), theme.secondary))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if built_in {
                                    // Built-in skills can only be toggled.
                                } else {
                                    this.skill_pending_removal =
                                        Some((skill_menu.clone(), plugin_id_menu.clone()));
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        detail = detail.child(skills_list);
    }

    // Footer
    let mut footer = div().flex().flex_row().items_center().gap(px(10.0)).child(div().flex_1());
    if plugin.id() != "codes.ssh.cos.settings" {
        let model_toggle = model.clone();
        let plugin_toggle = plugin.clone();
        let enabled = plugin.is_enabled;
        footer = footer.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(div().text_size(px(11.0)).text_color(theme.secondary).child("Enabled"))
                .child(switch(ElementId::Name("plugin-enabled".into()), enabled, theme, move |enabled, cx| {
                    model_toggle.update(cx, |model, cx| model.set_plugin(&plugin_toggle, enabled, cx));
                })),
        );
    }
    footer = footer.child(
        div()
            .text_size(px(11.0))
            .text_color(if plugin.is_trusted { theme::GREEN } else { theme::ORANGE })
            .child(if plugin.is_trusted { "Trusted" } else { "Review required" }),
    );
    detail = detail.child(div().flex_1()).child(footer);

    detail.into_any_element()
}

fn marketplace_detail(
    model: &Entity<AppModel>,
    listing: &CosMarketplaceListing,
    installing: bool,
    theme: &Theme,
    cx: &mut Context<PluginLibraryView>,
) -> AnyElement {
    let installed = model
        .read(cx)
        .plugins
        .iter()
        .find(|plugin| plugin.id() == listing.id)
        .cloned();
    let mut detail = div()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .max_w(px(560.0))
        .w_full()
        .p(px(30.0));

    let mut title_row = div().flex().flex_row().items_center().gap(px(8.0)).child(
        div()
            .text_size(px(22.0))
            .font_weight(FontWeight::SEMIBOLD)
            .child(listing.name.clone()),
    );
    if listing.featured == Some(true) {
        title_row = title_row.child(badge("OFFICIAL", theme::BLUE, theme));
    }
    detail = detail.child(
        div()
            .flex()
            .flex_row()
            .gap(px(14.0))
            .child(
                div()
                    .size(px(58.0))
                    .rounded(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::BLUE.opacity(0.14))
                    .child(icon(
                        "mp-icon",
                        if listing.id == "codes.ssh.cos.computer-use" { Icon::Display } else { Icon::Box },
                        px(23.0),
                        theme::BLUE,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(title_row)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.secondary)
                            .child(format!("{} · version {}", listing.author, listing.version)),
                    ),
            ),
    );
    detail = detail.child(
        div()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .whitespace_normal()
            .child(listing.description.clone()),
    );

    if let Some(tags) = &listing.tags {
        if !tags.is_empty() {
            let mut tag_row = div().flex().flex_row().flex_wrap().gap(px(7.0));
            for tag in tags {
                tag_row = tag_row.child(
                    div()
                        .px(px(8.0))
                        .py(px(5.0))
                        .rounded_full()
                        .bg(theme.fill_055)
                        .text_size(px(10.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(tag.clone()),
                );
            }
            detail = detail.child(tag_row);
        }
    }

    if let Some(manifest) = &listing.manifest {
        if !manifest.capabilities.is_empty() {
            detail = detail.child(div().h(px(1.0)).w_full().bg(theme.divider));
            detail = detail.child(section_label("CAPABILITIES", theme));
            for capability in &manifest.capabilities {
                let safe = capability.risk == "safe";
                detail = detail.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(9.0))
                        .child(icon(
                            ElementId::Name(format!("mpc-{}", capability.id).into()),
                            if safe { Icon::CheckmarkShield } else { Icon::ExclamationShield },
                            px(14.0),
                            if safe { theme::GREEN } else { theme::ORANGE },
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .font_family("Menlo")
                                        .child(capability.id.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(theme.secondary)
                                        .whitespace_normal()
                                        .child(capability.description.clone()),
                                ),
                        ),
                );
            }
        }
    }

    // Footer with install control
    let mut footer = div().flex().flex_row().items_center().gap(px(10.0));
    let listing_id = listing.id.clone();
    footer = footer.child(
        div()
            .id("view-on-site")
            .text_size(px(11.5))
            .text_color(theme::BLUE)
            .cursor_pointer()
            .child("View on cos.ssh.codes")
            .on_click(move |_, _, _| {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(format!("https://cos.ssh.codes/plugins/{listing_id}"))
                    .spawn();
            }),
    );
    footer = footer.child(div().flex_1());
    if installing {
        footer = footer.child(super::chat::spinner(theme));
    } else if listing.kind != "plugin" {
        footer = footer.child(div().text_size(px(11.5)).text_color(theme.secondary).child("Template"));
    } else if listing.id == "codes.ssh.cos.computer-use"
        && installed.is_some()
        && !model.read(cx).computer_use_access_granted
    {
        let model = model.clone();
        let listing = listing.clone();
        footer = footer.child(primary_button("Allow Access…", theme, move |cx| {
            model.update(cx, |model, cx| model.install_marketplace_plugin(&listing, cx));
        }));
    } else if let Some(installed) = &installed {
        if installed.manifest.version == listing.version {
            footer = footer.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(5.0))
                    .child(icon("installed", Icon::CheckmarkCircle, px(13.0), theme::GREEN))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::GREEN)
                            .child(if listing.built_in == Some(true) { "Included" } else { "Installed" }),
                    ),
            );
        } else {
            let model = model.clone();
            let listing = listing.clone();
            footer = footer.child(primary_button("Update", theme, move |cx| {
                model.update(cx, |model, cx| model.install_marketplace_plugin(&listing, cx));
            }));
        }
    } else {
        let model = model.clone();
        let listing = listing.clone();
        footer = footer.child(primary_button("Install", theme, move |cx| {
            model.update(cx, |model, cx| model.install_marketplace_plugin(&listing, cx));
        }));
    }
    detail = detail.child(div().flex_1()).child(footer);
    detail.into_any_element()
}

fn empty_detail(text: &str, icon_kind: Icon, theme: &Theme) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .child(icon("empty", icon_kind, px(24.0), theme.secondary))
        .child(div().text_size(px(13.0)).text_color(theme.secondary).child(text.to_string()))
        .into_any_element()
}

fn section_label(text: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(9.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.tertiary)
        .child(text.to_string())
}

fn badge(text: &str, color: Hsla, _theme: &Theme) -> Div {
    div()
        .px(px(6.0))
        .py(px(3.0))
        .rounded_full()
        .bg(color.opacity(0.1))
        .text_size(px(8.0))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(text.to_string())
}

pub(crate) fn switch(
    id: ElementId,
    on: bool,
    theme: &Theme,
    on_change: impl Fn(bool, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(30.0))
        .h(px(18.0))
        .rounded_full()
        .cursor_pointer()
        .flex()
        .items_center()
        .px(px(2.0))
        .when(on, |switch| switch.bg(theme::GREEN).justify_end())
        .when(!on, |switch| switch.bg(theme.fill_12).justify_start())
        .child(div().size(px(14.0)).rounded_full().bg(gpui::white()))
        .on_click(move |_, _, cx| on_change(!on, cx))
}

fn primary_button(label: &'static str, _theme: &Theme, action: impl Fn(&mut App) + 'static) -> impl IntoElement {
    div()
        .id(ElementId::Name(label.into()))
        .px(px(12.0))
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded(px(7.0))
        .bg(theme::BLUE)
        .text_color(gpui::white())
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|style| style.bg(theme::BLUE.opacity(0.85)))
        .child(label)
        .on_click(move |_, _, cx| action(cx))
}

fn confirm_overlay(
    title: &str,
    message: &str,
    theme: &Theme,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.4))
        .child(
            div()
                .w(px(300.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(16.0))
                .rounded(px(12.0))
                .bg(theme.composer_background)
                .border_1()
                .border_color(theme.divider)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.secondary)
                        .whitespace_normal()
                        .child(message.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap(px(8.0))
                        .pt(px(6.0))
                        .child(
                            div()
                                .id("confirm-cancel")
                                .px(px(10.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .hover(|style| style.bg(theme.fill_075))
                                .child("Cancel")
                                .on_click(on_cancel),
                        )
                        .child(
                            div()
                                .id("confirm-delete")
                                .px(px(10.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .bg(theme::RED)
                                .text_color(gpui::white())
                                .text_size(px(11.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Move to Trash")
                                .on_click(on_confirm),
                        ),
                ),
        )
}
