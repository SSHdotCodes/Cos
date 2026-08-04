import AppKit
import SwiftUI

struct ProviderMark: View {
    let providerID: String
    var size: CGFloat = 15

    var body: some View {
        Group {
            if let image = Self.image(named: logoName) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
            } else {
                Image(systemName: "sparkles")
                    .resizable()
                    .scaledToFit()
            }
        }
        .frame(width: size, height: size)
        .foregroundStyle(.secondary)
        .accessibilityLabel(accessibilityName)
        .help(accessibilityName)
    }

    private var logoName: String {
        switch providerID {
        case "chatgpt", "openai-api": "openai"
        case "anthropic": "claude"
        case "xai": "grok"
        case "opencode-go": "opencode"
        case "qwen": "qwen"
        case "pi": "pi"
        default: ""
        }
    }

    private var accessibilityName: String {
        switch providerID {
        case "chatgpt", "openai-api": "OpenAI"
        case "anthropic": "Claude"
        case "xai": "Grok"
        case "opencode-go": "OpenCode"
        case "qwen": "Qwen"
        case "pi": "Pi"
        default: "Custom provider"
        }
    }

    private static func image(named name: String) -> NSImage? {
        guard !name.isEmpty,
              let url = Bundle.module.url(forResource: name, withExtension: "svg", subdirectory: "ProviderLogos"),
              let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        return image
    }
}
