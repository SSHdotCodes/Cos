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
