import AppKit
import CosCore
import SwiftUI

struct ComposerView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var text = ""
    @State private var modelPopover = false
    @State private var editorFocused = false
    @State private var selectionUTF16Offset = 0
    @State private var selectedSuggestionIndex = 0
    @State private var dismissedSuggestionSignature: String?

    var body: some View {
        VStack(spacing: 0) {
            if let request = model.pendingDirectoryTrust,
               request.threadID == model.selectedThreadID {
                HStack(spacing: 9) {
                    Image(systemName: "folder.badge.questionmark")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(CosTheme.orange)
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Trust \(URL(fileURLWithPath: request.workspacePath).lastPathComponent)?")
                            .font(.system(size: 12, weight: .semibold))
                        Text("Allow Codex to work in this directory from now on.")
                            .font(.system(size: 10.5))
                            .foregroundStyle(.secondary)
                    }
                    .lineLimit(1)
                    .help(request.workspacePath)
                    Spacer(minLength: 8)
                    Button("Not now") { model.declinePendingWorkspaceTrust() }
                        .buttonStyle(.plain)
                        .font(.system(size: 11.5, weight: .medium))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 9)
                        .frame(height: 27)
                    Button("Trust & continue") { model.trustPendingWorkspaceAndContinue() }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .tint(CosTheme.blue)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                Divider().opacity(0.45)
            }

            ZStack(alignment: .topLeading) {
                if text.isEmpty {
                    Text(composerPlaceholder)
                        .font(.system(size: 13))
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 14)
                        .padding(.top, 9)
                        .allowsHitTesting(false)
                }
                AgentTextEditor(
                    text: $text,
                    isFocused: $editorFocused,
                    selectionUTF16Offset: $selectionUTF16Offset,
                    suggestionsVisible: !referenceSuggestions.isEmpty,
                    onMoveSuggestion: moveSuggestion,
                    onAcceptSuggestion: acceptSelectedSuggestion,
                    onDismissSuggestions: dismissSuggestions,
                    onSubmit: submit
                )
                    .padding(.horizontal, 9)
                    .frame(height: editorHeight)
            }

            HStack(spacing: 7) {
                Menu {
                    Button("Choose workspace…", systemImage: "folder") { model.chooseWorkspace() }
                    Button("New task", systemImage: "plus.bubble") { model.newThread() }
                    Menu("Ask a subagent", systemImage: "person.2") {
                        if model.subagentRoutes.isEmpty {
                            Text("Connect a model provider in Settings")
                        } else {
                            ForEach(model.subagentRoutes) { route in
                                Menu(route.model.name) {
                                    ForEach(route.model.effortOptions) { effort in
                                        Button(effort.title) {
                                            prepareSubagentPrompt(route: route, effort: effort)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Divider()
                    Button("Plugin library…", systemImage: "shippingbox") { model.isPluginLibraryPresented = true }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 28, height: 28)
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()

                Button { model.preferences.fullAccess.toggle(); model.persistPreferences() } label: {
                    Label(model.preferences.fullAccess ? "Full access" : "Workspace", systemImage: model.preferences.fullAccess ? "shield.lefthalf.filled" : "folder.badge.gearshape")
                        .font(.system(size: 11.5, weight: .medium))
                        .foregroundStyle(model.preferences.fullAccess ? CosTheme.orange : .secondary)
                        .padding(.horizontal, 8)
                        .frame(height: 28)
                        .background(.primary.opacity(0.05), in: Capsule())
                }
                .buttonStyle(.plain)
                .help("Toggle agent file access")

                Spacer(minLength: 4)

                Button { modelPopover.toggle() } label: {
                    HStack(spacing: 6) {
                        if model.selectedModel.supportsFastMode && model.preferences.fastMode {
                            Image(systemName: "bolt.fill")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(CosTheme.blue)
                        }
                        Text(model.selectedModel.name)
                            .lineLimit(1)
                            .layoutPriority(1)
                        ProviderMark(providerID: model.selectedModel.providerID, size: 13)
                        Spacer(minLength: 0)
                        Text(model.selectedThread?.effort.shortTitle ?? model.preferences.defaultEffort.shortTitle)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Image(systemName: "chevron.down")
                            .font(.system(size: 8, weight: .semibold))
                            .foregroundStyle(.secondary)
                    }
                    .font(.system(size: 11.5, weight: .medium))
                    .padding(.horizontal, 9)
                    .frame(height: 28)
                    .frame(width: 194)
                    .background(.primary.opacity(0.055), in: Capsule())
                }
                .buttonStyle(.plain)
                .popover(isPresented: $modelPopover, arrowEdge: .bottom) {
                    ModelPickerView(isPresented: $modelPopover)
                        .environmentObject(model)
                }

                Button { editorFocused = true } label: {
                    Image(systemName: "mic")
                        .font(.system(size: 12))
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .help("Use macOS Dictation")

                Button { primaryAction() } label: {
                    Image(systemName: showsStopAction ? "stop.fill" : "arrow.up")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundStyle(model.isRunning || !trimmedText.isEmpty ? Color(nsColor: .windowBackgroundColor) : .secondary)
                        .frame(width: 29, height: 29)
                        .background(model.isRunning || !trimmedText.isEmpty ? AnyShapeStyle(.primary) : AnyShapeStyle(.primary.opacity(0.11)), in: Circle())
                }
                .buttonStyle(.plain)
                .keyboardShortcut(.return, modifiers: .command)
                .help(showsStopAction ? "Stop active run" : model.isRunning ? "Steer active run" : "Send")
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 7)
        }
        .glassCard(cornerRadius: CosTheme.composerRadius, trueDark: model.preferences.appearance == .trueDark)
        .overlay(alignment: .topLeading) {
            if !referenceSuggestions.isEmpty {
                referenceSuggestionMenu
                    .padding(.horizontal, 10)
                    .offset(y: -referenceSuggestionMenuHeight - 8)
                    .transition(.opacity.combined(with: .scale(scale: 0.985, anchor: .bottomLeading)))
                    .zIndex(20)
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: model.preferences.fastMode)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.11), value: referenceQuery?.signature)
        .onChange(of: referenceQuery?.signature) { _, _ in
            selectedSuggestionIndex = 0
        }
        .frame(maxWidth: 820)
        .frame(maxWidth: .infinity)
        .zIndex(10)
    }

    private var referenceQuery: ComposerReferenceQuery? {
        ComposerReferenceResolver.query(in: text, selectionUTF16Offset: selectionUTF16Offset)
    }

    private var referenceSuggestions: [ComposerReferenceSuggestion] {
        guard let query = referenceQuery, query.signature != dismissedSuggestionSignature else { return [] }
        let plugins = model.plugins.map { plugin in
            var visible = plugin
            visible.manifest.skills = plugin.manifest.skills.filter { model.isSkillEnabled($0, in: plugin) }
            return visible
        }
        return ComposerReferenceResolver.suggestions(for: query, plugins: plugins)
    }

    private var referenceSuggestionMenuHeight: CGFloat {
        CGFloat(referenceSuggestions.count) * 40 + 12
    }

    private var referenceSuggestionMenu: some View {
        VStack(spacing: 2) {
            ForEach(Array(referenceSuggestions.enumerated()), id: \.element.id) { index, suggestion in
                Button { acceptSuggestion(at: index) } label: {
                    HStack(spacing: 9) {
                        Image(systemName: suggestion.kind == .plugin ? "shippingbox" : suggestion.kind == .skill ? "wand.and.stars" : "chevron.forward")
                            .font(.system(size: 10.5, weight: .semibold))
                            .foregroundStyle(index == selectedSuggestionIndex ? CosTheme.blue : .secondary)
                            .frame(width: 16)
                        Text(suggestion.title)
                            .font(.system(size: 12, weight: .semibold))
                            .lineLimit(1)
                        Text(suggestion.detail)
                            .font(.system(size: 10.5))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(suggestion.kind.title.uppercased())
                            .font(.system(size: 8, weight: .bold))
                            .foregroundStyle(.tertiary)
                    }
                    .padding(.horizontal, 10)
                    .frame(height: 38)
                    .background(index == selectedSuggestionIndex ? CosTheme.blue.opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(6)
        .frame(maxWidth: 520)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(.primary.opacity(0.1), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.22), radius: 18, y: 8)
    }

    private var editorHeight: CGFloat {
        let explicitLines = text.reduce(1) { count, character in count + (character == "\n" ? 1 : 0) }
        let wrappedLines = max(0, text.count / 88)
        return min(132, max(46, CGFloat(explicitLines + wrappedLines) * 18 + 28))
    }

    private var trimmedText: String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var composerPlaceholder: String {
        if model.canSteerSelectedThread { return "Steer the active run…" }
        if model.isRunning { return "Another task is running…" }
        return "Ask Cos to build, inspect, fix, or run anything…"
    }

    private var showsStopAction: Bool {
        model.isRunning && (trimmedText.isEmpty || !model.canSteerSelectedThread)
    }

    private func primaryAction() {
        if showsStopAction {
            model.cancel()
        } else {
            submit()
        }
    }

    private func submit() {
        let prompt = trimmedText
        guard !prompt.isEmpty else { return }
        if model.isRunning {
            guard model.canSteerSelectedThread else { return }
            model.steer(prompt)
        } else {
            model.send(prompt)
        }
        text = ""
        selectionUTF16Offset = 0
        dismissedSuggestionSignature = nil
    }

    private func prepareSubagentPrompt(route: SubagentRoute, effort: ReasoningEffort) {
        text = "/subagent Ask \(route.model.name) [\(route.model.id)] at \(effort.rawValue) reasoning to "
        selectionUTF16Offset = text.utf16.count
        dismissedSuggestionSignature = nil
        editorFocused = true
    }

    private func moveSuggestion(_ offset: Int) {
        guard !referenceSuggestions.isEmpty else { return }
        selectedSuggestionIndex = (selectedSuggestionIndex + offset + referenceSuggestions.count) % referenceSuggestions.count
    }

    private func acceptSelectedSuggestion() {
        acceptSuggestion(at: selectedSuggestionIndex)
    }

    private func acceptSuggestion(at index: Int) {
        guard let query = referenceQuery, referenceSuggestions.indices.contains(index) else { return }
        let suggestion = referenceSuggestions[index]
        let replacement = ComposerReferenceResolver.replacingQuery(in: text, query: query, with: suggestion.insertion)
        text = replacement.text
        selectionUTF16Offset = replacement.selectionUTF16Offset
        selectedSuggestionIndex = 0
        dismissedSuggestionSignature = nil
        editorFocused = true
    }

    private func dismissSuggestions() {
        dismissedSuggestionSignature = referenceQuery?.signature
    }
}

private struct AgentTextEditor: NSViewRepresentable {
    @Binding var text: String
    @Binding var isFocused: Bool
    @Binding var selectionUTF16Offset: Int
    let suggestionsVisible: Bool
    let onMoveSuggestion: (Int) -> Void
    let onAcceptSuggestion: () -> Void
    let onDismissSuggestions: () -> Void
    let onSubmit: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = false
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.verticalScrollElasticity = .none
        scrollView.horizontalScrollElasticity = .none

        let textView = CommandTextView()
        textView.delegate = context.coordinator
        textView.submit = onSubmit
        textView.suggestionsVisible = suggestionsVisible
        textView.moveSuggestion = onMoveSuggestion
        textView.acceptSuggestion = onAcceptSuggestion
        textView.dismissSuggestions = onDismissSuggestions
        textView.string = text
        textView.drawsBackground = false
        textView.backgroundColor = .clear
        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]
        textView.font = .systemFont(ofSize: 13.2)
        textView.textColor = .labelColor
        textView.insertionPointColor = .controlAccentColor
        textView.textContainerInset = NSSize(width: 5, height: 8)
        textView.minSize = .zero
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.lineFragmentPadding = 0
        textView.setAccessibilityLabel("Message Cos")
        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? CommandTextView else { return }
        context.coordinator.parent = self
        textView.submit = onSubmit
        textView.suggestionsVisible = suggestionsVisible
        textView.moveSuggestion = onMoveSuggestion
        textView.acceptSuggestion = onAcceptSuggestion
        textView.dismissSuggestions = onDismissSuggestions
        if textView.string != text {
            textView.string = text
            let location = min(selectionUTF16Offset, text.utf16.count)
            textView.setSelectedRange(NSRange(location: location, length: 0))
        } else if textView.selectedRange().length == 0,
                  textView.selectedRange().location != selectionUTF16Offset,
                  selectionUTF16Offset <= text.utf16.count {
            textView.setSelectedRange(NSRange(location: selectionUTF16Offset, length: 0))
        }
        if isFocused, textView.window?.firstResponder !== textView {
            textView.window?.makeFirstResponder(textView)
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: AgentTextEditor

        init(parent: AgentTextEditor) { self.parent = parent }

        func textDidBeginEditing(_ notification: Notification) { parent.isFocused = true }
        func textDidEndEditing(_ notification: Notification) { parent.isFocused = false }
        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
            parent.selectionUTF16Offset = textView.selectedRange().location
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.selectionUTF16Offset = textView.selectedRange().location
        }
    }
}

private final class CommandTextView: NSTextView {
    var submit: (() -> Void)?
    var suggestionsVisible = false
    var moveSuggestion: ((Int) -> Void)?
    var acceptSuggestion: (() -> Void)?
    var dismissSuggestions: (() -> Void)?

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36, event.modifierFlags.contains(.command) {
            submit?()
            return
        }
        if suggestionsVisible {
            switch event.keyCode {
            case 125:
                moveSuggestion?(1)
                return
            case 126:
                moveSuggestion?(-1)
                return
            case 36, 48:
                acceptSuggestion?()
                return
            case 53:
                dismissSuggestions?()
                return
            default:
                break
            }
        }
        super.keyDown(with: event)
    }
}

private struct ModelPickerView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Binding var isPresented: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Model & reasoning").font(.system(size: 13, weight: .semibold))
                    Text(model.selectedProvider.name).font(.system(size: 10.5)).foregroundStyle(.secondary)
                }
                Spacer()
                if model.selectedModel.supportsFastMode {
                    Button { model.preferences.fastMode.toggle(); model.persistPreferences() } label: {
                        Image(systemName: "bolt.fill")
                            .foregroundStyle(model.preferences.fastMode ? CosTheme.blue : .secondary)
                            .frame(width: 27, height: 27)
                            .background(.primary.opacity(0.055), in: Circle())
                    }
                    .buttonStyle(.plain)
                    .help("Fast mode")
                } else {
                    Color.clear.frame(width: 27, height: 27)
                }
            }
            .padding(13)

            Divider()

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    ForEach(model.providers.filter(\.isEnabled)) { provider in
                        let providerModels = model.models.filter { $0.providerID == provider.id }
                        if !providerModels.isEmpty {
                            Text(provider.name.uppercased())
                                .font(.system(size: 9.5, weight: .semibold))
                                .foregroundStyle(.tertiary)
                                .padding(.horizontal, 10)
                                .padding(.top, 8)
                            ForEach(providerModels) { item in
                                Button {
                                    model.selectModel(item)
                                } label: {
                                    HStack {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(item.name).font(.system(size: 12.5, weight: .medium))
                                            Text(item.model).font(.system(size: 9.5)).foregroundStyle(.secondary)
                                        }
                                        Spacer()
                                        if model.selectedModel.id == item.id {
                                            Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundStyle(CosTheme.blue)
                                        }
                                        ProviderMark(providerID: item.providerID, size: 15)
                                    }
                                    .contentShape(Rectangle())
                                    .padding(.horizontal, 10)
                                    .frame(height: 38)
                                    .background(model.selectedModel.id == item.id ? CosTheme.blue.opacity(0.09) : .clear, in: RoundedRectangle(cornerRadius: 8))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(7)
            }
            .frame(height: 238)

            Divider()

            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("Reasoning effort").font(.system(size: 11.5, weight: .medium))
                    Spacer()
                    Text(model.selectedThread?.effort.title ?? "High")
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                        .frame(width: 64, alignment: .trailing)
                }
                EffortSlider(
                    selection: Binding(
                        get: { model.selectedThread?.effort ?? model.preferences.defaultEffort },
                        set: { model.setEffort($0) }
                    ),
                    options: model.selectedModel.effortOptions
                )
                .id(model.selectedModel.id)
                Group {
                    if model.selectedModel.supportsFastMode {
                        HStack(alignment: .center, spacing: 10) {
                            VStack(alignment: .leading, spacing: 2) {
                                Label("Fast mode", systemImage: "bolt.fill").font(.system(size: 11.5, weight: .medium))
                                Text("Prefer the provider’s lower-latency route").font(.system(size: 9.5)).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 8)
                            Toggle("Fast mode", isOn: Binding(
                                get: { model.preferences.fastMode },
                                set: { model.preferences.fastMode = $0; model.persistPreferences() }
                            ))
                            .labelsHidden()
                            .toggleStyle(.switch)
                            .controlSize(.small)
                            .fixedSize()
                        }
                    } else {
                        HStack(alignment: .center, spacing: 10) {
                            VStack(alignment: .leading, spacing: 2) {
                                Label("Standard latency", systemImage: "clock").font(.system(size: 11.5, weight: .medium))
                                Text("Fast mode isn’t offered for this model").font(.system(size: 9.5)).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 8)
                        }
                    }
                }
                .frame(minHeight: 30, alignment: .center)
            }
            .padding(13)
        }
        .frame(width: 330)
    }
}

struct EffortSlider: View {
    @Binding var selection: ReasoningEffort
    let options: [ReasoningEffort]
    @State private var dragPosition: CGFloat?

    var body: some View {
        GeometryReader { geometry in
            let count = max(options.count, 2)
            let inset: CGFloat = 14
            let usable = geometry.size.width - inset * 2
            let step = usable / CGFloat(count - 1)
            let selectedIndex = options.firstIndex(of: selection) ?? 0
            let selectedPosition = inset + CGFloat(selectedIndex) * step
            let thumbPosition = dragPosition ?? selectedPosition
            let normalizedPosition = min(max((thumbPosition - inset) / max(usable, 1), 0), 1)
            let sunIntensity = max(0, (normalizedPosition - 0.2) / 0.8)
            ZStack(alignment: .leading) {
                Capsule().fill(.primary.opacity(0.12)).frame(height: 24)
                Capsule()
                    .fill(LinearGradient(colors: [CosTheme.blue, CosTheme.violet], startPoint: .leading, endPoint: .trailing))
                    .frame(width: thumbPosition, height: 24)
                ForEach(options.indices, id: \.self) { index in
                    Circle()
                        .fill(index <= selectedIndex ? .white.opacity(0.65) : .secondary.opacity(0.55))
                        .frame(width: 4, height: 4)
                        .position(x: inset + CGFloat(index) * step, y: 14)
                }
                ReasoningSunThumb(intensity: sunIntensity, isDragging: dragPosition != nil)
                    .position(x: thumbPosition, y: 14)
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { value in
                let position = min(max(value.location.x, inset), geometry.size.width - inset)
                dragPosition = position
                let raw = Int(round((position - inset) / max(step, 1)))
                let index = min(max(raw, 0), options.count - 1)
                guard options.indices.contains(index), selection != options[index] else { return }
                var transaction = Transaction()
                transaction.disablesAnimations = true
                withTransaction(transaction) { selection = options[index] }
            }.onEnded { _ in
                withAnimation(.easeOut(duration: 0.17)) { dragPosition = nil }
            })
            .animation(dragPosition == nil ? .easeInOut(duration: 0.17) : nil, value: selection)
        }
        .frame(height: 28)
        .accessibilityElement()
        .accessibilityLabel("Reasoning effort")
        .accessibilityValue(selection.title)
        .accessibilityAdjustableAction { direction in
            guard let index = options.firstIndex(of: selection) else { return }
            switch direction {
            case .increment: selection = options[min(index + 1, options.count - 1)]
            case .decrement: selection = options[max(index - 1, 0)]
            @unknown default: break
            }
        }
    }
}

private struct ReasoningSunThumb: View {
    let intensity: CGFloat
    let isDragging: Bool

    var body: some View {
        ZStack {
            ForEach(0..<8, id: \.self) { index in
                Capsule()
                    .fill(.white.opacity(0.88 * intensity))
                    .frame(width: 1.5, height: 4)
                    .offset(y: -15)
                    .rotationEffect(.degrees(Double(index) * 45))
            }

            Circle()
                .fill(.white)
                .overlay {
                    Circle()
                        .fill(
                            RadialGradient(
                                colors: [
                                    .white,
                                    Color(red: 1, green: 0.91, blue: 0.64).opacity(intensity),
                                ],
                                center: .center,
                                startRadius: 2,
                                endRadius: 14
                            )
                        )
                        .opacity(intensity * 0.72)
                }
                .frame(width: 28, height: 28)
                .shadow(color: .white.opacity(0.62 * intensity), radius: 3 + 7 * intensity)
                .shadow(color: Color(red: 1, green: 0.72, blue: 0.25).opacity(0.52 * intensity), radius: 2 + 8 * intensity)
                .shadow(color: .black.opacity(0.16), radius: 3, y: 1)
        }
        .frame(width: 28, height: 28)
        .animation(isDragging ? nil : .easeInOut(duration: 0.2), value: intensity)
        .accessibilityHidden(true)
    }
}
