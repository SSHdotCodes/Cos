//! Cos — native macOS agentic coding workspace, GPUI edition.
//! Port of CosApp.swift + ContentView.swift.

mod editor;
mod embedded;
mod icons;
mod markdown;
mod prefs;
mod state;
mod theme;
mod views;

use gpui::{prelude::FluentBuilder, *};
use state::AppModel;
use views::{CancelRun, ChooseWorkspace, FocusComposer, MainView, NewTask, OpenPluginLibrary, OpenSettings, ToggleBrowserPanel};

actions!(cos_app, [Quit]);

pub fn app_version() -> (String, i64) {
    (env!("CARGO_PKG_VERSION").to_string(), 1)
}

/// Filesystem-backed asset source so `svg().path(...)` can load the
/// materialized provider logos from Application Support.
struct DiskAssets;

impl AssetSource for DiskAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match std::fs::read(path) {
            Ok(data) => Ok(Some(std::borrow::Cow::Owned(data))),
            Err(_) => Ok(None),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                out.push(entry.path().to_string_lossy().into_owned().into());
            }
        }
        Ok(out)
    }
}

fn main() {
    let app = Application::new().with_assets(DiskAssets);
    app.run(|cx: &mut App| {
        cx.set_menus(menus());
        cx.bind_keys(editor::key_bindings());
        cx.bind_keys([
            KeyBinding::new("cmd-n", NewTask, Some("CosMain")),
            KeyBinding::new("cmd-.", CancelRun, Some("CosMain")),
            KeyBinding::new("cmd-shift-b", ToggleBrowserPanel, Some("CosMain")),
            KeyBinding::new("cmd-,", OpenSettings, Some("CosMain")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &HideApp, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &UnhideAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &Minimize, cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.minimize_window());
            }
        });
        cx.on_action(|_: &Zoom, cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.zoom_window());
            }
        });
        cx.on_action(|_: &CloseWindow, cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.on_action(|_: &OpenAbout, cx| {
            let bounds = Bounds::centered(None, size(px(320.0), px(220.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("About Cos".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    kind: WindowKind::Normal,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| AboutView),
            )
            .ok();
        });

        let model = cx.new(AppModel::new);
        let bounds = Bounds::centered(None, size(px(1180.0), px(780.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(14.0), px(16.0))),
                    }),
                    window_background: WindowBackgroundAppearance::Opaque,
                    app_id: Some("codes.ssh.cos".into()),
                    window_min_size: Some(size(px(900.0), px(620.0))),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| MainView::new(model.clone(), window, cx)),
            )
            .expect("failed to open main window");

        window
            .update(cx, |_, window, _| {
                window.activate_window();
            })
            .ok();

        // Re-apply chrome when appearance changes (true dark ↔ others).
        cx.observe(&model, |_, cx| {
            cx.refresh_windows();
        })
        .detach();
    });
}

struct AboutView;

impl Render for AboutView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .w_full()
            .h_full()
            .child(icons::cos_mark(false, theme::Theme::resolve(cos_core::AppearanceMode::System, window.appearance())))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Cos"),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(hsla(0.0, 0.0, 0.5, 1.0))
                    .child(format!("Version {} ({})", app_version().0, app_version().1)),
            )
    }
}

fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Cos".into(),
            items: vec![
                MenuItem::action("About Cos", OpenAbout),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Cos", HideApp),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", UnhideAll),
                MenuItem::separator(),
                MenuItem::action("Quit Cos", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Task", NewTask),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Cut", editor::EditorCut),
                MenuItem::action("Copy", editor::EditorCopy),
                MenuItem::action("Paste", editor::EditorPaste),
                MenuItem::action("Select All", editor::EditorSelectAll),
            ],
        },
        Menu {
            name: "Task".into(),
            items: vec![
                MenuItem::action("Stop", CancelRun),
                MenuItem::separator(),
                MenuItem::action("Choose Workspace…", ChooseWorkspace),
                MenuItem::action("Focus Composer", FocusComposer),
                MenuItem::separator(),
                MenuItem::action("Plugin Library…", OpenPluginLibrary),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![MenuItem::action("Toggle Browser", ToggleBrowserPanel)],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
            ],
        },
    ]
}

actions!(cos_window, [OpenAbout, HideApp, HideOthers, UnhideAll, CloseWindow, Minimize, Zoom]);
