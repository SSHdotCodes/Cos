import SwiftUI
import WebKit

struct BetterWrightBrowserPanel: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var controller = BetterWrightBrowserController()

    let sessionID: String

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.55)
            content
        }
        .background(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor))
        .task(id: sessionID) {
            controller.open(session: sessionID)
        }
        .onDisappear {
            controller.close()
        }
    }

    private var header: some View {
        HStack(spacing: 9) {
            Image(systemName: "globe")
                .font(.system(size: 12, weight: .semibold))
            VStack(alignment: .leading, spacing: 1) {
                Text("Browser")
                    .font(.system(size: 12.5, weight: .semibold))
                Text("Local · Interactive")
                    .font(.system(size: 9.5))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if case .ready = controller.phase {
                Button { controller.closeActiveTab() } label: {
                    Image(systemName: "xmark.square")
                        .frame(width: 26, height: 26)
                }
                .buttonStyle(.plain)
                .help("Close browser tab")

                Button { controller.restartViewer() } label: {
                    Image(systemName: "arrow.clockwise")
                        .frame(width: 26, height: 26)
                }
                .buttonStyle(.plain)
                .help("Reconnect live view")
            }
            Button {
                model.isBrowserPanelPresented = false
            } label: {
                Image(systemName: "xmark")
                    .frame(width: 26, height: 26)
            }
            .buttonStyle(.plain)
            .help("Close Browser")
        }
        .padding(.horizontal, 12)
        .frame(height: 52)
    }

    @ViewBuilder
    private var content: some View {
        switch controller.phase {
        case .ready(let url):
            BetterWrightWebView(url: url) { message in
                controller.webViewFailed("The local browser view could not load: \(message)")
            }
        case .setupRequired:
            BrowserCenteredState(
                icon: "arrow.down.circle",
                title: "Install agentic browser",
                detail: "Cos includes BetterWright 1.6.3. Its managed browser downloads once (about 200 MB) and stays off when you are not using it.",
                actionTitle: "Install Browser",
                action: controller.installAndOpen
            )
        case .installing:
            BrowserProgressState(
                title: "Installing Browser…",
                detail: "Downloading and verifying BetterWright's managed browser."
            )
        case .checking:
            BrowserProgressState(title: "Checking Browser…", detail: "Looking for the managed BetterWright runtime.")
        case .launching:
            BrowserProgressState(title: "Opening Browser…", detail: "Connecting this panel to the task's live session.")
        case .failed(let message):
            BrowserCenteredState(
                icon: "exclamationmark.triangle",
                title: "Browser unavailable",
                detail: message,
                actionTitle: "Try Again",
                action: controller.retry
            )
        case .idle:
            BrowserProgressState(title: "Opening Browser…", detail: "Preparing the local live view.")
        }
    }
}

private struct BrowserProgressState: View {
    let title: String
    let detail: String

    var body: some View {
        VStack(spacing: 12) {
            ProgressView().controlSize(.small)
            Text(title).font(.system(size: 13, weight: .semibold))
            Text(detail)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 280)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(28)
    }
}

private struct BrowserCenteredState: View {
    let icon: String
    let title: String
    let detail: String
    let actionTitle: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 13) {
            Image(systemName: icon)
                .font(.system(size: 25, weight: .light))
                .foregroundStyle(.secondary)
            Text(title).font(.system(size: 14, weight: .semibold))
            Text(detail)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 290)
            Button(actionTitle, action: action)
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(28)
    }
}

private struct BetterWrightWebView: NSViewRepresentable {
    let url: URL
    let onFailure: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onFailure: onFailure)
    }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let compactStyle = """
        (() => {
          const style = document.createElement('style');
          style.textContent = `
            body { padding: 0 !important; background: #080809 !important; }
            #win { border: 0 !important; border-radius: 0 !important; box-shadow: none !important; }
            #brandbar, #dock, #dims { display: none !important; }
            #tabStrip { padding-top: 2px !important; }
            #toolbar { height: 40px !important; padding: 0 10px !important; }
            #addrPill { height: 28px !important; }
          `;
          document.documentElement.appendChild(style);
        })();
        """
        configuration.userContentController.addUserScript(
            WKUserScript(source: compactStyle, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        )
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.setValue(false, forKey: "drawsBackground")
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.onFailure = onFailure
        guard context.coordinator.loadedURL != url else { return }
        context.coordinator.loadedURL = url
        webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData))
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var loadedURL: URL?
        var onFailure: (String) -> Void

        init(onFailure: @escaping (String) -> Void) {
            self.onFailure = onFailure
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            onFailure(error.localizedDescription)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            onFailure(error.localizedDescription)
        }
    }
}
