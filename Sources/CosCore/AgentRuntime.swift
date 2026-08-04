import Foundation

public enum AgentRuntimeError: LocalizedError {
    case missingAPIKey(String)
    case unsupportedProvider(String)
    case launchFailed(String)
    case requestFailed(Int, String)
    case directoryTrustRequired(String)
    case invalidProviderResponse(String)

    public var errorDescription: String? {
        switch self {
        case .missingAPIKey(let provider): "Sign in or add a key for \(provider) in Settings → Providers."
        case .unsupportedProvider(let provider): "\(provider) is not supported by the Cos harness."
        case .launchFailed(let detail): "The Cos harness could not run the task: \(detail)"
        case .requestFailed(let code, let detail): "The model request failed (HTTP \(code)): \(detail)"
        case .directoryTrustRequired(let path): "Cos needs permission to trust \(path) before it can work there."
        case .invalidProviderResponse(let detail): "The provider returned an invalid response: \(detail)"
        }
    }
}

public struct AgentCredential: Sendable {
    public var token: String
    public var accountID: String?
    public var email: String?

    public init(token: String, accountID: String? = nil, email: String? = nil) {
        self.token = token
        self.accountID = accountID
        self.email = email
    }
}

public struct ProviderSessionInfo: Equatable, Sendable {
    public var email: String?
    public var accountID: String?

    public init(email: String? = nil, accountID: String? = nil) {
        self.email = email
        self.accountID = accountID
    }

    public var displayName: String {
        email ?? accountID ?? "Connected subscription"
    }
}

public struct AgentRuntime: Sendable {
    private let secureStore: SecureStore
    private let credentials: LocalSubscriptionCredentialResolver
    private let harness: CosHarness

    public init(secureStore: SecureStore = SecureStore()) {
        self.secureStore = secureStore
        self.credentials = LocalSubscriptionCredentialResolver()
        self.harness = CosHarness()
    }

    public func stream(request: AgentRequest) throws -> AsyncThrowingStream<AgentEvent, Error> {
        guard request.workspaceIsTrusted else {
            throw AgentRuntimeError.directoryTrustRequired(request.thread.workspacePath)
        }

        let (routedRequest, credential) = try routedRequestAndCredential(for: request)
        return harness.stream(
            request: routedRequest,
            credential: credential,
            subagentRunner: { subagentRequest in
                try subagentStream(parent: routedRequest, request: subagentRequest)
            }
        )
    }

    public func accessibleSubagentRoutes(
        providers: [ProviderProfile],
        models: [ModelProfile]
    ) -> [SubagentRoute] {
        let providerPairs: [(String, ProviderProfile)] = providers.compactMap { provider in
            guard provider.isEnabled,
                  provider.bridge != .pi,
                  (try? credential(for: provider)) != nil else { return nil }
            return (provider.id, provider)
        }
        let usableProviders = Dictionary(uniqueKeysWithValues: providerPairs)
        return models.compactMap { model in
            guard model.supportsTools, let provider = usableProviders[model.providerID] else { return nil }
            return SubagentRoute(model: model, provider: provider)
        }
    }

    public func sessionInfo(for provider: ProviderProfile) -> ProviderSessionInfo? {
        guard provider.authMode == .subscription,
              let credential = try? credentials.subscriptionCredential(for: provider) else { return nil }
        return ProviderSessionInfo(email: credential.email, accountID: credential.accountID)
    }

    private func subagentStream(
        parent: AgentRequest,
        request: CosSubagentRequest
    ) throws -> AsyncThrowingStream<AgentEvent, Error> {
        guard parent.subagentsAuthorized, parent.agentDepth == 0 else {
            throw AgentRuntimeError.launchFailed("subagents were not authorized by the user for this run")
        }
        guard let route = parent.availableSubagentRoutes.first(where: { $0.id == request.modelID }) else {
            throw AgentRuntimeError.launchFailed("\(request.modelID) is not in this run's accessible subagent model allowlist")
        }
        guard route.accepts(request.effort) else {
            let valid = route.model.effortOptions.map(\.title).joined(separator: ", ")
            throw AgentRuntimeError.launchFailed("\(route.model.name) does not support \(request.effort.title) reasoning. Available efforts: \(valid)")
        }

        let childThread = CosThread(
            workspacePath: parent.thread.workspacePath,
            modelID: route.model.id,
            effort: request.effort,
            messages: [.init(role: .user, content: request.task)]
        )
        let childRequest = AgentRequest(
            prompt: """
            You are a focused Cos subagent. Complete this bounded delegated task and return a concise, evidence-based result to the parent agent.

            Delegated task:
            \(request.task)
            """,
            latestUserRequest: request.task,
            thread: childThread,
            model: route.model,
            provider: route.provider,
            effort: request.effort,
            fastMode: parent.fastMode && route.model.supportsFastMode,
            fullAccess: parent.fullAccess,
            workspaceIsTrusted: parent.workspaceIsTrusted,
            extensionInstructions: parent.extensionInstructions,
            toolsEnabled: true,
            computerUseEnabled: false,
            availableSubagentRoutes: [],
            subagentsAuthorized: false,
            agentDepth: parent.agentDepth + 1
        )
        let (routedRequest, credential) = try routedRequestAndCredential(for: childRequest)
        return harness.stream(request: routedRequest, credential: credential, subagentRunner: nil)
    }

    private func routedRequestAndCredential(for request: AgentRequest) throws -> (AgentRequest, AgentCredential) {
        var routedRequest = request
        if request.provider.bridge == .pi {
            guard let chatGPT = DefaultCatalog.providers.first(where: { $0.id == "chatgpt" }),
                  let model = DefaultCatalog.models.first(where: { $0.providerID == "chatgpt" }) else {
                throw AgentRuntimeError.unsupportedProvider(request.provider.name)
            }
            routedRequest.provider = chatGPT
            routedRequest.model = model
            guard let credential = try credential(for: chatGPT) else {
                throw AgentRuntimeError.missingAPIKey(chatGPT.name)
            }
            return (routedRequest, credential)
        }
        guard let credential = try credential(for: request.provider) else {
            throw AgentRuntimeError.missingAPIKey(request.provider.name)
        }
        return (routedRequest, credential)
    }

    private func credential(for provider: ProviderProfile) throws -> AgentCredential? {
        if let account = provider.keychainAccount,
           let token = try secureStore.get(account: account) {
            return AgentCredential(token: token)
        }
        if provider.authMode == .subscription {
            return try credentials.subscriptionCredential(for: provider)
        }
        return nil
    }
}

private struct LocalSubscriptionCredentialResolver: Sendable {
    func subscriptionCredential(for provider: ProviderProfile) throws -> AgentCredential? {
        switch provider.id {
        case "chatgpt": return chatGPTCredential()
        case "xai": return openCodeCredential(named: "xai")
        case "opencode-go": return openCodeCredential(named: "opencode-go")
        case "anthropic":
            if let value = ProcessInfo.processInfo.environment["ANTHROPIC_API_KEY"], !value.isEmpty {
                return AgentCredential(token: value)
            }
            return credentialFromJSON(
                at: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".claude/.credentials.json"),
                preferredKeys: ["accessToken", "access_token", "token", "key"]
            )
        default: return nil
        }
    }

    private func chatGPTCredential() -> AgentCredential? {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex/auth.json")
        guard let root = json(at: url),
              let tokens = root["tokens"] as? [String: Any],
              let access = tokens["access_token"] as? String,
              !access.isEmpty else { return nil }
        let account = (tokens["account_id"] as? String) ?? accountID(fromJWT: access)
        let idToken = tokens["id_token"] as? String
        let email = findString(named: "email", in: root)
            ?? idToken.flatMap { string(named: "email", inJWT: $0) }
            ?? string(named: "email", inJWT: access)
        return AgentCredential(token: access, accountID: account, email: email)
    }

    private func openCodeCredential(named name: String) -> AgentCredential? {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/share/opencode/auth.json")
        guard let root = json(at: url), let entry = root[name] as? [String: Any] else { return nil }
        for key in ["access", "token", "key"] {
            if let token = entry[key] as? String, !token.isEmpty {
                let account = (entry["accountId"] as? String) ?? accountID(fromJWT: token)
                let email = (entry["email"] as? String) ?? string(named: "email", inJWT: token)
                return AgentCredential(token: token, accountID: account, email: email)
            }
        }
        return nil
    }

    private func credentialFromJSON(at url: URL, preferredKeys: [String]) -> AgentCredential? {
        guard let object = json(at: url) else { return nil }
        for key in preferredKeys {
            if let token = findString(named: key, in: object) {
                return AgentCredential(
                    token: token,
                    accountID: findString(named: "account_id", in: object) ?? accountID(fromJWT: token),
                    email: findString(named: "email", in: object) ?? string(named: "email", inJWT: token)
                )
            }
        }
        return nil
    }

    private func json(at url: URL) -> [String: Any]? {
        guard let data = try? Data(contentsOf: url), data.count <= 2_000_000 else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    private func findString(named name: String, in object: Any) -> String? {
        if let dictionary = object as? [String: Any] {
            if let value = dictionary[name] as? String, !value.isEmpty { return value }
            for value in dictionary.values {
                if let found = findString(named: name, in: value) { return found }
            }
        } else if let array = object as? [Any] {
            for value in array {
                if let found = findString(named: name, in: value) { return found }
            }
        }
        return nil
    }

    private func accountID(fromJWT token: String) -> String? {
        guard let payload = jwtPayload(token) else { return nil }
        if let auth = payload["https://api.openai.com/auth"] as? [String: Any],
           let account = auth["chatgpt_account_id"] as? String { return account }
        return payload["chatgpt_account_id"] as? String ?? payload["account_id"] as? String
    }

    private func string(named name: String, inJWT token: String) -> String? {
        guard let payload = jwtPayload(token) else { return nil }
        return findString(named: name, in: payload)
    }

    private func jwtPayload(_ token: String) -> [String: Any]? {
        let parts = token.split(separator: ".")
        guard parts.count == 3 else { return nil }
        var encoded = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        encoded += String(repeating: "=", count: (4 - encoded.count % 4) % 4)
        guard let data = Data(base64Encoded: encoded),
              let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return payload
    }
}
