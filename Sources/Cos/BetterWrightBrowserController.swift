import Combine
import CosCore
import Foundation

@MainActor
final class BetterWrightBrowserController: ObservableObject {
    enum Phase: Equatable {
        case idle
        case checking
        case setupRequired
        case installing
        case launching
        case ready(URL)
        case failed(String)
    }

    @Published private(set) var phase: Phase = .idle

    private var session = "default"
    private var operationTask: Task<Void, Never>?
    private var timeoutTask: Task<Void, Never>?
    private var viewerProcess: Process?
    private var viewerPipe: Pipe?
    private var outputBuffer = ""
    private var launchToken = UUID()

    func open(session rawSession: String) {
        let requestedSession = CosBetterWrightRuntime.sanitizedSession(rawSession)
        if requestedSession == session, case .ready = phase, viewerProcess?.isRunning == true { return }
        session = requestedSession
        beginReadinessCheck()
    }

    func installAndOpen() {
        operationTask?.cancel()
        stopViewer(resetPhase: false)
        phase = .installing
        operationTask = Task { [weak self] in
            do {
                let result = try await CosBetterWrightRuntime.setup()
                try Task.checkCancellation()
                guard result.status == 0 else {
                    let detail = [result.errorOutput, result.output]
                        .joined(separator: "\n")
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                    throw BetterWrightRuntimeError.failed(detail.isEmpty ? "BetterWright setup failed." : detail)
                }
                guard await CosBetterWrightRuntime.isReady() else {
                    throw BetterWrightRuntimeError.failed("BetterWright installed its browser, but the readiness check did not pass.")
                }
                guard let self else { return }
                try await CosBetterWrightRuntime.prepareForViewing(session: session)
                try Task.checkCancellation()
                launchViewer()
            } catch is CancellationError {
                return
            } catch {
                self?.phase = .failed(error.localizedDescription)
            }
        }
    }

    func retry() {
        beginReadinessCheck()
    }

    func webViewFailed(_ message: String) {
        stopViewer(resetPhase: false)
        phase = .failed(message)
    }

    func restartViewer() {
        guard case .ready = phase else {
            beginReadinessCheck()
            return
        }
        stopViewer(resetPhase: false)
        launchViewer()
    }

    func closeActiveTab() {
        let currentSession = session
        operationTask?.cancel()
        operationTask = Task { [weak self] in
            do {
                _ = try await CosBetterWrightRuntime.runBrowser(
                    code: "await closePage(); return 'closed'",
                    session: currentSession
                )
                try await CosBetterWrightRuntime.prepareForViewing(session: currentSession)
            } catch is CancellationError {
                return
            } catch {
                self?.phase = .failed("Could not close the browser tab: \(error.localizedDescription)")
            }
        }
    }

    func close() {
        operationTask?.cancel()
        operationTask = nil
        stopViewer(resetPhase: true)
    }

    private func beginReadinessCheck() {
        operationTask?.cancel()
        stopViewer(resetPhase: false)
        phase = .checking
        operationTask = Task { [weak self] in
            let ready = await CosBetterWrightRuntime.isReady()
            guard !Task.isCancelled, let self else { return }
            if ready {
                do {
                    try await CosBetterWrightRuntime.prepareForViewing(session: session)
                    try Task.checkCancellation()
                    launchViewer()
                } catch is CancellationError {
                    return
                } catch {
                    phase = .failed(error.localizedDescription)
                }
            } else {
                phase = .setupRequired
            }
        }
    }

    private func launchViewer() {
        do {
            stopViewer(resetPhase: false)
            phase = .launching
            outputBuffer = ""
            let token = UUID()
            launchToken = token
            let invocation = try CosBetterWrightRuntime.invocation(arguments: [
                "view",
                "--expose", "local",
                "--session", session,
                "--profile", CosBetterWrightRuntime.profile,
            ])
            let process = Process()
            let pipe = Pipe()
            process.executableURL = invocation.executableURL
            process.arguments = invocation.arguments
            process.environment = invocation.environment
            process.standardOutput = pipe
            process.standardError = pipe
            pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
                let data = handle.availableData
                guard !data.isEmpty else { return }
                let chunk = String(decoding: data, as: UTF8.self)
                Task { @MainActor in self?.consume(chunk, token: token) }
            }
            process.terminationHandler = { [weak self] process in
                Task { @MainActor in self?.viewerDidTerminate(process, token: token) }
            }
            try process.run()
            viewerProcess = process
            viewerPipe = pipe
            timeoutTask?.cancel()
            timeoutTask = Task { [weak self] in
                try? await Task.sleep(for: .seconds(20))
                guard !Task.isCancelled, let self, launchToken == token, case .launching = phase else { return }
                stopViewer(resetPhase: false)
                phase = .failed("BetterWright started, but its local live view did not become ready.")
            }
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    private func consume(_ chunk: String, token: UUID) {
        guard token == launchToken else { return }
        outputBuffer += chunk
        if outputBuffer.utf8.count > 64_000 {
            outputBuffer = String(outputBuffer.suffix(48_000))
        }
        let clean = outputBuffer.replacingOccurrences(
            of: "\u{001B}\\[[0-9;]*m",
            with: "",
            options: .regularExpression
        )
        guard let label = clean.range(of: "Live view:") else { return }
        let remainder = clean[label.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
        guard let candidate = remainder.split(whereSeparator: { $0.isWhitespace }).first,
              let url = URL(string: String(candidate)),
              url.scheme == "http",
              ["127.0.0.1", "localhost", "::1"].contains(url.host?.lowercased() ?? "") else { return }
        timeoutTask?.cancel()
        phase = .ready(url)
    }

    private func viewerDidTerminate(_ process: Process, token: UUID) {
        guard token == launchToken else { return }
        viewerPipe?.fileHandleForReading.readabilityHandler = nil
        viewerPipe = nil
        viewerProcess = nil
        timeoutTask?.cancel()
        if case .ready = phase {
            phase = .failed("The BetterWright live view ended unexpectedly (exit \(process.terminationStatus)).")
        } else if case .launching = phase {
            let detail = outputBuffer.trimmingCharacters(in: .whitespacesAndNewlines)
            phase = .failed(detail.isEmpty ? "BetterWright could not start its live view." : String(detail.suffix(2_000)))
        }
    }

    private func stopViewer(resetPhase: Bool) {
        timeoutTask?.cancel()
        timeoutTask = nil
        launchToken = UUID()
        viewerPipe?.fileHandleForReading.readabilityHandler = nil
        viewerPipe = nil
        if let process = viewerProcess, process.isRunning {
            process.terminate()
        }
        viewerProcess = nil
        outputBuffer = ""
        if resetPhase { phase = .idle }
    }
}
