import CosCore
import SwiftUI

struct ChatView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.55)
            if let thread = model.selectedThread {
                transcript(thread)
            } else {
                ContentUnavailableView("No task selected", systemImage: "bubble.left.and.bubble.right")
            }
            ComposerView()
                .padding(.horizontal, 18)
                .padding(.bottom, 14)
        }
        .background {
            if model.preferences.appearance == .trueDark {
                Color.black
            } else {
                LinearGradient(
                    colors: [Color(nsColor: .textBackgroundColor).opacity(0.72), Color(nsColor: .windowBackgroundColor)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(model.selectedThread?.title ?? "Cos")
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                if let path = model.selectedThread?.workspacePath {
                    Text(path.replacingOccurrences(of: FileManager.default.homeDirectoryForCurrentUser.path, with: "~"))
                        .font(.system(size: 10.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            if let goal = model.selectedThread?.goal {
                Label(goal.status == .active ? "Goal active" : goal.status.rawValue.capitalized, systemImage: "scope")
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(CosTheme.blue)
                    .help(goal.objective)
            }

            Button { model.chooseWorkspace() } label: {
                Image(systemName: "folder")
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.plain)
            .help("Choose workspace")
        }
        .padding(.horizontal, 16)
        .frame(height: 52)
    }

    @ViewBuilder
    private func transcript(_ thread: CosThread) -> some View {
        if thread.messages.isEmpty {
            EmptyTaskView { prompt in model.send(prompt) }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 14) {
                        ForEach(thread.messages) { message in
                            MessageView(message: message)
                                .id(message.id)
                        }
                    }
                    .frame(maxWidth: 760)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 28)
                    .padding(.vertical, 18)
                }
                .onChange(of: thread.messages.count) { _, _ in
                    if let id = thread.messages.last?.id {
                        withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(id, anchor: .bottom) }
                    }
                }
                .onChange(of: thread.messages.last?.content) { _, _ in
                    if model.isRunning, let id = thread.messages.last?.id { proxy.scrollTo(id, anchor: .bottom) }
                }
            }
        }
    }
}

private struct EmptyTaskView: View {
    let send: (String) -> Void
    private let suggestions = [
        ("Inspect this project", "Find the architecture, risks, and the best first improvement."),
        ("Build a feature", "Implement the next high-impact feature and verify it."),
        ("Fix a bug", "Reproduce the current failure, find its cause, and fix it."),
    ]

    var body: some View {
        VStack(spacing: 22) {
            Spacer()
            CosMark(compact: false).scaleEffect(1.18)
            Text("What should Cos work on?")
                .font(.system(size: 24, weight: .semibold, design: .rounded))
            HStack(spacing: 9) {
                ForEach(suggestions, id: \.0) { item in
                    Button { send(item.1) } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(item.0).font(.system(size: 12.5, weight: .semibold))
                            Text(item.1).font(.system(size: 10.5)).foregroundStyle(.secondary).lineLimit(2)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .background(.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: 650)
            Spacer()
        }
        .padding(30)
    }
}

private struct MessageView: View {
    let message: ChatMessage
    @State private var workExpanded = false

    var body: some View {
        HStack(alignment: .top, spacing: 11) {
            if message.role == .assistant {
                CosMark(compact: true)
            } else {
                Spacer(minLength: 70)
            }
            VStack(alignment: message.role == .user ? .trailing : .leading, spacing: 7) {
                if message.role == .assistant {
                    HStack(spacing: 7) {
                        Text("Cos").font(.system(size: 11.5, weight: .semibold))
                        if message.isStreaming {
                            ProgressView().controlSize(.mini)
                        }
                    }
                    if let items = message.workItems, !items.isEmpty {
                        WorkTraceView(items: items, isExpanded: $workExpanded, isRunning: message.isStreaming)
                    }
                    if message.content.isEmpty && message.isStreaming && (message.workItems?.isEmpty ?? true) {
                        ThinkingIndicator()
                    } else if !message.content.isEmpty {
                        Text(.init(message.content))
                            .font(.system(size: 13.2))
                            .textSelection(.enabled)
                            .lineSpacing(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else {
                    Text(message.content)
                        .font(.system(size: 13))
                        .textSelection(.enabled)
                        .padding(.horizontal, 13)
                        .padding(.vertical, 9)
                        .background(.primary.opacity(0.075), in: RoundedRectangle(cornerRadius: 13, style: .continuous))
                }
            }
            if message.role == .assistant { Spacer(minLength: 30) }
        }
        .frame(maxWidth: .infinity)
        .onAppear { workExpanded = message.isStreaming && !(message.workItems?.isEmpty ?? true) }
        .onChange(of: message.isStreaming) { _, running in
            withAnimation(.easeOut(duration: 0.16)) { workExpanded = running }
        }
        .onChange(of: message.workItems?.count) { _, _ in
            if message.isStreaming { workExpanded = true }
        }
    }
}

private struct WorkTraceView: View {
    let items: [WorkTraceItem]
    @Binding var isExpanded: Bool
    let isRunning: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.easeOut(duration: 0.16)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 8.5, weight: .bold))
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    Text(isRunning ? "Cos is working" : "Work")
                        .font(.system(size: 11.5, weight: .semibold))
                    Text("\(items.count)")
                        .font(.system(size: 9.5, weight: .semibold).monospacedDigit())
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(.primary.opacity(0.06), in: Capsule())
                    Spacer()
                    if isRunning { ProgressView().controlSize(.mini) }
                }
                .contentShape(Rectangle())
                .foregroundStyle(.secondary)
                .frame(height: 28)
            }
            .buttonStyle(.plain)

            if isExpanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                        HStack(alignment: .top, spacing: 9) {
                            VStack(spacing: 0) {
                                ZStack {
                                    Circle().fill(color(for: item.kind).opacity(0.14))
                                    Image(systemName: icon(for: item.kind))
                                        .font(.system(size: 8.5, weight: .semibold))
                                        .foregroundStyle(color(for: item.kind))
                                }
                                .frame(width: 20, height: 20)
                                if index < items.count - 1 {
                                    Rectangle().fill(.primary.opacity(0.09)).frame(width: 1, height: 16)
                                }
                            }
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.title)
                                    .font(.system(size: 11, weight: .medium))
                                if !item.detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                    Text(item.detail.trimmingCharacters(in: .whitespacesAndNewlines))
                                        .font(.system(size: 10.5))
                                        .foregroundStyle(.secondary)
                                        .lineLimit(item.kind == .reasoning ? 5 : 2)
                                        .textSelection(.enabled)
                                }
                            }
                            .padding(.bottom, index < items.count - 1 ? 7 : 2)
                            Spacer(minLength: 0)
                        }
                    }
                }
                .padding(.horizontal, 9)
                .padding(.vertical, 8)
                .background(.primary.opacity(0.032), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func icon(for kind: WorkTraceKind) -> String {
        switch kind {
        case .status: "waveform.path.ecg"
        case .reasoning: "text.bubble"
        case .tool: "wrench.and.screwdriver"
        }
    }

    private func color(for kind: WorkTraceKind) -> Color {
        switch kind {
        case .status: CosTheme.blue
        case .reasoning: .secondary
        case .tool: CosTheme.orange
        }
    }
}

private struct ThinkingIndicator: View {
    @State private var phase = false
    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<3, id: \.self) { index in
                Circle()
                    .fill(.secondary)
                    .frame(width: 4, height: 4)
                    .opacity(phase ? (index == 1 ? 1 : 0.32) : (index == 1 ? 0.32 : 0.7))
                    .animation(.easeInOut(duration: 0.7).repeatForever().delay(Double(index) * 0.12), value: phase)
            }
        }
        .padding(.vertical, 8)
        .onAppear { phase = true }
    }
}
