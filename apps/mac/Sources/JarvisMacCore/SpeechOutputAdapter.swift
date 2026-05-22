import Foundation

#if canImport(AVFoundation)
import AVFoundation
#endif

public enum JarvisSpeechOutputPhase: Equatable, Sendable {
    case idle
    case speaking
    case interrupted(reason: String)
    case degraded(reason: String)
    case unavailable(reason: String)
}

public enum JarvisSpeechOutputError: Error, Equatable, Sendable, CustomStringConvertible {
    case frameworkUnavailable(String)
    case emptyUtterance
    case playbackUnavailable(String)
    case noActiveSpeech

    public var description: String {
        switch self {
        case let .frameworkUnavailable(reason):
            return "Speech output framework unavailable: \(reason)"
        case .emptyUtterance:
            return "Speech output requires non-empty text."
        case let .playbackUnavailable(reason):
            return "Speech output playback unavailable: \(reason)"
        case .noActiveSpeech:
            return "No active speech output is available."
        }
    }
}

@MainActor
public protocol JarvisSpeechOutputAdapter: AnyObject {
    var phase: JarvisSpeechOutputPhase { get }
    func speak(_ text: String) async -> Result<Void, JarvisSpeechOutputError>
    func stop() async -> Result<Void, JarvisSpeechOutputError>
    func interrupt(reason: String) async -> Result<Void, JarvisSpeechOutputError>
}

@MainActor
public final class SpeechOutputStateModel: ObservableObject {
    @Published public private(set) var phase: JarvisSpeechOutputPhase
    @Published public private(set) var lastError: JarvisSpeechOutputError?
    @Published public private(set) var lastSpokenText: String?

    private let adapter: any JarvisSpeechOutputAdapter

    public init(adapter: any JarvisSpeechOutputAdapter) {
        self.adapter = adapter
        self.phase = adapter.phase
        self.lastError = nil
        self.lastSpokenText = nil
    }

    public var statusText: String {
        switch phase {
        case .idle:
            return "Speech output idle."
        case .speaking:
            return "Speech output speaking."
        case let .interrupted(reason):
            return "Speech output interrupted: \(reason)"
        case let .degraded(reason):
            return "Speech output degraded: \(reason)"
        case let .unavailable(reason):
            return "Speech output unavailable: \(reason)"
        }
    }

    public var isSpeaking: Bool {
        phase == .speaking
    }

    public var canSpeak: Bool {
        switch phase {
        case .idle, .degraded, .interrupted:
            return true
        case .speaking, .unavailable:
            return false
        }
    }

    public func speak(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            fail(.emptyUtterance)
            return
        }

        switch await adapter.speak(trimmed) {
        case .success:
            lastError = nil
            lastSpokenText = trimmed
            phase = adapter.phase
        case let .failure(error):
            fail(error)
        }
    }

    public func stop() async {
        switch await adapter.stop() {
        case .success:
            lastError = nil
            phase = adapter.phase
        case let .failure(error):
            fail(error)
        }
    }

    public func interrupt(reason: String) async {
        switch await adapter.interrupt(reason: reason) {
        case .success:
            lastError = nil
            phase = adapter.phase
        case let .failure(error):
            fail(error)
        }
    }

    private func fail(_ error: JarvisSpeechOutputError) {
        lastError = error
        phase = .unavailable(reason: error.description)
    }
}

#if canImport(AVFoundation)
@MainActor
public final class MacSpeechOutputAdapter: JarvisSpeechOutputAdapter {
    public private(set) var phase: JarvisSpeechOutputPhase

    private let synthesizer: AVSpeechSynthesizer

    public init() {
        self.synthesizer = AVSpeechSynthesizer()
        self.phase = .idle
    }

    public func speak(_ text: String) async -> Result<Void, JarvisSpeechOutputError> {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            phase = .unavailable(reason: JarvisSpeechOutputError.emptyUtterance.description)
            return .failure(.emptyUtterance)
        }

        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }

        let utterance = AVSpeechUtterance(string: trimmed)
        utterance.voice = AVSpeechSynthesisVoice(language: Locale.current.identifier)
        synthesizer.speak(utterance)
        phase = .speaking
        return .success(())
    }

    public func stop() async -> Result<Void, JarvisSpeechOutputError> {
        guard synthesizer.isSpeaking || phase == .speaking else {
            phase = .idle
            return .success(())
        }

        synthesizer.stopSpeaking(at: .word)
        phase = .idle
        return .success(())
    }

    public func interrupt(reason: String) async -> Result<Void, JarvisSpeechOutputError> {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
        phase = .interrupted(reason: reason)
        return .success(())
    }
}
#else
@MainActor
public final class MacSpeechOutputAdapter: JarvisSpeechOutputAdapter {
    public private(set) var phase: JarvisSpeechOutputPhase = .unavailable(
        reason: JarvisSpeechOutputError.frameworkUnavailable("AVFoundation is not available in this build.").description
    )

    public init() {}

    public func speak(_: String) async -> Result<Void, JarvisSpeechOutputError> {
        .failure(.frameworkUnavailable("AVFoundation is not available in this build."))
    }

    public func stop() async -> Result<Void, JarvisSpeechOutputError> {
        .failure(.frameworkUnavailable("AVFoundation is not available in this build."))
    }

    public func interrupt(reason _: String) async -> Result<Void, JarvisSpeechOutputError> {
        .failure(.frameworkUnavailable("AVFoundation is not available in this build."))
    }
}
#endif
