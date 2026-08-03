import Foundation

public enum ReasoningEffort: String, Codable, CaseIterable, Identifiable, Sendable {
    case minimal
    case low
    case medium
    case high
    case extraHigh
    case max

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .minimal: "Minimal"
        case .low: "Low"
        case .medium: "Medium"
        case .high: "High"
        case .extraHigh: "Extra High"
        case .max: "Max"
        }
    }

    public var shortTitle: String {
        self == .extraHigh ? "Extra High" : title
    }

    public var rank: Int {
        Self.allCases.firstIndex(of: self) ?? 0
    }
}

public enum ProviderBridge: String, Codable, CaseIterable, Identifiable, Sendable {
    case codex
    case pi
    case claude
    case opencode
    case qwen
    case openAICompatible

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .codex: "ChatGPT"
        case .pi: "Pi"
        case .claude: "Claude"
        case .opencode: "OpenCode"
        case .qwen: "Qwen"
        case .openAICompatible: "OpenAI compatible"
        }
    }
}

public enum AuthenticationMode: String, Codable, CaseIterable, Sendable {
    case subscription
    case apiKey
    case local
}

public struct ProviderProfile: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var name: String
    public var bridge: ProviderBridge
    public var authMode: AuthenticationMode
    public var baseURL: URL?
    public var keychainAccount: String?
    public var executable: String?
    public var isEnabled: Bool

    public init(
        id: String,
        name: String,
        bridge: ProviderBridge,
        authMode: AuthenticationMode,
        baseURL: URL? = nil,
        keychainAccount: String? = nil,
        executable: String? = nil,
        isEnabled: Bool = true
    ) {
        self.id = id
        self.name = name
        self.bridge = bridge
        self.authMode = authMode
        self.baseURL = baseURL
        self.keychainAccount = keychainAccount
        self.executable = executable
        self.isEnabled = isEnabled
    }
}

public struct ModelProfile: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var providerID: String
    public var name: String
    public var model: String
    public var contextWindow: Int
    public var supportsImages: Bool
    public var supportsTools: Bool
    public var supportedEfforts: [ReasoningEffort]

    public init(
        id: String,
        providerID: String,
        name: String,
        model: String,
        contextWindow: Int = 200_000,
        supportsImages: Bool = true,
        supportsTools: Bool = true,
        supportedEfforts: [ReasoningEffort] = ReasoningEffort.allCases
    ) {
        self.id = id
        self.providerID = providerID
        self.name = name
        self.model = model
        self.contextWindow = contextWindow
        self.supportsImages = supportsImages
        self.supportsTools = supportsTools
        self.supportedEfforts = supportedEfforts
    }

    public var effortOptions: [ReasoningEffort] {
        supportedEfforts.isEmpty ? [.high] : supportedEfforts
    }

    public var supportsFastMode: Bool {
        providerID == "chatgpt"
    }

    public func normalizedEffort(_ requested: ReasoningEffort) -> ReasoningEffort {
        let options = effortOptions
        if options.contains(requested) { return requested }
        return options.min {
            let left = abs($0.rank - requested.rank)
            let right = abs($1.rank - requested.rank)
            return left == right ? $0.rank > $1.rank : left < right
        } ?? .high
    }
}

public enum MessageRole: String, Codable, Sendable {
    case system
    case user
    case assistant
    case tool
}

public enum WorkTraceKind: String, Codable, Sendable {
    case status
    case reasoning
    case tool
}

public struct WorkTraceItem: Codable, Identifiable, Hashable, Sendable {
    public var id: UUID
    public var kind: WorkTraceKind
    public var title: String
    public var detail: String
    public var createdAt: Date

    public init(id: UUID = UUID(), kind: WorkTraceKind, title: String, detail: String = "", createdAt: Date = Date()) {
        self.id = id
        self.kind = kind
        self.title = title
        self.detail = detail
        self.createdAt = createdAt
    }
}

public struct ChatMessage: Codable, Identifiable, Hashable, Sendable {
    public var id: UUID
    public var role: MessageRole
    public var content: String
    public var createdAt: Date
    public var isStreaming: Bool
    public var workItems: [WorkTraceItem]?

    public init(
        id: UUID = UUID(),
        role: MessageRole,
        content: String,
        createdAt: Date = Date(),
        isStreaming: Bool = false,
        workItems: [WorkTraceItem]? = nil
    ) {
        self.id = id
        self.role = role
        self.content = content
        self.createdAt = createdAt
        self.isStreaming = isStreaming
        self.workItems = workItems
    }
}

public enum GoalStatus: String, Codable, CaseIterable, Sendable {
    case active
    case paused
    case blocked
    case budgetLimited
    case complete
}

public struct AgentGoal: Codable, Hashable, Sendable {
    public var objective: String
    public var status: GoalStatus
    public var tokenBudget: Int?
    public var usedTokens: Int
    public var createdAt: Date

    public init(
        objective: String,
        status: GoalStatus = .active,
        tokenBudget: Int? = nil,
        usedTokens: Int = 0,
        createdAt: Date = Date()
    ) {
        self.objective = objective
        self.status = status
        self.tokenBudget = tokenBudget
        self.usedTokens = usedTokens
        self.createdAt = createdAt
    }
}

public struct CosThread: Codable, Identifiable, Hashable, Sendable {
    public var id: UUID
    public var title: String
    public var workspacePath: String
    public var modelID: String
    public var effort: ReasoningEffort
    public var messages: [ChatMessage]
    public var goal: AgentGoal?
    public var compactedContext: String?
    public var createdAt: Date
    public var updatedAt: Date

    public init(
        id: UUID = UUID(),
        title: String = "New task",
        workspacePath: String,
        modelID: String,
        effort: ReasoningEffort = .high,
        messages: [ChatMessage] = [],
        goal: AgentGoal? = nil,
        compactedContext: String? = nil,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = id
        self.title = title
        self.workspacePath = workspacePath
        self.modelID = modelID
        self.effort = effort
        self.messages = messages
        self.goal = goal
        self.compactedContext = compactedContext
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public struct AgentRequest: Sendable {
    public var prompt: String
    public var latestUserRequest: String
    public var thread: CosThread
    public var model: ModelProfile
    public var provider: ProviderProfile
    public var effort: ReasoningEffort
    public var fastMode: Bool
    public var fullAccess: Bool
    public var workspaceIsTrusted: Bool
    public var extensionInstructions: String

    public init(
        prompt: String,
        latestUserRequest: String? = nil,
        thread: CosThread,
        model: ModelProfile,
        provider: ProviderProfile,
        effort: ReasoningEffort,
        fastMode: Bool,
        fullAccess: Bool,
        workspaceIsTrusted: Bool = false,
        extensionInstructions: String = ""
    ) {
        self.prompt = prompt
        self.latestUserRequest = latestUserRequest ?? prompt
        self.thread = thread
        self.model = model
        self.provider = provider
        self.effort = effort
        self.fastMode = fastMode
        self.fullAccess = fullAccess
        self.workspaceIsTrusted = workspaceIsTrusted
        self.extensionInstructions = extensionInstructions
    }
}

public enum AgentEvent: Sendable, Equatable {
    case status(String)
    case workDelta(String)
    case textDelta(String)
    case tool(name: String, detail: String)
    case usage(input: Int, output: Int)
    case completed
}

public struct AppPreferences: Codable, Equatable, Sendable {
    public var appearance = AppearanceMode.system
    public var fastMode = false
    public var fullAccess = true
    public var autoCompact = true
    public var compactAtPercent = 78.0
    public var keepRecentTokens = 20_000
    public var showTokenUsage = false
    public var animateStreaming = true
    public var defaultWorkspace = FileManager.default.homeDirectoryForCurrentUser.path
    public var selectedModelID = DefaultCatalog.models[0].id
    public var defaultEffort = ReasoningEffort.high

    public init() {}
}

public enum AppearanceMode: String, Codable, CaseIterable, Identifiable, Sendable {
    case system
    case light
    case dark
    case trueDark

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        case .trueDark: "True Dark"
        }
    }
}

public enum DefaultCatalog {
    public static let providers: [ProviderProfile] = [
        .init(id: "chatgpt", name: "ChatGPT Plus / Pro", bridge: .codex, authMode: .subscription, baseURL: URL(string: "https://chatgpt.com/backend-api/codex"), executable: "codex"),
        .init(id: "anthropic", name: "Claude Pro / Max", bridge: .claude, authMode: .subscription, baseURL: URL(string: "https://api.anthropic.com/v1"), keychainAccount: "anthropic-subscription", executable: "claude"),
        .init(id: "xai", name: "X Premium / SuperGrok", bridge: .opencode, authMode: .subscription, baseURL: URL(string: "https://api.x.ai/v1"), keychainAccount: "xai-subscription", executable: "opencode"),
        .init(id: "opencode-go", name: "OpenCode Go", bridge: .opencode, authMode: .apiKey, baseURL: URL(string: "https://api.opencode.ai/v1"), keychainAccount: "opencode-go", executable: "opencode"),
        .init(id: "qwen", name: "Qwen Token Plan", bridge: .qwen, authMode: .apiKey, baseURL: URL(string: "https://coding-intl.dashscope.aliyuncs.com/v1"), keychainAccount: "qwen-token-plan", executable: "qwen"),
        .init(id: "pi", name: "Pi harness", bridge: .pi, authMode: .local, executable: "pi"),
        .init(id: "openai-api", name: "OpenAI API", bridge: .openAICompatible, authMode: .apiKey, baseURL: URL(string: "https://api.openai.com/v1"), keychainAccount: "openai-api"),
    ]

    public static let models: [ModelProfile] = [
        .init(id: "chatgpt:gpt-5.6-sol", providerID: "chatgpt", name: "5.6 Sol", model: "gpt-5.6-sol", contextWindow: 400_000),
        .init(id: "chatgpt:gpt-5.6-terra", providerID: "chatgpt", name: "5.6 Terra", model: "gpt-5.6-terra", contextWindow: 400_000),
        .init(id: "anthropic:claude-opus-5", providerID: "anthropic", name: "Claude Opus 5", model: "claude-opus-5", contextWindow: 200_000, supportedEfforts: [.low, .medium, .high, .extraHigh, .max]),
        .init(id: "anthropic:claude-sonnet-5", providerID: "anthropic", name: "Claude Sonnet 5", model: "claude-sonnet-5", contextWindow: 200_000, supportedEfforts: [.low, .medium, .high, .extraHigh, .max]),
        .init(id: "anthropic:claude-fable-5", providerID: "anthropic", name: "Claude Fable 5", model: "claude-fable-5", contextWindow: 200_000, supportedEfforts: [.low, .medium, .high, .extraHigh, .max]),
        .init(id: "xai:grok-4.5", providerID: "xai", name: "Grok 4.5", model: "grok-4.5", contextWindow: 256_000, supportedEfforts: [.low, .medium, .high]),
        .init(id: "opencode-go:big-pickle", providerID: "opencode-go", name: "Big Pickle", model: "opencode/big-pickle", contextWindow: 200_000),
        .init(id: "qwen:qwen3.8-max", providerID: "qwen", name: "Qwen 3.8 Max", model: "qwen3.8-max", contextWindow: 262_144),
        .init(id: "pi:smart", providerID: "pi", name: "Pi Smart Route", model: "smart", contextWindow: 200_000),
        .init(id: "openai-api:custom", providerID: "openai-api", name: "OpenAI API model", model: "gpt-5.6", contextWindow: 400_000),
    ]
}
