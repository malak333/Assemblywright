import JarvisMacCore
import SwiftUI

@main
struct JarvisMacApp: App {
    @StateObject private var supervisor: JarvisCoreSupervisor
    @StateObject private var console: CommandConsoleModel
    @StateObject private var memory: MemoryManagerModel
    @StateObject private var plugins: PluginManagerModel
    @StateObject private var approvals: ApprovalManagementModel
    @StateObject private var runs: RunManagementModel
    @StateObject private var scheduler: SchedulerModel
    @StateObject private var schedulerNotifications: SchedulerNotificationModel
    @StateObject private var diagnostics: DiagnosticsModel
    @StateObject private var releaseReadiness: ReleaseReadinessModel
    @StateObject private var voice: VoiceStateModel
    @StateObject private var voiceAdapter: VoiceAdapterStateModel
    @StateObject private var speechOutput: SpeechOutputStateModel

    init() {
        let configuration = JarvisCoreSupervisorConfiguration()
        let client = JarvisIPCClient(endpoint: configuration.endpoint)
        let console = CommandConsoleModel(client: client)
        let voice = VoiceStateModel()
        _supervisor = StateObject(wrappedValue: JarvisCoreSupervisor(configuration: configuration, client: client))
        _console = StateObject(wrappedValue: console)
        _memory = StateObject(wrappedValue: MemoryManagerModel(client: client))
        _plugins = StateObject(wrappedValue: PluginManagerModel(client: client))
        _approvals = StateObject(wrappedValue: ApprovalManagementModel(client: client))
        _runs = StateObject(wrappedValue: RunManagementModel(client: client))
        _scheduler = StateObject(wrappedValue: SchedulerModel(client: client))
        _schedulerNotifications = StateObject(
            wrappedValue: SchedulerNotificationModel(adapter: MacSchedulerNotificationAdapter())
        )
        _diagnostics = StateObject(wrappedValue: DiagnosticsModel(client: client))
        _releaseReadiness = StateObject(wrappedValue: ReleaseReadinessModel(client: client))
        _voice = StateObject(wrappedValue: voice)
        _voiceAdapter = StateObject(
            wrappedValue: VoiceAdapterStateModel(
                adapter: MacSpeechVoiceAdapter(),
                voiceState: voice,
                shouldAutoSubmitFinalTranscript: { !console.isWorking },
                submitFinalTranscript: { handoff in
                    await console.submit(input: handoff.text, dryRun: handoff.dryRun)
                }
            )
        )
        _speechOutput = StateObject(wrappedValue: SpeechOutputStateModel(adapter: MacSpeechOutputAdapter()))
    }

    var body: some Scene {
        WindowGroup("Jarvis") {
            JarvisShellView(
                supervisor: supervisor,
                console: console,
                memory: memory,
                plugins: plugins,
                approvals: approvals,
                runs: runs,
                scheduler: scheduler,
                schedulerNotifications: schedulerNotifications,
                diagnostics: diagnostics,
                releaseReadiness: releaseReadiness,
                voice: voice,
                voiceAdapter: voiceAdapter,
                speechOutput: speechOutput
            )
                .task {
                    await supervisor.start()
                    if supervisor.isAvailable {
                        await console.refreshHealth()
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
                    }
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }
    }
}

struct JarvisShellView: View {
    @ObservedObject var supervisor: JarvisCoreSupervisor
    @ObservedObject var console: CommandConsoleModel
    @ObservedObject var memory: MemoryManagerModel
    @ObservedObject var plugins: PluginManagerModel
    @ObservedObject var approvals: ApprovalManagementModel
    @ObservedObject var runs: RunManagementModel
    @ObservedObject var scheduler: SchedulerModel
    @ObservedObject var schedulerNotifications: SchedulerNotificationModel
    @ObservedObject var diagnostics: DiagnosticsModel
    @ObservedObject var releaseReadiness: ReleaseReadinessModel
    @ObservedObject var voice: VoiceStateModel
    @ObservedObject var voiceAdapter: VoiceAdapterStateModel
    @ObservedObject var speechOutput: SpeechOutputStateModel

    var body: some View {
        VStack(spacing: 0) {
            CoreStatusBanner(supervisor: supervisor)

            TabView {
                CommandConsoleView(model: console)
                    .tabItem { Text("Console") }
                MemoryManagerView(model: memory)
                    .tabItem { Text("Memory") }
                PluginManagerView(model: plugins)
                    .tabItem { Text("Plugins") }
                ApprovalCenterView(model: approvals)
                    .tabItem { Text("Approvals") }
                RunManagementView(model: runs)
                    .tabItem { Text("Runs") }
                SchedulerJobsView(model: scheduler, notifications: schedulerNotifications)
                    .tabItem { Text("Scheduler") }
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

struct CoreStatusBanner: View {
    @ObservedObject var supervisor: JarvisCoreSupervisor

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
                Task { await supervisor.start() }
            }
            .disabled(supervisor.isAvailable)

            Button("Stop") {
                supervisor.stop()
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
                    .onSubmit(send)
                Button("Send", action: send)
                    .disabled(model.isWorking || input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding()
        }
        .frame(minWidth: 720, minHeight: 480)
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

struct PluginManagerView: View {
    @ObservedObject var model: PluginManagerModel

    var body: some View {
        ManagementListView(
            title: "Plugins",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List {
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
                                Text(plugin.sourcePath)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                    .textSelection(.enabled)
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
}

struct SchedulerJobsView: View {
    @ObservedObject var model: SchedulerModel
    @ObservedObject var notifications: SchedulerNotificationModel
    @State private var name = "manual check"
    @State private var command = "status check"
    @State private var runAt = "2026-05-20T13:00:00Z"
    @State private var intervalSeconds = "3600"
    @State private var triggerMode = "manual"

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
            }
            .buttonStyle(.bordered)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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
        let plugin = progress.pluginId ?? "installed plugin"
        let stage = progress.stage ?? "progress"
        let message = progress.message ?? "progress event"
        let sequence = progress.sequence.map { " #\($0)" } ?? ""
        return "\(plugin)\(sequence) \(stage): \(message)"
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
                    LabelValueRow(label: "Generated", value: readiness.generatedAt)
                    LabelValueRow(label: "Production Ready", value: readiness.productionReady ? "yes" : "no")
                    LabelValueRow(label: "Scope", value: readiness.readinessScope)
                    LabelValueRow(label: "Verified Features", value: String(readiness.verifiedFeatureCount))
                    LabelValueRow(label: "Pending Features", value: String(readiness.pendingFeatureCount))
                    LabelValueRow(label: "Proof Boundary", value: readiness.proofBoundary)

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
            Text(speechOutput.statusText)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)

            Toggle("Push to talk", isOn: .constant(model.isPushToTalkEnabled))
                .disabled(true)
            Toggle(
                "Auto-submit final transcript",
                isOn: Binding(
                    get: { adapter.isFinalTranscriptAutoSubmitEnabled },
                    set: { adapter.setFinalTranscriptAutoSubmitEnabled($0) }
                )
            )
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
                Button("Mark Voice Degraded") {
                    model.markDegraded(reason: "Voice capture or playback is degraded; typed transcript fallback remains available.")
                }
                Button("Mark Mic Permission Missing") {
                    model.setUnavailable(reason: "Microphone permission is missing or unavailable.")
                }
                Button("Reset Text Only") {
                    model.resetTextOnly()
                }
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
