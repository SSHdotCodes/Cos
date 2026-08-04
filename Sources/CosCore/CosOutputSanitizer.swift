import Foundation

public enum CosOutputSanitizer {
    public static func reasoning(_ raw: String) -> String {
        var text = raw.replacingOccurrences(of: "\r\n", with: "\n")
        text = text.replacingOccurrences(of: "```[^\n]*", with: "", options: .regularExpression)
        text = text.replacingOccurrences(of: "\\*{4,}", with: "\n", options: .regularExpression)
        text = text.replacingOccurrences(of: "(?m)^\\s{0,3}#{1,6}\\s+", with: "", options: .regularExpression)
        text = text.replacingOccurrences(of: "(?m)^\\s*[-*+]\\s+", with: "• ", options: .regularExpression)
        text = text.replacingOccurrences(of: "\\[([^\\]]+)\\]\\([^\\)]+\\)", with: "$1", options: .regularExpression)
        text = text.replacingOccurrences(of: "**", with: "")
        text = text.replacingOccurrences(of: "__", with: "")
        text = text.replacingOccurrences(of: "`", with: "")
        text = text.replacingOccurrences(of: "[ \\t]+\n", with: "\n", options: .regularExpression)
        text = text.replacingOccurrences(of: "\n{3,}", with: "\n\n", options: .regularExpression)
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func assistantText(_ raw: String) -> String {
        guard containsToolProtocol(raw) else { return raw }
        var result = ""
        var cursor = raw.startIndex

        while cursor < raw.endIndex,
              let marker = raw.range(of: "<cos-tool>", range: cursor..<raw.endIndex) {
            let prefix = String(raw[cursor..<marker.lowerBound])
            if !isProtocolChatter(prefix) { result += prefix }

            guard let envelope = CosToolEnvelopeParser.first(in: raw, marker: marker) else {
                cursor = marker.upperBound
                continue
            }
            cursor = envelope.payloadRange.upperBound
            let remainder = raw[cursor...]
            let trimmed = remainder.drop(while: { $0.isWhitespace })
            if trimmed.hasPrefix("</cos-tool>") {
                cursor = raw.index(trimmed.startIndex, offsetBy: "</cos-tool>".count)
            }
        }

        if cursor < raw.endIndex {
            let suffix = String(raw[cursor...])
            if !isProtocolChatter(suffix) { result += suffix }
        }
        result = result
            .replacingOccurrences(of: "</cos-tool>", with: "")
            .replacingOccurrences(of: "<cos-tool>", with: "")
        let lines = result.split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
            .filter { !isProtocolChatter($0) }
        return lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func containsToolProtocol(_ text: String) -> Bool {
        text.range(of: "<cos-tool", options: .caseInsensitive) != nil ||
            text.range(of: "to=cos-tool", options: .caseInsensitive) != nil
    }

    private static func isProtocolChatter(_ text: String) -> Bool {
        let value = text.lowercased()
        return value.contains("to=cos-tool") ||
            value.contains("exact marker") ||
            value.contains("must emit") && value.contains("cos-tool")
    }
}

struct CosToolEnvelope: Equatable {
    let payload: String
    let visiblePrefix: String
    let payloadRange: Range<String.Index>

    static func == (lhs: CosToolEnvelope, rhs: CosToolEnvelope) -> Bool {
        lhs.payload == rhs.payload && lhs.visiblePrefix == rhs.visiblePrefix
    }
}

enum CosToolEnvelopeParser {
    static func first(in text: String) -> CosToolEnvelope? {
        guard let marker = text.range(of: "<cos-tool>") else { return nil }
        return first(in: text, marker: marker)
    }

    static func first(in text: String, marker: Range<String.Index>) -> CosToolEnvelope? {
        var jsonStart = marker.upperBound
        while jsonStart < text.endIndex, text[jsonStart].isWhitespace {
            jsonStart = text.index(after: jsonStart)
        }
        guard jsonStart < text.endIndex, text[jsonStart] == "{" else { return nil }

        var index = jsonStart
        var depth = 0
        var insideString = false
        var escaped = false
        while index < text.endIndex {
            let character = text[index]
            if insideString {
                if escaped {
                    escaped = false
                } else if character == "\\" {
                    escaped = true
                } else if character == "\"" {
                    insideString = false
                }
            } else if character == "\"" {
                insideString = true
            } else if character == "{" {
                depth += 1
            } else if character == "}" {
                depth -= 1
                if depth == 0 {
                    let end = text.index(after: index)
                    let range = jsonStart..<end
                    return .init(
                        payload: String(text[range]),
                        visiblePrefix: String(text[..<marker.lowerBound]),
                        payloadRange: range
                    )
                }
            }
            index = text.index(after: index)
        }
        return nil
    }
}
