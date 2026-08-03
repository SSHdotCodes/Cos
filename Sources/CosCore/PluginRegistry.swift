import Foundation

public struct CosPluginManifest: Codable, Identifiable, Hashable, Sendable {
    public struct Capability: Codable, Hashable, Sendable {
        public var id: String
        public var description: String
        public var risk: String

        public init(id: String, description: String, risk: String) {
            self.id = id
            self.description = description
            self.risk = risk
        }
    }

    public var schemaVersion: Int
    public var id: String
    public var name: String
    public var version: String
    public var author: String
    public var description: String
    public var capabilities: [Capability]
    public var skills: [String]
    public var homepage: URL?
    public var builtIn: Bool?

    public init(
        schemaVersion: Int,
        id: String,
        name: String,
        version: String,
        author: String,
        description: String,
        capabilities: [Capability],
        skills: [String],
        homepage: URL?,
        builtIn: Bool?
    ) {
        self.schemaVersion = schemaVersion
        self.id = id
        self.name = name
        self.version = version
        self.author = author
        self.description = description
        self.capabilities = capabilities
        self.skills = skills
        self.homepage = homepage
        self.builtIn = builtIn
    }
}

public struct InstalledPlugin: Identifiable, Hashable, Sendable {
    public var manifest: CosPluginManifest
    public var location: URL
    public var isTrusted: Bool
    public var isEnabled: Bool
    public var id: String { manifest.id }
}

public struct CosMarketplaceListing: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var name: String
    public var version: String
    public var author: String
    public var description: String
    public var type: String
    public var featured: Bool?
    public var builtIn: Bool?
    public var tags: [String]?
    public var downloads: String?
    public var manifest: CosPluginManifest?
}

public struct CosMarketplaceResponse: Codable, Sendable {
    public var items: [CosMarketplaceListing]
    public var total: Int
}

public actor PluginRegistry {
    private let decoder = JSONDecoder()

    public init() {}

    public func discover(builtInURL: URL?, workspace: URL?) -> [InstalledPlugin] {
        var roots: [(URL, Bool)] = []
        if let builtInURL { roots.append((builtInURL, true)) }
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Cos/Plugins", isDirectory: true)
        roots.append((appSupport, false))
        if let workspace {
            roots.append((workspace.appendingPathComponent(".cos/plugins", isDirectory: true), false))
        }

        var found: [String: InstalledPlugin] = [:]
        for (root, trusted) in roots {
            guard let enumerator = FileManager.default.enumerator(
                at: root,
                includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles]
            ) else { continue }
            for case let url as URL in enumerator where url.lastPathComponent == "cos.plugin.json" {
                guard let data = try? Data(contentsOf: url),
                      let manifest = try? decoder.decode(CosPluginManifest.self, from: data) else { continue }
                found[manifest.id] = .init(
                    manifest: manifest,
                    location: url.deletingLastPathComponent(),
                    isTrusted: trusted || root == appSupport,
                    isEnabled: true
                )
            }
        }
        return found.values.sorted { lhs, rhs in
            if (lhs.manifest.builtIn == true) != (rhs.manifest.builtIn == true) {
                return lhs.manifest.builtIn == true
            }
            return lhs.manifest.name.localizedCaseInsensitiveCompare(rhs.manifest.name) == .orderedAscending
        }
    }
}

public enum SettingsMutation: Equatable, Sendable {
    case fastMode(Bool)
    case fullAccess(Bool)
    case autoCompact(Bool)
    case showTokenUsage(Bool)
    case effort(ReasoningEffort)
}

public enum CosManagementAction: Equatable, Sendable {
    case createSkill(id: String, name: String, description: String, instructions: String, pluginID: String?)
    case removeSkill(id: String, pluginID: String?)
    case createPlugin(id: String, name: String, description: String, instructions: String?)
    case removePlugin(id: String)
    case setPluginEnabled(id: String, enabled: Bool)
}

public enum CosSettingsPlugin {
    public static let systemPrompt = """
    Cos includes a trusted settings tool. When the user explicitly asks to change a Cos setting, include exactly one marker after your brief confirmation:
    <cos-settings>{\"key\":\"fastMode|fullAccess|autoCompact|showTokenUsage|effort\",\"value\":true}</cos-settings>
    For effort, value must be one of minimal, low, medium, high, extraHigh, max. Never emit this marker without an explicit user request.

    Cos also includes a guarded self-management tool for Cos-owned skills and plugins. When the user explicitly asks, include exactly one of these markers after your brief confirmation:
    <cos-manage>{\"action\":\"createSkill\",\"id\":\"slug\",\"name\":\"Name\",\"description\":\"Purpose\",\"instructions\":\"Complete skill instructions\",\"pluginID\":\"optional.plugin.id\"}</cos-manage>
    <cos-manage>{\"action\":\"removeSkill\",\"id\":\"slug\",\"pluginID\":\"optional.plugin.id\"}</cos-manage>
    <cos-manage>{\"action\":\"createPlugin\",\"id\":\"plugin.id\",\"name\":\"Name\",\"description\":\"Purpose\",\"instructions\":\"Optional main skill instructions\"}</cos-manage>
    <cos-manage>{\"action\":\"removePlugin|enablePlugin|disablePlugin\",\"id\":\"plugin.id\"}</cos-manage>
    These actions are restricted to Cos-managed directories. Never emit them without an explicit user request, never target the built-in Cos plugin, and use lowercase ASCII slugs containing only letters, numbers, dots, underscores, or hyphens.
    """

    public static func extract(from text: String) -> (visibleText: String, mutation: SettingsMutation?, managementAction: CosManagementAction?) {
        let settings = removingMarker(named: "cos-settings", from: text)
        let management = removingMarker(named: "cos-manage", from: settings.visible)
        return (
            management.visible.trimmingCharacters(in: .whitespacesAndNewlines),
            settings.payload.flatMap(parseSettings),
            management.payload.flatMap(parseManagement)
        )
    }

    private static func removingMarker(named name: String, from text: String) -> (visible: String, payload: String?) {
        let opening = "<\(name)>"
        let closing = "</\(name)>"
        guard let start = text.range(of: opening),
              let end = text.range(of: closing, range: start.upperBound..<text.endIndex) else {
            return (text, nil)
        }
        let payload = String(text[start.upperBound..<end.lowerBound])
        let visible = String(text[..<start.lowerBound] + text[end.upperBound...])
        return (visible, payload)
    }

    private static func object(from json: String) -> [String: Any]? {
        guard json.utf8.count <= 70_000, let data = json.data(using: .utf8) else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    private static func parseSettings(_ json: String) -> SettingsMutation? {
        guard let object = object(from: json), let key = object["key"] as? String else { return nil }
        switch key {
        case "fastMode": return (object["value"] as? Bool).map(SettingsMutation.fastMode)
        case "fullAccess": return (object["value"] as? Bool).map(SettingsMutation.fullAccess)
        case "autoCompact": return (object["value"] as? Bool).map(SettingsMutation.autoCompact)
        case "showTokenUsage": return (object["value"] as? Bool).map(SettingsMutation.showTokenUsage)
        case "effort":
            guard let raw = object["value"] as? String else { return nil }
            return ReasoningEffort(rawValue: raw).map(SettingsMutation.effort)
        default: return nil
        }
    }

    private static func parseManagement(_ json: String) -> CosManagementAction? {
        guard let object = object(from: json),
              let action = object["action"] as? String,
              let id = object["id"] as? String else { return nil }
        let pluginID = object["pluginID"] as? String
        switch action {
        case "createSkill":
            guard let name = object["name"] as? String,
                  let description = object["description"] as? String,
                  let instructions = object["instructions"] as? String else { return nil }
            return .createSkill(id: id, name: name, description: description, instructions: instructions, pluginID: pluginID)
        case "removeSkill": return .removeSkill(id: id, pluginID: pluginID)
        case "createPlugin":
            guard let name = object["name"] as? String,
                  let description = object["description"] as? String else { return nil }
            return .createPlugin(id: id, name: name, description: description, instructions: object["instructions"] as? String)
        case "removePlugin": return .removePlugin(id: id)
        case "enablePlugin": return .setPluginEnabled(id: id, enabled: true)
        case "disablePlugin": return .setPluginEnabled(id: id, enabled: false)
        default: return nil
        }
    }
}
