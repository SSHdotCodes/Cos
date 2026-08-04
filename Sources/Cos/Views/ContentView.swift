import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 205, ideal: CosTheme.sidebarWidth, max: 300)
        } detail: {
            ChatView()
                .inspector(isPresented: Binding(
                    get: { model.isBrowserPanelPresented && model.isBetterWrightEnabled },
                    set: { model.isBrowserPanelPresented = $0 }
                )) {
                    BetterWrightBrowserPanel(sessionID: model.selectedThreadID?.uuidString ?? "default")
                        .environmentObject(model)
                        .inspectorColumnWidth(min: 420, ideal: 520, max: 720)
                }
        }
        .navigationSplitViewStyle(.balanced)
        .toolbarBackground(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor), for: .windowToolbar)
        .toolbarBackground(.visible, for: .windowToolbar)
        .background(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor))
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            model.refreshComputerUseAccess()
        }
        .sheet(isPresented: $model.isPluginLibraryPresented) {
            PluginLibraryView()
                .environmentObject(model)
                .frame(minWidth: 760, minHeight: 560)
        }
        .alert("Cos needs attention", isPresented: Binding(
            get: { model.lastError != nil },
            set: { if !$0 { model.lastError = nil } }
        )) {
            Button("OK", role: .cancel) { model.lastError = nil }
        } message: {
            Text(model.lastError ?? "")
        }
    }
}
