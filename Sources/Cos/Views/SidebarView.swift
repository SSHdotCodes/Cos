import CosCore
import SwiftUI

struct SidebarView: View {
    @EnvironmentObject private var model: AppModel
    @State private var hoveredThread: UUID?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                CosMark()
                Spacer()
                if let update = model.availableUpdate {
                    Button { model.installAvailableUpdate() } label: {
                        Group {
                            if model.isInstallingUpdate {
                                ProgressView()
                                    .controlSize(.small)
                            } else {
                                Image(systemName: "arrow.down.circle.fill")
                                    .font(.system(size: 14, weight: .semibold))
                                    .foregroundStyle(CosTheme.blue)
                            }
                        }
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(model.isInstallingUpdate)
                    .help(model.isInstallingUpdate
                        ? "Installing Cos \(update.version)…"
                        : "Install Cos \(update.version) and restart")
                    .accessibilityLabel("Install Cos \(update.version) and restart")
                }
                Button { model.newThread() } label: {
                    Image(systemName: "square.and.pencil")
                        .frame(width: 24, height: 24)
                }
                .buttonStyle(.plain)
                .help("New task (⌘N)")
            }
            .padding(.horizontal, 13)
            .padding(.top, 12)
            .padding(.bottom, 10)

            Button { model.newThread() } label: {
                Label("New task", systemImage: "plus")
                    .font(.system(size: 13, weight: .medium))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 11)
                    .frame(height: 34)
                    .background(.primary.opacity(0.075), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 10)
            .padding(.bottom, 9)

            List(selection: $model.selectedThreadID) {
                Section("Tasks") {
                    ForEach(model.threads) { thread in
                        threadRow(thread)
                            .tag(thread.id)
                            .contextMenu {
                                Button("Delete", role: .destructive) { model.deleteThread(thread.id) }
                            }
                    }
                }
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)

            VStack(spacing: 2) {
                Button { model.isPluginLibraryPresented = true } label: {
                    bottomRow("Plugins & skills", icon: "shippingbox")
                }
                .buttonStyle(.plain)

                SettingsLink {
                    bottomRow("Settings", icon: "gearshape")
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 8)
            .overlay(alignment: .top) { Divider() }
        }
        .background {
            if model.preferences.appearance == .trueDark {
                Color.black.ignoresSafeArea(.container, edges: .top)
            } else {
                Rectangle().fill(.ultraThinMaterial)
            }
        }
    }

    private func threadRow(_ thread: CosThread) -> some View {
        HStack(spacing: 8) {
            Image(systemName: thread.goal?.status == .active ? "scope" : "text.bubble")
                .font(.system(size: 12))
                .foregroundStyle(thread.goal?.status == .active ? CosTheme.blue : .secondary)
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(thread.title)
                    .font(.system(size: 12.5, weight: .medium))
                    .lineLimit(1)
                Text(URL(fileURLWithPath: thread.workspacePath).lastPathComponent)
                    .font(.system(size: 10.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if hoveredThread == thread.id {
                Button { model.deleteThread(thread.id) } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .semibold))
                }
                .buttonStyle(.plain)
            }
        }
        .contentShape(Rectangle())
        .onHover { hoveredThread = $0 ? thread.id : nil }
    }

    private func bottomRow(_ title: String, icon: String) -> some View {
        Label(title, systemImage: icon)
            .font(.system(size: 12.5))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 9)
            .frame(height: 32)
            .contentShape(Rectangle())
    }
}
