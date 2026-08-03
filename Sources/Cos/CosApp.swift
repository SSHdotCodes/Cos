import AppKit
import SwiftUI

@main
struct CosApp: App {
    @NSApplicationDelegateAdaptor(CosAppDelegate.self) private var appDelegate
    @StateObject private var model: AppModel

    init() {
        let builtIns = Bundle.module.url(forResource: "BuiltInPlugins", withExtension: nil)
            ?? Bundle.module.resourceURL
        _model = StateObject(wrappedValue: AppModel(builtInPluginsURL: builtIns))
    }

    var body: some Scene {
        WindowGroup("Cos", id: "main") {
            ContentView()
                .environmentObject(model)
                .preferredColorScheme(model.preferences.appearance.preferredColorScheme)
                .background(WindowChromeConfigurator(trueDark: model.preferences.appearance == .trueDark))
                .frame(minWidth: 900, minHeight: 620)
        }
        .defaultSize(width: 1180, height: 780)
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Task") { model.newThread() }
                    .keyboardShortcut("n", modifiers: .command)
            }
            CommandMenu("Task") {
                Button(model.isRunning ? "Stop" : "Run") {
                    if model.isRunning { model.cancel() }
                }
                .keyboardShortcut(".", modifiers: .command)
                .disabled(!model.isRunning)
                Divider()
                Button("Choose Workspace…") { model.chooseWorkspace() }
            }
        }

        Settings {
            SettingsRootView()
                .environmentObject(model)
                .preferredColorScheme(model.preferences.appearance.preferredColorScheme)
                .background(WindowChromeConfigurator(trueDark: model.preferences.appearance == .trueDark))
                .frame(minWidth: 780, minHeight: 570)
        }
    }
}

final class CosAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
}

private struct WindowChromeConfigurator: NSViewRepresentable {
    let trueDark: Bool

    func makeNSView(context: Context) -> WindowChromeProbe {
        let view = WindowChromeProbe()
        view.trueDark = trueDark
        return view
    }

    func updateNSView(_ view: WindowChromeProbe, context: Context) {
        view.trueDark = trueDark
        view.apply()
    }
}

private final class WindowChromeProbe: NSView {
    var trueDark = false

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        apply()
    }

    func apply() {
        guard let window else { return }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0
            window.backgroundColor = trueDark ? .black : .windowBackgroundColor
            window.titlebarAppearsTransparent = trueDark
            window.titleVisibility = trueDark ? .hidden : .visible
        }
    }
}
