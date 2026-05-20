import JarvisMacCore
import SwiftUI

@main
struct JarvisMacApp: App {
    @StateObject private var supervisor: JarvisCoreSupervisor
    @StateObject private var console: CommandConsoleModel
    @StateObject private var memory: MemoryManagerModel
    @StateObject private var plugins: PluginManagerModel
    @StateObject private var scheduler: SchedulerModel
    @StateObject private var diagnostics: DiagnosticsModel

    init() {
        let client = JarvisIPCClient()
        _supervisor = StateObject(wrappedValue: JarvisCoreSupervisor(client: client))
        _console = StateObject(wrappedValue: CommandConsoleModel(client: client))
        _memory = StateObject(wrappedValue: MemoryManagerModel(client: client))
        _plugins = StateObject(wrappedValue: PluginManagerModel(client: client))
        _scheduler = StateObject(wrappedValue: SchedulerModel(client: client))
        _diagnostics = StateObject(wrappedValue: DiagnosticsModel(client: client))
    }

    var body: some Scene {
        WindowGroup("Jarvis") {
            JarvisShellView(
                supervisor: supervisor,
                console: console,
                memory: memory,
                plugins: plugins,
                scheduler: scheduler,
                diagnostics: diagnostics
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
    @ObservedObject var scheduler: SchedulerModel
    @ObservedObject var diagnostics: DiagnosticsModel

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
                SchedulerJobsView(model: scheduler)
                    .tabItem { Text("Scheduler") }
                DiagnosticsExportView(model: diagnostics)
                    .tabItem { Text("Diagnostics") }
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

    var body: some View {
        ManagementListView(
            title: "Memory",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List(model.items) { item in
                VStack(alignment: .leading, spacing: 4) {
                    Text("\(item.category) / \(item.key)")
                        .font(.subheadline)
                    Text(item.value)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    Text("\(item.sensitivity) | \(item.provenance)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 4)
            }
        }
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
            List(model.manifests) { manifest in
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

struct SchedulerJobsView: View {
    @ObservedObject var model: SchedulerModel

    var body: some View {
        ManagementListView(
            title: "Scheduler",
            isLoading: model.isLoading,
            lastError: model.lastError,
            refresh: { await model.refresh() }
        ) {
            List(model.jobs) { job in
                VStack(alignment: .leading, spacing: 5) {
                    Text(job.name)
                        .font(.subheadline)
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
                } else {
                    Text("No diagnostics loaded")
                        .foregroundStyle(.secondary)
                }
            }
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
