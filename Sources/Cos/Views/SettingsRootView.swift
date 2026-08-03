import CosCore
import SwiftUI

struct SettingsRootView: View {
    enum Section: String, CaseIterable, Identifiable {
        case general = "General"
        case models = "Models"
        case providers = "Providers"
        case agent = "Agent"
        case plugins = "Plugins"
        case importSkills = "Import"
        case security = "Security"
        case advanced = "Advanced"
        var id: String { rawValue }
        var icon: String {
            switch self {
            case .general: "slider.horizontal.3"
            case .models: "sparkles"
            case .providers: "person.crop.circle.badge.checkmark"
            case .agent: "terminal"
            case .plugins: "shippingbox"
            case .importSkills: "square.and.arrow.down"
            case .security: "lock.shield"
            case .advanced: "gearshape.2"
            }
        }
    }

    @EnvironmentObject private var model: AppModel
    @State private var selection: Section = .general

    var body: some View {
        NavigationSplitView {
            List(Section.allCases, selection: $selection) { item in
                Label(item.rawValue, systemImage: item.icon).tag(item)
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .background(model.preferences.appearance == .trueDark ? Color.black : Color.clear)
            .navigationSplitViewColumnWidth(min: 170, ideal: 185, max: 210)
            .safeAreaInset(edge: .top) {
                HStack { CosMark(); Spacer() }.padding(12)
            }
        } detail: {
            Group {
                switch selection {
                case .general: GeneralSettingsView()
                case .models: ModelSettingsView()
                case .providers: ProviderSettingsView()
                case .agent: AgentSettingsView()
                case .plugins: PluginSettingsView()
                case .importSkills: ImportSettingsView()
                case .security: SecuritySettingsView()
                case .advanced: AdvancedSettingsView()
                }
            }
            .environmentObject(model)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor))
        }
        .navigationSplitViewStyle(.balanced)
        .toolbarBackground(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor), for: .windowToolbar)
        .toolbarBackground(.visible, for: .windowToolbar)
    }
}

private struct SettingsPage<Content: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder let content: Content

    init(_ title: String, subtitle: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.subtitle = subtitle
        self.content = content()
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(title).font(.system(size: 22, weight: .semibold, design: .rounded))
                    Text(subtitle).font(.system(size: 12)).foregroundStyle(.secondary)
                }
                content
            }
            .frame(maxWidth: 580, alignment: .leading)
            .padding(28)
            .frame(maxWidth: .infinity, alignment: .top)
        }
    }
}

private struct SettingsGroup<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title.uppercased())
                .font(.system(size: 9.5, weight: .semibold))
                .foregroundStyle(.tertiary)
                .padding(.bottom, 7)
                .padding(.leading, 4)
            VStack(spacing: 0) { content }
                .padding(.horizontal, 13)
                .background(.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
    }
}

private struct SettingsRow<Content: View>: View {
    let title: String
    let detail: String?
    @ViewBuilder let content: Content

    init(_ title: String, detail: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.detail = detail
        self.content = content()
    }

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 12.5, weight: .medium))
                if let detail { Text(detail).font(.system(size: 10.5)).foregroundStyle(.secondary) }
            }
            Spacer()
            content
        }
        .padding(.vertical, 10)
        .overlay(alignment: .bottom) { Divider().opacity(0.4) }
    }
}

private struct GeneralSettingsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        SettingsPage("General", subtitle: "Make Cos feel right for the way you work.") {
            SettingsGroup("Experience") {
                SettingsRow("Appearance", detail: model.preferences.appearance == .trueDark ? "Pure black surfaces for OLED displays" : nil) {
                    Picker("", selection: Binding(get: { model.preferences.appearance }, set: { model.preferences.appearance = $0; model.persistPreferences() })) {
                        ForEach(AppearanceMode.allCases) { Text($0.title).tag($0) }
                    }.labelsHidden().frame(width: 120)
                }
                SettingsRow("Fast mode", detail: model.selectedModel.supportsFastMode ? "Prefer the selected model’s lower-latency route" : "Unavailable for the selected model") {
                    Toggle("", isOn: Binding(get: { model.preferences.fastMode }, set: { model.preferences.fastMode = $0; model.persistPreferences() })).labelsHidden()
                        .disabled(!model.selectedModel.supportsFastMode)
                }
                SettingsRow("Streaming animation", detail: "Animate new response content") {
                    Toggle("", isOn: Binding(get: { model.preferences.animateStreaming }, set: { model.preferences.animateStreaming = $0; model.persistPreferences() })).labelsHidden()
                }
                SettingsRow("Show token usage") {
                    Toggle("", isOn: Binding(get: { model.preferences.showTokenUsage }, set: { model.preferences.showTokenUsage = $0; model.persistPreferences() })).labelsHidden()
                }
            }
            SettingsGroup("Defaults") {
                SettingsRow("Reasoning effort") {
                    Picker("", selection: Binding(get: { model.preferences.defaultEffort }, set: { model.preferences.defaultEffort = $0; model.persistPreferences() })) {
                        ForEach(ReasoningEffort.allCases) { Text($0.title).tag($0) }
                    }.labelsHidden().frame(width: 120)
                }
                SettingsRow("Workspace", detail: model.preferences.defaultWorkspace) {
                    Button("Choose…") {
                        let panel = NSOpenPanel(); panel.canChooseDirectories = true; panel.canChooseFiles = false
                        if panel.runModal() == .OK, let path = panel.url?.path { model.preferences.defaultWorkspace = path; model.persistPreferences() }
                    }
                }
            }
            SettingsGroup("Updates") {
                SettingsRow("Check for Updates", detail: model.updateStatus ?? "Cos \(model.currentAppVersion)") {
                    if model.isCheckingForUpdate || model.isInstallingUpdate {
                        ProgressView().controlSize(.small)
                    } else if let update = model.availableUpdate {
                        Button("Install \(update.version) & Restart") { model.installAvailableUpdate() }
                            .buttonStyle(.borderedProminent)
                    } else {
                        Button("Check for Updates") {
                            Task { await model.checkForUpdates(manually: true) }
                        }
                    }
                }
            }
        }
    }
}

private struct ModelSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var showAdd = false

    var body: some View {
        SettingsPage("Models", subtitle: "Every connected provider appears in the main model selector.") {
            SettingsGroup("Task naming") {
                SettingsRow("Title model", detail: "Generates concise task names at Low reasoning") {
                    Picker("", selection: Binding(
                        get: { model.selectedTitleModel?.id ?? "chatgpt:gpt-5.6-luna" },
                        set: { model.preferences.titleModelID = $0; model.persistPreferences() }
                    )) {
                        ForEach(model.titleModels) { item in
                            Text("\(item.name) · Low").tag(item.id)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 190)
                }
            }
            SettingsGroup("Available models") {
                ForEach(model.models) { item in
                    SettingsRow(item.name, detail: "\(item.model) · \((item.contextWindow / 1_000).formatted())K context") {
                        if model.preferences.selectedModelID == item.id {
                            Text("Default").font(.caption).foregroundStyle(CosTheme.blue)
                        } else {
                            Button("Make default") { model.preferences.selectedModelID = item.id; model.persistPreferences() }
                        }
                    }
                }
            }
            Button("Add custom provider & model…", systemImage: "plus") { showAdd = true }
                .buttonStyle(.borderedProminent)
        }
        .sheet(isPresented: $showAdd) { AddProviderView(isPresented: $showAdd).environmentObject(model) }
    }
}

private struct AddProviderView: View {
    @EnvironmentObject private var model: AppModel
    @Binding var isPresented: Bool
    @State private var name = ""
    @State private var baseURL = "https://api.example.com/v1"
    @State private var modelName = ""
    @State private var modelID = ""
    @State private var key = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add an OpenAI-compatible model").font(.headline)
            TextField("Provider name", text: $name)
            TextField("Base URL", text: $baseURL)
            TextField("Model display name", text: $modelName)
            TextField("Model ID", text: $modelID)
            SecureField("API key", text: $key)
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
            Text("The key is saved only in macOS Keychain.").font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { isPresented = false }
                Button("Add") {
                    guard let url = URL(string: baseURL), !name.isEmpty, !modelID.isEmpty, !key.isEmpty else { error = "Complete every field with a valid URL."; return }
                    do { try model.addProvider(name: name, baseURL: url, modelName: modelName.ifEmpty(modelID), modelID: modelID, apiKey: key); isPresented = false }
                    catch { self.error = error.localizedDescription }
                }.buttonStyle(.borderedProminent)
            }
        }
        .padding(22)
        .frame(width: 430)
    }
}

private struct ProviderSettingsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        SettingsPage("Providers", subtitle: "Use official subscription sign-in flows or bring your own API key.") {
            ForEach(model.providers) { provider in
                ProviderCard(provider: provider)
            }
        }
    }
}

private struct ProviderCard: View {
    @EnvironmentObject private var model: AppModel
    let provider: ProviderProfile
    @State private var apiKey = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let session = model.providerSessions[provider.id] {
                VStack(spacing: 5) {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(.green)
                    Text(session.displayName)
                        .font(.system(size: 11.5, weight: .semibold))
                        .lineLimit(1)
                    Text("Connected")
                        .font(.system(size: 9.5, weight: .medium))
                        .foregroundStyle(.green)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 3)
            }
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(provider.name).font(.system(size: 13.5, weight: .semibold))
                    Text(provider.authMode == .subscription ? "Subscription credential · native Cos transport" : provider.authMode == .apiKey ? "Keychain-protected · native Cos transport" : "Cos smart route")
                        .font(.system(size: 10.5)).foregroundStyle(.secondary)
                }
                Spacer()
                if provider.authMode == .subscription {
                    if model.providerSessions[provider.id] != nil {
                        HStack(spacing: 7) {
                            Button("Switch Account…") { model.signIn(to: provider) }
                            Button("Refresh Token…") { model.signIn(to: provider) }
                                .buttonStyle(.borderedProminent)
                        }
                    } else {
                        Button("Sign In…") { model.signIn(to: provider) }.buttonStyle(.borderedProminent)
                    }
                } else if provider.authMode == .local {
                    Text("Local").font(.caption).foregroundStyle(.secondary)
                }
            }
            if provider.authMode == .apiKey {
                HStack {
                    SecureField(model.hasAPIKey(for: provider) ? "Key stored — enter to replace" : "API key", text: $apiKey)
                        .textFieldStyle(.roundedBorder)
                    Button("Save") {
                        do { try model.setAPIKey(apiKey, for: provider); apiKey = "" }
                        catch { model.lastError = error.localizedDescription }
                    }.disabled(apiKey.isEmpty)
                }
            }
            if let status = model.loginStatus[provider.id] {
                Text(status).font(.system(size: 10.5)).foregroundStyle(status.lowercased().contains("signed") || status.lowercased().contains("stored") ? .green : .secondary)
                    .lineLimit(3)
            }
        }
        .padding(14)
        .background(.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onAppear { model.refreshProviderSessions() }
    }
}

private struct AgentSettingsView: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        SettingsPage("Agent", subtitle: "Control access, compaction, and persistent execution behavior.") {
            SettingsGroup("Access") {
                SettingsRow("Full access", detail: "Allow Cos tools outside the workspace and enable commands") {
                    Toggle("", isOn: Binding(get: { model.preferences.fullAccess }, set: { model.preferences.fullAccess = $0; model.persistPreferences() })).labelsHidden()
                }
            }
            SettingsGroup("Compaction") {
                SettingsRow("Automatic compaction", detail: "Preserve a checkpoint plus recent verbatim context") {
                    Toggle("", isOn: Binding(get: { model.preferences.autoCompact }, set: { model.preferences.autoCompact = $0; model.persistPreferences() })).labelsHidden()
                }
                SettingsRow("Compact at") {
                    HStack { Slider(value: Binding(get: { model.preferences.compactAtPercent }, set: { model.preferences.compactAtPercent = $0; model.persistPreferences() }), in: 55...92, step: 1).frame(width: 130); Text("\(Int(model.preferences.compactAtPercent))%").monospacedDigit().frame(width: 32) }
                }
                SettingsRow("Keep recent context") {
                    Picker("", selection: Binding(get: { model.preferences.keepRecentTokens }, set: { model.preferences.keepRecentTokens = $0; model.persistPreferences() })) {
                        Text("10K tokens").tag(10_000); Text("20K tokens").tag(20_000); Text("40K tokens").tag(40_000)
                    }.labelsHidden().frame(width: 120)
                }
            }
        }
    }
}

private struct PluginSettingsView: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        SettingsPage("Plugins", subtitle: "Add capabilities without making the app core heavier.") {
            SettingsGroup("Installed") {
                ForEach(model.plugins) { plugin in
                    SettingsRow(plugin.manifest.name, detail: "v\(plugin.manifest.version) · \(plugin.manifest.author)") {
                        Text(plugin.manifest.builtIn == true ? "Built in" : "Enabled").font(.caption).foregroundStyle(plugin.manifest.builtIn == true ? CosTheme.blue : .secondary)
                    }
                }
            }
            HStack {
                Button("Open library") { model.isPluginLibraryPresented = true }.buttonStyle(.borderedProminent)
                Button("Install from disk…") { model.installPluginFromDisk() }
            }
        }
    }
}

private struct ImportSettingsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        SettingsPage("Import", subtitle: "Bring your existing agent skills into Cos without changing the originals.") {
            SettingsGroup("Skill libraries") {
                ForEach(ExternalSkillSource.allCases) { source in
                    SettingsRow(source.title, detail: importDetail(for: source)) {
                        HStack(spacing: 9) {
                            Image(systemName: source.systemImage)
                                .foregroundStyle(source == .folder ? .secondary : CosTheme.blue)
                                .frame(width: 18)
                            Button(buttonTitle(for: source)) { model.importSkills(from: source) }
                                .disabled(source != .folder && (model.skillImportCounts[source] ?? 0) == 0)
                        }
                    }
                }
            }
            SettingsGroup("How importing works") {
                SettingsRow("Portable bundles", detail: "Copies SKILL.md plus scripts, references, and assets up to 10 MB per skill") {
                    Image(systemName: "doc.on.doc.fill").foregroundStyle(CosTheme.blue)
                }
                SettingsRow("Local and recoverable", detail: "Original folders stay untouched; imported skills can be disabled or moved to Trash as plugins") {
                    Image(systemName: "checkmark.shield.fill").foregroundStyle(.green)
                }
            }
            HStack(spacing: 6) {
                Image(systemName: "arrow.clockwise")
                Button("Rescan skill libraries") { model.refreshSkillImportCounts() }
                    .buttonStyle(.plain)
            }
            .font(.system(size: 11.5, weight: .medium))
            .foregroundStyle(.secondary)
        }
        .onAppear { model.refreshSkillImportCounts() }
    }

    private func importDetail(for source: ExternalSkillSource) -> String {
        if let status = model.skillImportStatus[source] { return status }
        guard source != .folder else { return source.detail }
        let count = model.skillImportCounts[source] ?? 0
        return count == 0 ? "No skills found in the default folder" : "\(count) available · \(source.detail)"
    }

    private func buttonTitle(for source: ExternalSkillSource) -> String {
        guard source != .folder else { return "Choose…" }
        let count = model.skillImportCounts[source] ?? 0
        return count > 0 ? "Import \(count)" : "Import"
    }
}

private struct SecuritySettingsView: View {
    var body: some View {
        SettingsPage("Security", subtitle: "Cos keeps credentials local and makes authority visible.") {
            SettingsGroup("Credentials") {
                SettingsRow("BYOK secrets", detail: "Stored in macOS Keychain with device-only access") { Image(systemName: "checkmark.seal.fill").foregroundStyle(.green) }
                SettingsRow("Subscription sessions", detail: "Read locally into memory by the native Cos transport") { Image(systemName: "checkmark.seal.fill").foregroundStyle(.green) }
                SettingsRow("Plugin trust", detail: "Managed actions are scoped, validated, and recoverable") { Image(systemName: "shield.lefthalf.filled").foregroundStyle(CosTheme.blue) }
            }
            Text("Cos does not send subscription tokens to another agent harness. Only the selected native provider transport receives the credential. Full Access is shown in the composer whenever it is enabled.")
                .font(.system(size: 11.5)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct AdvancedSettingsView: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        SettingsPage("Advanced", subtitle: "Diagnostics and catalog controls for experienced users.") {
            SettingsGroup("Runtime") {
                SettingsRow("Harness activity", detail: model.activity) { Circle().fill(model.isRunning ? .green : .secondary).frame(width: 7, height: 7) }
                SettingsRow("Marketplace") { Link("cos.ssh.codes", destination: URL(string: "https://cos.ssh.codes")!) }
                SettingsRow("Reset provider catalog", detail: "Restore Cos defaults without deleting Keychain secrets") { Button("Reset") { model.resetCatalog() } }
            }
        }
    }
}

private extension String {
    func ifEmpty(_ fallback: @autoclosure () -> String) -> String { isEmpty ? fallback() : self }
}
