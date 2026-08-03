import CosCore
import SwiftUI

struct PluginLibraryView: View {
    private enum LibrarySection: String, CaseIterable, Identifiable {
        case installed = "Installed"
        case marketplace = "Marketplace"
        var id: String { rawValue }
    }

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var selection: String?
    @State private var section = LibrarySection.installed
    @State private var marketplaceQuery = ""

    private var filteredMarketplace: [CosMarketplaceListing] {
        let query = marketplaceQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return model.marketplacePlugins }
        return model.marketplacePlugins.filter {
            ([$0.name, $0.author, $0.description] + ($0.tags ?? []))
                .joined(separator: " ").lowercased().contains(query)
        }
    }

    var body: some View {
        ZStack(alignment: .topTrailing) {
            NavigationSplitView {
                VStack(spacing: 0) {
                    HStack { CosMark(); Spacer() }.padding(12)
                    Picker("Library section", selection: $section) {
                        ForEach(LibrarySection.allCases) { Text($0.rawValue).tag($0) }
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .padding(.horizontal, 10)
                    .padding(.bottom, 8)

                    if section == .installed {
                        List(model.plugins, selection: $selection) { plugin in
                            installedRow(plugin).tag(plugin.id)
                        }
                        .listStyle(.sidebar)
                    } else {
                        HStack(spacing: 6) {
                            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                            TextField("Search plugins", text: $marketplaceQuery).textFieldStyle(.plain)
                        }
                        .padding(.horizontal, 9)
                        .frame(height: 30)
                        .background(.primary.opacity(0.055), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                        .padding(.horizontal, 10)
                        .padding(.bottom, 5)

                        if model.isLoadingMarketplace && model.marketplacePlugins.isEmpty {
                            VStack(spacing: 8) {
                                ProgressView().controlSize(.small)
                                Text("Loading marketplace…").font(.caption).foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        } else if let error = model.marketplaceError, model.marketplacePlugins.isEmpty {
                            ContentUnavailableView("Marketplace unavailable", systemImage: "wifi.exclamationmark", description: Text(error))
                        } else {
                            List(filteredMarketplace, selection: $selection) { listing in
                                marketplaceRow(listing).tag(listing.id)
                            }
                            .listStyle(.sidebar)
                        }
                    }

                    HStack {
                        if section == .installed {
                            Button("Install from disk…") { model.installPluginFromDisk() }
                        } else {
                            Text("cos.ssh.codes").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button {
                            Task {
                                if section == .installed { await model.reloadPlugins() }
                                else { await model.loadMarketplace(force: true) }
                            }
                        } label: { Image(systemName: "arrow.clockwise") }
                        .disabled(model.isLoadingMarketplace)
                    }
                    .padding(10)
                }
                .navigationSplitViewColumnWidth(min: 210, ideal: 230, max: 280)
            } detail: {
                if section == .installed,
                   let plugin = model.plugins.first(where: { $0.id == (selection ?? model.plugins.first?.id) }) {
                    PluginDetail(plugin: plugin)
                } else if section == .marketplace,
                          let listing = model.marketplacePlugins.first(where: { $0.id == (selection ?? filteredMarketplace.first?.id) }) {
                    MarketplacePluginDetail(listing: listing)
                } else {
                    ContentUnavailableView(
                        section == .installed ? "No plugins installed" : "Choose a marketplace plugin",
                        systemImage: section == .installed ? "shippingbox" : "storefront"
                    )
                }
            }
            .background(model.preferences.appearance == .trueDark ? Color.black : Color(nsColor: .windowBackgroundColor))

            Button { dismiss() } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 28, height: 28)
                    .background(.regularMaterial, in: Circle())
            }
            .buttonStyle(.plain)
            .help("Close plugin library")
            .padding(12)
        }
        .onAppear {
            selection = selection ?? model.plugins.first?.id
            Task { await model.loadMarketplace() }
        }
        .onChange(of: section) { _, newSection in
            selection = newSection == .installed ? model.plugins.first?.id : filteredMarketplace.first?.id
            if newSection == .marketplace { Task { await model.loadMarketplace() } }
        }
        .onChange(of: model.marketplacePlugins.count) { _, _ in
            if section == .marketplace, selection == nil { selection = filteredMarketplace.first?.id }
        }
    }

    private func installedRow(_ plugin: InstalledPlugin) -> some View {
        HStack(spacing: 9) {
            Image(systemName: plugin.id == "codes.ssh.cos.computer-use" ? "display" : plugin.manifest.builtIn == true ? "gearshape.fill" : "shippingbox.fill")
                .foregroundStyle(plugin.manifest.builtIn == true ? CosTheme.blue : .secondary)
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(plugin.manifest.name).font(.system(size: 12.5, weight: .medium))
                Text(plugin.manifest.author).font(.system(size: 10.5)).foregroundStyle(.secondary)
            }
        }
    }

    private func marketplaceRow(_ listing: CosMarketplaceListing) -> some View {
        HStack(spacing: 9) {
            Image(systemName: listing.id == "codes.ssh.cos.computer-use" ? "display" : "shippingbox.fill")
                .foregroundStyle(listing.featured == true ? CosTheme.blue : .secondary)
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(listing.name).font(.system(size: 12.5, weight: .medium)).lineLimit(1)
                Text(model.plugins.contains(where: { $0.id == listing.id }) ? "Installed" : listing.author)
                    .font(.system(size: 10.5))
                    .foregroundStyle(model.plugins.contains(where: { $0.id == listing.id }) ? .green : .secondary)
            }
        }
    }
}

private struct MarketplacePluginDetail: View {
    @EnvironmentObject private var model: AppModel
    let listing: CosMarketplaceListing

    private var installed: InstalledPlugin? { model.plugins.first { $0.id == listing.id } }
    private var manifest: CosPluginManifest? { listing.manifest }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack(alignment: .top, spacing: 14) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 14, style: .continuous).fill(CosTheme.blue.opacity(0.14))
                        Image(systemName: listing.id == "codes.ssh.cos.computer-use" ? "display" : "shippingbox.fill")
                            .font(.system(size: 23, weight: .semibold)).foregroundStyle(CosTheme.blue)
                    }
                    .frame(width: 58, height: 58)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(listing.name).font(.system(size: 22, weight: .semibold, design: .rounded))
                            if listing.featured == true {
                                Text("OFFICIAL").font(.system(size: 8, weight: .bold)).foregroundStyle(CosTheme.blue)
                                    .padding(.horizontal, 6).padding(.vertical, 3).background(CosTheme.blue.opacity(0.1), in: Capsule())
                            }
                        }
                        Text("\(listing.author) · version \(listing.version)").font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                }

                Text(listing.description).font(.system(size: 13)).lineSpacing(3)

                if let tags = listing.tags, !tags.isEmpty {
                    FlowLayout(spacing: 7) {
                        ForEach(tags, id: \.self) { tag in
                            Text(tag).font(.system(size: 10.5, weight: .medium)).padding(.horizontal, 8).padding(.vertical, 5)
                                .background(.primary.opacity(0.055), in: Capsule())
                        }
                    }
                }

                if let capabilities = manifest?.capabilities, !capabilities.isEmpty {
                    Divider()
                    Text("CAPABILITIES").font(.system(size: 9.5, weight: .semibold)).foregroundStyle(.tertiary)
                    ForEach(capabilities, id: \.id) { capability in
                        HStack(alignment: .top, spacing: 9) {
                            Image(systemName: capability.risk == "safe" ? "checkmark.shield.fill" : "exclamationmark.shield.fill")
                                .foregroundStyle(capability.risk == "safe" ? .green : CosTheme.orange)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(capability.id).font(.system(size: 11.5, weight: .semibold)).monospaced()
                                Text(capability.description).font(.system(size: 10.5)).foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                Spacer()
                HStack {
                    Link("View on cos.ssh.codes", destination: URL(string: "https://cos.ssh.codes/plugins/\(listing.id)")!)
                    Spacer()
                    installControl
                }
            }
            .frame(maxWidth: 560, alignment: .leading)
            .padding(30)
            .frame(maxWidth: .infinity, alignment: .top)
        }
    }

    @ViewBuilder
    private var installControl: some View {
        if model.installingMarketplacePluginID == listing.id {
            ProgressView().controlSize(.small)
        } else if listing.type != "plugin" {
            Link("View Template", destination: URL(string: "https://cos.ssh.codes/api/plugins/\(listing.id)/manifest")!)
                .buttonStyle(.bordered)
        } else if listing.id == "codes.ssh.cos.computer-use", installed != nil, !model.computerUseAccessGranted {
            Button("Allow Access…") { model.installMarketplacePlugin(listing) }
                .buttonStyle(.borderedProminent)
        } else if let installed, installed.manifest.version == listing.version {
            Label(listing.builtIn == true ? "Included" : "Installed", systemImage: "checkmark.circle.fill")
                .font(.system(size: 11.5, weight: .medium)).foregroundStyle(.green)
        } else {
            Button(installed == nil ? "Install" : "Update") { model.installMarketplacePlugin(listing) }
                .buttonStyle(.borderedProminent)
        }
    }
}

private struct PluginDetail: View {
    @EnvironmentObject private var model: AppModel
    let plugin: InstalledPlugin
    @State private var confirmRemoval = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack(alignment: .top, spacing: 14) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(LinearGradient(colors: [CosTheme.blue, CosTheme.violet], startPoint: .topLeading, endPoint: .bottomTrailing))
                        Image(systemName: plugin.manifest.builtIn == true ? "gearshape.fill" : "shippingbox.fill")
                            .font(.system(size: 24, weight: .semibold)).foregroundStyle(.white)
                    }.frame(width: 58, height: 58)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack { Text(plugin.manifest.name).font(.system(size: 22, weight: .semibold, design: .rounded)); if plugin.manifest.builtIn == true { Text("BUILT IN").font(.system(size: 8, weight: .bold)).foregroundStyle(CosTheme.blue).padding(.horizontal, 6).padding(.vertical, 3).background(CosTheme.blue.opacity(0.1), in: Capsule()) } }
                        Text("\(plugin.manifest.author) · version \(plugin.manifest.version)").font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                }
                Text(plugin.manifest.description).font(.system(size: 13)).lineSpacing(3)
                Divider()
                Text("CAPABILITIES").font(.system(size: 9.5, weight: .semibold)).foregroundStyle(.tertiary)
                ForEach(plugin.manifest.capabilities, id: \.id) { capability in
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: capability.risk == "safe" ? "checkmark.shield.fill" : "exclamationmark.shield.fill")
                            .foregroundStyle(capability.risk == "safe" ? .green : CosTheme.orange)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(capability.id).font(.system(size: 12, weight: .semibold)).monospaced()
                            Text(capability.description).font(.system(size: 11)).foregroundStyle(.secondary)
                        }
                    }
                }
                if plugin.id == "codes.ssh.cos.computer-use" {
                    Divider()
                    Text("MACOS PERMISSION").font(.system(size: 9.5, weight: .semibold)).foregroundStyle(.tertiary)
                    HStack(spacing: 11) {
                        Image(systemName: model.computerUseAccessGranted ? "checkmark.shield.fill" : "hand.raised.fill")
                            .font(.system(size: 16))
                            .foregroundStyle(model.computerUseAccessGranted ? .green : CosTheme.orange)
                            .frame(width: 22)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(model.computerUseAccessGranted ? "Accessibility access granted" : "Accessibility access required")
                                .font(.system(size: 12, weight: .semibold))
                            Text(model.computerUseAccessStatus ?? "Cos needs this permission to read and operate visible Mac apps.")
                                .font(.system(size: 10.5))
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 10)
                        if model.computerUseAccessGranted {
                            Text("Ready").font(.caption).foregroundStyle(.green)
                        } else {
                            VStack(alignment: .trailing, spacing: 5) {
                                Button("Allow Access…") { model.requestComputerUseAccess() }
                                    .buttonStyle(.borderedProminent)
                                    .controlSize(.small)
                                Button("Open Settings") { model.openAccessibilitySettings() }
                                    .buttonStyle(.plain)
                                    .font(.system(size: 10.5))
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .padding(12)
                    .background(.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
                }
                if !plugin.manifest.skills.isEmpty {
                    Divider()
                    Text("SKILLS").font(.system(size: 9.5, weight: .semibold)).foregroundStyle(.tertiary)
                    FlowLayout(spacing: 7) {
                        ForEach(plugin.manifest.skills, id: \.self) { skill in
                            Text(skill).font(.system(size: 10.5, weight: .medium)).padding(.horizontal, 8).padding(.vertical, 5).background(.primary.opacity(0.055), in: Capsule())
                        }
                    }
                }
                Spacer()
                HStack {
                    if let homepage = plugin.manifest.homepage { Link("View on cos.ssh.codes", destination: homepage) }
                    Spacer()
                    if plugin.manifest.builtIn != true {
                        Toggle("Enabled", isOn: Binding(
                            get: { plugin.isEnabled },
                            set: { model.setPlugin(plugin, enabled: $0) }
                        ))
                        .toggleStyle(.switch)
                        .controlSize(.small)
                        Button("Remove…", role: .destructive) { confirmRemoval = true }
                            .buttonStyle(.borderless)
                    }
                    Text(plugin.isTrusted ? "Trusted" : "Review required")
                        .font(.caption)
                        .foregroundStyle(plugin.isTrusted ? .green : CosTheme.orange)
                }
            }
            .frame(maxWidth: 560, alignment: .leading)
            .padding(30)
            .frame(maxWidth: .infinity, alignment: .top)
        }
        .confirmationDialog("Move \(plugin.manifest.name) to Trash?", isPresented: $confirmRemoval) {
            Button("Move to Trash", role: .destructive) { model.removePlugin(plugin) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The plugin can be recovered from the Trash.")
        }
        .onAppear {
            if plugin.id == "codes.ssh.cos.computer-use" { model.refreshComputerUseAccess() }
        }
    }
}

private struct FlowLayout: Layout {
    var spacing: CGFloat
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 500
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            x += size.width + spacing; rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: width, height: y + rowHeight)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, rowHeight: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX, x > bounds.minX { x = bounds.minX; y += rowHeight + spacing; rowHeight = 0 }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing; rowHeight = max(rowHeight, size.height)
        }
    }
}
