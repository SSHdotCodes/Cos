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
        let preparedCode = """
        await page.setViewportSize({ width: \(viewerViewportWidth), height: \(viewerViewportHeight) });
        \(code)
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
            let detail = result.errorOutput.trimmingCharacters(in: .whitespacesAndNewlines)
            throw BetterWrightRuntimeError.failed(detail.isEmpty ? "BetterWright browser action failed." : detail)
        }
        return output
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
}
