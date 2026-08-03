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

        var routedRequest = request
        let credential: AgentCredential?
        if request.provider.bridge == .pi {
            guard let chatGPT = DefaultCatalog.providers.first(where: { $0.id == "chatgpt" }),
                  let model = DefaultCatalog.models.first(where: { $0.providerID == "chatgpt" }) else {
                throw AgentRuntimeError.unsupportedProvider(request.provider.name)
            }
            routedRequest.provider = chatGPT
            routedRequest.model = model
            credential = try credentials.subscriptionCredential(for: chatGPT)
        } else if let account = request.provider.keychainAccount,
                  let token = try secureStore.get(account: account) {
            credential = AgentCredential(token: token)
        } else if request.provider.authMode == .subscription {
            credential = try credentials.subscriptionCredential(for: request.provider)
        } else {
            credential = nil
        }

        guard let credential else { throw AgentRuntimeError.missingAPIKey(request.provider.name) }
        return harness.stream(request: routedRequest, credential: credential)
    }

    public func sessionInfo(for provider: ProviderProfile) -> ProviderSessionInfo? {
        guard provider.authMode == .subscription,
              let credential = try? credentials.subscriptionCredential(for: provider) else { return nil }
        return ProviderSessionInfo(email: credential.email, accountID: credential.accountID)
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
