import CosCore
import SwiftUI

enum CosTheme {
    static let blue = Color(red: 0.23, green: 0.57, blue: 1.0)
    static let violet = Color(red: 0.54, green: 0.36, blue: 0.98)
    static let orange = Color(red: 1.0, green: 0.48, blue: 0.18)
    static let sidebarWidth: CGFloat = 236
    static let composerRadius: CGFloat = 18
}

struct CosMark: View {
    var compact = false

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: compact ? 7 : 9, style: .continuous)
                .fill(.black)
                .overlay {
                    RoundedRectangle(cornerRadius: compact ? 7 : 9, style: .continuous)
                        .strokeBorder(.white.opacity(0.14), lineWidth: 0.7)
                }
            Text("cos θ")
                .font(.system(size: compact ? 9.5 : 12, weight: .medium, design: .serif))
                .foregroundStyle(.white)
                .minimumScaleFactor(0.7)
        }
        .frame(width: compact ? 38 : 46, height: compact ? 28 : 32)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("cosine theta")
    }
}

struct GlassCardModifier: ViewModifier {
    var cornerRadius: CGFloat
    var trueDark: Bool

    func body(content: Content) -> some View {
        content
            .background {
                if trueDark {
                    RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).fill(.black)
                } else {
                    RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).fill(.regularMaterial)
                }
            }
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(.white.opacity(0.08), lineWidth: 0.7)
            }
            .shadow(color: .black.opacity(0.09), radius: 18, y: 7)
    }
}

extension View {
    func glassCard(cornerRadius: CGFloat = 16, trueDark: Bool = false) -> some View {
        modifier(GlassCardModifier(cornerRadius: cornerRadius, trueDark: trueDark))
    }
}

extension AppearanceMode {
    var preferredColorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark, .trueDark: .dark
        }
    }
}
