import Foundation

public struct BetterWrightInvocation: Sendable {
    public var executableURL: URL
    public var arguments: [String]
    public var environment: [String: String]

    public init(executableURL: URL, arguments: [String], environment: [String: String]) {
        self.executableURL = executableURL
        self.arguments = arguments
        self.environment = environment
    }
}

public struct BetterWrightCommandResult: Sendable {
    public var status: Int32
    public var output: String
    public var errorOutput: String

    public init(status: Int32, output: String, errorOutput: String) {
        self.status = status
        self.output = output
        self.errorOutput = errorOutput
    }
}

public enum BetterWrightRuntimeError: LocalizedError {
    case unavailable
    case failed(String)

    public var errorDescription: String? {
        switch self {
        case .unavailable:
            "The bundled BetterWright runtime is unavailable. Reinstall Cos or install BetterWright 1.6.3."
        case .failed(let detail):
            detail
        }
    }
}

/// Host-side bridge to the pinned BetterWright CLI. Release bundles carry
/// their own Node runtime and npm package; source builds can use a compatible
/// global installation for development.
public enum CosBetterWrightRuntime {
    public static let packageVersion = "1.6.3"
    public static let profile = "cos"
    public static let viewerViewportWidth = 900
    public static let viewerViewportHeight = 900

    public static func operatorGuidance() -> String {
        let adapter = """
        Cos uses the official BetterWright browser workflow through native Cos tools.
        Host adapter (these rules override CLI-specific wording in the packaged skill):
        - Every run() or betterwright run instruction means call browser with async JavaScript in code and a short present-tense note.
        - Use browser_open for straightforward navigation and browser_inspect for a safe current-page read with proof.
        - The host captures a BetterWright proof screenshot after successful and failed browser calls and keeps the task session open.
        - Repetitive same-pattern work must be batched in one bounded loop of at most 10 items, re-locating and verifying each item after mutation.
        """
        guard let skillURL = packagedSkillURL(),
              let data = try? Data(contentsOf: skillURL, options: [.mappedIfSafe]) else { return adapter }
        return adapter + "\n\n" + String(decoding: data.prefix(30_000), as: UTF8.self)
    }

    public static func invocation(arguments: [String]) throws -> BetterWrightInvocation {
        var environment = ProcessInfo.processInfo.environment
        let commonPath = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .joined(separator: ":")
        environment["PATH"] = commonPath + ":" + (environment["PATH"] ?? "")
        environment["NODE_NO_WARNINGS"] = "1"

        if let resources = Bundle.main.resourceURL {
            let root = resources.appendingPathComponent("BetterWright", isDirectory: true)
            let node = root.appendingPathComponent("runtime/node")
            let cli = root.appendingPathComponent("package/node_modules/betterwright/dist/bin/betterwright.js")
            if FileManager.default.isExecutableFile(atPath: node.path),
               FileManager.default.fileExists(atPath: cli.path) {
                return .init(executableURL: node, arguments: [cli.path] + arguments, environment: environment)
            }
        }

        if let override = environment["COS_BETTERWRIGHT_EXECUTABLE"], !override.isEmpty,
           FileManager.default.isExecutableFile(atPath: override) {
            return .init(executableURL: URL(fileURLWithPath: override), arguments: arguments, environment: environment)
        }
        for path in ["/opt/homebrew/bin/betterwright", "/usr/local/bin/betterwright"] {
            if FileManager.default.isExecutableFile(atPath: path) {
                return .init(executableURL: URL(fileURLWithPath: path), arguments: arguments, environment: environment)
            }
        }
        throw BetterWrightRuntimeError.unavailable
    }

    public static func doctor() async throws -> BetterWrightCommandResult {
        try await run(arguments: ["doctor", "--json"], maximumBytes: 2_000_000)
    }

    public static func isReady() async -> Bool {
        guard let result = try? await doctor(), result.status == 0,
              let data = result.output.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return false }
        return object["ready"] as? Bool == true
    }

    public static func setup() async throws -> BetterWrightCommandResult {
        try await run(arguments: ["setup"], maximumBytes: 4_000_000)
    }

    public static func runBrowser(code: String, session: String) async throws -> String {
        guard code.utf8.count <= 64_000 else {
            throw BetterWrightRuntimeError.failed("Browser code exceeded Cos’s 64 KB limit.")
        }
        // BetterWright intentionally freezes its `human` API. Instrument calls in
        // the submitted program instead of mutating that object or its prototypes.
        let instrumentedCode = code
            .replacingOccurrences(of: "human.click(", with: "__cosHumanClick(")
            .replacingOccurrences(of: "human.type(", with: "__cosHumanType(")
        let preparedCode = """
        await page.setViewportSize({ width: \(viewerViewportWidth), height: \(viewerViewportHeight) });
        await (async () => {
          const installCursor = () => {
            if (document.getElementById('cos-agent-cursor')) return;
            const cursor = document.createElement('div');
            cursor.id = 'cos-agent-cursor';
            cursor.setAttribute('aria-hidden', 'true');
            cursor.style.cssText = [
              'position:fixed', 'left:0', 'top:0', 'width:20px', 'height:24px',
              'pointer-events:none', 'z-index:2147483647', 'opacity:.96',
              'transform:translate(18px,18px)', 'transition:transform 160ms cubic-bezier(.2,.8,.2,1)',
              'filter:drop-shadow(0 2px 5px rgba(0,0,0,.5))'
            ].join(';');
            cursor.innerHTML = '<svg viewBox="0 0 20 24" width="20" height="24" xmlns="http://www.w3.org/2000/svg"><path d="M2 1.8v17.1l4.6-4.2 3.3 7.1 3.2-1.5-3.2-6.9h6.5L2 1.8Z" fill="#ff7a18" stroke="white" stroke-width="1.4" stroke-linejoin="round"/></svg>';
            document.documentElement.appendChild(cursor);
          };
          await page.addInitScript(installCursor);
          await page.evaluate(installCursor);

          globalThis[Symbol.for('codes.ssh.cos.moveAgentCursor')] = async locator => {
            try {
              const box = await locator.boundingBox();
              if (!box) return;
              const point = { x: Math.round(box.x + box.width / 2), y: Math.round(box.y + box.height / 2) };
              await page.evaluate(({ x, y }) => {
                const cursor = document.getElementById('cos-agent-cursor');
                if (!cursor) return;
                cursor.style.transform = 'translate(' + x + 'px,' + y + 'px)';
                cursor.animate([{ opacity: .55 }, { opacity: 1 }], { duration: 180, easing: 'ease-out' });
              }, point);
              await page.waitForTimeout(170);
            } catch {}
          };

        })();
        const __cosHumanClick = async (target, ...args) => {
          if (target && typeof target.boundingBox === 'function') {
            await globalThis[Symbol.for('codes.ssh.cos.moveAgentCursor')](target);
          }
          return await human.click(target, ...args);
        };
        const __cosHumanType = async (target, text, ...args) => {
          if (target && typeof target.boundingBox === 'function') {
            await globalThis[Symbol.for('codes.ssh.cos.moveAgentCursor')](target);
          }
          return await human.type(target, text, ...args);
        };
        \(instrumentedCode)
        """
        let result = try await run(
            arguments: [
                "run",
                "-c", preparedCode,
                "--session", sanitizedSession(session),
                "--profile", profile,
            ],
            maximumBytes: 2_000_000
        )
        let output = result.output.trimmingCharacters(in: .whitespacesAndNewlines)
        guard result.status == 0 else {
            let detail = [result.errorOutput, result.output]
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { !$0.isEmpty }
                .joined(separator: "\n")
            throw BetterWrightRuntimeError.failed(
                detail.isEmpty ? "BetterWright browser action failed (exit \(result.status))." : String(detail.suffix(8_000))
            )
        }
        return output
    }

    /// Mirrors the production Pro AI adapter: browser code returns a structured
    /// observation, failures remain recoverable, and every state gets a proof
    /// screenshot from BetterWright's guarded helper.
    public static func runBrowserWithProof(
        code: String,
        session: String,
        note: String?
    ) async throws -> String {
        let actionSucceeded: Bool
        let action: Any
        do {
            let output = try await runBrowser(code: code, session: session)
            actionSucceeded = true
            action = jsonValue(output)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            actionSucceeded = false
            action = ["ok": false, "error": error.localizedDescription]
        }

        let proofName = actionSucceeded ? "cos-browser-state" : "cos-browser-failure-state"
        let proof: Any?
        do {
            let output = try await runBrowser(
                code: "return await screenshot({kind: 'proof', name: '\(proofName)'})",
                session: session
            )
            proof = jsonValue(output)
        } catch {
            proof = ["ok": false, "error": error.localizedDescription]
        }

        var envelope: [String: Any] = [
            "ok": actionSucceeded,
            "browser": action,
            "proof": proof ?? NSNull(),
        ]
        if let note, !note.isEmpty { envelope["note"] = String(note.prefix(240)) }
        let data = try JSONSerialization.data(withJSONObject: envelope, options: [.prettyPrinted, .sortedKeys])
        return String(decoding: data, as: UTF8.self)
    }

    public static func prepareForViewing(session: String) async throws {
        _ = try await runBrowser(
            code: "return { viewport: page.viewportSize(), url: page.url() }",
            session: session
        )
    }

    public static func run(
        arguments: [String],
        maximumBytes: Int = 2_000_000
    ) async throws -> BetterWrightCommandResult {
        let invocation = try invocation(arguments: arguments)
        return try await Task.detached(priority: .userInitiated) {
            let process = Process()
            process.executableURL = invocation.executableURL
            process.arguments = invocation.arguments
            process.environment = invocation.environment
            let outputPipe = Pipe()
            let errorPipe = Pipe()
            process.standardOutput = outputPipe
            process.standardError = errorPipe
            try process.run()
            let outputTask = Task.detached { try outputPipe.fileHandleForReading.readToEnd() ?? Data() }
            let errorTask = Task.detached { try errorPipe.fileHandleForReading.readToEnd() ?? Data() }
            process.waitUntilExit()
            let output = try await outputTask.value
            let error = try await errorTask.value
            return .init(
                status: process.terminationStatus,
                output: String(decoding: output.prefix(maximumBytes), as: UTF8.self),
                errorOutput: String(decoding: error.prefix(maximumBytes), as: UTF8.self)
            )
        }.value
    }

    public static func sanitizedSession(_ value: String) -> String {
        let normalized = value.lowercased().map { character in
            character.isLetter || character.isNumber || character == "-" ? character : "-"
        }
        let compact = String(normalized).replacingOccurrences(of: "-+", with: "-", options: .regularExpression)
        let trimmed = compact.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return String((trimmed.isEmpty ? "default" : trimmed).prefix(80))
    }

    private static func jsonValue(_ output: String) -> Any {
        guard let data = output.data(using: .utf8),
              let value = try? JSONSerialization.jsonObject(with: data) else { return output }
        return value
    }

    private static func packagedSkillURL() -> URL? {
        if let resources = Bundle.main.resourceURL {
            let bundled = resources
                .appendingPathComponent("BetterWright/package/node_modules/betterwright/SKILL.md")
            if FileManager.default.fileExists(atPath: bundled.path) { return bundled }
        }
        for path in [
            "/opt/homebrew/lib/node_modules/betterwright/SKILL.md",
            "/usr/local/lib/node_modules/betterwright/SKILL.md",
        ] where FileManager.default.fileExists(atPath: path) {
            return URL(fileURLWithPath: path)
        }
        return nil
    }
}
