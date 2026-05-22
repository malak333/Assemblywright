import Foundation

#if canImport(AVFoundation)
import AVFoundation
#endif

#if canImport(Speech)
import Speech
#endif

public enum JarvisVoiceAdapterPhase: Equatable, Sendable {
    case idle
    case requestingPermission
    case listening
    case transcribing
    case interrupted(reason: String)
    case degraded(reason: String)
    case unavailable(reason: String)
}

public enum JarvisVoiceAdapterError: Error, Equatable, Sendable, CustomStringConvertible {
    case frameworkUnavailable(String)
    case permissionDenied(String)
    case permissionRestricted(String)
    case speechRecognizerUnavailable
    case alreadyCapturing
    case noActiveCapture
    case captureStartFailed(String)
    case recognitionFailed(String)

    public var description: String {
        switch self {
        case let .frameworkUnavailable(reason):
            return "Voice framework unavailable: \(reason)"
        case let .permissionDenied(reason):
            return "Voice permission denied: \(reason)"
        case let .permissionRestricted(reason):
            return "Voice permission restricted: \(reason)"
        case .speechRecognizerUnavailable:
            return "Speech recognizer is unavailable for the configured locale."
        case .alreadyCapturing:
            return "Voice capture is already active."
        case .noActiveCapture:
            return "No active voice capture is available."
        case let .captureStartFailed(reason):
            return "Voice capture failed to start: \(reason)"
        case let .recognitionFailed(reason):
            return "Speech recognition failed: \(reason)"
        }
    }
}

public struct JarvisVoiceCaptureCallbacks: Sendable {
    public var onPartialTranscript: @MainActor @Sendable (String) -> Void
    public var onFinalTranscript: @MainActor @Sendable (String) -> Void
    public var onError: @MainActor @Sendable (JarvisVoiceAdapterError) -> Void

    public init(
        onPartialTranscript: @escaping @MainActor @Sendable (String) -> Void,
        onFinalTranscript: @escaping @MainActor @Sendable (String) -> Void,
        onError: @escaping @MainActor @Sendable (JarvisVoiceAdapterError) -> Void
    ) {
        self.onPartialTranscript = onPartialTranscript
        self.onFinalTranscript = onFinalTranscript
        self.onError = onError
    }
}

@MainActor
public protocol JarvisVoiceAdapter: AnyObject {
    var phase: JarvisVoiceAdapterPhase { get }
    func requestPermissions() async -> Result<Void, JarvisVoiceAdapterError>
    func startCapture(callbacks: JarvisVoiceCaptureCallbacks) async -> Result<Void, JarvisVoiceAdapterError>
    func stopCapture() async -> Result<Void, JarvisVoiceAdapterError>
    func interrupt(reason: String) async -> Result<Void, JarvisVoiceAdapterError>
}

@MainActor
public final class VoiceAdapterStateModel: ObservableObject {
    @Published public private(set) var phase: JarvisVoiceAdapterPhase
    @Published public private(set) var lastError: JarvisVoiceAdapterError?

    private let adapter: any JarvisVoiceAdapter
    private let voiceState: VoiceStateModel

    public init(adapter: any JarvisVoiceAdapter, voiceState: VoiceStateModel) {
        self.adapter = adapter
        self.voiceState = voiceState
        self.phase = adapter.phase
        self.lastError = nil
    }

    public var statusText: String {
        switch phase {
        case .idle:
            return "Voice adapter idle."
        case .requestingPermission:
            return "Voice adapter requesting microphone and speech permissions."
        case .listening:
            return "Voice adapter listening."
        case .transcribing:
            return "Voice adapter transcribing."
        case let .interrupted(reason):
            return "Voice adapter interrupted: \(reason)"
        case let .degraded(reason):
            return "Voice adapter degraded: \(reason)"
        case let .unavailable(reason):
            return "Voice adapter unavailable: \(reason)"
        }
    }

    public var isCaptureActive: Bool {
        switch phase {
        case .listening, .transcribing:
            return true
        case .idle, .requestingPermission, .interrupted, .degraded, .unavailable:
            return false
        }
    }

    public var canStartCapture: Bool {
        switch phase {
        case .idle, .degraded:
            return true
        case .requestingPermission, .listening, .transcribing, .interrupted, .unavailable:
            return false
        }
    }

    public func requestPermissions() async {
        phase = .requestingPermission
        switch await adapter.requestPermissions() {
        case .success:
            lastError = nil
            phase = adapter.phase
        case let .failure(error):
            fail(error)
        }
    }

    public func startCapture() async {
        let callbacks = JarvisVoiceCaptureCallbacks(
            onPartialTranscript: { [weak self, weak voiceState] transcript in
                self?.phase = .transcribing
                voiceState?.apply(.updateTranscript(transcript))
            },
            onFinalTranscript: { [weak self, weak voiceState] transcript in
                self?.phase = .idle
                voiceState?.apply(.updateTranscript(transcript))
            },
            onError: { [weak self] error in
                self?.fail(error)
            }
        )

        switch await adapter.startCapture(callbacks: callbacks) {
        case .success:
            lastError = nil
            phase = adapter.phase
            voiceState.apply(.beginTranscript)
        case let .failure(error):
            fail(error)
        }
    }

    public func stopCapture() async {
        switch await adapter.stopCapture() {
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
            voiceState.interruptTranscript(reason: reason)
        case let .failure(error):
            fail(error)
        }
    }

    private func fail(_ error: JarvisVoiceAdapterError) {
        lastError = error
        phase = .unavailable(reason: error.description)
        voiceState.setUnavailable(reason: error.description)
    }
}

#if canImport(AVFoundation) && canImport(Speech)
@available(macOS 14.0, *)
@MainActor
public final class MacSpeechVoiceAdapter: JarvisVoiceAdapter {
    public private(set) var phase: JarvisVoiceAdapterPhase

    private let recognizer: SFSpeechRecognizer?
    private let audioEngine: AVAudioEngine
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var callbacks: JarvisVoiceCaptureCallbacks?

    public init(locale: Locale = Locale(identifier: "en_US")) {
        self.recognizer = SFSpeechRecognizer(locale: locale)
        self.audioEngine = AVAudioEngine()
        self.phase = .idle
    }

    public func requestPermissions() async -> Result<Void, JarvisVoiceAdapterError> {
        phase = .requestingPermission

        let speechStatus = await requestSpeechAuthorization()
        switch speechStatus {
        case .authorized:
            break
        case .denied:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Speech recognition permission was denied.").description)
            return .failure(.permissionDenied("Speech recognition permission was denied."))
        case .restricted:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionRestricted("Speech recognition is restricted on this Mac.").description)
            return .failure(.permissionRestricted("Speech recognition is restricted on this Mac."))
        case .notDetermined:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Speech recognition permission was not granted.").description)
            return .failure(.permissionDenied("Speech recognition permission was not granted."))
        @unknown default:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Unknown speech authorization status.").description)
            return .failure(.permissionDenied("Unknown speech authorization status."))
        }

        let microphoneStatus = await requestMicrophoneAuthorization()
        switch microphoneStatus {
        case .authorized:
            phase = .idle
            return .success(())
        case .denied:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Microphone permission was denied.").description)
            return .failure(.permissionDenied("Microphone permission was denied."))
        case .restricted:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionRestricted("Microphone access is restricted on this Mac.").description)
            return .failure(.permissionRestricted("Microphone access is restricted on this Mac."))
        case .notDetermined:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Microphone permission was not granted.").description)
            return .failure(.permissionDenied("Microphone permission was not granted."))
        @unknown default:
            phase = .unavailable(reason: JarvisVoiceAdapterError.permissionDenied("Unknown microphone authorization status.").description)
            return .failure(.permissionDenied("Unknown microphone authorization status."))
        }
    }

    public func startCapture(callbacks: JarvisVoiceCaptureCallbacks) async -> Result<Void, JarvisVoiceAdapterError> {
        guard recognitionTask == nil else {
            return .failure(.alreadyCapturing)
        }
        guard let recognizer, recognizer.isAvailable else {
            phase = .unavailable(reason: JarvisVoiceAdapterError.speechRecognizerUnavailable.description)
            return .failure(.speechRecognizerUnavailable)
        }

        self.callbacks = callbacks
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        recognitionRequest = request

        let inputNode = audioEngine.inputNode
        let recordingFormat = inputNode.outputFormat(forBus: 0)
        inputNode.removeTap(onBus: 0)
        inputNode.installTap(onBus: 0, bufferSize: 1024, format: recordingFormat) { buffer, _ in
            request.append(buffer)
        }

        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            Task { @MainActor in
                self?.handleRecognition(result: result, error: error)
            }
        }

        do {
            audioEngine.prepare()
            try audioEngine.start()
            phase = .listening
            return .success(())
        } catch {
            stopCaptureImmediately()
            let adapterError = JarvisVoiceAdapterError.captureStartFailed(String(describing: error))
            phase = .unavailable(reason: adapterError.description)
            return .failure(adapterError)
        }
    }

    public func stopCapture() async -> Result<Void, JarvisVoiceAdapterError> {
        guard recognitionTask != nil || audioEngine.isRunning else {
            return .failure(.noActiveCapture)
        }
        stopCaptureImmediately()
        phase = .idle
        return .success(())
    }

    public func interrupt(reason: String) async -> Result<Void, JarvisVoiceAdapterError> {
        guard recognitionTask != nil || audioEngine.isRunning else {
            return .failure(.noActiveCapture)
        }
        stopCaptureImmediately()
        phase = .interrupted(reason: reason)
        return .success(())
    }

    private func requestSpeechAuthorization() async -> SFSpeechRecognizerAuthorizationStatus {
        await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { status in
                continuation.resume(returning: status)
            }
        }
    }

    private func requestMicrophoneAuthorization() async -> AVAuthorizationStatus {
        let current = AVCaptureDevice.authorizationStatus(for: .audio)
        guard current == .notDetermined else {
            return current
        }
        let granted = await AVCaptureDevice.requestAccess(for: .audio)
        return granted ? .authorized : .denied
    }

    private func handleRecognition(result: SFSpeechRecognitionResult?, error: Error?) {
        if let result {
            let transcript = result.bestTranscription.formattedString
            phase = result.isFinal ? .idle : .transcribing
            if result.isFinal {
                callbacks?.onFinalTranscript(transcript)
                stopCaptureImmediately()
            } else {
                callbacks?.onPartialTranscript(transcript)
            }
        }

        if let error {
            stopCaptureImmediately()
            let adapterError = JarvisVoiceAdapterError.recognitionFailed(String(describing: error))
            phase = .unavailable(reason: adapterError.description)
            callbacks?.onError(adapterError)
        }
    }

    private func stopCaptureImmediately() {
        if audioEngine.isRunning {
            audioEngine.stop()
        }
        audioEngine.inputNode.removeTap(onBus: 0)
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionRequest = nil
        recognitionTask = nil
        callbacks = nil
    }
}
#else
@MainActor
public final class MacSpeechVoiceAdapter: JarvisVoiceAdapter {
    public private(set) var phase: JarvisVoiceAdapterPhase = .unavailable(
        reason: JarvisVoiceAdapterError.frameworkUnavailable("Speech and AVFoundation are not available in this build.").description
    )

    public init() {}

    public func requestPermissions() async -> Result<Void, JarvisVoiceAdapterError> {
        .failure(.frameworkUnavailable("Speech and AVFoundation are not available in this build."))
    }

    public func startCapture(callbacks _: JarvisVoiceCaptureCallbacks) async -> Result<Void, JarvisVoiceAdapterError> {
        .failure(.frameworkUnavailable("Speech and AVFoundation are not available in this build."))
    }

    public func stopCapture() async -> Result<Void, JarvisVoiceAdapterError> {
        .failure(.noActiveCapture)
    }

    public func interrupt(reason _: String) async -> Result<Void, JarvisVoiceAdapterError> {
        .failure(.noActiveCapture)
    }
}
#endif
