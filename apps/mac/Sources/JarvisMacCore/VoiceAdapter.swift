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

public enum JarvisVoicePermissionState: Equatable, Sendable {
    case notRequested
    case requesting
    case granted
    case denied(reason: String)
}

public enum JarvisVoiceAdapterError: Error, Equatable, Sendable, CustomStringConvertible {
    case frameworkUnavailable(String)
    case permissionNotRequested(String)
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
        case let .permissionNotRequested(reason):
            return "Voice permission not requested: \(reason)"
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

public enum JarvisVoiceAutoSubmitAvailability: Equatable, Sendable {
    case available
    case unavailable(reason: String)

    public var isAvailable: Bool {
        self == .available
    }

    public var blockedReason: String? {
        switch self {
        case .available:
            return nil
        case let .unavailable(reason):
            return reason
        }
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
    @Published public private(set) var permissionState: JarvisVoicePermissionState
    @Published public private(set) var lastError: JarvisVoiceAdapterError?
    @Published public private(set) var isFinalTranscriptAutoSubmitEnabled: Bool

    private let adapter: any JarvisVoiceAdapter
    private let voiceState: VoiceStateModel
    private let shouldAutoSubmitFinalTranscript: @MainActor () -> Bool
    private let autoSubmitUnavailableReason: @MainActor () -> String?
    private let submitFinalTranscript: (@MainActor (JarvisVoiceCommandHandoff) async -> Void)?

    public init(
        adapter: any JarvisVoiceAdapter,
        voiceState: VoiceStateModel,
        shouldAutoSubmitFinalTranscript: @escaping @MainActor () -> Bool = { true },
        autoSubmitUnavailableReason: @escaping @MainActor () -> String? = { nil },
        submitFinalTranscript: (@MainActor (JarvisVoiceCommandHandoff) async -> Void)? = nil
    ) {
        self.adapter = adapter
        self.voiceState = voiceState
        self.shouldAutoSubmitFinalTranscript = shouldAutoSubmitFinalTranscript
        self.autoSubmitUnavailableReason = autoSubmitUnavailableReason
        self.submitFinalTranscript = submitFinalTranscript
        self.phase = adapter.phase
        switch adapter.phase {
        case .listening, .transcribing:
            self.permissionState = .granted
        case let .unavailable(reason):
            self.permissionState = .denied(reason: reason)
        case .idle, .requestingPermission, .interrupted, .degraded:
            self.permissionState = .notRequested
        }
        self.lastError = nil
        self.isFinalTranscriptAutoSubmitEnabled = false
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

    public var autoSubmitAvailability: JarvisVoiceAutoSubmitAvailability {
        guard submitFinalTranscript != nil else {
            return .unavailable(reason: "Auto-submit is unavailable because no command submit handler is configured.")
        }
        switch phase {
        case .requestingPermission:
            return .unavailable(reason: "Auto-submit is unavailable while voice permissions are being requested.")
        case let .unavailable(reason):
            return .unavailable(reason: "Auto-submit is unavailable while voice capture is unavailable: \(reason)")
        case .idle, .listening, .transcribing, .interrupted, .degraded:
            break
        }
        guard permissionState == .granted else {
            return .unavailable(reason: "Auto-submit is unavailable until microphone and speech permissions are granted.")
        }
        if let reason = autoSubmitUnavailableReason() {
            return .unavailable(reason: reason)
        }
        return .available
    }

    public var isFinalTranscriptAutoSubmitToggleEnabled: Bool {
        autoSubmitAvailability.isAvailable
    }

    public var isCaptureActive: Bool {
        switch phase {
        case .listening, .transcribing:
            return true
        case .idle, .requestingPermission, .interrupted, .degraded, .unavailable:
            return false
        }
    }

    public var permissionStatusText: String {
        switch permissionState {
        case .notRequested:
            return "Voice permissions not requested."
        case .requesting:
            return "Voice permissions request in progress."
        case .granted:
            return "Voice permissions granted."
        case let .denied(reason):
            return "Voice permissions unavailable: \(reason)"
        }
    }

    public var canRequestPermissions: Bool {
        switch phase {
        case .listening, .transcribing, .requestingPermission:
            return false
        case .idle, .interrupted, .degraded, .unavailable:
            return true
        }
    }

    public var canStartCapture: Bool {
        guard permissionState == .granted else {
            return false
        }
        switch phase {
        case .idle, .degraded:
            return true
        case .requestingPermission, .listening, .transcribing, .interrupted, .unavailable:
            return false
        }
    }

    public func requestPermissions() async {
        phase = .requestingPermission
        permissionState = .requesting
        let previousPhase: JarvisVoiceAdapterPhase = .idle
        switch await adapter.requestPermissions() {
        case .success:
            lastError = nil
            permissionState = .granted
            phase = adapter.phase
        case let .failure(error):
            permissionState = .denied(reason: error.description)
            fail(error, preserving: previousPhase)
        }
    }

    public func startCapture() async {
        let previousPhase = phase
        guard permissionState == .granted else {
            fail(
                .permissionNotRequested("Request microphone and speech permissions before starting capture."),
                preserving: previousPhase
            )
            return
        }
        let callbacks = JarvisVoiceCaptureCallbacks(
            onPartialTranscript: { [weak self, weak voiceState] transcript in
                self?.phase = .transcribing
                voiceState?.apply(.updateTranscript(transcript))
            },
            onFinalTranscript: { [weak self] transcript in
                guard let self else { return }
                if case .interrupted = self.phase {
                    return
                }
                self.phase = .idle
                self.voiceState.apply(.updateTranscript(transcript))
                guard self.isFinalTranscriptAutoSubmitEnabled,
                      self.shouldAutoSubmitFinalTranscript(),
                      let submitFinalTranscript = self.submitFinalTranscript,
                      let handoff = self.voiceState.submitTranscript(source: "voice-final-transcript")
                else {
                    return
                }
                Task {
                    await submitFinalTranscript(handoff)
                }
            },
            onError: { [weak self] error in
                guard let self else { return }
                self.fail(error, preserving: self.phase)
            }
        )

        switch await adapter.startCapture(callbacks: callbacks) {
        case .success:
            lastError = nil
            phase = adapter.phase
            voiceState.apply(.beginTranscript)
        case let .failure(error):
            fail(error, preserving: previousPhase)
        }
    }

    public func stopCapture() async {
        let previousPhase = phase
        switch await adapter.stopCapture() {
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
            voiceState.interruptTranscript(reason: reason)
        case let .failure(error):
            fail(error, preserving: previousPhase)
        }
    }

    public func setFinalTranscriptAutoSubmitEnabled(_ enabled: Bool) {
        guard enabled else {
            isFinalTranscriptAutoSubmitEnabled = false
            return
        }
        isFinalTranscriptAutoSubmitEnabled = isFinalTranscriptAutoSubmitToggleEnabled
    }

    private func fail(_ error: JarvisVoiceAdapterError, preserving previousPhase: JarvisVoiceAdapterPhase) {
        lastError = error
        if error.isRecoverableCommandState {
            phase = previousPhase
        } else {
            phase = .unavailable(reason: error.description)
            isFinalTranscriptAutoSubmitEnabled = false
            voiceState.setUnavailable(reason: error.description)
        }
    }
}

private extension JarvisVoiceAdapterError {
    var isRecoverableCommandState: Bool {
        switch self {
        case .permissionNotRequested, .alreadyCapturing, .noActiveCapture:
            return true
        case .frameworkUnavailable,
             .permissionDenied,
             .permissionRestricted,
             .speechRecognizerUnavailable,
             .captureStartFailed,
             .recognitionFailed:
            return false
        }
    }
}

#if canImport(AVFoundation) && canImport(Speech)
@available(macOS 14.0, *)
@MainActor
public final class MacSpeechVoiceAdapter: JarvisVoiceAdapter {
    public private(set) var phase: JarvisVoiceAdapterPhase

    private let recognizer: SFSpeechRecognizer?
    private let audioEngine: AVAudioEngine
    private let currentSpeechAuthorization: @MainActor () -> SFSpeechRecognizerAuthorizationStatus
    private let currentMicrophoneAuthorization: @MainActor () -> AVAuthorizationStatus
    private let speechAuthorizationRequest: @MainActor () async -> SFSpeechRecognizerAuthorizationStatus
    private let microphoneAuthorizationRequest: @MainActor () async -> AVAuthorizationStatus
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var callbacks: JarvisVoiceCaptureCallbacks?

    public init(
        locale: Locale = Locale(identifier: "en_US"),
        currentSpeechAuthorization: @escaping @MainActor () -> SFSpeechRecognizerAuthorizationStatus = {
            SFSpeechRecognizer.authorizationStatus()
        },
        currentMicrophoneAuthorization: @escaping @MainActor () -> AVAuthorizationStatus = {
            AVCaptureDevice.authorizationStatus(for: .audio)
        },
        speechAuthorizationRequest: @escaping @MainActor () async -> SFSpeechRecognizerAuthorizationStatus = {
            await withCheckedContinuation { continuation in
                SFSpeechRecognizer.requestAuthorization { status in
                    continuation.resume(returning: status)
                }
            }
        },
        microphoneAuthorizationRequest: @escaping @MainActor () async -> AVAuthorizationStatus = {
            let current = AVCaptureDevice.authorizationStatus(for: .audio)
            guard current == .notDetermined else {
                return current
            }
            let granted = await AVCaptureDevice.requestAccess(for: .audio)
            return granted ? .authorized : .denied
        }
    ) {
        self.recognizer = SFSpeechRecognizer(locale: locale)
        self.audioEngine = AVAudioEngine()
        self.currentSpeechAuthorization = currentSpeechAuthorization
        self.currentMicrophoneAuthorization = currentMicrophoneAuthorization
        self.speechAuthorizationRequest = speechAuthorizationRequest
        self.microphoneAuthorizationRequest = microphoneAuthorizationRequest
        self.phase = .idle
    }

    public func requestPermissions() async -> Result<Void, JarvisVoiceAdapterError> {
        phase = .requestingPermission

        let speechStatus = await speechAuthorizationRequest()
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

        let microphoneStatus = await microphoneAuthorizationRequest()
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
        if let error = currentPermissionError() {
            phase = .unavailable(reason: error.description)
            return .failure(error)
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

    private func currentPermissionError() -> JarvisVoiceAdapterError? {
        switch currentSpeechAuthorization() {
        case .authorized:
            break
        case .denied:
            return .permissionDenied("Speech recognition permission was denied.")
        case .restricted:
            return .permissionRestricted("Speech recognition is restricted on this Mac.")
        case .notDetermined:
            return .permissionNotRequested("Speech recognition permission has not been requested.")
        @unknown default:
            return .permissionDenied("Unknown speech authorization status.")
        }

        switch currentMicrophoneAuthorization() {
        case .authorized:
            return nil
        case .denied:
            return .permissionDenied("Microphone permission was denied.")
        case .restricted:
            return .permissionRestricted("Microphone access is restricted on this Mac.")
        case .notDetermined:
            return .permissionNotRequested("Microphone permission has not been requested.")
        @unknown default:
            return .permissionDenied("Unknown microphone authorization status.")
        }
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
