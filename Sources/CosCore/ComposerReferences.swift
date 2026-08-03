import Foundation

public enum ComposerReferenceKind: String, Hashable, Sendable {
    case command
    case skill
    case plugin

    public var title: String {
        switch self {
        case .command: "Command"
        case .skill: "Skill"
        case .plugin: "Plugin"
        }
    }
}

public struct ComposerReferenceQuery: Hashable, Sendable {
    public var trigger: Character
    public var term: String
    public var rangeLocation: Int
    public var rangeLength: Int

    public init(trigger: Character, term: String, rangeLocation: Int, rangeLength: Int) {
        self.trigger = trigger
        self.term = term
        self.rangeLocation = rangeLocation
        self.rangeLength = rangeLength
    }

    public var range: NSRange { NSRange(location: rangeLocation, length: rangeLength) }
    public var signature: String { "\(trigger):\(rangeLocation):\(rangeLength):\(term.lowercased())" }
}

public struct ComposerReferenceSuggestion: Identifiable, Hashable, Sendable {
    public var id: String
    public var kind: ComposerReferenceKind
    public var title: String
    public var detail: String
    public var insertion: String

    public init(id: String, kind: ComposerReferenceKind, title: String, detail: String, insertion: String) {
        self.id = id
        self.kind = kind
        self.title = title
        self.detail = detail
        self.insertion = insertion
    }
}

public enum ComposerReferenceResolver {
    private static let commands: [ComposerReferenceSuggestion] = [
        .init(id: "command:goal", kind: .command, title: "/goal", detail: "Set a goal or show the active goal", insertion: "/goal "),
        .init(id: "command:goal-budget", kind: .command, title: "/goal --budget", detail: "Set a goal with a token budget", insertion: "/goal --budget "),
        .init(id: "command:goal-status", kind: .command, title: "/goal status", detail: "Show goal progress and token usage", insertion: "/goal status"),
        .init(id: "command:goal-complete", kind: .command, title: "/goal complete", detail: "Mark the active goal complete", insertion: "/goal complete"),
        .init(id: "command:goal-clear", kind: .command, title: "/goal clear", detail: "Remove the active goal", insertion: "/goal clear"),
    ]

    public static func query(in text: String, selectionUTF16Offset: Int) -> ComposerReferenceQuery? {
        let source = text as NSString
        let cursor = min(max(selectionUTF16Offset, 0), source.length)
        var start = cursor

        while start > 0 {
            let scalar = source.character(at: start - 1)
            guard let unicode = UnicodeScalar(scalar), !CharacterSet.whitespacesAndNewlines.contains(unicode) else { break }
            start -= 1
        }

        guard start < cursor else { return nil }
        let tokenRange = NSRange(location: start, length: cursor - start)
        let token = source.substring(with: tokenRange)
        guard let trigger = token.first, trigger == "/" || trigger == "@" else { return nil }
        let term = String(token.dropFirst())
        guard term.rangeOfCharacter(from: CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._- ")).inverted) == nil else {
            return nil
        }
        return .init(trigger: trigger, term: term, rangeLocation: start, rangeLength: cursor - start)
    }

    public static func suggestions(
        for query: ComposerReferenceQuery,
        plugins: [InstalledPlugin],
        limit: Int = 8
    ) -> [ComposerReferenceSuggestion] {
        let enabledPlugins = plugins.filter(\.isEnabled)
        let candidates: [ComposerReferenceSuggestion]

        if query.trigger == "/" {
            let skills = enabledPlugins.flatMap { plugin in
                plugin.manifest.skills.map { skill in
                    ComposerReferenceSuggestion(
                        id: "skill:\(plugin.id):\(skill)",
                        kind: .skill,
                        title: "/\(skill)",
                        detail: "\(plugin.manifest.name) · \(plugin.manifest.description)",
                        insertion: "/\(skill) "
                    )
                }
            }
            candidates = commands + skills
        } else {
            candidates = enabledPlugins.map { plugin in
                let handle = pluginHandle(for: plugin.manifest.name)
                return ComposerReferenceSuggestion(
                    id: "plugin:\(plugin.id)",
                    kind: .plugin,
                    title: "@\(handle)",
                    detail: plugin.manifest.description,
                    insertion: "@\(handle) "
                )
            }
        }

        let normalizedTerm = query.term.lowercased()
        return candidates
            .filter { suggestion in
                normalizedTerm.isEmpty || searchableText(for: suggestion).contains(normalizedTerm)
            }
            .sorted { left, right in
                let leftPrefix = left.title.dropFirst().lowercased().hasPrefix(normalizedTerm)
                let rightPrefix = right.title.dropFirst().lowercased().hasPrefix(normalizedTerm)
                if leftPrefix != rightPrefix { return leftPrefix }
                if left.kind != right.kind { return kindRank(left.kind) < kindRank(right.kind) }
                return left.title.localizedCaseInsensitiveCompare(right.title) == .orderedAscending
            }
            .prefix(max(0, limit))
            .map { $0 }
    }

    public static func replacingQuery(
        in text: String,
        query: ComposerReferenceQuery,
        with insertion: String
    ) -> (text: String, selectionUTF16Offset: Int) {
        let source = text as NSString
        guard query.range.location != NSNotFound, NSMaxRange(query.range) <= source.length else {
            return (text, min(source.length, max(0, query.rangeLocation)))
        }
        let updated = source.replacingCharacters(in: query.range, with: insertion)
        return (updated, query.rangeLocation + (insertion as NSString).length)
    }

    public static func referenceContext(in prompt: String, plugins: [InstalledPlugin]) -> String {
        let enabledPlugins = plugins.filter(\.isEnabled)
        let words = prompt.split(whereSeparator: \Character.isWhitespace).map {
            String($0).trimmingCharacters(in: CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "/@._-")).inverted)
        }
        var lines: [String] = []
        var seen = Set<String>()

        for word in words where word.count > 1 {
            if word.hasPrefix("/") {
                let requested = String(word.dropFirst())
                guard requested.lowercased() != "goal" else { continue }
                for plugin in enabledPlugins {
                    guard let skill = plugin.manifest.skills.first(where: { $0.caseInsensitiveCompare(requested) == .orderedSame }) else { continue }
                    let key = "skill:\(plugin.id):\(skill)"
                    guard seen.insert(key).inserted else { continue }
                    lines.append("- Apply skill /\(skill) from \(plugin.manifest.name) (\(plugin.id)).")
                }
            } else if word.hasPrefix("@") {
                let requested = String(word.dropFirst())
                for plugin in enabledPlugins where pluginMatches(plugin, requested: requested) {
                    let key = "plugin:\(plugin.id)"
                    guard seen.insert(key).inserted else { continue }
                    lines.append("- Use plugin @\(pluginHandle(for: plugin.manifest.name)) (\(plugin.id)) and its declared capabilities.")
                }
            }
        }

        guard !lines.isEmpty else { return "" }
        return "The user explicitly referenced these Cos extensions:\n" + lines.joined(separator: "\n")
    }

    public static func pluginHandle(for name: String) -> String {
        let lowered = name.lowercased()
        let pieces = lowered.components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }
        return pieces.joined(separator: "-")
    }

    private static func searchableText(for suggestion: ComposerReferenceSuggestion) -> String {
        "\(suggestion.title) \(suggestion.detail)".lowercased()
    }

    private static func kindRank(_ kind: ComposerReferenceKind) -> Int {
        switch kind {
        case .command: 0
        case .skill: 1
        case .plugin: 2
        }
    }

    private static func pluginMatches(_ plugin: InstalledPlugin, requested: String) -> Bool {
        let normalized = requested.lowercased()
        return pluginHandle(for: plugin.manifest.name) == normalized
            || plugin.id.lowercased() == normalized
            || plugin.id.lowercased().hasSuffix(".\(normalized)")
    }
}
