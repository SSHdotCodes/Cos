import CryptoKit
import Foundation

public struct CosUpdateManifest: Codable, Equatable, Sendable {
    public var version: String
    public var build: Int
    public var downloadURL: URL
    public var sha256: String
    public var minimumSystemVersion: String
    public var releaseNotes: String

    public init(
        version: String,
        build: Int,
        downloadURL: URL,
        sha256: String,
        minimumSystemVersion: String,
        releaseNotes: String
    ) {
        self.version = version
        self.build = build
        self.downloadURL = downloadURL
        self.sha256 = sha256
        self.minimumSystemVersion = minimumSystemVersion
        self.releaseNotes = releaseNotes
    }
}

public struct PreparedCosUpdate: Sendable {
    public let appURL: URL
    public let workingDirectory: URL
    public let manifest: CosUpdateManifest
}

public struct CosUpdateService: Sendable {
    public static let defaultFeedURL = URL(string: "https://cos.ssh.codes/api/update")!
    private let feedURL: URL
    private let session: URLSession

    public init(feedURL: URL = Self.defaultFeedURL, session: URLSession = .shared) {
        self.feedURL = feedURL
        self.session = session
    }

    public func check(currentVersion: String, currentBuild: Int = 0) async throws -> CosUpdateManifest? {
        var request = URLRequest(url: feedURL, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: 15)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await session.data(for: request)
        try Self.validateHTTP(response, maximumBytes: 64_000, receivedBytes: data.count)
        let manifest = try JSONDecoder().decode(CosUpdateManifest.self, from: data)
        try Self.validate(manifest)
        let versionIsNewer = Self.isNewer(manifest.version, than: currentVersion)
        let versionMatches = !versionIsNewer && !Self.isNewer(currentVersion, than: manifest.version)
        guard versionIsNewer || (versionMatches && manifest.build > currentBuild) else { return nil }
        guard Self.isSystemVersionSupported(manifest.minimumSystemVersion) else {
            throw CosUpdateError.unsupportedSystem(manifest.minimumSystemVersion)
        }
        return manifest
    }

    public func downloadAndVerify(_ manifest: CosUpdateManifest) async throws -> PreparedCosUpdate {
        try Self.validate(manifest)
        var request = URLRequest(url: manifest.downloadURL, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: 120)
        request.setValue("application/zip", forHTTPHeaderField: "Accept")
        let (temporaryDownload, response) = try await session.download(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw CosUpdateError.http(http.statusCode)
        }

        let manager = FileManager.default
        let workingDirectory = manager.temporaryDirectory.appendingPathComponent("CosUpdate-\(UUID().uuidString)", isDirectory: true)
        let archiveURL = workingDirectory.appendingPathComponent("Cos.zip")
        let unpackedURL = workingDirectory.appendingPathComponent("Unpacked", isDirectory: true)
        try manager.createDirectory(at: unpackedURL, withIntermediateDirectories: true)
        do {
            try manager.moveItem(at: temporaryDownload, to: archiveURL)
            let size = try archiveURL.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
            guard size > 0, size <= 250_000_000 else { throw CosUpdateError.archiveTooLarge }
            let actualHash = try Self.sha256(of: archiveURL)
            guard actualHash.caseInsensitiveCompare(manifest.sha256) == .orderedSame else {
                throw CosUpdateError.hashMismatch
            }
            try Self.run("/usr/bin/ditto", arguments: ["-x", "-k", archiveURL.path, unpackedURL.path])
            let appURL = try Self.findCosApp(in: unpackedURL)
            try Self.validateBundle(at: appURL, manifest: manifest)
            return .init(appURL: appURL, workingDirectory: workingDirectory, manifest: manifest)
        } catch {
            try? manager.removeItem(at: workingDirectory)
            throw error
        }
    }

    /// Stages the verified app beside the running bundle and starts a detached
    /// helper that swaps it only after the current process exits. The helper
    /// restores the previous bundle if the replacement cannot launch.
    public func validateInstallLocation(_ currentAppURL: URL) throws {
        let manager = FileManager.default
        let current = currentAppURL.standardizedFileURL
        guard current.pathExtension == "app", manager.fileExists(atPath: current.path) else {
            throw CosUpdateError.notRunningFromApp
        }
        guard !current.path.hasPrefix("/Volumes/") else { throw CosUpdateError.runningFromDiskImage }
        let parent = current.deletingLastPathComponent()
        guard manager.isWritableFile(atPath: parent.path) else {
            throw CosUpdateError.installLocationNotWritable(parent.path)
        }
    }

    public func scheduleReplacement(
        prepared: PreparedCosUpdate,
        currentAppURL: URL,
        processID: Int32
    ) throws {
        let manager = FileManager.default
        let current = currentAppURL.standardizedFileURL
        try validateInstallLocation(current)
        let parent = current.deletingLastPathComponent()

        let identifier = UUID().uuidString
        let staged = parent.appendingPathComponent(".Cos-update-\(identifier).app", isDirectory: true)
        let backup = parent.appendingPathComponent(".Cos-previous-\(identifier).app", isDirectory: true)
        try manager.copyItem(at: prepared.appURL, to: staged)
        do {
            try Self.validateBundle(at: staged, manifest: prepared.manifest)
        } catch {
            try? manager.removeItem(at: staged)
            throw error
        }

        let script = """
        set -u
        current="$1"
        staged="$2"
        backup="$3"
        old_pid="$4"
        cleanup="$5"

        while /bin/kill -0 "$old_pid" 2>/dev/null; do /bin/sleep 0.15; done
        if ! /bin/mv "$current" "$backup"; then exit 20; fi
        if /bin/mv "$staged" "$current"; then
          /usr/bin/nohup "$current/Contents/MacOS/Cos" >/dev/null 2>&1 &
          new_pid=$!
          /bin/sleep 3
          if /bin/kill -0 "$new_pid" 2>/dev/null; then
            /bin/rm -rf "$backup" "$cleanup"
            exit 0
          fi
        fi
        /bin/rm -rf "$current"
        /bin/mv "$backup" "$current"
        /usr/bin/open -n "$current"
        /bin/rm -rf "$staged" "$cleanup"
        exit 21
        """

        let helper = Process()
        helper.executableURL = URL(fileURLWithPath: "/bin/zsh")
        helper.arguments = [
            "-c", script, "cos-updater",
            current.path, staged.path, backup.path,
            String(processID), prepared.workingDirectory.path,
        ]
        helper.standardOutput = FileHandle.nullDevice
        helper.standardError = FileHandle.nullDevice
        do {
            try helper.run()
        } catch {
            try? manager.removeItem(at: staged)
            throw CosUpdateError.couldNotStartInstaller(error.localizedDescription)
        }
    }

    public static func isNewer(_ candidate: String, than current: String) -> Bool {
        let candidateParts = numericVersion(candidate)
        let currentParts = numericVersion(current)
        for index in 0..<max(candidateParts.count, currentParts.count) {
            let lhs = index < candidateParts.count ? candidateParts[index] : 0
            let rhs = index < currentParts.count ? currentParts[index] : 0
            if lhs != rhs { return lhs > rhs }
        }
        return false
    }

    private static func numericVersion(_ value: String) -> [Int] {
        value.split(separator: ".").map { component in
            Int(component.prefix { $0.isNumber }) ?? 0
        }
    }

    private static func validate(_ manifest: CosUpdateManifest) throws {
        guard manifest.downloadURL.scheme == "https", manifest.downloadURL.host == "cos.ssh.codes" else {
            throw CosUpdateError.untrustedDownloadHost
        }
        guard manifest.version.range(of: "^[0-9]+(\\.[0-9]+){1,3}$", options: .regularExpression) != nil,
              manifest.build > 0,
              manifest.sha256.range(of: "^[a-fA-F0-9]{64}$", options: .regularExpression) != nil else {
            throw CosUpdateError.invalidManifest
        }
    }

    private static func validateHTTP(_ response: URLResponse, maximumBytes: Int, receivedBytes: Int) throws {
        guard let http = response as? HTTPURLResponse else { throw CosUpdateError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else { throw CosUpdateError.http(http.statusCode) }
        guard receivedBytes <= maximumBytes else { throw CosUpdateError.invalidManifest }
    }

    private static func isSystemVersionSupported(_ minimum: String) -> Bool {
        let version = ProcessInfo.processInfo.operatingSystemVersion
        let current = "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)"
        return !isNewer(minimum, than: current)
    }

    private static func sha256(of url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let data = try handle.read(upToCount: 64 * 1024), !data.isEmpty {
            hasher.update(data: data)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func findCosApp(in root: URL) throws -> URL {
        let direct = root.appendingPathComponent("Cos.app", isDirectory: true)
        if FileManager.default.fileExists(atPath: direct.path) { return direct }
        guard let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        ) else { throw CosUpdateError.missingApp }
        for case let url as URL in enumerator where url.lastPathComponent == "Cos.app" { return url }
        throw CosUpdateError.missingApp
    }

    private static func validateBundle(at appURL: URL, manifest: CosUpdateManifest) throws {
        guard let bundle = Bundle(url: appURL),
              bundle.bundleIdentifier == "codes.ssh.cos",
              bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String == manifest.version,
              (bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String).flatMap(Int.init) == manifest.build,
              let executableURL = bundle.executableURL,
              FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw CosUpdateError.invalidBundle
        }
        try run("/usr/bin/codesign", arguments: ["--verify", "--deep", "--strict", appURL.path])
    }

    private static func run(_ executable: String, arguments: [String]) throws {
        let process = Process()
        let output = Pipe()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = output
        process.standardError = output
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            let detail = String(decoding: output.fileHandleForReading.readDataToEndOfFile().prefix(4_000), as: UTF8.self)
            throw CosUpdateError.validationFailed(detail.trimmingCharacters(in: .whitespacesAndNewlines))
        }
    }
}

public enum CosUpdateError: LocalizedError {
    case invalidResponse
    case http(Int)
    case invalidManifest
    case untrustedDownloadHost
    case unsupportedSystem(String)
    case archiveTooLarge
    case hashMismatch
    case missingApp
    case invalidBundle
    case validationFailed(String)
    case notRunningFromApp
    case runningFromDiskImage
    case installLocationNotWritable(String)
    case couldNotStartInstaller(String)

    public var errorDescription: String? {
        switch self {
        case .invalidResponse: "The update server returned an invalid response."
        case .http(let code): "The update server returned HTTP \(code)."
        case .invalidManifest: "The update manifest is malformed."
        case .untrustedDownloadHost: "Cos refused an update from an untrusted download host."
        case .unsupportedSystem(let version): "This update requires macOS \(version) or later."
        case .archiveTooLarge: "The update archive is empty or exceeds 250 MB."
        case .hashMismatch: "The update failed its SHA-256 integrity check."
        case .missingApp: "The update archive does not contain Cos.app."
        case .invalidBundle: "The update is not a valid Cos application bundle."
        case .validationFailed(let detail): "The update’s code signature is invalid. \(detail)"
        case .notRunningFromApp: "Cos must be launched from its application bundle to update itself."
        case .runningFromDiskImage: "Move Cos to Applications, reopen it there, and try the update again."
        case .installLocationNotWritable(let path): "Cos cannot update \(path). Move it to Applications or another writable folder."
        case .couldNotStartInstaller(let detail): "Cos could not start the update installer. \(detail)"
        }
    }
}
