import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 205, ideal: CosTheme.sidebarWidth, max: 300)
        } detail: {
            ChatView()
        }
        .navigationSplitViewStyle(.balanced)
        .toolbarBackground(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor), for: .windowToolbar)
        .toolbarBackground(.visible, for: .windowToolbar)
        .background(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor))
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
