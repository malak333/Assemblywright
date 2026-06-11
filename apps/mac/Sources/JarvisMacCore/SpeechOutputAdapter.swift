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
    var onPhaseChange: (@MainActor (JarvisSpeechOutputPhase) -> Void)? { get set }
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
        self.adapter.onPhaseChange = { [weak self] phase in
            self?.phase = phase
        }
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
            fail(.emptyUtterance, preserving: phase)
            return
        }

        let previousPhase = phase
        switch await adapter.speak(trimmed) {
        case .success:
            lastError = nil
            lastSpokenText = trimmed
            phase = adapter.phase
        case let .failure(error):
            fail(error, preserving: previousPhase)
        }
    }

    public func stop() async {
        let previousPhase = phase
        switch await adapter.stop() {
        case .success:
            lastError = nil
            phase = adapter.phase
        case let .failure(error):
            fail(error, preserving: previousPhase)
        }
    }

    public func interrupt(reason: String) async {
        let previousPhase = phase
        switch await adapter.interrupt(reason: reason) {
        case .success:
            lastError = nil
            phase = adapter.phase
        case let .failure(error):
            fail(error, preserving: previousPhase)
        }
    }

    private func fail(_ error: JarvisSpeechOutputError, preserving previousPhase: JarvisSpeechOutputPhase) {
        lastError = error
        if error.isRecoverableCommandState {
            phase = previousPhase
        } else {
            phase = .unavailable(reason: error.description)
        }
    }
}

private extension JarvisSpeechOutputError {
    var isRecoverableCommandState: Bool {
        switch self {
        case .emptyUtterance, .noActiveSpeech:
            return true
        case .frameworkUnavailable, .playbackUnavailable:
            return false
        }
    }
}

#if canImport(AVFoundation)
@MainActor
public final class MacSpeechOutputAdapter: NSObject, JarvisSpeechOutputAdapter, AVSpeechSynthesizerDelegate {
    public private(set) var phase: JarvisSpeechOutputPhase {
        didSet {
            onPhaseChange?(phase)
        }
    }

    public var onPhaseChange: (@MainActor (JarvisSpeechOutputPhase) -> Void)?

    private let synthesizer: AVSpeechSynthesizer
    private var activeUtteranceID: ObjectIdentifier?

    override public init() {
        self.synthesizer = AVSpeechSynthesizer()
        self.phase = .idle
        self.onPhaseChange = nil
        super.init()
        self.synthesizer.delegate = self
    }

    public func speak(_ text: String) async -> Result<Void, JarvisSpeechOutputError> {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return .failure(.emptyUtterance)
        }

        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }

        let utterance = AVSpeechUtterance(string: trimmed)
        utterance.voice = AVSpeechSynthesisVoice(language: Locale.current.identifier)
        beginSpeechTracking(for: utterance)
        synthesizer.speak(utterance)
        return .success(())
    }

    public func stop() async -> Result<Void, JarvisSpeechOutputError> {
        guard synthesizer.isSpeaking || phase == .speaking else {
            phase = .idle
            return .success(())
        }

        synthesizer.stopSpeaking(at: .word)
        activeUtteranceID = nil
        phase = .idle
        return .success(())
    }

    public func interrupt(reason: String) async -> Result<Void, JarvisSpeechOutputError> {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
        activeUtteranceID = nil
        phase = .interrupted(reason: reason)
        return .success(())
    }

    nonisolated public func speechSynthesizer(
        _: AVSpeechSynthesizer,
        didFinish utterance: AVSpeechUtterance
    ) {
        let utteranceID = ObjectIdentifier(utterance)
        Task { @MainActor [weak self] in
            self?.markSpeechCompleted(for: utteranceID)
        }
    }

    nonisolated public func speechSynthesizer(
        _: AVSpeechSynthesizer,
        didCancel utterance: AVSpeechUtterance
    ) {
        let utteranceID = ObjectIdentifier(utterance)
        Task { @MainActor [weak self] in
            self?.markSpeechCompleted(for: utteranceID)
        }
    }

    func beginSpeechTracking(for utterance: AVSpeechUtterance) {
        activeUtteranceID = ObjectIdentifier(utterance)
        phase = .speaking
    }

    func markSpeechCompleted(for utterance: AVSpeechUtterance) {
        markSpeechCompleted(for: ObjectIdentifier(utterance))
    }

    private func markSpeechCompleted(for utteranceID: ObjectIdentifier) {
        guard activeUtteranceID == utteranceID else { return }
        activeUtteranceID = nil
        guard phase == .speaking else { return }
        phase = .idle
    }
}
#else
@MainActor
public final class MacSpeechOutputAdapter: JarvisSpeechOutputAdapter {
    public private(set) var phase: JarvisSpeechOutputPhase = .unavailable(
        reason: JarvisSpeechOutputError.frameworkUnavailable("AVFoundation is not available in this build.").description
    )
    public var onPhaseChange: (@MainActor (JarvisSpeechOutputPhase) -> Void)?

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
