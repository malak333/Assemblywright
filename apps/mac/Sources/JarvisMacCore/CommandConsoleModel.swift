import Foundation

@MainActor
public final class CommandConsoleModel: ObservableObject {
    @Published public private(set) var health: JarvisHealth?
    @Published public private(set) var transcript: [TranscriptEntry]
    @Published public private(set) var isPaused: Bool
    @Published public private(set) var isWorking: Bool
    @Published public private(set) var lastError: String?

    private let client: JarvisIPCClient

    public init(client: JarvisIPCClient = JarvisIPCClient()) {
        self.client = client
        self.transcript = []
        self.isPaused = false
        self.isWorking = false
        self.lastError = nil
    }

    public func refreshHealth() async {
        await run {
            let health = try await self.client.health()
            self.health = health
            self.isPaused = health.emergencyPaused
        }
    }

    public func submit(input: String) async {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        transcript.append(TranscriptEntry(role: .user, text: trimmed))
        await run {
            let response = try await self.client.submit(JarvisCommandRequest(input: trimmed))
            self.isPaused = response.task.status == "blocked" ? true : self.isPaused
            self.transcript.append(
                TranscriptEntry(role: .assistant, text: response.message)
            )
        }
    }

    public func pause() async {
        await run {
            let response = try await self.client.pause(reason: "user requested from Mac shell")
            self.isPaused = response.paused
        }
    }

    public func resume() async {
        await run {
            let response = try await self.client.resume()
            self.isPaused = response.paused
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isWorking = true
        lastError = nil
        defer { isWorking = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

public struct TranscriptEntry: Identifiable, Equatable, Sendable {
    public enum Role: String, Equatable, Sendable {
        case user
        case assistant
    }

    public var id: UUID
    public var role: Role
    public var text: String

    public init(id: UUID = UUID(), role: Role, text: String) {
        self.id = id
        self.role = role
        self.text = text
    }
}
