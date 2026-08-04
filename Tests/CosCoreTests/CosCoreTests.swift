import XCTest
@testable import CosCore

final class CosCoreTests: XCTestCase {
    func testUpdateCheckFindsNewVersionOrBuild() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [UpdateURLProtocolStub.self]
        let session = URLSession(configuration: configuration)
        let service = CosUpdateService(
            feedURL: URL(string: "https://updates.example.test/cos.json")!,
            session: session
        )

        let newerVersion = try await service.check(currentVersion: "0.3.0", currentBuild: 4)
        XCTAssertEqual(newerVersion?.version, "1.0.0")

        let newerBuild = try await service.check(currentVersion: "1.0.0", currentBuild: 99)
        XCTAssertEqual(newerBuild?.build, 100)

        let current = try await service.check(currentVersion: "1.0.0", currentBuild: 100)
        XCTAssertNil(current)
    }

    func testUpdateVersionComparisonHandlesSemanticComponents() {
        XCTAssertTrue(CosUpdateService.isNewer("0.3.0", than: "0.2.9"))
        XCTAssertTrue(CosUpdateService.isNewer("1.0.0", than: "0.99.99"))
        XCTAssertFalse(CosUpdateService.isNewer("0.3", than: "0.3.0"))
        XCTAssertFalse(CosUpdateService.isNewer("0.2.9", than: "0.3.0"))
    }

    func testUpdateManifestDecodesReleaseMetadata() throws {
        let json = """
        {
          "version": "1.0.0",
          "build": 100,
          "downloadURL": "https://cos.ssh.codes/downloads/Cos-1.0.0-macOS-arm64.zip",
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "minimumSystemVersion": "15.0",
          "releaseNotes": "One-click updates."
        }
        """
        let manifest = try JSONDecoder().decode(CosUpdateManifest.self, from: Data(json.utf8))
        XCTAssertEqual(manifest.version, "1.0.0")
        XCTAssertEqual(manifest.build, 100)
        XCTAssertEqual(manifest.downloadURL.host, "cos.ssh.codes")
    }

    func testCompactionKeepsRecentMessagesAndCreatesCheckpoint() {
        let messages = (0..<20).map { index in
            ChatMessage(role: index.isMultiple(of: 2) ? .user : .assistant, content: String(repeating: "context \(index) ", count: 120))
        }
        let result = CompactionEngine().prepare(
            messages: messages,
            previousSummary: "Earlier checkpoint",
            contextWindow: 2_000,
            thresholdPercent: 50,
            keepRecentTokens: 500
        )
        XCTAssertTrue(result.didCompact)
        XCTAssertNotNil(result.compactedSummary)
        XCTAssertTrue(result.promptContext.contains("Recent verbatim context"))
        XCTAssertTrue(result.promptContext.contains("context 19"))
    }

    func testSettingsPluginAcceptsOnlyAllowlistedMutation() {
        let valid = "Done. <cos-settings>{\"key\":\"fastMode\",\"value\":true}</cos-settings>"
        let result = CosSettingsPlugin.extract(from: valid)
        XCTAssertEqual(result.visibleText, "Done.")
        XCTAssertEqual(result.mutation, .fastMode(true))

        let invalid = CosSettingsPlugin.extract(from: "No. <cos-settings>{\"key\":\"shellCommand\",\"value\":\"rm\"}</cos-settings>")
        XCTAssertNil(invalid.mutation)
    }

    func testCosPluginParsesGuardedSkillManagement() {
        let text = "Created. <cos-manage>{\"action\":\"createSkill\",\"id\":\"release-check\",\"name\":\"Release Check\",\"description\":\"Verify a release\",\"instructions\":\"Build and test it.\"}</cos-manage>"
        let result = CosSettingsPlugin.extract(from: text)
        XCTAssertEqual(result.visibleText, "Created.")
        XCTAssertEqual(
            result.managementAction,
            .createSkill(
                id: "release-check",
                name: "Release Check",
                description: "Verify a release",
                instructions: "Build and test it.",
                pluginID: nil
            )
        )
    }

    func testDefaultCatalogReferencesKnownProviders() {
        let ids = Set(DefaultCatalog.providers.map(\.id))
        XCTAssertFalse(DefaultCatalog.models.isEmpty)
        XCTAssertTrue(DefaultCatalog.models.allSatisfy { ids.contains($0.providerID) })
        XCTAssertTrue(DefaultCatalog.providers.filter { $0.bridge != .pi }.allSatisfy { $0.baseURL != nil })
    }

    func testCatalogUsesModelSpecificReasoningEfforts() throws {
        let grok = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "xai:grok-4.5" })
        XCTAssertEqual(grok.model, "grok-4.5")
        XCTAssertEqual(grok.effortOptions, [.low, .medium, .high])
        XCTAssertEqual(grok.normalizedEffort(.max), .high)
        XCTAssertEqual(grok.normalizedEffort(.minimal), .low)
        XCTAssertFalse(grok.supportsFastMode)

        let opus = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "anthropic:claude-opus-5" })
        XCTAssertEqual(opus.effortOptions, [.low, .medium, .high, .extraHigh, .max])
        let sol = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "chatgpt:gpt-5.6-sol" })
        XCTAssertTrue(sol.supportsFastMode)

        let luna = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "chatgpt:gpt-5.6-luna" })
        XCTAssertEqual(luna.effortOptions, ReasoningEffort.allCases)
        let haiku = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "anthropic:claude-haiku-4.5" })
        XCTAssertEqual(haiku.effortOptions, [.low])
    }

    func testComposerReferenceSuggestionsIncludeCommandsSkillsAndPlugins() throws {
        let manifest = CosPluginManifest(
            schemaVersion: 1,
            id: "codes.ssh.cos.computer-use",
            name: "Computer Use",
            version: "1.0.0",
            author: "Cos",
            description: "Operate Mac apps",
            capabilities: [],
            skills: ["computer-use"],
            homepage: nil,
            builtIn: true
        )
        let plugin = InstalledPlugin(manifest: manifest, location: URL(fileURLWithPath: "/tmp/computer-use"), isTrusted: true, isEnabled: true)

        let slashQuery = try XCTUnwrap(ComposerReferenceResolver.query(in: "/", selectionUTF16Offset: 1))
        let slashSuggestions = ComposerReferenceResolver.suggestions(for: slashQuery, plugins: [plugin])
        XCTAssertTrue(slashSuggestions.contains { $0.title == "/subagent" })
        XCTAssertTrue(slashSuggestions.contains { $0.title == "/goal" })
        XCTAssertTrue(slashSuggestions.contains { $0.title == "/computer-use" })

        let pluginQuery = try XCTUnwrap(ComposerReferenceResolver.query(in: "@comp", selectionUTF16Offset: 5))
        let pluginSuggestions = ComposerReferenceResolver.suggestions(for: pluginQuery, plugins: [plugin])
        XCTAssertEqual(pluginSuggestions.first?.title, "@computer-use")

        let replacement = try XCTUnwrap(pluginSuggestions.first).insertion
        let updated = ComposerReferenceResolver.replacingQuery(in: "Use @comp", query: .init(trigger: "@", term: "comp", rangeLocation: 4, rangeLength: 5), with: replacement)
        XCTAssertEqual(updated.text, "Use @computer-use ")
        XCTAssertEqual(updated.selectionUTF16Offset, 18)
    }

    func testSubagentRouteUsesExactModelEffortAllowlist() throws {
        let grok = try XCTUnwrap(DefaultCatalog.models.first { $0.id == "xai:grok-4.5" })
        let provider = try XCTUnwrap(DefaultCatalog.providers.first { $0.id == grok.providerID })
        let route = SubagentRoute(model: grok, provider: provider)

        XCTAssertTrue(route.accepts(.low))
        XCTAssertTrue(route.accepts(.high))
        XCTAssertFalse(route.accepts(.minimal))
        XCTAssertFalse(route.accepts(.max))
        XCTAssertEqual(route.id, "xai:grok-4.5")
    }

    func testSubagentAuthorityRequiresExplicitPositiveUserIntent() {
        XCTAssertTrue(SubagentAuthorization.isExplicitlyRequested(in: "/subagent ask Grok to review this"))
        XCTAssertTrue(SubagentAuthorization.isExplicitlyRequested(in: "Delegate this to another model"))
        XCTAssertFalse(SubagentAuthorization.isExplicitlyRequested(in: "Do not use subagents for this"))
        XCTAssertTrue(SubagentAuthorization.isExplicitlyForbidden(in: "Work without subagents"))
        XCTAssertFalse(SubagentAuthorization.isExplicitlyRequested(in: "Review this implementation"))
    }

    func testAgentRequestDefaultsToNoSubagentAuthorityOrRecursion() throws {
        let model = try XCTUnwrap(DefaultCatalog.models.first)
        let provider = try XCTUnwrap(DefaultCatalog.providers.first { $0.id == model.providerID })
        let thread = CosThread(workspacePath: "/tmp", modelID: model.id)
        let request = AgentRequest(
            prompt: "hello",
            thread: thread,
            model: model,
            provider: provider,
            effort: .low,
            fastMode: false,
            fullAccess: false
        )

        XCTAssertFalse(request.subagentsAuthorized)
        XCTAssertTrue(request.availableSubagentRoutes.isEmpty)
        XCTAssertEqual(request.agentDepth, 0)
        XCTAssertNil(request.runControl)
        XCTAssertFalse(request.browserEnabled)
    }

    func testBetterWrightUsesBoundedStableSessionNames() {
        XCTAssertEqual(CosBetterWrightRuntime.sanitizedSession("Task 123 / Browser"), "task-123-browser")
        XCTAssertEqual(CosBetterWrightRuntime.sanitizedSession("---"), "default")
        XCTAssertLessThanOrEqual(CosBetterWrightRuntime.sanitizedSession(String(repeating: "a", count: 200)).count, 80)
    }

    func testRunControlKeepsSteeringFIFOAndBoundsQueue() async {
        let control = AgentRunControl(maximumQueuedMessages: 2)
        let firstAccepted = await control.submit("first")
        let secondAccepted = await control.submit("second")
        let overflowAccepted = await control.submit("third")
        let messages = await control.drain()

        XCTAssertTrue(firstAccepted)
        XCTAssertTrue(secondAccepted)
        XCTAssertFalse(overflowAccepted)
        XCTAssertEqual(messages.map(\.content), ["first", "second"])
        let drainedAgain = await control.drain()
        XCTAssertTrue(drainedAgain.isEmpty)
    }

    func testRunControlInterruptsOnlyInstalledProviderGeneration() async {
        let control = AgentRunControl()
        let interrupted = expectation(description: "provider request interrupted")
        let currentToken = UUID()
        await control.installProviderInterrupt(token: currentToken) {
            interrupted.fulfill()
        }

        _ = await control.submit("change direction")
        await fulfillment(of: [interrupted], timeout: 1)
        await control.clearProviderInterrupt(token: UUID())

        let second = await control.drain()
        XCTAssertEqual(second.map(\.content), ["change direction"])
        await control.clearProviderInterrupt(token: currentToken)
    }

    func testOlderPreferencesDecodeWithoutTitleModel() throws {
        let json = """
        {"appearance":"system","fastMode":false,"fullAccess":true,"autoCompact":true,"compactAtPercent":78,"keepRecentTokens":20000,"showTokenUsage":false,"animateStreaming":true,"defaultWorkspace":"/tmp","selectedModelID":"chatgpt:gpt-5.6-sol","defaultEffort":"high"}
        """
        let preferences = try JSONDecoder().decode(AppPreferences.self, from: Data(json.utf8))
        XCTAssertNil(preferences.titleModelID)
    }

    func testComputerUseCanListForegroundApplicationsWithoutRetainedState() throws {
        let result = try CosComputerUseRuntime.execute(
            name: "computer_list_apps",
            app: nil,
            elementIndex: nil,
            x: nil,
            y: nil,
            text: nil,
            key: nil,
            direction: nil,
            pages: nil
        )
        XCTAssertFalse(result.isEmpty)
    }

    func testOlderMessagesDecodeWithoutWorkTrace() throws {
        let id = UUID()
        let json = "{\"id\":\"\(id.uuidString)\",\"role\":\"assistant\",\"content\":\"done\",\"createdAt\":0,\"isStreaming\":false}"
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        let message = try decoder.decode(ChatMessage.self, from: Data(json.utf8))
        XCTAssertNil(message.workItems)
    }

    func testThreadStoreRoundTrip() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = ThreadStore(directory: root)
        let timestamp = Date(timeIntervalSince1970: 2_000_000_000)
        let thread = CosThread(
            workspacePath: "/tmp",
            modelID: "test",
            messages: [.init(role: .user, content: "hello", createdAt: timestamp)],
            createdAt: timestamp,
            updatedAt: timestamp
        )
        try await store.upsert(thread)
        let loaded = try await store.loadAll()
        XCTAssertEqual(loaded, [thread])
    }
}

private final class UpdateURLProtocolStub: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let body = """
        {
          "version": "1.0.0",
          "build": 100,
          "downloadURL": "https://cos.ssh.codes/downloads/Cos-1.0.0-macOS-arm64.zip",
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "minimumSystemVersion": "15.0",
          "releaseNotes": "One-click updates."
        }
        """
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: ["Content-Type": "application/json"]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(body.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
