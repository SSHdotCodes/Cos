import AppKit
import Combine
import CosCore
import Foundation
import OSLog

struct PendingDirectoryTrust: Equatable {
    let threadID: UUID
    let workspacePath: String
    let prompt: String
}

struct PendingComputerUseRun: Equatable {
    let threadID: UUID
    let prompt: String
}

enum ExternalSkillSource: String, CaseIterable, Identifiable, Hashable {
    case codex
    case claudeCode
    case folder

    var id: String { rawValue }

    var title: String {
        switch self {
        case .codex: "Codex"
        case .claudeCode: "Claude Code"
        case .folder: "Another folder"
        }
    }

    var detail: String {
        switch self {
        case .codex: "Import skills from ~/.codex/skills"
        case .claudeCode: "Import skills from ~/.claude/skills"
        case .folder: "Choose any folder containing SKILL.md bundles"
        }
    }

    var systemImage: String {
        switch self {
        case .codex: "chevron.left.forwardslash.chevron.right"
        case .claudeCode: "sparkles"
        case .folder: "folder.badge.plus"
        }
    }

    var pluginID: String { "codes.ssh.cos.imported-\(rawValue.lowercased())" }

    var defaultRoots: [URL] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return switch self {
        case .codex: [home.appendingPathComponent(".codex/skills", isDirectory: true)]
        case .claudeCode: [home.appendingPathComponent(".claude/skills", isDirectory: true)]
        case .folder: []
        }
    }
}

@MainActor
final class AppModel: ObservableObject {
    @Published var threads: [CosThread] = []
    @Published var selectedThreadID: UUID?
    @Published var preferences: AppPreferences
    @Published var providers: [ProviderProfile]
    @Published var models: [ModelProfile]
    @Published var plugins: [InstalledPlugin] = []
    @Published var isRunning = false
    @Published var activity = "Ready"
    @Published var lastError: String?
    @Published var loginStatus: [String: String] = [:]
    @Published var providerSessions: [String: ProviderSessionInfo] = [:]
    @Published var isPluginLibraryPresented = false
    @Published var pendingDirectoryTrust: PendingDirectoryTrust?
    @Published var skillImportCounts: [ExternalSkillSource: Int] = [:]
    @Published var skillImportStatus: [ExternalSkillSource: String] = [:]
    @Published var availableUpdate: CosUpdateManifest?
    @Published var isCheckingForUpdate = false
    @Published var isInstallingUpdate = false
    @Published var updateStatus: String?
    @Published var computerUseAccessGranted = CosComputerUseAccess.isGranted
    @Published var computerUseAccessStatus: String?
    @Published var marketplacePlugins: [CosMarketplaceListing] = []
    @Published var isLoadingMarketplace = false
    @Published var installingMarketplacePluginID: String?
    @Published var marketplaceError: String?
    @Published var isBrowserPanelPresented = false
    @Published var pendingComputerUseRun: PendingComputerUseRun?

    private let store = ThreadStore()
    private let runtime = AgentRuntime()
    private let compactor = CompactionEngine()
    private let registry = PluginRegistry()
    private let secureStore = SecureStore()
    private let updateService = CosUpdateService()
    private var runningTask: Task<Void, Never>?
    private var activeRunID: UUID?
    private var activeRunThreadID: UUID?
    private var activeRunAssistantID: UUID?
    private var activeRunControl: AgentRunControl?
    private var reasoningBuffers: [UUID: String] = [:]
    private var titleTasks: [UUID: Task<Void, Never>] = [:]
    private var updateCheckTask: Task<Void, Never>?
    private var lastUpdateCheck: Date?
    private var trustedWorkspaces: Set<String>
    private var disabledPluginIDs: Set<String>
    private var disabledSkillKeys: Set<String>
    private let builtInPluginsURL: URL?
    private static let logger = Logger(subsystem: "codes.ssh.cos", category: "app-model")

    init(builtInPluginsURL: URL?) {
        self.builtInPluginsURL = builtInPluginsURL
        self.preferences = Self.load(AppPreferences.self, key: "preferences") ?? AppPreferences()
        self.providers = Self.mergeProviders(Self.load([ProviderProfile].self, key: "providers"))
        self.models = Self.mergeModels(Self.load([ModelProfile].self, key: "models"))
        self.trustedWorkspaces = Set(Self.load([String].self, key: "trustedWorkspaces") ?? [])
        self.disabledPluginIDs = Set(Self.load([String].self, key: "disabledPluginIDs") ?? [])
        self.disabledSkillKeys = Set(Self.load([String].self, key: "disabledSkillKeys") ?? [])
        Task { await bootstrap() }
    }

    var selectedThread: CosThread? {
        guard let selectedThreadID else { return nil }
        return threads.first { $0.id == selectedThreadID }
    }

    var selectedThreadBindingIndex: Int? {
        guard let selectedThreadID else { return nil }
        return threads.firstIndex { $0.id == selectedThreadID }
    }

    var selectedModel: ModelProfile {
        let id = selectedThread?.modelID ?? preferences.selectedModelID
        return models.first { $0.id == id } ?? models[0]
    }

    var selectedProvider: ProviderProfile {
        providers.first { $0.id == selectedModel.providerID } ?? providers[0]
    }

    var canSteerSelectedThread: Bool {
        isRunning && selectedThreadID == activeRunThreadID
    }

    var isBetterWrightEnabled: Bool {
        plugins.contains { $0.id == "codes.ssh.cos.betterwright" && $0.isEnabled }
    }

    var subagentRoutes: [SubagentRoute] {
        runtime.accessibleSubagentRoutes(providers: providers, models: models)
    }

    var titleModels: [ModelProfile] {
        let preferredIDs = ["chatgpt:gpt-5.6-luna", "xai:grok-4.5", "anthropic:claude-haiku-4.5"]
        return preferredIDs.compactMap { id in models.first { $0.id == id } }
    }

    var selectedTitleModel: ModelProfile? {
        let requested = preferences.titleModelID ?? "chatgpt:gpt-5.6-luna"
        return titleModels.first { $0.id == requested } ?? titleModels.first
    }

    private func bootstrap() async {
        do {
            threads = try await store.loadAll()
            normalizeLoadedThreadEfforts()
        } catch {
            lastError = "Could not load tasks: \(error.localizedDescription)"
        }
        if threads.isEmpty { newThread() } else { selectedThreadID = threads.first?.id }
        await reloadPlugins()
        refreshSkillImportCounts()
        refreshProviderSessions()
        await checkForUpdates()
        schedulePeriodicUpdateChecks()
    }

    var currentAppVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    func checkForUpdates(manually: Bool = false) async {
        guard !isCheckingForUpdate, !isInstallingUpdate else { return }
        if !manually, let lastUpdateCheck,
           Date().timeIntervalSince(lastUpdateCheck) < 6 * 60 * 60 { return }

        isCheckingForUpdate = true
        if manually { updateStatus = "Checking for updates…" }
        defer { isCheckingForUpdate = false }

        do {
            let build = Int(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "0") ?? 0
            availableUpdate = try await updateService.check(currentVersion: currentAppVersion, currentBuild: build)
            lastUpdateCheck = Date()
            if let availableUpdate {
                updateStatus = "Cos \(availableUpdate.version) is ready to install."
            } else {
                updateStatus = manually ? "Cos is up to date." : nil
            }
        } catch {
            if manually {
                updateStatus = nil
                lastError = "Could not check for updates: \(error.localizedDescription)"
            }
            Self.logger.error("Update check failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    func installAvailableUpdate() {
        guard let update = availableUpdate, !isInstallingUpdate else { return }
        guard !isRunning else {
            lastError = "Stop the current task before installing the update."
            return
        }

        let currentAppURL = Bundle.main.bundleURL
        do {
            try updateService.validateInstallLocation(currentAppURL)
        } catch {
            lastError = error.localizedDescription
            return
        }

        isInstallingUpdate = true
        updateStatus = "Downloading Cos \(update.version)…"
        let processID = ProcessInfo.processInfo.processIdentifier
        let service = updateService

        Task { [weak self] in
            var workingDirectory: URL?
            do {
                let prepared = try await service.downloadAndVerify(update)
                workingDirectory = prepared.workingDirectory
                guard let self else {
                    try? FileManager.default.removeItem(at: prepared.workingDirectory)
                    return
                }
                self.updateStatus = "Installing and restarting…"
                try await Task.detached(priority: .userInitiated) {
                    try service.scheduleReplacement(
                        prepared: prepared,
                        currentAppURL: currentAppURL,
                        processID: processID
                    )
                }.value
                NSApp.terminate(nil)
            } catch {
                if let workingDirectory { try? FileManager.default.removeItem(at: workingDirectory) }
                self?.isInstallingUpdate = false
                self?.updateStatus = nil
                self?.lastError = "Could not install Cos \(update.version): \(error.localizedDescription)"
                Self.logger.error("Update installation failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    private func schedulePeriodicUpdateChecks() {
        updateCheckTask?.cancel()
        updateCheckTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(6 * 60 * 60))
                guard !Task.isCancelled else { return }
                await self?.checkForUpdates()
            }
        }
    }

    func newThread(workspacePath: String? = nil) {
        let defaultModel = models.first { $0.id == preferences.selectedModelID } ?? models[0]
        let thread = CosThread(
            workspacePath: workspacePath ?? preferences.defaultWorkspace,
            modelID: preferences.selectedModelID,
            effort: defaultModel.normalizedEffort(preferences.defaultEffort)
        )
        threads.insert(thread, at: 0)
        selectedThreadID = thread.id
        persist(thread)
    }

    func deleteThread(_ id: UUID) {
        guard !isRunning || selectedThreadID != id else { return }
        titleTasks.removeValue(forKey: id)?.cancel()
        threads.removeAll { $0.id == id }
        if selectedThreadID == id { selectedThreadID = threads.first?.id }
        Task { try? await store.delete(id: id) }
        if threads.isEmpty { newThread() }
    }

    func chooseWorkspace() {
        let panel = NSOpenPanel()
        panel.title = "Choose a workspace for this task"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let path = panel.url?.path,
              let index = selectedThreadBindingIndex else { return }
        threads[index].workspacePath = path
        threads[index].updatedAt = Date()
        persist(threads[index])
    }

    func selectModel(_ model: ModelProfile) {
        guard let index = selectedThreadBindingIndex else { return }
        let effort = model.normalizedEffort(threads[index].effort)
        threads[index].modelID = model.id
        threads[index].effort = effort
        threads[index].updatedAt = Date()
        preferences.selectedModelID = model.id
        preferences.defaultEffort = effort
        persistPreferences()
        persist(threads[index])
    }

    func setEffort(_ effort: ReasoningEffort) {
        guard let index = selectedThreadBindingIndex else { return }
        let effort = selectedModel.normalizedEffort(effort)
        threads[index].effort = effort
        threads[index].updatedAt = Date()
        preferences.defaultEffort = effort
        persistPreferences()
        persist(threads[index])
    }

    func createGoal(objective: String, budget: Int?) {
        guard let index = selectedThreadBindingIndex else { return }
        threads[index].goal = AgentGoal(objective: objective, tokenBudget: budget)
        persist(threads[index])
    }

    func clearGoal() {
        guard let index = selectedThreadBindingIndex else { return }
        threads[index].goal = nil
        persist(threads[index])
    }

    func send(_ rawPrompt: String) {
        startRun(rawPrompt, appendUserMessage: true)
    }

    func steer(_ rawPrompt: String) {
        let prompt = rawPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty,
              isRunning,
              let activeRunThreadID,
              selectedThreadID == activeRunThreadID,
              let control = activeRunControl else { return }
        activity = "Applying steering…"
        Task { @MainActor [weak self] in
            guard let self else { return }
            if !(await control.submit(prompt)), activeRunThreadID == self.activeRunThreadID {
                activity = "Steering queue is full"
            }
        }
    }

    private func startRun(_ rawPrompt: String, appendUserMessage: Bool) {
        let prompt = rawPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty, !isRunning, let index = selectedThreadBindingIndex else { return }
        guard pendingDirectoryTrust?.threadID != threads[index].id else { return }

        if appendUserMessage {
            threads[index].messages.append(.init(role: .user, content: prompt))
            if threads[index].messages.count == 1 {
                threads[index].title = "New task"
                scheduleTitleGeneration(threadID: threads[index].id, prompt: prompt)
            }
            if handleSlashCommand(prompt, threadIndex: index) {
                threads[index].updatedAt = Date()
                persist(threads[index])
                return
            }
        }
        let computerUseEnabled = plugins.contains { $0.id == "codes.ssh.cos.computer-use" && $0.isEnabled }
        computerUseAccessGranted = CosComputerUseAccess.isGranted
        if computerUseEnabled,
           Self.looksLikeComputerUseRequest(prompt),
           !computerUseAccessGranted {
            pendingComputerUseRun = .init(threadID: threads[index].id, prompt: prompt)
            threads[index].messages.append(.init(
                role: .assistant,
                content: "Cos needs macOS Accessibility access for this task. I’ll continue automatically as soon as the permission becomes active."
            ))
            threads[index].updatedAt = Date()
            persist(threads[index])
            requestComputerUseAccess()
            return
        }
        let assistantID = UUID()
        let runID = UUID()
        let runControl = AgentRunControl()
        threads[index].messages.append(.init(id: assistantID, role: .assistant, content: "", isStreaming: true))
        threads[index].updatedAt = Date()
        isRunning = true
        activeRunID = runID
        activeRunThreadID = threads[index].id
        activeRunAssistantID = assistantID
        activeRunControl = runControl
        activity = "Preparing context…"
        lastError = nil

        let compaction = compactor.prepare(
            messages: Array(threads[index].messages.dropLast()),
            previousSummary: threads[index].compactedContext,
            contextWindow: selectedModel.contextWindow,
            thresholdPercent: preferences.autoCompact ? preferences.compactAtPercent : 101,
            keepRecentTokens: preferences.keepRecentTokens
        )
        if compaction.didCompact {
            threads[index].compactedContext = compaction.compactedSummary
            activity = "Context compacted"
        }

        let goalContext = threads[index].goal.map {
            "Active goal: \($0.objective)\nGoal status: \($0.status.rawValue)\nTokens used: \($0.usedTokens)\n"
        } ?? ""
        let referencePlugins = plugins.map { plugin in
            var visible = plugin
            visible.manifest.skills = plugin.manifest.skills.filter { isSkillEnabled($0, in: plugin) }
            return visible
        }
        let referenceContext = ComposerReferenceResolver.referenceContext(in: prompt, plugins: referencePlugins)
        let subagentsAuthorized = SubagentAuthorization.isExplicitlyRequested(in: prompt)
        let availableSubagentRoutes = subagentRoutes
        let browserEnabled = isBetterWrightEnabled
        let effectivePrompt = """
        \(CosSettingsPlugin.systemPrompt)

        \(goalContext)
        \(referenceContext)
        Conversation context:
        \(compaction.promptContext)

        Continue the task. The newest user request is: \(prompt)
        """
        let request = AgentRequest(
            prompt: effectivePrompt,
            latestUserRequest: prompt,
            thread: threads[index],
            model: selectedModel,
            provider: selectedProvider,
            effort: threads[index].effort,
            fastMode: preferences.fastMode,
            fullAccess: preferences.fullAccess,
            workspaceIsTrusted: isWorkspaceTrusted(threads[index].workspacePath),
            extensionInstructions: activeExtensionInstructions(),
            computerUseEnabled: computerUseEnabled,
            browserEnabled: browserEnabled,
            availableSubagentRoutes: availableSubagentRoutes,
            subagentsAuthorized: subagentsAuthorized,
            runControl: runControl
        )
        persist(threads[index])

        runningTask = Task { [weak self] in
            guard let self else { return }
            var currentAssistantID = assistantID
            do {
                let stream = try runtime.stream(request: request)
                for try await event in stream {
                    guard !Task.isCancelled, activeRunID == runID else { return }
                    if case .steeringApplied(let messages) = event {
                        currentAssistantID = applySteering(
                            messages,
                            assistantID: currentAssistantID,
                            threadID: request.thread.id
                        )
                    } else {
                        handle(event, assistantID: currentAssistantID, threadID: request.thread.id)
                    }
                }
                guard activeRunID == runID else { return }
                finishAssistant(id: currentAssistantID, threadID: request.thread.id)
            } catch {
                guard activeRunID == runID else { return }
                failAssistant(id: currentAssistantID, threadID: request.thread.id, retryPrompt: prompt, error: error)
            }
        }
    }

    func trustPendingWorkspaceAndContinue() {
        guard let pending = pendingDirectoryTrust,
              threads.contains(where: { $0.id == pending.threadID }) else { return }
        trustedWorkspaces.insert(normalizedWorkspacePath(pending.workspacePath))
        Self.save(Array(trustedWorkspaces).sorted(), key: "trustedWorkspaces")
        pendingDirectoryTrust = nil
        selectedThreadID = pending.threadID
        activity = "Directory trusted"
        startRun(pending.prompt, appendUserMessage: false)
    }

    func declinePendingWorkspaceTrust() {
        guard let pending = pendingDirectoryTrust else { return }
        pendingDirectoryTrust = nil
        if let index = threads.firstIndex(where: { $0.id == pending.threadID }) {
            threads[index].messages.append(.init(role: .assistant, content: "Run canceled. This directory remains untrusted."))
            threads[index].updatedAt = Date()
            persist(threads[index])
        }
        activity = "Ready"
    }

    func cancel() {
        let runningThreadID = activeRunThreadID
        let runningAssistantID = activeRunAssistantID
        activeRunID = nil
        activeRunThreadID = nil
        activeRunAssistantID = nil
        activeRunControl = nil
        runningTask?.cancel()
        runningTask = nil
        isRunning = false
        activity = "Stopped"
        guard let runningThreadID,
              let threadIndex = threads.firstIndex(where: { $0.id == runningThreadID }),
              let runningAssistantID,
              let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == runningAssistantID }) else { return }
        threads[threadIndex].messages[messageIndex].isStreaming = false
        persist(threads[threadIndex])
    }

    private func handleSlashCommand(_ prompt: String, threadIndex: Int) -> Bool {
        let pieces = prompt.split(maxSplits: 1, whereSeparator: \Character.isWhitespace)
        guard pieces.first?.lowercased() == "/goal" else { return false }
        let argument = pieces.count > 1 ? String(pieces[1]).trimmingCharacters(in: .whitespacesAndNewlines) : ""
        let response: String

        if argument.isEmpty || argument.lowercased() == "status" {
            if let goal = threads[threadIndex].goal {
                let budget = goal.tokenBudget.map { " of \($0.formatted())" } ?? ""
                response = "Goal: **\(goal.objective)**\n\nStatus: \(goal.status.rawValue) · \(goal.usedTokens.formatted())\(budget) tokens used."
            } else {
                response = "No goal is active. Use `/goal Write the objective here` or `/goal --budget 100000 Write the objective here`."
            }
        } else if argument.lowercased() == "clear" {
            threads[threadIndex].goal = nil
            response = "Goal cleared."
        } else if argument.lowercased() == "complete" {
            if var goal = threads[threadIndex].goal {
                goal.status = .complete
                threads[threadIndex].goal = goal
                response = "Goal marked complete: **\(goal.objective)**"
            } else {
                response = "No active goal to complete."
            }
        } else {
            var objective = argument
            var budget: Int?
            let arguments = argument.split(separator: " ", omittingEmptySubsequences: true)
            if arguments.count >= 3, arguments[0] == "--budget", let parsed = Int(arguments[1]) {
                budget = parsed
                objective = arguments.dropFirst(2).joined(separator: " ")
            }
            guard !objective.isEmpty else {
                threads[threadIndex].messages.append(.init(role: .assistant, content: "Add an objective after `/goal`."))
                return true
            }
            threads[threadIndex].goal = AgentGoal(objective: objective, tokenBudget: budget)
            response = budget.map { "Goal pinned with a \($0.formatted()) token budget: **\(objective)**" } ?? "Goal pinned: **\(objective)**"
        }

        threads[threadIndex].messages.append(.init(role: .assistant, content: response))
        activity = "Ready"
        return true
    }

    private func handle(_ event: AgentEvent, assistantID: UUID, threadID: UUID) {
        guard let threadIndex = threads.firstIndex(where: { $0.id == threadID }) else { return }
        switch event {
        case .status(let status):
            activity = status
            appendWork(.init(kind: .status, title: status), assistantID: assistantID, threadIndex: threadIndex, coalesce: true)
        case .workDelta(let text):
            appendReasoning(text, assistantID: assistantID, threadIndex: threadIndex)
        case .textDelta(let text):
            guard let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == assistantID }) else { return }
            threads[threadIndex].messages[messageIndex].content += text
            activity = "Working…"
        case .tool(let name, let detail):
            activity = detail.isEmpty ? "Using \(name)…" : "\(name): \(detail)"
            appendWork(.init(kind: .tool, title: name.replacingOccurrences(of: "_", with: " ").capitalized, detail: detail), assistantID: assistantID, threadIndex: threadIndex, coalesce: false)
        case .subagent(let name, let detail):
            activity = "\(name): \(detail)"
            upsertSubagentWork(name: name, detail: detail, assistantID: assistantID, threadIndex: threadIndex)
        case .steeringApplied:
            break
        case .usage(let input, let output):
            if var goal = threads[threadIndex].goal {
                goal.usedTokens += input + output
                if let budget = goal.tokenBudget, goal.usedTokens >= budget { goal.status = .budgetLimited }
                threads[threadIndex].goal = goal
            }
            if preferences.showTokenUsage { activity = "↑ \(input.formatted())  ↓ \(output.formatted()) tokens" }
        case .completed: activity = "Complete"
        }
    }

    private func appendWork(_ item: WorkTraceItem, assistantID: UUID, threadIndex: Int, coalesce: Bool) {
        guard let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == assistantID }) else { return }
        if item.kind != .reasoning { reasoningBuffers[assistantID] = nil }
        var items = threads[threadIndex].messages[messageIndex].workItems ?? []
        if coalesce, items.last?.kind == item.kind, items.last?.title == item.title { return }
        if items.count < 120 { items.append(item) }
        threads[threadIndex].messages[messageIndex].workItems = items
    }

    private func appendReasoning(_ text: String, assistantID: UUID, threadIndex: Int) {
        guard !text.isEmpty,
              let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == assistantID }) else { return }
        var items = threads[threadIndex].messages[messageIndex].workItems ?? []
        var raw: String
        if let last = items.indices.last, items[last].kind == .reasoning, items[last].detail.utf8.count < 24_000 {
            raw = reasoningBuffers[assistantID] ?? items[last].detail
            raw += text
            if raw.utf8.count > 24_000 { raw = String(raw.suffix(24_000)) }
            reasoningBuffers[assistantID] = raw
            items[last].detail = CosOutputSanitizer.reasoning(raw)
        } else if items.count < 120 {
            raw = text
            reasoningBuffers[assistantID] = raw
            items.append(.init(kind: .reasoning, title: "Reasoning", detail: CosOutputSanitizer.reasoning(raw)))
        }
        threads[threadIndex].messages[messageIndex].workItems = items
    }

    private func upsertSubagentWork(name: String, detail: String, assistantID: UUID, threadIndex: Int) {
        guard let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == assistantID }) else { return }
        var items = threads[threadIndex].messages[messageIndex].workItems ?? []
        if let last = items.indices.last, items[last].kind == .subagent, items[last].title == name {
            items[last].detail = detail
        } else if items.count < 120 {
            items.append(.init(kind: .subagent, title: name, detail: detail))
        }
        threads[threadIndex].messages[messageIndex].workItems = items
    }

    private func applySteering(
        _ messages: [SteeringMessage],
        assistantID: UUID,
        threadID: UUID
    ) -> UUID {
        guard !messages.isEmpty,
              let threadIndex = threads.firstIndex(where: { $0.id == threadID }),
              let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == assistantID }) else {
            return assistantID
        }
        let detail = messages.map(\.content).joined(separator: "\n")
        threads[threadIndex].messages[messageIndex].isStreaming = false
        reasoningBuffers[assistantID] = nil
        appendWork(
            .init(kind: .status, title: "Steered", detail: detail),
            assistantID: assistantID,
            threadIndex: threadIndex,
            coalesce: false
        )
        for message in messages {
            threads[threadIndex].messages.append(.init(role: .user, content: message.content))
        }
        let nextAssistantID = UUID()
        threads[threadIndex].messages.append(.init(
            id: nextAssistantID,
            role: .assistant,
            content: "",
            isStreaming: true
        ))
        threads[threadIndex].updatedAt = Date()
        activeRunAssistantID = nextAssistantID
        activity = "Steering applied"
        persist(threads[threadIndex])
        return nextAssistantID
    }

    private func finishAssistant(id: UUID, threadID: UUID) {
        guard let threadIndex = threads.firstIndex(where: { $0.id == threadID }),
              let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == id }) else { return }
        reasoningBuffers[id] = nil
        let visibleContent = CosOutputSanitizer.assistantText(threads[threadIndex].messages[messageIndex].content)
        let extracted = CosSettingsPlugin.extract(from: visibleContent)
        threads[threadIndex].messages[messageIndex].content = extracted.visibleText
        threads[threadIndex].messages[messageIndex].isStreaming = false
        if let mutation = extracted.mutation { apply(mutation) }
        if let action = extracted.managementAction { apply(action) }
        threads[threadIndex].updatedAt = Date()
        activeRunID = nil
        activeRunThreadID = nil
        activeRunAssistantID = nil
        activeRunControl = nil
        isRunning = false
        runningTask = nil
        activity = "Ready"
        persist(threads[threadIndex])
    }

    private func failAssistant(id: UUID, threadID: UUID, retryPrompt: String, error: Error) {
        guard let threadIndex = threads.firstIndex(where: { $0.id == threadID }),
              let messageIndex = threads[threadIndex].messages.firstIndex(where: { $0.id == id }) else { return }
        reasoningBuffers[id] = nil
        if let runtimeError = error as? AgentRuntimeError,
           case .directoryTrustRequired(let workspacePath) = runtimeError {
            threads[threadIndex].messages.remove(at: messageIndex)
            threads[threadIndex].updatedAt = Date()
            activeRunID = nil
            activeRunThreadID = nil
            activeRunAssistantID = nil
            activeRunControl = nil
            isRunning = false
            runningTask = nil
            activity = "Waiting for directory trust"
            lastError = nil
            pendingDirectoryTrust = .init(threadID: threadID, workspacePath: workspacePath, prompt: retryPrompt)
            persist(threads[threadIndex])
            return
        }
        if threads[threadIndex].messages[messageIndex].content.isEmpty {
            threads[threadIndex].messages[messageIndex].content = "I couldn’t start this run. \(error.localizedDescription)"
        }
        threads[threadIndex].messages[messageIndex].isStreaming = false
        activeRunID = nil
        activeRunThreadID = nil
        activeRunAssistantID = nil
        activeRunControl = nil
        isRunning = false
        runningTask = nil
        activity = "Needs attention"
        lastError = error.localizedDescription
        persist(threads[threadIndex])
    }

    private func apply(_ mutation: SettingsMutation) {
        switch mutation {
        case .fastMode(let value): preferences.fastMode = value
        case .fullAccess(let value): preferences.fullAccess = value
        case .autoCompact(let value): preferences.autoCompact = value
        case .showTokenUsage(let value): preferences.showTokenUsage = value
        case .effort(let effort): setEffort(effort)
        }
        persistPreferences()
    }

    private func apply(_ action: CosManagementAction) {
        do {
            switch action {
            case .createSkill(let id, let name, let description, let instructions, let pluginID):
                try createManagedSkill(id: id, name: name, description: description, instructions: instructions, pluginID: pluginID)
                activity = "Skill created"
            case .removeSkill(let id, let pluginID):
                try removeManagedSkill(id: id, pluginID: pluginID)
                activity = "Skill moved to Trash"
            case .createPlugin(let id, let name, let description, let instructions):
                try createManagedPlugin(id: id, name: name, description: description, instructions: instructions)
                activity = "Plugin created"
            case .removePlugin(let id):
                try removeManagedPlugin(id: id)
                activity = "Plugin moved to Trash"
            case .setPluginEnabled(let id, let enabled):
                try setPlugin(id: id, enabled: enabled)
                activity = enabled ? "Plugin enabled" : "Plugin disabled"
            }
            Task { await reloadPlugins() }
        } catch {
            lastError = "Cos could not manage that skill or plugin: \(error.localizedDescription)"
            activity = "Needs attention"
        }
    }

    private var managedPluginsRoot: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Cos/Plugins", isDirectory: true)
    }

    private func createManagedSkill(id: String, name: String, description: String, instructions: String, pluginID: String?) throws {
        try validateManagedID(id)
        try validateManagedText(name, maximum: 100)
        try validateManagedText(description, maximum: 500)
        try validateManagedText(instructions, maximum: 64_000)
        let ownerID = pluginID ?? "codes.ssh.cos.user-skills"
        try validateManagedID(ownerID)
        guard ownerID != "codes.ssh.cos.settings" else { throw ManagedArtifactError.builtInProtected }

        let pluginRoot = managedPluginsRoot.appendingPathComponent(ownerID, isDirectory: true)
        try FileManager.default.createDirectory(at: pluginRoot, withIntermediateDirectories: true)
        var manifest: CosPluginManifest
        let manifestURL = pluginRoot.appendingPathComponent("cos.plugin.json")
        if FileManager.default.fileExists(atPath: manifestURL.path) {
            manifest = try JSONDecoder().decode(CosPluginManifest.self, from: Data(contentsOf: manifestURL))
        } else if pluginID == nil {
            manifest = .init(
                schemaVersion: 1,
                id: ownerID,
                name: "My Cos Skills",
                version: "1.0.0",
                author: NSFullUserName().isEmpty ? "Cos user" : NSFullUserName(),
                description: "Skills created and managed through the built-in Cos plugin.",
                capabilities: [.init(id: "cos.skills.user", description: "User-authored Cos skills.", risk: "guarded")],
                skills: [],
                homepage: nil,
                builtIn: false
            )
        } else {
            throw ManagedArtifactError.pluginNotFound(ownerID)
        }

        let skillRoot = pluginRoot.appendingPathComponent("skills/\(id)", isDirectory: true)
        try FileManager.default.createDirectory(at: skillRoot, withIntermediateDirectories: true)
        let safeDescription = description.replacingOccurrences(of: "\n", with: " ").replacingOccurrences(of: "\"", with: "'")
        let markdown = """
        ---
        name: \(id)
        description: "\(safeDescription)"
        ---

        # \(name)

        \(instructions)
        """
        try Data(markdown.utf8).write(to: skillRoot.appendingPathComponent("SKILL.md"), options: .atomic)
        if !manifest.skills.contains(id) { manifest.skills.append(id) }
        manifest.skills.sort()
        try writeManagedManifest(manifest, to: manifestURL)
    }

    private func removeManagedSkill(id: String, pluginID: String?) throws {
        try validateManagedID(id)
        let ownerID = pluginID ?? "codes.ssh.cos.user-skills"
        try validateManagedID(ownerID)
        guard ownerID != "codes.ssh.cos.settings" else { throw ManagedArtifactError.builtInProtected }
        let pluginRoot = managedPluginsRoot.appendingPathComponent(ownerID, isDirectory: true)
        let manifestURL = pluginRoot.appendingPathComponent("cos.plugin.json")
        guard FileManager.default.fileExists(atPath: manifestURL.path) else { throw ManagedArtifactError.pluginNotFound(ownerID) }
        let skillRoot = pluginRoot.appendingPathComponent("skills/\(id)", isDirectory: true)
        guard FileManager.default.fileExists(atPath: skillRoot.path) else { throw ManagedArtifactError.skillNotFound(id) }
        try FileManager.default.trashItem(at: skillRoot, resultingItemURL: nil)
        var manifest = try JSONDecoder().decode(CosPluginManifest.self, from: Data(contentsOf: manifestURL))
        manifest.skills.removeAll { $0 == id }
        try writeManagedManifest(manifest, to: manifestURL)
        disabledSkillKeys.remove(skillKey(id, pluginID: ownerID))
        persistDisabledSkills()
    }

    private func createManagedPlugin(id: String, name: String, description: String, instructions: String?) throws {
        try validateManagedID(id)
        guard id != "codes.ssh.cos.settings" else { throw ManagedArtifactError.builtInProtected }
        try validateManagedText(name, maximum: 100)
        try validateManagedText(description, maximum: 500)
        if let instructions { try validateManagedText(instructions, maximum: 64_000) }
        let root = managedPluginsRoot.appendingPathComponent(id, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let hasInstructions = instructions?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        let manifest = CosPluginManifest(
            schemaVersion: 1,
            id: id,
            name: name,
            version: "1.0.0",
            author: NSFullUserName().isEmpty ? "Cos user" : NSFullUserName(),
            description: description,
            capabilities: [.init(id: "\(id).managed", description: "Plugin created through Cos self-management.", risk: "guarded")],
            skills: hasInstructions ? ["main"] : [],
            homepage: nil,
            builtIn: false
        )
        try writeManagedManifest(manifest, to: root.appendingPathComponent("cos.plugin.json"))
        if let instructions, hasInstructions {
            try createManagedSkill(id: "main", name: name, description: description, instructions: instructions, pluginID: id)
        }
    }

    private func removeManagedPlugin(id: String) throws {
        try validateManagedID(id)
        guard id != "codes.ssh.cos.settings" else { throw ManagedArtifactError.builtInProtected }
        let root = managedPluginsRoot.appendingPathComponent(id, isDirectory: true)
        guard FileManager.default.fileExists(atPath: root.path) else { throw ManagedArtifactError.pluginNotFound(id) }
        try FileManager.default.trashItem(at: root, resultingItemURL: nil)
        disabledPluginIDs.remove(id)
        disabledSkillKeys = Set(disabledSkillKeys.filter { !$0.hasPrefix(id + ":") })
        persistDisabledPlugins()
        persistDisabledSkills()
    }

    private func setPlugin(id: String, enabled: Bool) throws {
        try validateManagedID(id)
        guard id != "codes.ssh.cos.settings" else { throw ManagedArtifactError.builtInProtected }
        if enabled { disabledPluginIDs.remove(id) } else { disabledPluginIDs.insert(id) }
        persistDisabledPlugins()
    }

    func setPlugin(_ plugin: InstalledPlugin, enabled: Bool) {
        do {
            try setPlugin(id: plugin.id, enabled: enabled)
            if enabled, plugin.id == "codes.ssh.cos.computer-use" { requestComputerUseAccess() }
            if !enabled, plugin.id == "codes.ssh.cos.computer-use" { pendingComputerUseRun = nil }
            if !enabled, plugin.id == "codes.ssh.cos.betterwright" { isBrowserPanelPresented = false }
            Task { await reloadPlugins() }
        } catch {
            lastError = error.localizedDescription
        }
    }

    func removePlugin(_ plugin: InstalledPlugin) {
        do {
            guard plugin.manifest.builtIn != true else { throw ManagedArtifactError.builtInProtected }
            try FileManager.default.trashItem(at: plugin.location, resultingItemURL: nil)
            disabledPluginIDs.remove(plugin.id)
            disabledSkillKeys = Set(disabledSkillKeys.filter { !$0.hasPrefix(plugin.id + ":") })
            persistDisabledPlugins()
            persistDisabledSkills()
            activity = "Plugin moved to Trash"
            Task { await reloadPlugins() }
        } catch {
            lastError = error.localizedDescription
        }
    }

    func isSkillEnabled(_ skill: String, in plugin: InstalledPlugin) -> Bool {
        !disabledSkillKeys.contains(skillKey(skill, pluginID: plugin.id))
    }

    func setSkill(_ skill: String, in plugin: InstalledPlugin, enabled: Bool) {
        let key = skillKey(skill, pluginID: plugin.id)
        if enabled { disabledSkillKeys.remove(key) } else { disabledSkillKeys.insert(key) }
        persistDisabledSkills()
        activity = enabled ? "Skill enabled" : "Skill disabled"
        objectWillChange.send()
    }

    func removeSkill(_ skill: String, from plugin: InstalledPlugin) {
        do {
            guard plugin.manifest.builtIn != true else { throw ManagedArtifactError.builtInProtected }
            try validateManagedID(skill)
            let manifestURL = plugin.location.appendingPathComponent("cos.plugin.json")
            var manifest = try JSONDecoder().decode(CosPluginManifest.self, from: Data(contentsOf: manifestURL))
            guard manifest.skills.contains(skill) else { throw ManagedArtifactError.skillNotFound(skill) }
            let candidates = [
                plugin.location.appendingPathComponent("skills/\(skill)", isDirectory: true),
                plugin.location.appendingPathComponent(skill, isDirectory: true),
            ]
            guard let skillRoot = candidates.first(where: { FileManager.default.fileExists(atPath: $0.path) }) else {
                throw ManagedArtifactError.skillNotFound(skill)
            }
            try FileManager.default.trashItem(at: skillRoot, resultingItemURL: nil)
            manifest.skills.removeAll { $0 == skill }
            try writeManagedManifest(manifest, to: manifestURL)
            disabledSkillKeys.remove(skillKey(skill, pluginID: plugin.id))
            persistDisabledSkills()
            activity = "Skill moved to Trash"
            Task { await reloadPlugins() }
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func validateManagedID(_ id: String) throws {
        guard (2...64).contains(id.count),
              id.range(of: "^[a-z0-9][a-z0-9._-]*$", options: .regularExpression) != nil else {
            throw ManagedArtifactError.invalidID
        }
    }

    private func validateManagedText(_ text: String, maximum: Int) throws {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              text.utf8.count <= maximum else { throw ManagedArtifactError.invalidContent }
    }

    private func writeManagedManifest(_ manifest: CosPluginManifest, to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(manifest).write(to: url, options: .atomic)
    }

    private func persistDisabledPlugins() {
        Self.save(Array(disabledPluginIDs).sorted(), key: "disabledPluginIDs")
    }

    private func persistDisabledSkills() {
        Self.save(Array(disabledSkillKeys).sorted(), key: "disabledSkillKeys")
    }

    private func skillKey(_ skill: String, pluginID: String) -> String {
        pluginID + ":" + skill
    }

    func persistPreferences() {
        Self.save(preferences, key: "preferences")
    }

    private func normalizedWorkspacePath(_ path: String) -> String {
        URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.resolvingSymlinksInPath().path
    }

    private nonisolated static func looksLikeComputerUseRequest(_ prompt: String) -> Bool {
        let value = prompt.lowercased()
        if value.contains("@computer") || value.contains("computer use") { return true }
        if value.contains("@betterwright") || value.contains("/browser") { return false }
        let actions = ["open ", "click ", "type ", "send ", "log in", "login", "navigate ", "go to "]
        let destinations = [" app", "safari", "chrome", "chat "]
        return actions.contains(where: value.contains) && destinations.contains(where: value.contains)
    }

    private func scheduleTitleGeneration(threadID: UUID, prompt: String) {
        guard let model = selectedTitleModel,
              let provider = providers.first(where: { $0.id == model.providerID }) else { return }
        titleTasks.removeValue(forKey: threadID)?.cancel()
        let workspace = threads.first(where: { $0.id == threadID })?.workspacePath ?? preferences.defaultWorkspace
        let titlePrompt = """
        Write a specific 3–7 word task title for this user request. Use plain title case text only: no quotes, no markdown, no period, and no prefix such as “Title:”.

        User request:
        \(String(prompt.prefix(2_000)))
        """
        let titleThread = CosThread(id: threadID, workspacePath: workspace, modelID: model.id, effort: .low)
        let request = AgentRequest(
            prompt: titlePrompt,
            latestUserRequest: titlePrompt,
            thread: titleThread,
            model: model,
            provider: provider,
            effort: .low,
            fastMode: false,
            fullAccess: false,
            workspaceIsTrusted: true,
            toolsEnabled: false
        )

        titleTasks[threadID] = Task { [weak self] in
            guard let self else { return }
            do {
                var output = ""
                let stream = try runtime.stream(request: request)
                for try await event in stream {
                    guard !Task.isCancelled else { return }
                    if case .textDelta(let text) = event { output += text }
                }
                guard let title = Self.cleanGeneratedTitle(output),
                      let index = threads.firstIndex(where: { $0.id == threadID }) else { return }
                threads[index].title = title
                threads[index].updatedAt = Date()
                persist(threads[index])
            } catch {
                guard let index = threads.firstIndex(where: { $0.id == threadID }), threads[index].title == "New task" else { return }
                threads[index].title = Self.fallbackTitle(for: prompt)
                persist(threads[index])
            }
            titleTasks.removeValue(forKey: threadID)
        }
    }

    private nonisolated static func cleanGeneratedTitle(_ raw: String) -> String? {
        var title = raw.components(separatedBy: .newlines).first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        title = title.replacingOccurrences(of: "(?i)^title\\s*:\\s*", with: "", options: .regularExpression)
        title = title.trimmingCharacters(in: CharacterSet(charactersIn: "\"'`*_#–—-. "))
        title = title.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        guard title.count >= 3 else { return nil }
        return String(title.prefix(54)).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private nonisolated static func fallbackTitle(for prompt: String) -> String {
        let oneLine = prompt.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return String(oneLine.prefix(54))
    }

    private func normalizeLoadedThreadEfforts() {
        for index in threads.indices {
            guard let profile = models.first(where: { $0.id == threads[index].modelID }) else { continue }
            threads[index].effort = profile.normalizedEffort(threads[index].effort)
        }
        if let profile = models.first(where: { $0.id == preferences.selectedModelID }) {
            preferences.defaultEffort = profile.normalizedEffort(preferences.defaultEffort)
        }
    }

    private func isWorkspaceTrusted(_ path: String) -> Bool {
        trustedWorkspaces.contains(normalizedWorkspacePath(path))
    }

    private func activeExtensionInstructions() -> String {
        var sections: [String] = []
        var remaining = 48_000
        for plugin in plugins where plugin.isEnabled {
            let capabilitySummary = plugin.manifest.capabilities.map { "\($0.id): \($0.description)" }.joined(separator: "\n")
            sections.append("Plugin \(plugin.manifest.id) — \(plugin.manifest.description)\n\(capabilitySummary)")
            for skill in plugin.manifest.skills where remaining > 0 && isSkillEnabled(skill, in: plugin) {
                let candidates = [
                    plugin.location.appendingPathComponent("skills/\(skill)/SKILL.md"),
                    plugin.location.appendingPathComponent("\(skill)/SKILL.md"),
                ]
                guard let url = candidates.first(where: { FileManager.default.fileExists(atPath: $0.path) }),
                      let data = try? Data(contentsOf: url, options: [.mappedIfSafe]) else { continue }
                let slice = data.prefix(min(remaining, data.count))
                sections.append("Skill \(plugin.manifest.id):\(skill)\n\(String(decoding: slice, as: UTF8.self))")
                remaining -= slice.count
            }
        }
        return sections.joined(separator: "\n\n")
    }

    func saveCatalog() {
        Self.save(providers, key: "providers")
        Self.save(models, key: "models")
    }

    func resetCatalog() {
        providers = DefaultCatalog.providers
        models = DefaultCatalog.models
        saveCatalog()
    }

    func addProvider(name: String, baseURL: URL, modelName: String, modelID: String, apiKey: String) throws {
        let slug = "custom-" + UUID().uuidString.lowercased()
        let account = slug + "-key"
        try secureStore.set(apiKey, account: account)
        providers.append(.init(id: slug, name: name, bridge: .openAICompatible, authMode: .apiKey, baseURL: baseURL, keychainAccount: account))
        models.append(.init(id: "\(slug):\(modelID)", providerID: slug, name: modelName, model: modelID))
        saveCatalog()
    }

    func setAPIKey(_ value: String, for provider: ProviderProfile) throws {
        guard let account = provider.keychainAccount else { return }
        try secureStore.set(value, account: account)
        loginStatus[provider.id] = "Key stored in this Mac’s Keychain"
    }

    func hasAPIKey(for provider: ProviderProfile) -> Bool {
        guard let account = provider.keychainAccount else { return false }
        return (try? secureStore.get(account: account)) != nil
    }

    func signIn(to provider: ProviderProfile) {
        let command: [String]
        switch provider.bridge {
        case .codex: command = [provider.executable ?? "codex", "login"]
        case .claude: command = [provider.executable ?? "claude", "auth", "login"]
        case .opencode:
            command = provider.id == "xai"
                ? [provider.executable ?? "opencode", "auth", "login", "--provider", "xai"]
                : [provider.executable ?? "opencode", "auth", "login"]
        default:
            loginStatus[provider.id] = "This provider uses an API key."
            return
        }
        let shellCommand = command.map(Self.shellQuoted).joined(separator: " ")
        let script = "tell application \"Terminal\" to do script \"\(Self.appleScriptQuoted(shellCommand))\""
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", "tell application \"Terminal\" to activate", "-e", script]
        do {
            try process.run()
            loginStatus[provider.id] = provider.id == "xai"
                ? "Continue the SuperGrok / X Premium sign-in in Terminal"
                : "Continue sign-in in Terminal"
            monitorProviderSignIn(provider)
        } catch {
            loginStatus[provider.id] = "Could not open Terminal: \(error.localizedDescription)"
        }
    }

    func refreshProviderSessions() {
        var sessions: [String: ProviderSessionInfo] = [:]
        for provider in providers where provider.authMode == .subscription {
            if let session = runtime.sessionInfo(for: provider) { sessions[provider.id] = session }
        }
        providerSessions = sessions
    }

    private func monitorProviderSignIn(_ provider: ProviderProfile) {
        Task { [weak self] in
            for _ in 0..<90 {
                try? await Task.sleep(for: .seconds(2))
                guard let self else { return }
                self.refreshProviderSessions()
                if let session = self.providerSessions[provider.id] {
                    self.loginStatus[provider.id] = "Signed in as \(session.displayName)"
                    return
                }
            }
        }
    }

    private static func shellQuoted(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private static func appleScriptQuoted(_ value: String) -> String {
        value.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
    }

    func reloadPlugins() async {
        let workspace = selectedThread.map { URL(fileURLWithPath: $0.workspacePath, isDirectory: true) }
        var discovered = await registry.discover(builtInURL: builtInPluginsURL, workspace: workspace)
        for index in discovered.indices {
            discovered[index].isEnabled = discovered[index].id == "codes.ssh.cos.settings" || !disabledPluginIDs.contains(discovered[index].id)
        }
        plugins = discovered
        refreshComputerUseAccess()
    }

    func loadMarketplace(force: Bool = false) async {
        guard !isLoadingMarketplace else { return }
        if !force, !marketplacePlugins.isEmpty { return }
        isLoadingMarketplace = true
        marketplaceError = nil
        defer { isLoadingMarketplace = false }
        do {
            var request = URLRequest(
                url: URL(string: "https://cos.ssh.codes/api/plugins")!,
                cachePolicy: force ? .reloadIgnoringLocalAndRemoteCacheData : .returnCacheDataElseLoad,
                timeoutInterval: 15
            )
            request.setValue("application/json", forHTTPHeaderField: "Accept")
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode), data.count <= 2_000_000 else {
                throw MarketplaceError.invalidResponse
            }
            let catalog = try JSONDecoder().decode(CosMarketplaceResponse.self, from: data)
            marketplacePlugins = catalog.items.sorted {
                if ($0.featured == true) != ($1.featured == true) { return $0.featured == true }
                return $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
            }
        } catch {
            marketplaceError = error.localizedDescription
        }
    }

    func installMarketplacePlugin(_ listing: CosMarketplaceListing) {
        guard listing.type == "plugin", installingMarketplacePluginID == nil else { return }
        if listing.builtIn == true {
            if listing.id == "codes.ssh.cos.computer-use" { requestComputerUseAccess() }
            if !plugins.contains(where: { $0.id == listing.id }) {
                lastError = "\(listing.name) is included with the latest Cos build. Install the current Cos update, then reopen Plugins & Skills."
            }
            return
        }

        installingMarketplacePluginID = listing.id
        Task { [weak self] in
            guard let self else { return }
            do {
                let manifest = try await self.marketplaceManifest(for: listing)
                guard manifest.schemaVersion == 1, manifest.id == listing.id,
                      manifest.id.range(of: "^[a-z0-9][a-z0-9._-]{1,63}$", options: .regularExpression) != nil else {
                    throw MarketplaceError.invalidManifest
                }
                let target = self.managedPluginsRoot.appendingPathComponent(manifest.id, isDirectory: true)
                try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
                try self.writeManagedManifest(manifest, to: target.appendingPathComponent("cos.plugin.json"))
                self.disabledPluginIDs.remove(manifest.id)
                self.persistDisabledPlugins()
                await self.reloadPlugins()
                self.activity = "\(manifest.name) installed"
            } catch {
                self.lastError = "Could not install \(listing.name): \(error.localizedDescription)"
            }
            self.installingMarketplacePluginID = nil
        }
    }

    private func marketplaceManifest(for listing: CosMarketplaceListing) async throws -> CosPluginManifest {
        if let manifest = listing.manifest { return manifest }
        guard let encodedID = listing.id.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed),
              let url = URL(string: "https://cos.ssh.codes/api/plugins/\(encodedID)/manifest") else {
            throw MarketplaceError.invalidManifest
        }
        let (data, response) = try await URLSession.shared.data(from: url)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode), data.count <= 256_000 else {
            throw MarketplaceError.invalidResponse
        }
        return try JSONDecoder().decode(CosPluginManifest.self, from: data)
    }

    func refreshComputerUseAccess() {
        computerUseAccessGranted = CosComputerUseAccess.isGranted
        if computerUseAccessGranted {
            computerUseAccessStatus = "Accessibility access granted"
            resumePendingComputerUseRun()
        }
    }

    func requestComputerUseAccess() {
        if CosComputerUseAccess.isGranted {
            refreshComputerUseAccess()
            return
        }
        computerUseAccessStatus = "Use the macOS prompt to allow Cos in Accessibility."
        _ = CosComputerUseAccess.request()
        Task { [weak self] in
            for _ in 0..<60 {
                try? await Task.sleep(for: .milliseconds(500))
                guard let self else { return }
                self.refreshComputerUseAccess()
                if self.computerUseAccessGranted { return }
            }
        }
    }

    func openAccessibilitySettings() {
        guard let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") else { return }
        NSWorkspace.shared.open(url)
    }

    private func resumePendingComputerUseRun() {
        guard computerUseAccessGranted,
              !isRunning,
              let pendingComputerUseRun,
              selectedThreadID == pendingComputerUseRun.threadID else { return }
        self.pendingComputerUseRun = nil
        startRun(pendingComputerUseRun.prompt, appendUserMessage: false)
    }

    func installPluginFromDisk() {
        let panel = NSOpenPanel()
        panel.title = "Choose a Cos plugin manifest"
        panel.prompt = "Install"
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let manifestURL = panel.url,
              manifestURL.lastPathComponent == "cos.plugin.json" else {
            if panel.url != nil { lastError = "Choose a file named cos.plugin.json." }
            return
        }
        do {
            let manifest = try JSONDecoder().decode(CosPluginManifest.self, from: Data(contentsOf: manifestURL))
            let pluginsRoot = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Cos/Plugins", isDirectory: true)
            try FileManager.default.createDirectory(at: pluginsRoot, withIntermediateDirectories: true)
            let target = pluginsRoot.appendingPathComponent(manifest.id, isDirectory: true)
            if FileManager.default.fileExists(atPath: target.path) {
                try FileManager.default.removeItem(at: target)
            }
            try FileManager.default.copyItem(at: manifestURL.deletingLastPathComponent(), to: target)
            if manifest.id == "codes.ssh.cos.computer-use" { requestComputerUseAccess() }
            Task { await reloadPlugins() }
        } catch {
            lastError = "Could not install the plugin: \(error.localizedDescription)"
        }
    }

    func refreshSkillImportCounts() {
        for source in ExternalSkillSource.allCases where source != .folder {
            skillImportCounts[source] = Self.discoverSkillDirectories(in: source.defaultRoots).count
        }
    }

    func importSkills(from source: ExternalSkillSource) {
        guard source != .folder else { importSkillsFromFolder(); return }
        importSkills(from: source.defaultRoots, source: source)
    }

    func importSkillsFromFolder() {
        let panel = NSOpenPanel()
        panel.title = "Choose a skills folder"
        panel.prompt = "Import Skills"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK, !panel.urls.isEmpty else { return }
        importSkills(from: panel.urls, source: .folder)
    }

    private func importSkills(from roots: [URL], source: ExternalSkillSource) {
        skillImportStatus[source] = "Importing…"
        do {
            let result = try Self.performSkillImport(from: roots, source: source, pluginsRoot: managedPluginsRoot)
            skillImportStatus[source] = result.imported == 0
                ? "No compatible skills found"
                : "Imported \(result.imported) skill\(result.imported == 1 ? "" : "s")\(result.skipped > 0 ? " · \(result.skipped) skipped" : "")"
            activity = result.imported == 0 ? "No skills imported" : "Skills imported"
            refreshSkillImportCounts()
            Task { await reloadPlugins() }
        } catch {
            skillImportStatus[source] = "Import failed"
            lastError = "Could not import skills: \(error.localizedDescription)"
            activity = "Needs attention"
        }
    }

    private nonisolated static func discoverSkillDirectories(in roots: [URL]) -> [URL] {
        var found: [String: URL] = [:]
        let manager = FileManager.default
        for root in roots {
            let resolvedRoot = root.standardizedFileURL.resolvingSymlinksInPath()
            var isDirectory: ObjCBool = false
            guard manager.fileExists(atPath: resolvedRoot.path, isDirectory: &isDirectory), isDirectory.boolValue else { continue }
            let directManifest = resolvedRoot.appendingPathComponent("SKILL.md")
            if manager.fileExists(atPath: directManifest.path) { found[resolvedRoot.path] = resolvedRoot }
            guard let enumerator = manager.enumerator(
                at: resolvedRoot,
                includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { continue }
            for case let url as URL in enumerator {
                if ["node_modules", ".git", ".build"].contains(url.lastPathComponent) {
                    enumerator.skipDescendants()
                    continue
                }
                guard url.lastPathComponent == "SKILL.md" else { continue }
                let directory = url.deletingLastPathComponent().standardizedFileURL
                found[directory.path] = directory
                enumerator.skipDescendants()
            }
        }
        return found.values.sorted { $0.path.localizedCaseInsensitiveCompare($1.path) == .orderedAscending }
    }

    private nonisolated static func performSkillImport(
        from roots: [URL],
        source: ExternalSkillSource,
        pluginsRoot: URL
    ) throws -> (imported: Int, skipped: Int) {
        let directories = discoverSkillDirectories(in: roots)
        guard !directories.isEmpty else { return (0, 0) }
        let manager = FileManager.default
        let pluginRoot = pluginsRoot.appendingPathComponent(source.pluginID, isDirectory: true)
        let skillsRoot = pluginRoot.appendingPathComponent("skills", isDirectory: true)
        try manager.createDirectory(at: skillsRoot, withIntermediateDirectories: true)

        var imported: [String] = []
        var skipped = 0
        for directory in directories {
            guard let id = importedSkillID(at: directory), !imported.contains(id) else { skipped += 1; continue }
            do {
                try copyImportedSkill(from: directory, to: skillsRoot.appendingPathComponent(id, isDirectory: true))
                imported.append(id)
            } catch {
                skipped += 1
            }
        }

        let manifest = CosPluginManifest(
            schemaVersion: 1,
            id: source.pluginID,
            name: "Imported from \(source.title)",
            version: "1.0.0",
            author: "Cos Importer",
            description: "Portable skills imported locally from \(source.title). Original source folders are left unchanged.",
            capabilities: [.init(id: "cos.skills.import", description: "Read-only import of portable SKILL.md bundles selected by the user.", risk: "safe")],
            skills: imported.sorted(),
            homepage: nil,
            builtIn: false
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(manifest).write(to: pluginRoot.appendingPathComponent("cos.plugin.json"), options: .atomic)
        return (imported.count, skipped)
    }

    private nonisolated static func importedSkillID(at directory: URL) -> String? {
        let manifest = directory.appendingPathComponent("SKILL.md")
        guard let data = try? Data(contentsOf: manifest, options: [.mappedIfSafe]), data.count <= 1_000_000 else { return nil }
        let text = String(decoding: data.prefix(64_000), as: UTF8.self)
        let expression = try? NSRegularExpression(pattern: "(?m)^name:\\s*[\\\"']?([^\\\"'\\n]+)")
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        let raw: String
        if let match = expression?.firstMatch(in: text, range: range),
           let matchRange = Range(match.range(at: 1), in: text) {
            raw = String(text[matchRange])
        } else {
            raw = directory.lastPathComponent
        }
        let normalized = raw.lowercased()
            .replacingOccurrences(of: "[^a-z0-9._-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-._"))
        guard (2...64).contains(normalized.count), normalized.first?.isLetter == true || normalized.first?.isNumber == true else { return nil }
        return normalized
    }

    private nonisolated static func copyImportedSkill(from source: URL, to target: URL) throws {
        let manager = FileManager.default
        let staging = target.deletingLastPathComponent().appendingPathComponent(".import-\(UUID().uuidString)", isDirectory: true)
        try manager.createDirectory(at: staging, withIntermediateDirectories: true)
        var totalBytes = 0
        var fileCount = 0
        do {
            guard let enumerator = manager.enumerator(
                at: source,
                includingPropertiesForKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { throw SkillImportError.unreadable }
            for case let item as URL in enumerator {
                if ["node_modules", ".git", ".build"].contains(item.lastPathComponent) {
                    enumerator.skipDescendants()
                    continue
                }
                let values = try item.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
                if values.isSymbolicLink == true { enumerator.skipDescendants(); continue }
                let relative = String(item.path.dropFirst(source.path.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                guard !relative.isEmpty, !relative.contains("../") else { continue }
                let destination = staging.appendingPathComponent(relative, isDirectory: values.isDirectory == true)
                if values.isDirectory == true {
                    try manager.createDirectory(at: destination, withIntermediateDirectories: true)
                } else if values.isRegularFile == true {
                    fileCount += 1
                    totalBytes += values.fileSize ?? 0
                    guard fileCount <= 1_000, totalBytes <= 10_000_000, (values.fileSize ?? 0) <= 2_000_000 else { throw SkillImportError.tooLarge }
                    try manager.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
                    try manager.copyItem(at: item, to: destination)
                }
            }
            guard manager.fileExists(atPath: staging.appendingPathComponent("SKILL.md").path) else { throw SkillImportError.unreadable }
            if manager.fileExists(atPath: target.path) { try manager.trashItem(at: target, resultingItemURL: nil) }
            try manager.moveItem(at: staging, to: target)
        } catch {
            try? manager.removeItem(at: staging)
            throw error
        }
    }

    private func persist(_ thread: CosThread) {
        Task {
            do { try await store.upsert(thread) }
            catch { Self.logger.error("Could not save thread: \(error.localizedDescription, privacy: .public)") }
        }
    }

    private static func load<T: Decodable>(_ type: T.Type, key: String) -> T? {
        guard let data = UserDefaults.standard.data(forKey: "cos.\(key)") else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }

    private static func save<T: Encodable>(_ value: T, key: String) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        UserDefaults.standard.set(data, forKey: "cos.\(key)")
    }

    private static func mergeProviders(_ saved: [ProviderProfile]?) -> [ProviderProfile] {
        var result = saved ?? []
        for item in DefaultCatalog.providers {
            if let index = result.firstIndex(where: { $0.id == item.id }) {
                result[index].bridge = item.bridge
                result[index].authMode = item.authMode
                result[index].baseURL = item.baseURL
                result[index].keychainAccount = item.keychainAccount
                result[index].executable = item.executable
            } else {
                result.append(item)
            }
        }
        return result
    }

    private static func mergeModels(_ saved: [ModelProfile]?) -> [ModelProfile] {
        var result = (saved ?? []).filter { $0.id != "anthropic:claude-5" }
        for item in DefaultCatalog.models {
            if let index = result.firstIndex(where: { $0.id == item.id }) {
                result[index].providerID = item.providerID
                result[index].name = item.name
                result[index].model = item.model
                result[index].contextWindow = item.contextWindow
                result[index].supportsImages = item.supportsImages
                result[index].supportsTools = item.supportsTools
                result[index].supportedEfforts = item.supportedEfforts
            } else {
                result.append(item)
            }
        }
        return result
    }
}

private enum ManagedArtifactError: LocalizedError {
    case invalidID
    case invalidContent
    case builtInProtected
    case pluginNotFound(String)
    case skillNotFound(String)

    var errorDescription: String? {
        switch self {
        case .invalidID: "Use a 2–64 character lowercase ID made from letters, numbers, dots, underscores, or hyphens."
        case .invalidContent: "Names and instructions must be non-empty and within Cos’s size limits."
        case .builtInProtected: "The built-in Cos plugin cannot be disabled, removed, or overwritten."
        case .pluginNotFound(let id): "Plugin \(id) was not found in Cos-managed storage."
        case .skillNotFound(let id): "Skill \(id) was not found in that plugin."
        }
    }
}

private enum MarketplaceError: LocalizedError {
    case invalidResponse
    case invalidManifest

    var errorDescription: String? {
        switch self {
        case .invalidResponse: "The Cos marketplace returned an invalid response."
        case .invalidManifest: "The marketplace plugin manifest is invalid or does not match its listing."
        }
    }
}

private enum SkillImportError: LocalizedError {
    case unreadable
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .unreadable: "The selected folder does not contain a readable SKILL.md bundle."
        case .tooLarge: "A skill exceeded Cos’s 10 MB or 1,000-file import limit."
        }
    }
}
