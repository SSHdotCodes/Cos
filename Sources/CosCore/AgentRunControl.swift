import Foundation

public struct SteeringMessage: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var content: String

    public init(id: UUID = UUID(), content: String) {
        self.id = id
        self.content = content
    }
}

/// A per-run control plane for low-overhead steering. Messages are queued while
/// a native tool is active and interrupt only the current provider request.
public actor AgentRunControl {
    private let maximumQueuedMessages: Int
    private var queued: [SteeringMessage] = []
    private var providerInterrupt: (token: UUID, action: @Sendable () -> Void)?

    public init(maximumQueuedMessages: Int = 16) {
        self.maximumQueuedMessages = max(1, maximumQueuedMessages)
    }

    @discardableResult
    public func submit(_ rawMessage: String) -> Bool {
        let message = rawMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !message.isEmpty, queued.count < maximumQueuedMessages else { return false }
        queued.append(.init(content: message))
        providerInterrupt?.action()
        return true
    }

    public func drain() -> [SteeringMessage] {
        let messages = queued
        queued.removeAll(keepingCapacity: true)
        return messages
    }

    public func installProviderInterrupt(token: UUID, action: @escaping @Sendable () -> Void) {
        providerInterrupt = (token, action)
        if !queued.isEmpty { action() }
    }

    public func clearProviderInterrupt(token: UUID) {
        guard providerInterrupt?.token == token else { return }
        providerInterrupt = nil
    }
}
