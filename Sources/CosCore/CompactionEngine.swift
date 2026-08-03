import Foundation

public struct CompactionResult: Equatable, Sendable {
    public var promptContext: String
    public var compactedSummary: String?
    public var estimatedTokens: Int
    public var didCompact: Bool
}

public struct CompactionEngine: Sendable {
    public init() {}

    public func estimateTokens(_ text: String) -> Int {
        max(1, Int(ceil(Double(text.utf8.count) / 3.8)))
    }

    public func prepare(
        messages: [ChatMessage],
        previousSummary: String?,
        contextWindow: Int,
        thresholdPercent: Double,
        keepRecentTokens: Int
    ) -> CompactionResult {
        let rendered = render(messages)
        let total = estimateTokens(rendered)
        let threshold = Int(Double(contextWindow) * thresholdPercent / 100)
        guard total > threshold, messages.count > 4 else {
            let prefix = previousSummary.map { "Earlier context (compacted):\n\($0)\n\n" } ?? ""
            return .init(promptContext: prefix + rendered, compactedSummary: previousSummary, estimatedTokens: total, didCompact: false)
        }

        var recent: [ChatMessage] = []
        var used = 0
        for message in messages.reversed() {
            let cost = estimateTokens(message.content) + 8
            if !recent.isEmpty && used + cost > keepRecentTokens { break }
            recent.append(message)
            used += cost
        }
        recent.reverse()
        let oldCount = messages.count - recent.count
        let older = Array(messages.prefix(max(oldCount, 0)))
        let summary = summarize(older, previous: previousSummary)
        let prompt = "Earlier context (compacted checkpoint):\n\(summary)\n\nRecent verbatim context:\n\(render(recent))"
        return .init(promptContext: prompt, compactedSummary: summary, estimatedTokens: estimateTokens(prompt), didCompact: true)
    }

    private func render(_ messages: [ChatMessage]) -> String {
        messages.map { "[\($0.role.rawValue.uppercased())]\n\($0.content)" }.joined(separator: "\n\n")
    }

    private func summarize(_ messages: [ChatMessage], previous: String?) -> String {
        var lines: [String] = []
        if let previous, !previous.isEmpty {
            lines.append(previous)
        }
        for message in messages {
            let clean = message.content.replacingOccurrences(of: "\n", with: " ")
            let clipped = clean.count > 420 ? String(clean.prefix(420)) + "…" : clean
            lines.append("• \(message.role.rawValue): \(clipped)")
        }
        let joined = lines.joined(separator: "\n")
        return joined.count > 16_000 ? String(joined.suffix(16_000)) : joined
    }
}
