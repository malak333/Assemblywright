import Foundation

@MainActor
public final class MemoryManagerModel: ObservableObject {
    @Published public private(set) var items: [JarvisMemoryItem]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.items = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh(includeDeleted: Bool = false) async {
        await run {
            self.items = try await self.client.listMemoryItems(includeDeleted: includeDeleted)
        }
    }

    public func create(category: String, key: String, value: String, provenance: String, sensitivity: String) async {
        await run {
            let item = try await self.client.createMemoryItem(
                JarvisCreateMemoryItemRequest(
                    category: category,
                    key: key,
                    value: value,
                    provenance: provenance,
                    sensitivity: sensitivity
                )
            )
            self.items.insert(item, at: 0)
        }
    }

    public func review(id: UUID) async {
        await replace(id: id) {
            try await self.client.reviewMemoryItem(id: id)
        }
    }

    public func delete(id: UUID) async {
        await replace(id: id) {
            try await self.client.deleteMemoryItem(id: id)
        }
    }

    private func replace(id: UUID, operation: @escaping () async throws -> JarvisMemoryItem) async {
        await run {
            let item = try await operation()
            if let index = self.items.firstIndex(where: { $0.id == id }) {
                self.items[index] = item
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class PluginManagerModel: ObservableObject {
    @Published public private(set) var manifests: [JarvisPluginManifest]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.manifests = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            manifests = try await client.listPluginManifests()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class SchedulerModel: ObservableObject {
    @Published public private(set) var jobs: [JarvisSchedulerJob]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.jobs = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        await run {
            self.jobs = try await self.client.listSchedulerJobs()
        }
    }

    public func scheduleManual(name: String, command: String) async {
        await run {
            let job = try await self.client.createSchedulerJob(
                JarvisCreateSchedulerJobRequest(name: name, command: command, trigger: .manual)
            )
            self.jobs.insert(job, at: 0)
        }
    }

    public func cancel(id: UUID) async {
        await run {
            let job = try await self.client.cancelSchedulerJob(id: id)
            if let index = self.jobs.firstIndex(where: { $0.id == id }) {
                self.jobs[index] = job
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class DiagnosticsModel: ObservableObject {
    @Published public private(set) var export: JarvisDiagnosticsExport?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.export = nil
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            export = try await client.diagnosticsExport()
        } catch {
            lastError = String(describing: error)
        }
    }
}
