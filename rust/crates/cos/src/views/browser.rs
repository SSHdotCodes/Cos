//! BetterWright browser inspector panel (BetterWrightBrowserPanel.swift +
//! BetterWrightBrowserController.swift). Drives the `bw` CLI live-view and
//! hosts a WKWebView overlay positioned over the panel content area.

use crate::icons::{icon, Icon};
use crate::state::AppModel;
use crate::theme::{self, Theme};
use cos_core::CosBetterWrightRuntime;
use gpui::{prelude::FluentBuilder, *};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    SetupRequired,
    Installing,
    Launching,
    Ready(String),
    Failed(String),
}

pub struct BrowserPanel {
    phase: Phase,
    session: String,
    open_requested: bool,
    generation: u64,
    viewer: Option<std::process::Child>,
    viewer_lines: Arc<Mutex<Vec<String>>>,
    overlay: WebViewOverlay,
    anchor_bounds: Arc<Mutex<Option<(Bounds<Pixels>, Pixels)>>>,
    poll_active: bool,
}

impl BrowserPanel {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            phase: Phase::Idle,
            session: "default".into(),
            open_requested: false,
            generation: 0,
            viewer: None,
            viewer_lines: Arc::new(Mutex::new(Vec::new())),
            overlay: WebViewOverlay::default(),
            anchor_bounds: Arc::new(Mutex::new(None)),
            poll_active: false,
        }
    }

    pub fn open(&mut self, raw_session: &str, cx: &mut Context<Self>) {
        let session = CosBetterWrightRuntime::sanitized_session(raw_session);
        if self.open_requested && session == self.session && matches!(self.phase, Phase::Ready(_)) {
            return;
        }
        if self.open_requested && session == self.session && !matches!(self.phase, Phase::Idle | Phase::Failed(_)) {
            return;
        }
        self.session = session;
        self.open_requested = true;
        self.begin_readiness_check(cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open_requested = false;
        self.generation += 1;
        self.stop_viewer();
        self.phase = Phase::Idle;
        self.sync_overlay();
        cx.notify();
    }

    pub fn install_and_open(&mut self, cx: &mut Context<Self>) {
        self.stop_viewer();
        self.phase = Phase::Installing;
        cx.notify();
        self.generation += 1;
        let generation = self.generation;
        let session = self.session.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        crate::state::tokio_runtime().spawn(async move {
            let result = async {
                let result = CosBetterWrightRuntime::setup().await?;
                if result.status != 0 {
                    let detail = format!("{}\n{}", result.error_output, result.output)
                        .trim()
                        .to_string();
                    return Err(cos_core::BetterWrightRuntimeError::Failed(
                        if detail.is_empty() {
                            "BetterWright setup failed.".into()
                        } else {
                            detail
                        },
                    ));
                }
                if !CosBetterWrightRuntime::is_ready().await {
                    return Err(cos_core::BetterWrightRuntimeError::Failed(
                        "BetterWright installed its browser, but the readiness check did not pass.".into(),
                    ));
                }
                CosBetterWrightRuntime::prepare_for_viewing(session).await
            }
            .await;
            let _ = sender.send(result);
        });
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |panel, cx| {
                if panel.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok(())) => panel.launch_viewer(cx),
                    Ok(Err(error)) => {
                        panel.phase = Phase::Failed(error.to_string());
                        cx.notify();
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.begin_readiness_check(cx);
    }

    pub fn restart_viewer(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase, Phase::Ready(_)) {
            self.launch_viewer(cx);
        } else {
            self.begin_readiness_check(cx);
        }
    }

    pub fn close_active_tab(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        crate::state::tokio_runtime().spawn(async move {
            let closed = CosBetterWrightRuntime::run_browser("await closePage(); return 'closed'".into(), session.clone()).await;
            let prepared = CosBetterWrightRuntime::prepare_for_viewing(session).await;
            let _ = sender.send(closed.and_then(|_| prepared));
        });
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |panel, cx| {
                if let Ok(Err(error)) = result {
                    panel.phase = Phase::Failed(format!("Could not close the browser tab: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn begin_readiness_check(&mut self, cx: &mut Context<Self>) {
        self.stop_viewer();
        self.phase = Phase::Checking;
        cx.notify();
        self.generation += 1;
        let generation = self.generation;
        let session = self.session.clone();
        let (sender, receiver) = futures::channel::oneshot::channel();
        crate::state::tokio_runtime().spawn(async move {
            let ready = CosBetterWrightRuntime::is_ready().await;
            let result = if ready {
                CosBetterWrightRuntime::prepare_for_viewing(session)
                    .await
                    .map(|_| true)
            } else {
                Ok(false)
            };
            let _ = sender.send(result);
        });
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = receiver.await;
            let _ = this.update(cx, |panel, cx| {
                if panel.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok(true)) => panel.launch_viewer(cx),
                    Ok(Ok(false)) => {
                        panel.phase = Phase::SetupRequired;
                        cx.notify();
                    }
                    Ok(Err(error)) => {
                        panel.phase = Phase::Failed(error.to_string());
                        cx.notify();
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    fn launch_viewer(&mut self, cx: &mut Context<Self>) {
        self.stop_viewer();
        self.phase = Phase::Launching;
        self.generation += 1;
        let generation = self.generation;
        let invocation = CosBetterWrightRuntime::invocation(&[
            "view".into(),
            "--expose".into(),
            "local".into(),
            "--session".into(),
            self.session.clone(),
            "--profile".into(),
            CosBetterWrightRuntime::PROFILE.into(),
        ]);
        let invocation = match invocation {
            Ok(invocation) => invocation,
            Err(error) => {
                self.phase = Phase::Failed(error.to_string());
                cx.notify();
                return;
            }
        };
        let mut command = std::process::Command::new(&invocation.executable);
        command.args(&invocation.arguments);
        command.envs(invocation.environment.iter().cloned());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.phase = Phase::Failed(format!("BetterWright could not start its live view: {error}"));
                cx.notify();
                return;
            }
        };
        let lines = self.viewer_lines.clone();
        lines.lock().unwrap().clear();
        if let Some(stdout) = child.stdout.take() {
            let lines = lines.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => lines.lock().unwrap().push(line),
                        Err(_) => break,
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let lines = lines.clone();
            std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(line) => lines.lock().unwrap().push(line),
                        Err(_) => break,
                    }
                }
            });
        }
        self.viewer = Some(child);
        cx.notify();
        self.start_viewer_polling(generation, cx);
    }

    fn start_viewer_polling(&mut self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let mut elapsed_ms = 0u64;
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(200)).await;
                elapsed_ms += 200;
                let alive = this
                    .update(cx, |panel, cx| {
                        if panel.generation != generation || !matches!(panel.phase, Phase::Launching) {
                            return false;
                        }
                        let buffer = panel.viewer_lines.lock().unwrap().join("\n");
                        if let Some(url) = parse_live_view_url(&buffer) {
                            panel.phase = Phase::Ready(url);
                            panel.sync_overlay();
                            cx.notify();
                            return true; // keep polling to detect early exit
                        }
                        let exited = panel
                            .viewer
                            .as_mut()
                            .map(|child| child.try_wait().ok().flatten().is_some())
                            .unwrap_or(false);
                        if exited {
                            let detail = buffer.trim().to_string();
                            panel.phase = Phase::Failed(if detail.is_empty() {
                                "BetterWright could not start its live view.".into()
                            } else {
                                detail.chars().skip(detail.len().saturating_sub(2_000)).collect()
                            });
                            panel.stop_viewer();
                            cx.notify();
                            return false;
                        }
                        if elapsed_ms > 20_000 {
                            panel.stop_viewer();
                            panel.phase = Phase::Failed(
                                "BetterWright started, but its local live view did not become ready.".into(),
                            );
                            cx.notify();
                            return false;
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
            // Watch for an unexpected viewer exit while ready.
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
                let alive = this
                    .update(cx, |panel, cx| {
                        if panel.generation != generation {
                            return false;
                        }
                        if !matches!(panel.phase, Phase::Ready(_)) {
                            return false;
                        }
                        let exited = panel
                            .viewer
                            .as_mut()
                            .map(|child| child.try_wait().ok().flatten())
                            .unwrap_or(None);
                        if let Some(status) = exited {
                            panel.phase = Phase::Failed(format!(
                                "The BetterWright live view ended unexpectedly (exit {}).",
                                status.code().unwrap_or(-1)
                            ));
                            panel.stop_viewer();
                            panel.sync_overlay();
                            cx.notify();
                            return false;
                        }
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn stop_viewer(&mut self) {
        if let Some(mut child) = self.viewer.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.viewer_lines.lock().unwrap().clear();
    }

    fn sync_overlay(&mut self) {
        let url = match &self.phase {
            Phase::Ready(url) => Some(url.clone()),
            _ => None,
        };
        let visible = self.open_requested && url.is_some();
        self.overlay.set_target(if visible { url } else { None });
        self.overlay.sync(&self.anchor_bounds);
    }
}

pub fn parse_live_view_url(buffer: &str) -> Option<String> {
    static ANSI: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new("\u{001B}\\[[0-9;]*m").unwrap());
    let clean = ANSI.replace_all(buffer, "");
    let label = clean.find("Live view:")?;
    let remainder = clean[label + "Live view:".len()..].trim();
    let candidate = remainder.split_whitespace().next()?;
    let url = url::Url::parse(candidate).ok()?;
    if url.scheme() != "http" {
        return None;
    }
    let host = url.host_str().unwrap_or("").to_lowercase();
    if !["127.0.0.1", "localhost", "::1"].contains(&host.as_str()) {
        return None;
    }
    Some(candidate.to_string())
}

pub fn browser_panel(
    _model: &Entity<AppModel>,
    panel: &Entity<BrowserPanel>,
    theme: &Theme,
    cx: &mut App,
) -> Div {
    let (phase, session_id) = {
        let panel = panel.read(cx);
        (panel.phase.clone(), panel.session.clone())
    };
    let _ = session_id;

    let mut content: AnyElement = match &phase {
        Phase::Ready(url) => {
            let panel = panel.clone();
            let url = url.clone();
            webview_anchor(panel, url).into_any_element()
        }
        Phase::SetupRequired => centered_state(
            Icon::UpdateAvailable,
            "Install agentic browser",
            &format!(
                "Cos includes BetterWright {}. Its managed browser downloads once (about 200 MB) and stays off when you are not using it.",
                CosBetterWrightRuntime::PACKAGE_VERSION
            ),
            Some(("Install Browser", BrowserAction::Install)),
            panel,
            theme,
        ),
        Phase::Installing => progress_state(
            "Installing Browser…",
            "Downloading and verifying BetterWright's managed browser.",
            theme,
        ),
        Phase::Checking => progress_state(
            "Checking Browser…",
            "Looking for the managed BetterWright runtime.",
            theme,
        ),
        Phase::Launching => progress_state(
            "Opening Browser…",
            "Connecting this panel to the task's live session.",
            theme,
        ),
        Phase::Failed(message) => centered_state(
            Icon::Warning,
            "Browser unavailable",
            message,
            Some(("Try Again", BrowserAction::Retry)),
            panel,
            theme,
        ),
        Phase::Idle => progress_state("Opening Browser…", "Preparing the local live view.", theme),
    };

    let ready = matches!(phase, Phase::Ready(_));
    let mut header = div()
        .h(px(52.0))
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .px(px(12.0))
        .child(icon("globe", Icon::Globe, px(12.0), theme.primary))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Browser"),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(theme.secondary)
                        .child("Local · Interactive"),
                ),
        )
        .child(div().flex_1());
    if ready {
        let panel_close_tab = panel.clone();
        let panel_restart = panel.clone();
        header = header
            .child(
                div()
                    .id("close-browser-tab")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon("xmark-square", Icon::XmarkSquare, px(13.0), theme.secondary))
                    .on_click(move |_, _, cx| {
                        panel_close_tab.update(cx, |panel, cx| panel.close_active_tab(cx));
                    }),
            )
            .child(
                div()
                    .id("reconnect-browser")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.fill_075))
                    .child(icon("restart", Icon::Refresh, px(12.0), theme.secondary))
                    .on_click(move |_, _, cx| {
                        panel_restart.update(cx, |panel, cx| panel.restart_viewer(cx));
                    }),
            );
    }
    let model_close = _model.clone();
    header = header.child(
        div()
            .id("close-browser-panel")
            .size(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme.fill_075))
            .child(icon("xmark-browser", Icon::Xmark, px(11.0), theme.secondary))
            .on_click(move |_, _, cx| {
                model_close.update(cx, |model, cx| {
                    model.is_browser_panel_presented = false;
                    cx.notify();
                });
            }),
    );

    div()
        .w(px(520.0))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .bg(theme.window_background)
        .border_l_1()
        .border_color(theme.divider.opacity(0.55))
        .child(header)
        .child(div().h(px(1.0)).w_full().bg(theme.divider.opacity(0.55)))
        .child(div().flex_1().relative().child(std::mem::replace(&mut content, div().into_any_element())))
}

#[derive(Clone, Copy)]
enum BrowserAction {
    Install,
    Retry,
}

fn centered_state(
    icon_kind: Icon,
    title: &str,
    detail: &str,
    action: Option<(&str, BrowserAction)>,
    panel: &Entity<BrowserPanel>,
    theme: &Theme,
) -> AnyElement {
    let mut container = div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(13.0))
        .p(px(28.0))
        .child(icon("state-icon", icon_kind, px(25.0), theme.secondary))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme.secondary)
                .whitespace_normal()
                .line_height(px(15.0))
                .max_w(px(290.0))
                .child(detail.to_string()),
        );
    if let Some((label, action)) = action {
        let panel = panel.clone();
        let label = label.to_string();
        container = container.child(
            div()
                .id("browser-action")
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
                .child(label)
                .on_click(move |_, _, cx| {
                    panel.update(cx, |panel, cx| match action {
                        BrowserAction::Install => panel.install_and_open(cx),
                        BrowserAction::Retry => panel.retry(cx),
                    });
                }),
        );
    }
    container.into_any_element()
}

fn progress_state(title: &str, detail: &str, theme: &Theme) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .p(px(28.0))
        .child(super::chat::spinner(theme))
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
                .line_height(px(15.0))
                .max_w(px(280.0))
                .child(detail.to_string()),
        )
        .into_any_element()
}

// MARK: - WKWebView overlay

/// Positions + hosts the WKWebView. The anchor element reports its bounds
/// each paint; the overlay applies them to the native view.
pub struct WebViewOverlay {
    webview: Option<objc2::rc::Retained<objc2_web_kit::WKWebView>>,
    target_url: Option<String>,
    loaded_url: Option<String>,
    last_applied: Option<(Bounds<Pixels>, Pixels)>,
}

impl Default for WebViewOverlay {
    fn default() -> Self {
        Self {
            webview: None,
            target_url: None,
            loaded_url: None,
            last_applied: None,
        }
    }
}

impl WebViewOverlay {
    fn set_target(&mut self, url: Option<String>) {
        if self.target_url != url {
            self.target_url = url;
            self.last_applied = None; // force re-apply
        }
    }

    fn sync(&mut self, anchor_bounds: &Arc<Mutex<Option<(Bounds<Pixels>, Pixels)>>>) {
        let bounds = *anchor_bounds.lock().unwrap();
        let Some(mtm) = objc2::MainThreadMarker::new() else { return };
        unsafe {
            match (self.target_url.clone(), bounds) {
                (Some(url), Some((bounds, window_height))) => {
                    if self.webview.is_none() {
                        use objc2_app_kit::NSApplication;
                        use objc2_foundation::{NSURL, NSURLRequest};
                        let app = NSApplication::sharedApplication(mtm);
                        let window = app.mainWindow().or_else(|| app.windows().firstObject());
                        let Some(window) = window else { return };
                        let Some(content_view) = window.contentView() else { return };
                        let config = objc2_web_kit::WKWebViewConfiguration::new(mtm);
                        let frame = objc2_core_foundation::CGRect {
                            origin: objc2_core_foundation::CGPoint { x: 0.0, y: 0.0 },
                            size: objc2_core_foundation::CGSize { width: 100.0, height: 100.0 },
                        };
                        let webview = objc2_web_kit::WKWebView::initWithFrame_configuration(
                            mtm.alloc(),
                            frame,
                            &config,
                        );
                        content_view.addSubview(&webview);
                        if let Some(ns_url) = NSURL::URLWithString(&objc2_foundation::NSString::from_str(&url)) {
                            let request = NSURLRequest::requestWithURL(&ns_url);
                            webview.loadRequest(&request);
                            self.loaded_url = Some(url.clone());
                        }
                        self.webview = Some(webview);
                    }
                    if let Some(webview) = &self.webview {
                        if self.loaded_url.as_deref() != Some(url.as_str()) {
                            use objc2_foundation::{NSURL, NSURLRequest};
                            if let Some(ns_url) = NSURL::URLWithString(&objc2_foundation::NSString::from_str(&url)) {
                                let request = NSURLRequest::requestWithURL(&ns_url);
                                webview.loadRequest(&request);
                                self.loaded_url = Some(url);
                            }
                        }
                        if self.last_applied != Some((bounds, window_height)) {
                            let frame = objc2_core_foundation::CGRect {
                                origin: objc2_core_foundation::CGPoint {
                                    x: f64::from(bounds.origin.x),
                                    y: f64::from(window_height - bounds.origin.y - bounds.size.height),
                                },
                                size: objc2_core_foundation::CGSize {
                                    width: f64::from(bounds.size.width),
                                    height: f64::from(bounds.size.height),
                                },
                            };
                            webview.setFrame(frame);
                            webview.setHidden(false);
                            self.last_applied = Some((bounds, window_height));
                        }
                    }
                }
                _ => {
                    if let Some(webview) = &self.webview {
                        webview.setHidden(true);
                    }
                    self.last_applied = None;
                }
            }
        }
    }
}

impl Drop for WebViewOverlay {
    fn drop(&mut self) {
        if let Some(webview) = self.webview.take() {
            unsafe { webview.removeFromSuperview() };
        }
    }
}

/// Invisible anchor: reports its bounds to the overlay on every paint.
fn webview_anchor(panel: Entity<BrowserPanel>, _url: String) -> Div {
    div()
        .w_full()
        .h_full()
        .child(AnchorElement { panel })
}

struct AnchorElement {
    panel: Entity<BrowserPanel>,
}

impl IntoElement for AnchorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AnchorElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let window_height = window.viewport_size().height;
        self.panel
            .read(_cx)
            .anchor_bounds
            .lock()
            .unwrap()
            .replace((bounds, window_height));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        cx: &mut App,
    ) {
        // Apply the (possibly new) bounds to the native overlay.
        let anchor = self.panel.read(cx).anchor_bounds.clone();
        self.panel.update(cx, |panel, _| {
            panel.overlay.sync(&anchor);
        });
    }
}
