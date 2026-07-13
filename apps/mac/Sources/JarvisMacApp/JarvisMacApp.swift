import JarvisMacCore
import AppKit
import SwiftUI

@main
struct JarvisMacApp: App {
    @StateObject private var supervisor: JarvisCoreSupervisor
    @StateObject private var console: CommandConsoleModel
    @StateObject private var memory: MemoryManagerModel
    @StateObject private var plugins: PluginManagerModel
    @StateObject private var workspaceRoots: JarvisWorkspaceRootBookmarkCoordinator
    @StateObject private var approvals: ApprovalManagementModel
    @StateObject private var runs: RunManagementModel
    @StateObject private var scheduler: SchedulerModel
    @StateObject private var schedulerNotifications: SchedulerNotificationModel
    @StateObject private var trustedWake: TrustedWakeModel
    @StateObject private var wakeCoordinator: MacSystemWakeCoordinator
    @StateObject private var diagnostics: DiagnosticsModel
    @StateObject private var releaseReadiness: ReleaseReadinessModel
    @StateObject private var voice: VoiceStateModel
    @StateObject private var voiceAdapter: VoiceAdapterStateModel
    @StateObject private var speechOutput: SpeechOutputStateModel
    @StateObject private var modelConfiguration: ModelConfigurationModel

    init() {
        let configuration = JarvisCoreSupervisorConfiguration()
        let client = JarvisIPCClient(endpoint: configuration.endpoint)
        let console = CommandConsoleModel(client: client)
        let voice = VoiceStateModel()
        let workspaceRoots = JarvisWorkspaceRootBookmarkCoordinator()
        let supervisor = JarvisCoreSupervisor(
            configuration: configuration,
            client: client,
            workspaceRootProvider: workspaceRoots
        )
        _supervisor = StateObject(wrappedValue: supervisor)
        _console = StateObject(wrappedValue: console)
        _memory = StateObject(wrappedValue: MemoryManagerModel(client: client))
        _plugins = StateObject(wrappedValue: PluginManagerModel(client: client))
        _workspaceRoots = StateObject(wrappedValue: workspaceRoots)
        _approvals = StateObject(wrappedValue: ApprovalManagementModel(client: client))
        _runs = StateObject(wrappedValue: RunManagementModel(client: client))
        _scheduler = StateObject(wrappedValue: SchedulerModel(client: client))
        _schedulerNotifications = StateObject(
            wrappedValue: SchedulerNotificationModel(adapter: MacSchedulerNotificationAdapter())
        )
        let trustedWake = TrustedWakeModel(
            client: client,
            provision: { try await supervisor.provisionTrustedWake() },
            installKeyControl: { try await supervisor.installTrustedWakeKeyControl() }
        )
        _trustedWake = StateObject(wrappedValue: trustedWake)
        _wakeCoordinator = StateObject(
            wrappedValue: MacSystemWakeCoordinator {
                Task { @MainActor in await trustedWake.handleSystemWake() }
            }
        )
        _diagnostics = StateObject(wrappedValue: DiagnosticsModel(client: client))
        _releaseReadiness = StateObject(wrappedValue: ReleaseReadinessModel(client: client))
        _voice = StateObject(wrappedValue: voice)
        _voiceAdapter = StateObject(
            wrappedValue: VoiceAdapterStateModel(
                adapter: JarvisMacApp.defaultVoiceAdapter(),
                voiceState: voice,
                shouldAutoSubmitFinalTranscript: { !console.isWorking },
                autoSubmitUnavailableReason: {
                    console.isWorking ? "Auto-submit is unavailable while a command is already running." : nil
                },
                submitFinalTranscript: { handoff in
                    await console.submit(input: handoff.text, dryRun: handoff.dryRun)
                }
            )
        )
        _speechOutput = StateObject(wrappedValue: SpeechOutputStateModel(adapter: MacSpeechOutputAdapter()))
        _modelConfiguration = StateObject(wrappedValue: ModelConfigurationModel())
    }

    @MainActor
    private static func defaultVoiceAdapter(bundleURL: URL = Bundle.main.bundleURL) -> any JarvisVoiceAdapter {
        guard bundleURL.pathExtension == "app" else {
            return UnavailableVoiceAdapter(
                reason: "Voice capture is unavailable while running from SwiftPM; launch a packaged app bundle for microphone and Speech permissions."
            )
        }

        return MacSpeechVoiceAdapter()
    }

    var body: some Scene {
        WindowGroup("Jarvis", id: JarvisMenuBarContract.mainWindowID) {
            JarvisShellView(
                supervisor: supervisor,
                console: console,
                memory: memory,
                plugins: plugins,
                workspaceRoots: workspaceRoots,
                approvals: approvals,
                runs: runs,
                scheduler: scheduler,
                schedulerNotifications: schedulerNotifications,
                trustedWake: trustedWake,
                diagnostics: diagnostics,
                releaseReadiness: releaseReadiness,
                voice: voice,
                voiceAdapter: voiceAdapter,
                speechOutput: speechOutput,
                modelConfiguration: modelConfiguration
            )
                .background(AppActivationView())
                .task {
                    await supervisor.start(environmentOverrides: modelConfiguration.launchEnvironmentOverrides)
                    if supervisor.isAvailable {
                        await console.refreshHealth()
                        await trustedWake.refresh()
                        modelConfiguration.applyHealth(console.health)
                    } else if case let .degraded(reason) = supervisor.mode {
                        console.markDegraded(reason)
                    }
                }
        }
        .commands {
            CommandMenu("Jarvis") {
                Button("Refresh Health") {
                    Task {
                        await supervisor.refreshHealth()
                        await console.refreshHealth()
                        modelConfiguration.applyHealth(console.health)
                    }
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }

        MenuBarExtra {
            JarvisMenuBarView(
                supervisor: supervisor,
                console: console,
                modelConfiguration: modelConfiguration
            )
        } label: {
            Label(
                JarvisMenuBarContract.title,
                systemImage: JarvisMenuBarPresentation(mode: supervisor.mode).systemImage
            )
        }
        .menuBarExtraStyle(.menu)
    }
}

private struct AppActivationView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

struct JarvisShellView: View {
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var console: CommandConsoleModel
    @ObservedObject var memory: MemoryManagerModel
    @ObservedObject var plugins: PluginManagerModel
    @ObservedObject var workspaceRoots: JarvisWorkspaceRootBookmarkCoordinator
    @ObservedObject var approvals: ApprovalManagementModel
    @ObservedObject var runs: RunManagementModel
    @ObservedObject var scheduler: SchedulerModel
    @ObservedObject var schedulerNotifications: SchedulerNotificationModel
    @ObservedObject var trustedWake: TrustedWakeModel
    @ObservedObject var diagnostics: DiagnosticsModel
    @ObservedObject var releaseReadiness: ReleaseReadinessModel
    @ObservedObject var voice: VoiceStateModel
    @ObservedObject var voiceAdapter: VoiceAdapterStateModel
    @ObservedObject var speechOutput: SpeechOutputStateModel
    @ObservedObject var modelConfiguration: ModelConfigurationModel

    var body: some View {
        VStack(spacing: 0) {
            CoreStatusBanner(supervisor: supervisor, modelConfiguration: modelConfiguration)

            TabView {
                CommandConsoleView(model: console)
                    .tabItem { Text("Console") }
                ModelConfigurationView(
                    model: modelConfiguration,
                    supervisor: supervisor,
                    console: console
                )
                    .tabItem { Text("Model") }
                MemoryManagerView(model: memory)
                    .tabItem { Text("Memory") }
                PluginManagerView(
                    model: plugins,
                    workspaceRoots: workspaceRoots,
                    supervisor: supervisor,
                    modelConfiguration: modelConfiguration
                )
                    .tabItem { Text("Plugins") }
                ApprovalCenterView(model: approvals)
                    .tabItem { Text("Approvals") }
                RunManagementView(model: runs)
                    .tabItem { Text("Runs") }
                SchedulerJobsView(model: scheduler, notifications: schedulerNotifications)
                    .tabItem { Text("Scheduler") }
                TrustedWakeView(model: trustedWake)
                    .tabItem { Text("Wake") }
                DiagnosticsExportView(model: diagnostics)
                    .tabItem { Text("Diagnostics") }
                ReleaseReadinessView(model: releaseReadiness)
                    .tabItem { Text("Release") }
                VoiceStateView(model: voice, adapter: voiceAdapter, speechOutput: speechOutput, console: console)
                    .tabItem { Text("Voice") }
            }
        }
        .frame(minWidth: 860, minHeight: 560)
    }
}

struct TrustedWakeView: View {
    @ObservedObject var model: TrustedWakeModel
    @State private var keyControlConfirmation = ""

    var body: some View {
        Form {
            Section("Trusted macOS system-wake event") {
                Text(model.status?.rule == nil ? "Not enrolled" : (model.status?.rule?.enabled == true ? "Enabled" : "Disabled"))
                if let rule = model.status?.rule {
                    Text("Enrollment generation \(rule.generation); durable high-water \(rule.highestCounter)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Toggle(
                        "Run the bounded wake rule",
                        isOn: Binding(
                            get: { rule.enabled },
                            set: { enabled in Task { await model.setEnabled(enabled) } }
                        )
                    )
                    .disabled(model.isWorking || model.status?.pendingKeyControl != nil)
                    if model.status?.pendingKeyControl != nil {
                        Text("Enablement is quarantined while a key change is pending. Complete or cancel/reset the change first.")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                } else if model.status != nil {
                    Button("Provision trusted wake") {
                        Task { await model.provision() }
                    }
                    .disabled(model.isWorking)
                }
                if let status = model.status {
                    Text(status.proofBoundary)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if status.attentionRequired {
                        Text("\(status.ambiguousDispatchCount) ambiguous dispatch(es) require review; Jarvis will not retry them automatically.")
                            .foregroundStyle(.orange)
                    }
                }
                if model.status?.rule != nil {
                    TextField("Type the exact key-control confirmation", text: $keyControlConfirmation)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Rotate confirmation: \(jarvisTrustedWakeRotateConfirmation)")
                        Text("Lost-key recovery confirmation: \(jarvisTrustedWakeRecoverConfirmation)")
                            .foregroundStyle(.orange)
                        Text("Cancel/reset confirmation: \(jarvisTrustedWakeCancelConfirmation)")
                    }
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    HStack {
                        Button("Rotate key") {
                            let confirmation = keyControlConfirmation
                            Task {
                                await model.beginKeyControl(
                                    operation: .rotate,
                                    confirmation: confirmation
                                )
                            }
                        }
                        .disabled(
                            model.isWorking
                                || keyControlConfirmation != jarvisTrustedWakeRotateConfirmation
                                || model.status?.pendingKeyControl != nil
                        )
                        Button("Recover lost key") {
                            let confirmation = keyControlConfirmation
                            Task {
                                await model.beginKeyControl(
                                    operation: .recover,
                                    confirmation: confirmation
                                )
                            }
                        }
                        .disabled(
                            model.isWorking
                                || keyControlConfirmation != jarvisTrustedWakeRecoverConfirmation
                                || model.status?.pendingKeyControl != nil
                        )
                    }
                    Text("Rotate requires the currently enrolled private key. Recovery does not prove old-key possession: the exact phrase is destructive-action accident prevention on an unauthenticated loopback route, not authorization, device authentication, ownership proof, or same-user/process isolation.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                if let pending = model.status?.pendingKeyControl {
                    Text("Pending \(pending.operation.rawValue) to generation \(pending.targetGeneration). The rule is quarantined disabled; Jarvis will not retry or enable it automatically.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                    HStack {
                        Button("Resume one-shot install") {
                            Task { await model.resumeKeyControl() }
                        }
                        .disabled(model.isWorking)
                        Button("Cancel/reset pending change") {
                            let confirmation = keyControlConfirmation
                            Task { await model.cancelKeyControl(confirmation: confirmation) }
                        }
                        .disabled(
                            model.isWorking
                                || keyControlConfirmation != jarvisTrustedWakeCancelConfirmation
                        )
                    }
                }
                Text("Key rotation and lost-key recovery are explicit local control workflows. They do not establish Apple attestation, OS provenance, background execution, or production readiness; do not manually mutate Keychain or SQLite state.")
                    .font(.caption)
                    .foregroundStyle(.orange)
                if let message = model.keyControlMessage {
                    Text(message).foregroundStyle(.secondary)
                }
                if let event = model.lastEvent {
                    Text("Last wake event: \(event.state)")
                }
                ForEach(model.attentionItems) { item in
                    HStack {
                        Text("Ambiguous event \(item.eventId.uuidString)")
                            .font(.caption)
                        Spacer()
                        Button("Resolve without retry") {
                            Task { await model.resolve(item) }
                        }
                        .disabled(model.isWorking)
                    }
                }
                if let error = model.errorMessage {
                    Text(error).foregroundStyle(.red)
                }
                HStack {
                    Button("Refresh") { Task { await model.refresh() } }
                    Button("Execute test wake rule") { Task { await model.handleSystemWake() } }
                        .disabled(model.status?.rule?.enabled != true || model.isWorking)
                }
            }
        }
        .formStyle(.grouped)
        .task { await model.refresh() }
    }
}

@MainActor
final class MacSystemWakeCoordinator: ObservableObject {
    private var observer: NSObjectProtocol?

    init(onWake: @escaping @Sendable () -> Void) {
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { _ in onWake() }
    }

}

struct CoreStatusBanner: View {
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var modelConfiguration: ModelConfigurationModel

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Core")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(statusText)
                    .font(.subheadline)
            }

            Spacer()

            Text(supervisor.smokeSnapshot.executableConfigured ? "Packaged core configured" : "Core binary not configured")
                .font(.caption)
                .foregroundStyle(.secondary)

            Button("Start") {
                Task { await supervisor.start(environmentOverrides: modelConfiguration.launchEnvironmentOverrides) }
            }
            .disabled(supervisor.isAvailable)

            Button("Stop") {
                Task { _ = await supervisor.stop() }
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .background(background)
    }

    private var statusText: String {
        switch supervisor.mode {
        case .stopped:
            return "Stopped"
        case .starting:
            return "Starting jarvis core"
        case .available:
            return "Available at \(supervisor.configuration.endpoint.baseURL.absoluteString)"
        case let .degraded(reason):
            return "Degraded: \(reason)"
        }
    }

    private var background: some ShapeStyle {
        switch supervisor.mode {
        case .available:
            return AnyShapeStyle(Color.green.opacity(0.12))
        case .degraded:
            return AnyShapeStyle(Color.yellow.opacity(0.18))
        case .starting:
            return AnyShapeStyle(Color.blue.opacity(0.12))
        case .stopped:
            return AnyShapeStyle(Color.secondary.opacity(0.10))
        }
    }
}

struct CommandConsoleView: View {
    @ObservedObject var model: CommandConsoleModel
    @State private var input = ""
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            statusBar

            HSplitView {
                List(model.transcript) { entry in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.role.rawValue.capitalized)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(entry.text)
                            .textSelection(.enabled)
                    }
                    .padding(.vertical, 4)
                }
                .frame(minWidth: 390)

                ActivityAuditView(entries: model.activity)
                    .frame(minWidth: 280)
            }

            if let lastError = model.lastError {
                Text(lastError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal)
                    .padding(.vertical, 6)
            }

            HStack(spacing: 8) {
                TextField("Ask Jarvis", text: $input)
                    .textFieldStyle(.roundedBorder)
                    .focused($inputFocused)
                    .onSubmit(send)
                Button("Send", action: send)
                    .disabled(model.isWorking || input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding()
        }
        .frame(minWidth: 720, minHeight: 480)
        .onAppear {
            inputFocused = true
        }
    }

    private var statusBar: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("Jarvis")
                    .font(.headline)
                Text(statusText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button(model.isPaused ? "Resume" : "Pause") {
                Task {
                    if model.isPaused {
                        await model.resume()
                    } else {
                        await model.pause()
                    }
                }
            }
            .disabled(model.isWorking)
        }
        .padding()
        .background(.bar)
    }

    private var statusText: String {
        if model.isDegraded {
            return "Degraded mode: \(model.degradedReason ?? "core unavailable")"
        }

        guard let health = model.health else {
            return "Core status unknown"
        }

        let pause = model.pauseStatus?.reason.map { " | paused: \($0)" } ?? ""
        return "\(health.status) | \(health.commandRuntime) | jobs: \(health.schedulerJobs)\(pause)"
    }

    private func send() {
        let command = input
        input = ""
        Task {
            await model.submit(input: command)
        }
    }
}

struct ModelConfigurationView: View {
    @ObservedObject var model: ModelConfigurationModel
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var console: CommandConsoleModel

    private var controlsPresentation: ModelConfigurationPresentation {
        ModelConfigurationPresentation(
            canControlSelectedModelRuntime: model.canControlSelectedModelRuntime,
            isWorking: model.isWorking,
            selectedModelIsInstalled: model.selectedModelIsInstalled,
            downloadProgress: model.downloadProgress
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Model")
                        .font(.headline)
                    Text(activeRuntimeText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer()

                Button("Refresh") {
                    Task {
                        await supervisor.refreshHealth()
                        await console.refreshHealth()
                        model.applyHealth(console.health)
                        await model.refreshAvailableModels()
                    }
                }
            }

            Form {
                Picker("Provider", selection: providerBinding) {
                    ForEach(JarvisModelProviderSelection.allCases) { provider in
                        Text(provider.label).tag(provider)
                    }
                }
                .pickerStyle(.segmented)

                TextField("Codex model", text: codexModelBinding)
                    .textFieldStyle(.roundedBorder)
                    .disabled(!selectedProviderUsesCodexModel)

                TextField("OpenAI API URL", text: codexBaseURLBinding)
                    .textFieldStyle(.roundedBorder)
                    .disabled(model.configuration.provider != .codex)

                TextField("Codex executable", text: codexExecutableBinding)
                    .textFieldStyle(.roundedBorder)
                    .disabled(model.configuration.provider != .codexAccount)

                TextField("Ollama URL", text: ollamaBaseURLBinding)
                    .textFieldStyle(.roundedBorder)
                    .disabled(model.configuration.provider != .ollama)

                TextField("Timeout ms", text: timeoutBinding)
                    .textFieldStyle(.roundedBorder)
            }
            .formStyle(.grouped)

            if model.configuration.provider == .ollama {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("Available Models")
                            .font(.subheadline)
                        Spacer()
                        Button("Reload") {
                            Task { await model.refreshAvailableModels() }
                        }
                        .disabled(model.isWorking)
                    }

                    ScrollView {
                        LazyVStack(spacing: 6) {
                            ForEach(model.availableModels) { availableModel in
                                Button {
                                    Task { await model.selectModel(availableModel) }
                                } label: {
                                    ModelSelectionRow(
                                        model: availableModel,
                                        selected: availableModel.matches(name: model.configuration.sanitizedModel)
                                    )
                                }
                                .buttonStyle(.plain)
                                .disabled(model.isWorking)
                            }
                        }
                    }
                    .frame(minHeight: 180, maxHeight: 260)
                }
            }

            if model.configuration.provider == .codexAccount {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("Account Authentication")
                            .font(.subheadline)
                        Spacer()
                        Text("Uses Codex CLI login")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if model.configuration.provider == .codex {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text("Application Credential")
                            .font(.subheadline)
                        Spacer()
                        Text(model.hasStoredCodexCredential ? "Saved in Keychain" : "Not saved")
                            .font(.caption)
                            .foregroundStyle(model.hasStoredCodexCredential ? .green : .orange)
                    }

                    SecureField("OpenAI API key", text: codexAPIKeyBinding)
                        .textFieldStyle(.roundedBorder)

                    HStack(spacing: 8) {
                        Button("Save Credential") {
                            model.saveCodexCredential()
                        }
                        Button("Forget Credential") {
                            model.deleteCodexCredential()
                        }
                        .disabled(!model.hasStoredCodexCredential)
                        Spacer()
                    }
                }
            }

            HStack(spacing: 8) {
                Button("Restart Core With Selection") {
                    Task {
                        model.saveEnteredCodexCredentialIfNeeded()
                        await model.ensureSelectedModelAvailable()
                        guard await supervisor.stop() else {
                            await console.refreshHealth()
                            model.applyHealth(console.health)
                            return
                        }
                        await supervisor.start(
                            environmentOverrides: model.launchEnvironmentOverrides,
                            requireMatchingConfiguration: true
                        )
                        await console.refreshHealth()
                        model.applyHealth(console.health)
                    }
                }

                Button("Start Model") {
                    Task { await model.loadSelectedModel() }
                }
                .disabled(!controlsPresentation.canStartModel)

                Button("Download Selected") {
                    Task { await model.downloadSelectedModel() }
                }
                .disabled(!controlsPresentation.canDownloadSelected)

                Button("Stop Model") {
                    Task { await model.unloadSelectedModel() }
                }
                .disabled(!controlsPresentation.canStopModel)

                Spacer()
            }

            if let progress = model.downloadProgress {
                VStack(alignment: .leading, spacing: 6) {
                    if let fraction = controlsPresentation.progressValue {
                        ProgressView(value: fraction)
                    } else {
                        ProgressView()
                    }
                    Text(controlsPresentation.progressDetailLine ?? progress.detailLine)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if let statusMessage = model.statusMessage {
                Text(statusMessage)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text("Selection changes apply after restarting the supervised core. If another terminal already owns the core port, stop that process before restarting from the app.")
                .font(.caption)
                .foregroundStyle(.secondary)

            Spacer()
        }
        .padding()
        .frame(minWidth: 720, minHeight: 480, alignment: .topLeading)
        .task {
            await model.refreshAvailableModels()
        }
    }

    private var activeRuntimeText: String {
        let provider = model.activeProvider ?? "unknown provider"
        let activeModel = model.activeModel ?? "unknown model"
        return "Active: \(provider) / \(activeModel)"
    }

    private var providerBinding: Binding<JarvisModelProviderSelection> {
        Binding(
            get: { model.configuration.provider },
            set: { provider in
                model.configuration.provider = provider
                if provider == .fake {
                    model.configuration.localModel = "fake-local-model"
                } else if model.configuration.localModel == "fake-local-model" {
                    model.configuration.localModel = "llama3.2"
                }
                if provider == .codexAccount && model.configuration.codexModel == "gpt-4.1-mini" {
                    model.configuration.codexModel = "gpt-5.5"
                }
                if provider == .codex && model.configuration.codexModel == "gpt-5.5" {
                    model.configuration.codexModel = "gpt-4.1-mini"
                }
                model.refreshCodexCredentialState()
            }
        )
    }

    private var selectedProviderUsesCodexModel: Bool {
        model.configuration.provider == .codex || model.configuration.provider == .codexAccount
    }

    private var codexModelBinding: Binding<String> {
        Binding(
            get: { model.configuration.codexModel },
            set: { model.configuration.codexModel = $0 }
        )
    }

    private var codexBaseURLBinding: Binding<String> {
        Binding(
            get: { model.configuration.codexBaseURL },
            set: { model.configuration.codexBaseURL = $0 }
        )
    }

    private var codexExecutableBinding: Binding<String> {
        Binding(
            get: { model.configuration.codexExecutable },
            set: { model.configuration.codexExecutable = $0 }
        )
    }

    private var codexAPIKeyBinding: Binding<String> {
        Binding(
            get: { model.codexAPIKeyEntry },
            set: { model.codexAPIKeyEntry = $0 }
        )
    }

    private var ollamaBaseURLBinding: Binding<String> {
        Binding(
            get: { model.configuration.ollamaBaseURL },
            set: { model.configuration.ollamaBaseURL = $0 }
        )
    }

    private var timeoutBinding: Binding<String> {
        Binding(
            get: { model.configuration.timeoutMilliseconds },
            set: { model.configuration.timeoutMilliseconds = $0 }
        )
    }
}

struct ModelConfigurationPresentation: Equatable {
    var canStartModel: Bool
    var canDownloadSelected: Bool
    var canStopModel: Bool
    var progressValue: Double?
    var progressDetailLine: String?

    init(
        canControlSelectedModelRuntime: Bool,
        isWorking: Bool,
        selectedModelIsInstalled: Bool,
        downloadProgress: JarvisOllamaPullProgress?
    ) {
        canStartModel = canControlSelectedModelRuntime && !isWorking && selectedModelIsInstalled
        canDownloadSelected = canControlSelectedModelRuntime && !isWorking && !selectedModelIsInstalled
        canStopModel = canControlSelectedModelRuntime && !isWorking
        progressValue = downloadProgress?.fractionCompleted
        progressDetailLine = downloadProgress?.detailLine
    }
}

private struct ModelSelectionRow: View {
    let model: JarvisOllamaModelInfo
    let selected: Bool

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(selected ? .blue : .secondary)
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text(model.name)
                        .font(.subheadline)
                    Text(model.installed ? "Installed" : "Downloads on select")
                        .font(.caption2)
                        .foregroundStyle(model.installed ? .green : .orange)
                }
                Text(model.memoryLine)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(model.details ?? model.sizeLine)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(selected ? Color.accentColor.opacity(0.14) : Color.secondary.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

struct MemoryManagerView: View {
    @ObservedObject var model: MemoryManagerModel
    @State private var includeDeleted = false
    @State private var category = "workflow"
    @State private var key = "release-gate"
    @State private var value = "Run local release verification before opening a PR."
    @State private var provenance = "manual"
    @State private var sensitivity = "workspace"
    @State private var selectedId: UUID?
    @State private var categoryFilter = ""
    @State private var sensitivityFilter = "all"

    private let sensitivities = [
        "public",
        "workspace",
        "personal",
        "private",
        "credential_adjacent",
        "restricted"
    ]

    var body: some View {
        ManagementListView(
            title: "Memory",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh(includeDeleted: includeDeleted) }
        ) {
            VStack(alignment: .leading, spacing: 10) {
                memoryForm
                    .padding(.horizontal)

                if let classification = model.classification {
                    MemoryClassificationSummaryView(summary: classification)
                        .padding(.horizontal)
                }

                if let index = model.indexStatus {
                    HStack(spacing: 10) {
                        Label("Index: \(index.state)", systemImage: index.state == "current" ? "checkmark.circle" : "arrow.triangle.2.circlepath")
                        Text("\(index.currentEntryCount) current, \(index.missingEntryCount) missing, \(index.staleEntryCount) stale, \(index.deletedProjectionCount) deleted, \(index.orphanedEntryCount) orphaned")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Button("Rebuild from SQLite") {
                            Task { await model.rebuildIndex() }
                        }
                        .disabled(model.isLoading)
                    }
                    .font(.caption)
                    .padding(.horizontal)
                    Text("SQLite remains canonical; this local projection is rebuildable and is not used for retrieval or cloud context.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal)
                }

                if let retentionPlan = model.retentionPlan {
                    MemoryRetentionPlanView(plan: retentionPlan)
                        .padding(.horizontal)
                }

                HStack {
                    Toggle("Include deleted", isOn: $includeDeleted)
                        .onChange(of: includeDeleted) { _, newValue in
                            Task { await model.refresh(includeDeleted: newValue) }
                        }
                    TextField("Category filter", text: $categoryFilter)
                        .textFieldStyle(.roundedBorder)
                        .frame(maxWidth: 180)
                    Picker("Sensitivity", selection: $sensitivityFilter) {
                        Text("All").tag("all")
                        ForEach(sensitivities, id: \.self) { sensitivity in
                            Text(sensitivity).tag(sensitivity)
                        }
                    }
                    .frame(maxWidth: 210)
                    Spacer()
                }
                .padding(.horizontal)

                List(filteredItems) { item in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline) {
                            Text("\(item.category) / \(item.key)")
                                .font(.subheadline)
                            Spacer()
                            if item.reviewedAt != nil {
                                Text("reviewed")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            if item.deletedAt != nil {
                                Text("deleted")
                                    .font(.caption2)
                                    .foregroundStyle(.red)
                            }
                        }
                        Text(item.value)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                        Text("\(item.sensitivity) | \(item.provenance)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        HStack {
                            Button("Edit") {
                                loadIntoForm(item)
                            }
                            .disabled(item.deletedAt != nil)
                            Button("Review") {
                                Task { await model.review(id: item.id) }
                            }
                            .disabled(item.reviewedAt != nil || item.deletedAt != nil)
                            Button("Delete") {
                                Task { await model.delete(id: item.id) }
                            }
                            .disabled(item.deletedAt != nil)
                            Button("Restore") {
                                Task { await model.restore(id: item.id) }
                            }
                            .disabled(item.deletedAt == nil)
                        }
                        .font(.caption)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private var filteredItems: [JarvisMemoryItem] {
        model.items.filter { item in
            let categoryMatches = categoryFilter.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                || item.category.localizedCaseInsensitiveContains(categoryFilter)
            let sensitivityMatches = sensitivityFilter == "all" || item.sensitivity == sensitivityFilter
            return categoryMatches && sensitivityMatches
        }
    }

    private var canSubmit: Bool {
        !category.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !provenance.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var memoryForm: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                TextField("Category", text: $category)
                    .textFieldStyle(.roundedBorder)
                    .disabled(selectedId != nil)
                TextField("Key", text: $key)
                    .textFieldStyle(.roundedBorder)
                    .disabled(selectedId != nil)
                Picker("Sensitivity", selection: $sensitivity) {
                    ForEach(sensitivities, id: \.self) { sensitivity in
                        Text(sensitivity).tag(sensitivity)
                    }
                }
                .frame(maxWidth: 220)
            }
            TextField("Value", text: $value)
                .textFieldStyle(.roundedBorder)
            HStack {
                TextField("Provenance", text: $provenance)
                    .textFieldStyle(.roundedBorder)
                Button(selectedId == nil ? "Create" : "Update") {
                    submit()
                }
                .disabled(!canSubmit || model.isLoading)
                if selectedId != nil {
                    Button("Clear") {
                        clearForm()
                    }
                }
            }
        }
    }

    private func submit() {
        let trimmedCategory = category.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedKey = key.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedValue = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedProvenance = provenance.trimmingCharacters(in: .whitespacesAndNewlines)
        let selectedSensitivity = sensitivity

        Task {
            if let selectedId {
                await model.update(
                    id: selectedId,
                    value: trimmedValue,
                    provenance: trimmedProvenance,
                    sensitivity: selectedSensitivity
                )
            } else {
                await model.create(
                    category: trimmedCategory,
                    key: trimmedKey,
                    value: trimmedValue,
                    provenance: trimmedProvenance,
                    sensitivity: selectedSensitivity
                )
            }
            clearForm()
        }
    }

    private func loadIntoForm(_ item: JarvisMemoryItem) {
        selectedId = item.id
        category = item.category
        key = item.key
        value = item.value
        provenance = item.provenance
        sensitivity = item.sensitivity
        Task { await model.load(id: item.id) }
    }

    private func clearForm() {
        selectedId = nil
        category = "workflow"
        key = ""
        value = ""
        provenance = "manual"
        sensitivity = "workspace"
    }
}

struct MemoryClassificationSummaryView: View {
    let summary: JarvisMemoryClassificationSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                Label("\(summary.activeCount) active", systemImage: "tray.full")
                Label("\(summary.unreviewedActiveCount) unreviewed", systemImage: "exclamationmark.circle")
                Label("\(summary.sensitiveActiveCount) sensitive", systemImage: "lock.shield")
                if summary.includeDeleted {
                    Label("\(summary.deletedCount) deleted", systemImage: "archivebox")
                }
            }
            .font(.caption)

            Text(classificationText)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
    }

    private var classificationText: String {
        let sensitivity = summary.bySensitivity
            .map { "\($0.label) \($0.activeCount)/\($0.count)" }
            .joined(separator: ", ")
        let categories = summary.byCategory
            .prefix(6)
            .map { "\($0.label) \($0.activeCount)/\($0.count)" }
            .joined(separator: ", ")
        return "sensitivity: \(sensitivity.isEmpty ? "none" : sensitivity) | categories: \(categories.isEmpty ? "none" : categories)"
    }
}

struct MemoryRetentionPlanView: View {
    let plan: JarvisMemoryRetentionPlan

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                Label("\(plan.candidateCount) retention action\(plan.candidateCount == 1 ? "" : "s")", systemImage: "checklist")
                Label("\(plan.unreviewedActiveCount) unreviewed", systemImage: "exclamationmark.circle")
                Label("\(plan.deletedSensitiveRetainedCount) retained sensitive", systemImage: "lock.shield")
                Text(plan.automationEnabled ? "automation enabled" : "operator review")
                    .font(.caption2)
                    .foregroundStyle(plan.automationEnabled ? .orange : .secondary)
            }
            .font(.caption)

            if plan.candidates.isEmpty {
                Text("No memory retention actions are pending.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(plan.candidates.prefix(4)) { candidate in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text("\(candidate.category) / \(candidate.key)")
                                .font(.caption)
                            Spacer()
                            Text(candidate.severity)
                                .font(.caption2)
                                .foregroundStyle(candidate.severity == "high" ? .red : .secondary)
                        }
                        Text("\(candidate.status) | \(candidate.sensitivity) | \(candidate.recommendedAction)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        Text(candidate.reason)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    .padding(.vertical, 2)
                }
                if plan.candidates.count > 4 {
                    Text("+\(plan.candidates.count - 4) more retention action\(plan.candidates.count - 4 == 1 ? "" : "s")")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if plan.valueRedactionRequired {
                Text("Memory values and provenance are redacted from this plan.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

struct PluginManagerView: View {
    @ObservedObject var model: PluginManagerModel
    @ObservedObject var workspaceRoots: JarvisWorkspaceRootBookmarkCoordinator
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var modelConfiguration: ModelConfigurationModel
    @State private var workspaceStatus: String?
    @State private var workspaceOperationInProgress = false

    var body: some View {
        ManagementListView(
            title: "Plugins",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List {
                Section("Workspace roots") {
                    ForEach(workspaceRoots.grants) { grant in
                        let presentation = WorkspaceRootGrantPresentation(grant: grant)
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(presentation.idLine)
                                    .font(.subheadline)
                                Text(presentation.detailLine)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("Remove", role: .destructive) {
                                Task { await removeWorkspaceRoot(grant.id) }
                            }
                            .disabled(workspaceOperationInProgress)
                        }
                    }

                    HStack {
                        Button("Add Folder") { addWorkspaceRoot() }
                            .disabled(workspaceOperationInProgress || workspaceRoots.grants.count >= 8)
                        Button("Restart Core") {
                            Task { await restartCoreForWorkspaceRoots() }
                        }
                        .disabled(workspaceOperationInProgress || supervisor.mode == .starting)
                        Spacer()
                    }

                    Text("Folder paths and bookmark bytes stay hidden. Changes take effect only after the app-supervised core restarts; removing a grant restarts immediately to revoke the held descriptor.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    if let status = workspaceStatus ?? workspaceRoots.lastError {
                        Text(status)
                            .font(.caption2)
                            .foregroundStyle(.orange)
                            .textSelection(.enabled)
                    }
                }

                if let warning = model.modelToolCatalogWarning {
                    Section("Production capabilities") {
                        Text(warning)
                            .font(.caption)
                            .foregroundStyle(.orange)
                            .textSelection(.enabled)
                    }
                }

                if !model.modelTools.isEmpty {
                    Section("Production capabilities") {
                        ForEach(model.modelTools) { tool in
                            VStack(alignment: .leading, spacing: 5) {
                                HStack {
                                    Text(tool.id)
                                        .font(.subheadline)
                                    Spacer()
                                    Text(tool.constraints.operatorBadge)
                                        .font(.caption2)
                                        .foregroundStyle(tool.constraints.hasProductionReadOnlyBoundary ? .green : .orange)
                                }
                                Text(tool.description)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("risk \(tool.riskTier) | scopes \(tool.scopes.joined(separator: ", ")) | proactive \(tool.proactive ? "yes" : "no")")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 4)
                        }
                    }
                }

                if let warning = model.installedRegistryWarning {
                    Section("Installed") {
                        Text(warning)
                            .font(.caption)
                            .foregroundStyle(.orange)
                            .textSelection(.enabled)
                    }
                }

                if !model.installedPlugins.isEmpty {
                    Section("Installed") {
                        ForEach(model.installedPlugins) { plugin in
                            VStack(alignment: .leading, spacing: 5) {
                                HStack {
                                    Text("\(plugin.manifest.name) \(plugin.manifest.version)")
                                        .font(.subheadline)
                                    Spacer()
                                    Text(plugin.executionEnabled ? plugin.executionGrant : "disabled")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Text(plugin.confinementSummary)
                                    .font(.caption)
                                    .foregroundStyle(plugin.hasEnforcedLanguageConfinement ? .green : .secondary)
                                    .lineLimit(1)
                                    .textSelection(.enabled)
                                Text("Local paths, commands, hashes, module bytes, and execution inputs are redacted.")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                Text(installedPluginStatus(plugin))
                                    .font(.caption2)
                                    .foregroundStyle(plugin.provenance.needsReview ? .orange : .secondary)
                                    .textSelection(.enabled)
                            }
                            .padding(.vertical, 4)
                        }
                    }
                }

                Section("First-party manifests") {
                    ForEach(model.manifests) { manifest in
                        VStack(alignment: .leading, spacing: 5) {
                            Text("\(manifest.name) \(manifest.version)")
                                .font(.subheadline)
                            Text("\(manifest.source) | \(manifest.author)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            ForEach(manifest.actions, id: \.name) { action in
                                Text("\(action.name): \(action.riskTier), \(action.permissions.joined(separator: ", "))")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
        }
    }

    private func installedPluginStatus(_ plugin: JarvisInstalledPluginRecord) -> String {
        let origin = plugin.provenance.originClaim.map { "origin \($0)" } ?? "origin unknown"
        let originReview = plugin.provenance.originClaimVerified ? "origin reviewed" : "origin unreviewed"
        let executable = plugin.isExecutable ? "executable" : "not executable"
        return "\(plugin.provenance.integrityStatus) | \(origin) | \(originReview) | \(executable)"
    }

    private func addWorkspaceRoot() {
        let panel = NSOpenPanel()
        panel.title = "Authorize a workspace folder"
        panel.prompt = "Authorize"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let grant = try workspaceRoots.addRoot(url)
            workspaceStatus = "Grant \(grant.id) stored. Restart the core to activate it."
        } catch {
            workspaceStatus = String(describing: error)
        }
    }

    private func removeWorkspaceRoot(_ id: String) async {
        workspaceOperationInProgress = true
        defer { workspaceOperationInProgress = false }
        if supervisor.isAvailable, !supervisor.isSupervisingCoreProcess {
            workspaceStatus = "Another process owns the core endpoint. Stop it before changing app-owned workspace authority."
            return
        }
        guard await supervisor.stop() else {
            workspaceStatus = "The supervised core did not stop, so the workspace grant was not removed."
            return
        }
        do {
            try workspaceRoots.removeRoot(id: id)
        } catch {
            workspaceStatus = String(describing: error)
            return
        }
        await startCoreForWorkspaceRoots()
    }

    private func restartCoreForWorkspaceRoots(manageWorkingState: Bool = true) async {
        if manageWorkingState { workspaceOperationInProgress = true }
        defer {
            if manageWorkingState { workspaceOperationInProgress = false }
        }
        if supervisor.isAvailable, !supervisor.isSupervisingCoreProcess {
            workspaceStatus = "Another process owns the core endpoint. Stop it before applying app-owned workspace grants."
            return
        }
        guard await supervisor.stop() else {
            workspaceStatus = "The supervised core did not stop; workspace authority was not relaunched."
            return
        }
        await startCoreForWorkspaceRoots()
    }

    private func startCoreForWorkspaceRoots() async {
        await supervisor.start(
            environmentOverrides: modelConfiguration.launchEnvironmentOverrides,
            requireMatchingConfiguration: true
        )
        if supervisor.isAvailable {
            workspaceStatus = WorkspaceRootActivationPresentation(
                isAvailable: true,
                isAppSupervised: supervisor.isSupervisingCoreProcess
            ).statusMessage
        } else if case let .degraded(reason) = supervisor.mode {
            workspaceStatus = reason
        }
    }
}

struct SchedulerJobsView: View {
    @ObservedObject var model: SchedulerModel
    @ObservedObject var notifications: SchedulerNotificationModel
    @State private var name = "manual check"
    @State private var command = "status check"
    @State private var runAt = "2026-05-20T13:00:00Z"
    @State private var intervalSeconds = "3600"
    @State private var triggerMode = "manual"
    @State private var runDueLimit = "8"
    @State private var staleOlderThanSeconds = "3600"
    @State private var staleRecoveryLimit = "8"

    var body: some View {
        ManagementListView(
            title: "Scheduler",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            VStack(spacing: 0) {
                if let attention = model.attention {
                    SchedulerAttentionSummaryView(attention: attention, notifications: notifications)
                        .padding(.horizontal)
                        .padding(.bottom, 8)
                }

                schedulerForm
                    .padding(.horizontal)
                    .padding(.bottom, 8)

                schedulerActions
                    .padding(.horizontal)
                    .padding(.bottom, 8)

                List(model.jobs) { job in
                    VStack(alignment: .leading, spacing: 5) {
                        HStack {
                            Text(job.name)
                                .font(.subheadline)
                            Spacer()
                            Button("Inspect") {
                                Task { await model.select(id: job.id) }
                            }
                            Button("Cancel") {
                                Task { await model.cancel(id: job.id) }
                            }
                            .disabled(job.cancelledAt != nil)
                        }
                        Text(job.command)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text("\(job.status) | \(triggerDescription(job.trigger))")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private var schedulerForm: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                TextField("Name", text: $name)
                    .textFieldStyle(.roundedBorder)
                TextField("Command", text: $command)
                    .textFieldStyle(.roundedBorder)
            }

            HStack {
                Picker("Trigger", selection: $triggerMode) {
                    Text("Manual").tag("manual")
                    Text("Once").tag("once")
                    Text("Interval").tag("interval")
                }
                .pickerStyle(.segmented)
                .frame(width: 260)

                if triggerMode == "once" {
                    TextField("Run at ISO-8601", text: $runAt)
                        .textFieldStyle(.roundedBorder)
                } else if triggerMode == "interval" {
                    TextField("Seconds", text: $intervalSeconds)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 120)
                }

                Button("Schedule") {
                    schedule()
                }
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            if let selectedJob = model.selectedJob {
                Text("Selected \(selectedJob.id.uuidString): \(selectedJob.status)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }

    private var schedulerActions: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                TextField("Due limit", text: $runDueLimit)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 90)
                Button {
                    Task { await model.runDue(limit: Int(runDueLimit) ?? 8) }
                } label: {
                    Label("Run Due", systemImage: "play.circle")
                }
                .disabled(model.isLoading)

                Divider()
                    .frame(height: 22)

                TextField("Older than seconds", text: $staleOlderThanSeconds)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 150)
                TextField("Limit", text: $staleRecoveryLimit)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 70)
                Button {
                    Task {
                        await model.recoverStale(
                            olderThanSeconds: UInt64(staleOlderThanSeconds) ?? 3600,
                            limit: Int(staleRecoveryLimit) ?? 8
                        )
                    }
                } label: {
                    Label("Recover Stale", systemImage: "arrow.clockwise.circle")
                }
                .disabled(model.isLoading)
            }
            .buttonStyle(.bordered)

            if let lastRunDue = model.lastRunDue {
                Text(
                    "Run due checked \(lastRunDue.executions.count) job(s); emergency pause: \(lastRunDue.emergencyPaused ? "yes" : "no")"
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            }

            if let lastStaleRecovery = model.lastStaleRecovery {
                Text("Recovered \(lastStaleRecovery.recovered.count) stale job(s)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }

    private func schedule() {
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedCommand = command.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            switch triggerMode {
            case "once":
                await model.scheduleOnce(name: trimmedName, command: trimmedCommand, runAt: runAt)
            case "interval":
                await model.scheduleInterval(
                    name: trimmedName,
                    command: trimmedCommand,
                    everySeconds: UInt64(intervalSeconds) ?? 3600
                )
            default:
                await model.scheduleManual(name: trimmedName, command: trimmedCommand)
            }
        }
    }

    private func triggerDescription(_ trigger: JarvisSchedulerTrigger) -> String {
        switch trigger {
        case .manual:
            return "manual"
        case let .onceAt(runAt):
            return "once at \(runAt)"
        case let .interval(everySeconds):
            return "every \(everySeconds)s"
        }
    }

}

struct WorkspaceRootGrantPresentation: Equatable {
    let idLine: String
    let detailLine: String

    init(grant: JarvisWorkspaceRootGrant) {
        idLine = grant.id
        detailLine = "\(grant.status); authorized directory path hidden"
    }
}

struct WorkspaceRootActivationPresentation: Equatable {
    let statusMessage: String

    init(isAvailable: Bool, isAppSupervised: Bool) {
        if isAvailable, isAppSupervised {
            statusMessage = "Workspace grants are active for the current supervised core."
        } else if isAvailable {
            statusMessage = "Another process owns the core endpoint; app-owned workspace grants are not active there."
        } else {
            statusMessage = "Workspace grants are not active because the core is unavailable."
        }
    }
}

struct SchedulerAttentionSummaryView: View {
    let attention: JarvisSchedulerAttentionSummary
    @ObservedObject var notifications: SchedulerNotificationModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 14) {
                Label(
                    attention.attentionRequired ? "Attention required" : "No scheduler attention",
                    systemImage: attention.attentionRequired ? "bell.badge" : "bell"
                )
                Label("\(attention.dueCount) due", systemImage: "clock")
                Label("\(attention.runningCount) running", systemImage: "play.circle")
                Label("\(attention.failedCount) failed", systemImage: "exclamationmark.triangle")
                if attention.emergencyPaused {
                    Label("Paused", systemImage: "pause.circle")
                }
            }
            .font(.caption)
            .foregroundStyle(attention.attentionRequired ? .orange : .secondary)

            if !attention.items.isEmpty {
                Text(attention.items.prefix(4).map { "\($0.name): \($0.notificationKind)" }.joined(separator: " | "))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .textSelection(.enabled)
            } else if let nextDueAt = attention.nextDueAt {
                Text("Next scheduled handoff: \(nextDueAt)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 8) {
                Button {
                    Task { await notifications.requestAuthorization() }
                } label: {
                    Label("Authorize", systemImage: "bell")
                }

                Button {
                    Task { await notifications.notify(attention: attention) }
                } label: {
                    Label("Notify", systemImage: "bell.badge")
                }
                .disabled(!attention.attentionRequired || notifications.isWorking)

                Text(notifications.status.label)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                Button {
                    notifications.resetDeliveredHistory()
                } label: {
                    Label("Reset", systemImage: "arrow.counterclockwise")
                }
                .disabled(notifications.lastDeliveredRequests.isEmpty)
            }
            .buttonStyle(.bordered)

            if !notifications.lastDeliveredRequests.isEmpty {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(notifications.lastDeliveredRequests.prefix(3)) { request in
                        let evidence = SchedulerNotificationEvidencePresentation(request: request)
                        Text(evidence.summary)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                            .textSelection(.enabled)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct SchedulerNotificationEvidencePresentation: Equatable {
    let summary: String

    init(request: JarvisSchedulerNotificationRequest) {
        summary = [
            "JARVIS_QA_NOTIFICATION_KIND=\(request.notificationKind)",
            "JARVIS_QA_NOTIFICATION_TITLE=\(request.title)",
            "JARVIS_QA_NOTIFICATION_BODY=\(request.body)",
            "JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER=\(request.threadIdentifier)",
        ].joined(separator: " | ")
    }
}

struct SpeechOutputEvidencePresentation: Equatable {
    let deviceLabelField: String
    let evidenceNoteField: String

    init(statusText: String, lastSpokenText: String?) {
        deviceLabelField = "JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL=<record actual output device>"
        let spokenText = lastSpokenText?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let spokenText, !spokenText.isEmpty {
            evidenceNoteField = "JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=Observed playback for \"\(spokenText)\"; \(statusText)"
        } else {
            evidenceNoteField = "JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=Record playback observation after Speak Preview; \(statusText)"
        }
    }
}

struct ApprovalCenterView: View {
    @ObservedObject var model: ApprovalManagementModel
    @State private var decisionReasons: [UUID: String] = [:]

    var body: some View {
        ManagementListView(
            title: "Approvals",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List {
                PermissionSurfaceSummaryView(surface: model.permissionSurface)

                if let grantSummary = model.grantSummary {
                    PermissionGrantHistoryView(summary: grantSummary)
                }

                if let policyReview = model.policyReview {
                    PermissionPolicyReviewView(review: policyReview)
                }

                if let limitation = model.limitationText {
                    Text(limitation)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if let decision = model.lastDecision {
                    Text("\(decision.action) marked \(decision.status)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if let execution = model.lastExecution {
                    Text("\(execution.approval.action) executed: \(execution.task.status)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                ForEach(model.pendingItems) { item in
                    VStack(alignment: .leading, spacing: 5) {
                        HStack {
                            Text(item.title)
                                .font(.subheadline)
                            Spacer()
                            Text(item.approvalStatus)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Text(item.detail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(3)
                            .textSelection(.enabled)
                        approvalMetadata(for: item)
                        if item.actionAvailable {
                            TextField("Decision reason", text: reasonBinding(for: item.id))
                                .textFieldStyle(.roundedBorder)
                            HStack {
                                Button {
                                    decide(item.id, approved: true)
                                } label: {
                                    Label("Approve", systemImage: "checkmark.circle")
                                }
                                Button(role: .destructive) {
                                    decide(item.id, approved: false)
                                } label: {
                                    Label("Deny", systemImage: "xmark.circle")
                                }
                            }
                            .buttonStyle(.bordered)
                            .disabled(model.isLoading)
                        } else if item.executionAvailable {
                            Button {
                                execute(item.id)
                            } label: {
                                Label("Run Approved", systemImage: "play.circle")
                            }
                            .buttonStyle(.bordered)
                            .disabled(model.isLoading)
                        } else {
                            Text("Inspection only")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private func approvalMetadata(for item: JarvisApprovalQueueItem) -> some View {
        let metadata = [
            item.riskTier.map { "risk: \($0)" },
            item.sensitivity.map { "sensitivity: \($0)" },
            item.requestedScopes.isEmpty ? nil : "scopes: \(item.requestedScopes.joined(separator: ", "))",
            item.requestedAt.map { "requested: \($0)" }
        ].compactMap { $0 }

        return Text(metadata.isEmpty ? "Core approval action exposed" : metadata.joined(separator: " | "))
            .font(.caption2)
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
    }

    private func reasonBinding(for id: UUID) -> Binding<String> {
        Binding(
            get: { decisionReasons[id, default: ""] },
            set: { decisionReasons[id] = $0 }
        )
    }

    private func decide(_ id: UUID, approved: Bool) {
        let reason = decisionReasons[id]
        Task {
            if approved {
                await model.approve(id: id, reason: reason)
            } else {
                await model.deny(id: id, reason: reason)
            }
            if model.lastError == nil {
                decisionReasons[id] = nil
            }
        }
    }

    private func execute(_ id: UUID) {
        Task {
            await model.execute(id: id)
            decisionReasons[id] = nil
        }
    }
}

struct PermissionGrantHistoryView: View {
    let summary: JarvisPermissionGrantSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Grant history", systemImage: "clock.badge.checkmark")
                .font(.subheadline)

            Text(historyText)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)

            if !summary.installedPluginGrants.isEmpty {
                ForEach(summary.installedPluginGrants) { grant in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Label(grant.name, systemImage: grant.needsProvenanceReview ? "exclamationmark.shield" : "checkmark.shield")
                                .font(.caption)
                            Spacer()
                            Text(grant.integrityStatus)
                                .font(.caption2)
                                .foregroundStyle(grant.needsProvenanceReview ? .orange : .secondary)
                        }

                        Text(pluginGrantDetail(grant))
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                }
            }
        }
        .padding(.vertical, 4)
    }

    private var historyText: String {
        let pending = summary.count(for: "pending")
        let approved = summary.count(for: "approved")
        let denied = summary.count(for: "denied")
        let approvalGate = summary.sideEffectsRequireApproval ? "side effects approval-gated" : "approval gate unknown"
        return "pending \(pending) | approved \(approved) | denied \(denied) | high-risk pending \(summary.highRiskPendingCount) | unverified plugins \(summary.unverifiedInstalledPluginCount) | \(approvalGate)"
    }

    private func pluginGrantDetail(_ grant: JarvisInstalledPluginGrantSurface) -> String {
        let executable = grant.executionEnabled ? "executable" : "disabled"
        let verifiedAt = grant.lastVerifiedAt.map { "verified \($0)" } ?? "not verified"
        let origin = grant.originClaim.map {
            grant.originClaimVerified ? "origin \($0) verified" : "origin \($0) unverified"
        } ?? "origin unknown"
        return "\(grant.pluginId): \(grant.executionGrant), \(executable), \(grant.captureMethod), \(verifiedAt), \(origin), high-risk actions \(grant.highRiskActionCount)"
    }
}

struct PermissionPolicyReviewView: View {
    let review: JarvisPermissionPolicyReview

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(statusTitle, systemImage: review.reviewItemCount == 0 ? "checkmark.shield" : "shield.lefthalf.filled")
                    .font(.subheadline)
                Spacer()
                Text("\(review.reviewItemCount) item(s)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text(summaryText)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)

            ForEach(review.items.prefix(6)) { item in
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(item.title)
                            .font(.caption)
                        Spacer()
                        Text(item.severity)
                            .font(.caption2)
                            .foregroundStyle(severityColor(item.severity))
                    }
                    Text(item.detail)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(.vertical, 4)
    }

    private var statusTitle: String {
        review.status == "clear" ? "Policy review clear" : "Policy review required"
    }

    private var summaryText: String {
        let gate = review.sideEffectsRequireApproval ? "side effects approval-gated" : "approval gate unknown"
        return "high-risk pending \(review.highRiskPendingCount) | executable plugins \(review.executableInstalledPluginCount) | unverified plugins \(review.unverifiedInstalledPluginCount) | \(gate)"
    }

    private func severityColor(_ severity: String) -> Color {
        switch severity {
        case "critical", "high":
            return .red
        case "medium":
            return .orange
        default:
            return .secondary
        }
    }
}

struct PermissionSurfaceSummaryView: View {
    let surface: JarvisPermissionSurfaceState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(statusTitle, systemImage: statusIcon)
                    .font(.subheadline)
                Spacer()
                Text(surface.approvalActionsAvailable ? "Decisions enabled" : "Inspection only")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Text(surface.summaryText)
                .font(.caption)
                .foregroundStyle(.secondary)

            Text(permissionDetails)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 4)
    }

    private var statusTitle: String {
        switch surface.status {
        case .clear:
            return "Permission surface clear"
        case .reviewRequired:
            return "Approval review required"
        case .inspectionOnly:
            return "Approval evidence visible"
        }
    }

    private var statusIcon: String {
        switch surface.status {
        case .clear:
            return "checkmark.shield"
        case .reviewRequired:
            return "exclamationmark.shield"
        case .inspectionOnly:
            return "eye"
        }
    }

    private var permissionDetails: String {
        let scopes = surface.declaredScopes.isEmpty
            ? "scopes: none declared"
            : "scopes: \(surface.declaredScopes.joined(separator: ", "))"
        let risks = surface.riskTierCounts.isEmpty
            ? "risk tiers: none declared"
            : "risk tiers: \(surface.riskTierCounts.map { "\($0.riskTier) \($0.count)" }.joined(separator: ", "))"
        let grants = "grants: approved \(surface.approvedGrantCount), denied \(surface.deniedGrantCount), installed \(surface.installedPluginGrantCount), executable \(surface.executableInstalledPluginGrantCount), unverified \(surface.unverifiedInstalledPluginGrantCount)"
        let sideEffects = surface.sideEffectsRequireApproval ? "side effects gated" : "side effects gate unknown"
        return "\(scopes) | \(risks) | proactive actions: \(surface.proactiveActionCount) | \(grants) | \(sideEffects)"
    }
}

struct RunManagementView: View {
    @ObservedObject var model: RunManagementModel

    var body: some View {
        ManagementListView(
            title: "Runs",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            HSplitView {
                List {
                    Button {
                        Task { await model.watchActivity() }
                    } label: {
                        Label("Watch Events", systemImage: "dot.radiowaves.left.and.right")
                    }
                    .disabled(model.isLoading)

                    if let activitySummary = model.activitySummary {
                        RunActivitySummaryView(summary: activitySummary)
                    }

                    if !model.activityEvents.isEmpty {
                        Section("Recent Event Stream") {
                            ForEach(model.activityEvents) { event in
                                RunActivityEventView(event: event)
                            }
                        }
                    }

                    ForEach(model.tasks) { task in
                        VStack(alignment: .leading, spacing: 5) {
                            Text(task.status)
                                .font(.subheadline)
                            Text(task.userInput)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                            Text(task.id.uuidString)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                        }
                        .padding(.vertical, 4)
                    }
                }
                .frame(minWidth: 300)

                List(model.auditEntries) { entry in
                    VStack(alignment: .leading, spacing: 5) {
                        Text(entry.eventType)
                            .font(.subheadline)
                        Text(entry.summary)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(3)
                        Text(entry.createdAt)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
                .frame(minWidth: 320)
            }
        }
    }
}

struct RunActivityEventView: View {
    let event: JarvisActivityEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 8) {
                Label(event.event, systemImage: eventIcon)
                    .font(.caption)
                    .foregroundStyle(event.error == nil ? Color.secondary : Color.orange)
                Spacer()
                Text("#\(event.sequence + 1)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if let summary = event.summary {
                Text("\(summary.activeTaskCount) active | \(summary.taskCount) tasks | \(summary.auditEntryCount) audit")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            } else if let progress = event.progress {
                Text(progressLabel(progress))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                    .textSelection(.enabled)
            } else if let error = event.error {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }
        }
        .padding(.vertical, 4)
    }

    private var eventIcon: String {
        if event.error != nil {
            return "exclamationmark.triangle"
        }
        if event.progress != nil {
            return "point.3.connected.trianglepath.dotted"
        }
        return "waveform.path"
    }

    private func progressLabel(_ progress: JarvisActivityProgressEvent) -> String {
        let source: String
        if let plugin = progress.pluginId {
            source = plugin
        } else if let model = progress.model {
            source = model
        } else if let provider = progress.provider {
            source = provider
        } else if progress.kind == "model_step" {
            source = "model"
        } else {
            source = "installed plugin"
        }
        let stage = progress.stage ?? "progress"
        let message = progress.message ?? "progress event"
        let sequence = progress.sequence.map { " #\($0)" } ?? ""
        if progress.kind == "model_output" {
            let byteCount = progress.byteCount.map { ", \($0) bytes" } ?? ""
            let charCount = progress.charCount.map { ", \($0) chars" } ?? ""
            let redacted = progress.contentRedacted == true ? ", redacted" : ""
            let transport = progress.providerNative == true ? " native transport" : " output"
            let terminal = progress.finalChunk == true ? ", terminal" : ""
            return "\(source)\(sequence)\(transport) chunk metadata\(byteCount)\(charCount)\(terminal)\(redacted)"
        }
        return "\(source)\(sequence) \(stage): \(message)"
    }
}

struct RunActivitySummaryView: View {
    let summary: JarvisActivitySummary

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                Label("\(summary.activeTaskCount) active", systemImage: "waveform.path.ecg")
                Label("\(summary.taskCount) tasks", systemImage: "list.bullet.rectangle")
                Label("\(summary.auditEntryCount) audit", systemImage: "doc.text.magnifyingglass")
            }
            .font(.caption)
            .foregroundStyle(summary.activeTaskCount > 0 ? .orange : .secondary)

            if !summary.statusCounts.isEmpty {
                Text(summary.statusCounts.map { "\($0.status) \($0.count)" }.joined(separator: " | "))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .textSelection(.enabled)
            }
        }
        .padding(.vertical, 4)
    }
}

struct DiagnosticsExportView: View {
    @ObservedObject var model: DiagnosticsModel

    var body: some View {
        ManagementListView(
            title: "Diagnostics",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List {
                if let export = model.export {
                    LabelValueRow(label: "Generated", value: export.generatedAt)
                    LabelValueRow(label: "Core", value: "\(export.health.status) \(export.health.version)")
                    LabelValueRow(label: "Repository", value: export.repositoryBacked ? "backed" : "in memory")
                    LabelValueRow(label: "Schema", value: export.schemaVersion.map(String.init) ?? "none")
                    LabelValueRow(label: "Tasks", value: export.taskCount.map(String.init) ?? "unknown")
                    LabelValueRow(label: "Audit Entries", value: export.auditEntryCount.map(String.init) ?? "unknown")
                    LabelValueRow(label: "Active Memory", value: export.activeMemoryItemCount.map(String.init) ?? "unknown")
                    LabelValueRow(label: "Redaction", value: export.redaction)
                    ForEach(export.schedulerJobs) { job in
                        LabelValueRow(
                            label: "Scheduler \(job.name)",
                            value: "\(job.status), cancellation reason present: \(job.cancellationReasonPresent)"
                        )
                    }
                } else {
                    Text("No diagnostics loaded")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

struct ReleaseReadinessView: View {
    @ObservedObject var model: ReleaseReadinessModel

    var body: some View {
        ManagementListView(
            title: "Release Readiness",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List {
                if let readiness = model.readiness {
                    let presentation = ReleaseReadinessPresentation(
                        readiness: readiness,
                        effectiveProductionReady: model.effectiveProductionReady,
                        isShowingStaleReadiness: model.isShowingStaleReadiness
                    )
                    LabelValueRow(label: "Generated", value: readiness.generatedAt)
                    if let staleWarning = presentation.staleWarning {
                        Label(staleWarning, systemImage: "clock.badge.exclamationmark")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                    LabelValueRow(label: "Production Ready", value: presentation.productionReadyLine)
                    LabelValueRow(label: "External Evidence Mode", value: presentation.evidenceModeLine)
                    if let blockedReadinessWarning = presentation.blockedReadinessWarning {
                        Label(blockedReadinessWarning, systemImage: "lock.shield")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                    LabelValueRow(label: "Scope", value: readiness.readinessScope)
                    LabelValueRow(label: "Verified Features", value: String(readiness.verifiedFeatureCount))
                    LabelValueRow(label: "Pending Features", value: String(readiness.pendingFeatureCount))
                    LabelValueRow(label: "Proof Boundary", value: readiness.proofBoundary)

                    if let evidence = model.evidenceStatus {
                        Section("Evidence Status") {
                            LabelValueRow(label: "Generated", value: evidence.generatedAt)
                            LabelValueRow(label: "Complete", value: evidence.complete ? "yes" : "no")
                            LabelValueRow(label: "Satisfied", value: String(evidence.satisfiedCount))
                            LabelValueRow(label: "Missing", value: String(evidence.missingCount))
                            LabelValueRow(label: "Invalid", value: String(evidence.invalidCount))
                            LabelValueRow(label: "Proof Boundary", value: evidence.proofBoundary)
                            ForEach(evidence.items) { item in
                                ReleaseEvidenceStatusRow(item: item)
                            }
                        }
                    }

                    if let releaseRunbookWarning = model.releaseRunbookWarning {
                        Label(releaseRunbookWarning, systemImage: "book.closed")
                            .font(.caption)
                            .foregroundStyle(.orange)
                            .textSelection(.enabled)
                    }

                    if !model.releaseRunbooks.isEmpty {
                        Section("Release Runbooks") {
                            ForEach(model.releaseRunbooks) { runbook in
                                ReleaseRunbookRow(
                                    runbook: runbook,
                                    effectiveProductionReady: presentation.effectiveProductionReady
                                )
                            }
                        }
                    }

                    Section("Blocking Gates") {
                        ForEach(readiness.blockingManualGates, id: \.self) { gate in
                            Label(gate, systemImage: "exclamationmark.triangle")
                                .font(.caption)
                                .foregroundStyle(.orange)
                                .textSelection(.enabled)
                        }
                    }

                    Section("Verification") {
                        ForEach(readiness.recommendedVerificationCommands, id: \.self) { command in
                            Text(command)
                                .font(.caption.monospaced())
                                .textSelection(.enabled)
                        }
                    }

                    Section("Implemented") {
                        ForEach(readiness.implementedFeatures) { feature in
                            ReleaseReadinessFeatureRow(feature: feature)
                        }
                    }

                    Section("Pending") {
                        ForEach(readiness.pendingFeatures) { feature in
                            ReleaseReadinessFeatureRow(feature: feature)
                        }
                    }
                } else {
                    Text("No release readiness loaded")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

struct ReleaseReadinessPresentation: Equatable {
    let productionReadyLine: String
    let evidenceModeLine: String
    let staleWarning: String?
    let blockedReadinessWarning: String?
    let effectiveProductionReady: Bool

    init(
        readiness: JarvisReleaseReadiness,
        effectiveProductionReady: Bool,
        isShowingStaleReadiness: Bool
    ) {
        self.productionReadyLine = effectiveProductionReady ? "yes" : "no"
        self.evidenceModeLine = readiness.evidenceModeEnabled ? "yes" : "no"
        self.staleWarning = isShowingStaleReadiness
            ? "Showing cached readiness; refresh failed."
            : nil
        self.blockedReadinessWarning = readiness.productionReady && !effectiveProductionReady
            ? "Readiness claim is blocked until current evidence status is complete."
            : nil
        self.effectiveProductionReady = effectiveProductionReady
    }
}

struct ReleaseRunbookRow: View {
    let runbook: JarvisReleaseRunbook
    let effectiveProductionReady: Bool

    private var presentation: ReleaseRunbookPresentation {
        ReleaseRunbookPresentation(
            runbook: runbook,
            effectiveProductionReady: effectiveProductionReady
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title)
                    .font(.subheadline)
                Spacer()
                Text(presentation.readinessLine)
                    .font(.caption)
                    .foregroundStyle(presentation.isReady ? .green : .orange)
            }
            if let liveVoiceFeature = runbook.liveVoiceFeature {
                Text("\(liveVoiceFeature.key): \(liveVoiceFeature.status)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            ForEach(runbook.evidenceItems) { item in
                HStack {
                    Text(item.label)
                    Spacer()
                    Text(item.status)
                        .foregroundStyle(item.status == "present" ? .green : .orange)
                }
                .font(.caption)
            }
            if !presentation.commands.isEmpty {
                Text("Commands")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                ForEach(Array(presentation.commands.enumerated()), id: \.offset) { _, command in
                    Text(command)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            if !presentation.manualChecks.isEmpty {
                Text("Manual Checks")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                ForEach(Array(presentation.manualChecks.enumerated()), id: \.offset) { _, manualCheck in
                    Text(manualCheck)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
            Text(runbook.proofBoundary)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 4)
    }

    private var title: String {
        switch runbook.runbook {
        case "signed_distribution":
            return "Signed Distribution"
        case "live_device":
            return "Live Device"
        case "plugin_trust":
            return "Plugin Trust"
        default:
            return runbook.runbook
        }
    }
}

struct ReleaseRunbookPresentation: Equatable {
    let readinessLine: String
    let isReady: Bool
    let commands: [String]
    let manualChecks: [String]
    let liveVoiceFeatureLine: String?

    init(runbook: JarvisReleaseRunbook, effectiveProductionReady: Bool) {
        let blockedByEffectiveReadiness = runbook.productionReady && !effectiveProductionReady
        readinessLine = blockedByEffectiveReadiness ? "blocked" : (runbook.productionReady ? "ready" : "not ready")
        isReady = runbook.productionReady && effectiveProductionReady
        commands = runbook.commands
        manualChecks = runbook.manualChecks
        liveVoiceFeatureLine = runbook.liveVoiceFeature.map { "\($0.key): \($0.status)" }
    }
}

struct ReleaseEvidenceStatusRow: View {
    let item: JarvisReleaseEvidenceStatusItem
    private var presentation: ReleaseEvidenceStatusPresentation {
        ReleaseEvidenceStatusPresentation(item: item)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(item.label)
                    .font(.subheadline)
                Spacer()
                Text(presentation.statusLine)
                    .font(.caption)
                    .foregroundStyle(item.status == "present" ? .green : .orange)
            }
            Text(presentation.pathLine)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(presentation.detailLine)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(presentation.requirementLine)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 4)
    }
}

struct ReleaseEvidenceStatusPresentation: Equatable {
    let statusLine: String
    let pathLine: String
    let detailLine: String
    let requirementLine: String

    init(item: JarvisReleaseEvidenceStatusItem) {
        if item.status == "present", item.detail.contains("presence only") {
            statusLine = "\(item.status); presence-only caveat"
        } else {
            statusLine = item.status
        }
        pathLine = "Path: \(item.path)"
        detailLine = "Detail: \(item.detail)"
        if item.requiredForProduction && item.manualGate {
            requirementLine = "Required for production; manual evidence gate"
        } else if item.requiredForProduction {
            requirementLine = "Required for production"
        } else if item.manualGate {
            requirementLine = "Manual evidence gate"
        } else {
            requirementLine = "Informational evidence"
        }
    }
}

struct ReleaseReadinessFeatureRow: View {
    let feature: JarvisReleaseReadinessFeature

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(feature.key)
                    .font(.subheadline)
                Spacer()
                Text(feature.status)
                    .font(.caption)
                    .foregroundStyle(feature.status == "implemented" ? .green : .orange)
            }
            Text(feature.proof)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(feature.boundary)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(.vertical, 4)
    }
}

struct VoiceStateView: View {
    @ObservedObject var model: VoiceStateModel
    @ObservedObject var adapter: VoiceAdapterStateModel
    @ObservedObject var speechOutput: SpeechOutputStateModel
    @ObservedObject var console: CommandConsoleModel
    @State private var speechPreview = "Jarvis voice output is ready."
    private var speechOutputEvidence: SpeechOutputEvidencePresentation {
        SpeechOutputEvidencePresentation(
            statusText: speechOutput.statusText,
            lastSpokenText: speechOutput.lastSpokenText
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Voice")
                .font(.headline)
            Text(model.statusText)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(adapter.statusText)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Text(adapter.permissionStatusText)
                .foregroundStyle(adapter.permissionState == .granted ? Color.secondary : Color.orange)
                .textSelection(.enabled)
            Text(speechOutput.statusText)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            VStack(alignment: .leading, spacing: 3) {
                Text(speechOutputEvidence.deviceLabelField)
                Text(speechOutputEvidence.evidenceNoteField)
            }
            .font(.caption2.monospaced())
            .foregroundStyle(.secondary)
            .textSelection(.enabled)

            Text(model.isPushToTalkEnabled ? "Manual capture ready." : "Manual capture unavailable in the current voice state.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            Toggle(
                "Auto-submit final transcript",
                isOn: Binding(
                    get: { adapter.isFinalTranscriptAutoSubmitEnabled },
                    set: { adapter.setFinalTranscriptAutoSubmitEnabled($0) }
                )
            )
            .disabled(!adapter.isFinalTranscriptAutoSubmitToggleEnabled)
            if let blockedReason = adapter.autoSubmitAvailability.blockedReason {
                Text(blockedReason)
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }
            Text("Live capture uses the macOS Speech/AVFoundation adapter and the same transcript handoff path as typed commands. Speech output uses an AVFoundation adapter. Release claims still require entitlements and manual device validation.")
                .font(.caption)
                .foregroundStyle(.secondary)

            TextField(
                "Typed transcript",
                text: Binding(
                    get: { model.transcriptDraft },
                    set: { model.apply(.updateTranscript($0)) }
                )
            )
            .textFieldStyle(.roundedBorder)
            .onSubmit(sendTranscript)

            HStack {
                Button("Request Permissions") {
                    Task { await adapter.requestPermissions() }
                }
                .disabled(!adapter.canRequestPermissions)
                Button(adapter.isCaptureActive ? "Listening" : "Start Capture") {
                    Task { await adapter.startCapture() }
                }
                .disabled(!adapter.canStartCapture)
                Button("Stop Capture") {
                    Task { await adapter.stopCapture() }
                }
                .disabled(!adapter.isCaptureActive)
                Button("Interrupt Capture") {
                    Task { await adapter.interrupt(reason: "User interrupted voice capture.") }
                }
                .disabled(!adapter.isCaptureActive)
            }

            TextField("Speech output preview", text: $speechPreview)
                .textFieldStyle(.roundedBorder)
                .onSubmit(speakPreview)

            HStack {
                Button(speechOutput.isSpeaking ? "Speaking" : "Speak Preview") {
                    speakPreview()
                }
                .disabled(!speechOutput.canSpeak || speechPreview.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Stop Speech") {
                    Task { await speechOutput.stop() }
                }
                .disabled(!speechOutput.isSpeaking)
                Button("Interrupt Speech") {
                    Task { await speechOutput.interrupt(reason: "User interrupted speech output.") }
                }
                .disabled(!speechOutput.isSpeaking)
            }

            HStack {
                Button("Start Transcript") {
                    model.apply(.beginTranscript)
                }
                Button("Send as Command") {
                    sendTranscript()
                }
                .disabled(console.isWorking || model.transcriptDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Interrupt") {
                    model.interruptTranscript(reason: "User interrupted transcript staging.")
                }
                Button("Resume") {
                    model.apply(.resumeInterruptedTranscript)
                }
                Button("Cancel") {
                    model.apply(.cancelTranscript)
                }
                #if DEBUG
                Button("Mark Voice Degraded") {
                    model.markDegraded(reason: "Voice capture or playback is degraded; typed transcript fallback remains available.")
                }
                Button("Mark Mic Permission Missing") {
                    model.setUnavailable(reason: "Microphone permission is missing or unavailable.")
                }
                Button("Reset Text Only") {
                    model.resetTextOnly()
                }
                #endif
            }

            if let handoff = model.lastHandoff {
                Text("Last handoff: \(handoff.source), dry-run \(handoff.dryRun ? "enabled" : "disabled")")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }

            if let lastError = model.lastError {
                Text(lastError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            if let lastError = adapter.lastError {
                Text(lastError.description)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
            if let lastError = speechOutput.lastError {
                Text(lastError.description)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }

            Spacer()
        }
        .padding()
    }

    private func sendTranscript() {
        guard let handoff = model.apply(.submitTranscript) else { return }
        Task {
            await console.submit(input: handoff.text, dryRun: handoff.dryRun)
        }
    }

    private func speakPreview() {
        Task {
            await speechOutput.speak(speechPreview)
        }
    }
}

struct ManagementListView<Content: View>: View {
    var title: String
    var isLoading: Bool
    var lastError: String?
    var refresh: () async -> Void
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(title)
                    .font(.headline)
                Spacer()
                Button(isLoading ? "Loading" : "Refresh") {
                    Task { await refresh() }
                }
                .disabled(isLoading)
            }
            .padding()

            if let lastError {
                Text(lastError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal)
                    .padding(.bottom, 6)
            }

            content
        }
        .task {
            await refresh()
        }
    }
}

struct LabelValueRow: View {
    var label: String
    var value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .textSelection(.enabled)
        }
        .padding(.vertical, 4)
    }
}

struct ActivityAuditView: View {
    var entries: [ActivityEntry]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Activity")
                .font(.headline)
                .padding([.horizontal, .top])

            List(entries) { entry in
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 6) {
                        Text(entry.title)
                            .font(.subheadline)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        Text(entry.badge)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Text(entry.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                        .textSelection(.enabled)
                }
                .padding(.vertical, 4)
            }
        }
    }
}
