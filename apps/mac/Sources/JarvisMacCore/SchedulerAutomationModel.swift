import Foundation

public struct JarvisSchedulerAutomationConfiguration: Equatable, Sendable {
    public static let defaultIntervalMilliseconds: UInt64 = 30_000
    public static let defaultRunLimit = 16
    public static let defaultStaleAgeSeconds: UInt64 = 3_600
    public static let defaultStaleRecoveryLimit = 16

    public let isEnabled: Bool
    public let intervalMilliseconds: UInt64
    public let runLimit: Int
    public let recoverStaleOnStartup: Bool
    public let staleAgeSeconds: UInt64
    public let staleRecoveryLimit: Int

    public init(
        isEnabled: Bool = false,
        intervalMilliseconds: UInt64 = Self.defaultIntervalMilliseconds,
        runLimit: Int = Self.defaultRunLimit,
        recoverStaleOnStartup: Bool = true,
        staleAgeSeconds: UInt64 = Self.defaultStaleAgeSeconds,
        staleRecoveryLimit: Int = Self.defaultStaleRecoveryLimit
    ) {
        self.isEnabled = isEnabled
        self.intervalMilliseconds = max(1_000, intervalMilliseconds)
        self.runLimit = min(64, max(1, runLimit))
        self.recoverStaleOnStartup = recoverStaleOnStartup
        self.staleAgeSeconds = max(60, staleAgeSeconds)
        self.staleRecoveryLimit = min(64, max(1, staleRecoveryLimit))
    }

    public var launchArguments: [String] {
        guard isEnabled else { return [] }

        var arguments = [
            "--scheduler-background",
            "--scheduler-interval-ms", String(intervalMilliseconds),
            "--scheduler-limit", String(runLimit)
        ]
        if recoverStaleOnStartup {
            arguments.append(contentsOf: [
                "--scheduler-recover-stale-on-startup",
                "--scheduler-stale-older-than-seconds", String(staleAgeSeconds),
                "--scheduler-stale-recovery-limit", String(staleRecoveryLimit)
            ])
        }
        return arguments
    }
}

@MainActor
public protocol JarvisSchedulerAutomationConfigurationProviding: AnyObject {
    var schedulerAutomationConfiguration: JarvisSchedulerAutomationConfiguration { get }
}

@MainActor
public final class StaticSchedulerAutomationConfigurationProvider:
    JarvisSchedulerAutomationConfigurationProviding
{
    public let schedulerAutomationConfiguration: JarvisSchedulerAutomationConfiguration

    public init(
        configuration: JarvisSchedulerAutomationConfiguration = .init()
    ) {
        self.schedulerAutomationConfiguration = configuration
    }
}

@MainActor
public final class SchedulerAutomationSettingsModel: ObservableObject,
    JarvisSchedulerAutomationConfigurationProviding
{
    public static let enabledEnvironmentKey = "JARVIS_MAC_SCHEDULER_AUTOMATION_ENABLED"
    public static let intervalEnvironmentKey = "JARVIS_MAC_SCHEDULER_AUTOMATION_INTERVAL_MS"

    @Published public private(set) var isEnabled: Bool
    @Published public private(set) var intervalMilliseconds: UInt64
    @Published public private(set) var runLimit: Int
    @Published public private(set) var recoverStaleOnStartup: Bool
    @Published public private(set) var staleAgeSeconds: UInt64
    @Published public private(set) var staleRecoveryLimit: Int

    private enum Key {
        static let enabled = "schedulerAutomation.enabled"
        static let intervalMilliseconds = "schedulerAutomation.intervalMilliseconds"
        static let runLimit = "schedulerAutomation.runLimit"
        static let recoverStaleOnStartup = "schedulerAutomation.recoverStaleOnStartup"
        static let staleAgeSeconds = "schedulerAutomation.staleAgeSeconds"
        static let staleRecoveryLimit = "schedulerAutomation.staleRecoveryLimit"
    }

    private let defaults: UserDefaults
    private var persistedIsEnabled: Bool
    private let environmentEnabled: Bool

    public init(
        defaults: UserDefaults = .standard,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) {
        self.defaults = defaults
        let environmentEnabled = environment[Self.enabledEnvironmentKey] == "true"
        let persistedIsEnabled = defaults.bool(forKey: Key.enabled)
        self.environmentEnabled = environmentEnabled
        self.persistedIsEnabled = persistedIsEnabled
        self.isEnabled = environmentEnabled || persistedIsEnabled
        self.intervalMilliseconds = UInt64(environment[Self.intervalEnvironmentKey] ?? "")
            ?? Self.uint64(
                defaults.object(forKey: Key.intervalMilliseconds),
                fallback: JarvisSchedulerAutomationConfiguration.defaultIntervalMilliseconds
            )
        self.runLimit = Self.integer(
            defaults.object(forKey: Key.runLimit),
            fallback: JarvisSchedulerAutomationConfiguration.defaultRunLimit
        )
        self.recoverStaleOnStartup = defaults.object(forKey: Key.recoverStaleOnStartup) == nil
            ? true
            : defaults.bool(forKey: Key.recoverStaleOnStartup)
        self.staleAgeSeconds = Self.uint64(
            defaults.object(forKey: Key.staleAgeSeconds),
            fallback: JarvisSchedulerAutomationConfiguration.defaultStaleAgeSeconds
        )
        self.staleRecoveryLimit = Self.integer(
            defaults.object(forKey: Key.staleRecoveryLimit),
            fallback: JarvisSchedulerAutomationConfiguration.defaultStaleRecoveryLimit
        )
        normalizeAndPersist(persist: false)
    }

    public var schedulerAutomationConfiguration: JarvisSchedulerAutomationConfiguration {
        JarvisSchedulerAutomationConfiguration(
            isEnabled: isEnabled,
            intervalMilliseconds: intervalMilliseconds,
            runLimit: runLimit,
            recoverStaleOnStartup: recoverStaleOnStartup,
            staleAgeSeconds: staleAgeSeconds,
            staleRecoveryLimit: staleRecoveryLimit
        )
    }

    public func update(
        isEnabled: Bool? = nil,
        intervalMilliseconds: UInt64? = nil,
        runLimit: Int? = nil,
        recoverStaleOnStartup: Bool? = nil,
        staleAgeSeconds: UInt64? = nil,
        staleRecoveryLimit: Int? = nil
    ) {
        if let isEnabled { persistedIsEnabled = isEnabled }
        if let intervalMilliseconds { self.intervalMilliseconds = intervalMilliseconds }
        if let runLimit { self.runLimit = runLimit }
        if let recoverStaleOnStartup { self.recoverStaleOnStartup = recoverStaleOnStartup }
        if let staleAgeSeconds { self.staleAgeSeconds = staleAgeSeconds }
        if let staleRecoveryLimit { self.staleRecoveryLimit = staleRecoveryLimit }
        normalizeAndPersist(persist: true)
    }

    private func normalizeAndPersist(persist: Bool) {
        isEnabled = persistedIsEnabled || environmentEnabled
        let normalized = schedulerAutomationConfiguration
        intervalMilliseconds = normalized.intervalMilliseconds
        runLimit = normalized.runLimit
        staleAgeSeconds = normalized.staleAgeSeconds
        staleRecoveryLimit = normalized.staleRecoveryLimit
        guard persist else { return }
        defaults.set(persistedIsEnabled, forKey: Key.enabled)
        defaults.set(intervalMilliseconds, forKey: Key.intervalMilliseconds)
        defaults.set(runLimit, forKey: Key.runLimit)
        defaults.set(recoverStaleOnStartup, forKey: Key.recoverStaleOnStartup)
        defaults.set(staleAgeSeconds, forKey: Key.staleAgeSeconds)
        defaults.set(staleRecoveryLimit, forKey: Key.staleRecoveryLimit)
    }

    private static func uint64(_ value: Any?, fallback: UInt64) -> UInt64 {
        (value as? NSNumber)?.uint64Value ?? fallback
    }

    private static func integer(_ value: Any?, fallback: Int) -> Int {
        (value as? NSNumber)?.intValue ?? fallback
    }
}

@MainActor
public final class SchedulerAttentionCoordinator: ObservableObject {
    @Published public private(set) var isRunning = false
    @Published public private(set) var lastError: String?

    private let scheduler: SchedulerModel
    private let notifications: SchedulerNotificationModel
    private let settings: any JarvisSchedulerAutomationConfigurationProviding
    private let isCoreAvailable: () -> Bool
    private let pollInterval: Duration
    private var task: Task<Void, Never>?
    private var generation: UInt64 = 0

    public init(
        scheduler: SchedulerModel,
        notifications: SchedulerNotificationModel,
        settings: any JarvisSchedulerAutomationConfigurationProviding,
        pollInterval: Duration = .seconds(30),
        isCoreAvailable: @escaping () -> Bool
    ) {
        self.scheduler = scheduler
        self.notifications = notifications
        self.settings = settings
        self.pollInterval = pollInterval
        self.isCoreAvailable = isCoreAvailable
    }

    deinit {
        task?.cancel()
    }

    public func start() {
        stop()
        guard settings.schedulerAutomationConfiguration.isEnabled, isCoreAvailable() else { return }
        generation &+= 1
        let currentGeneration = generation
        isRunning = true
        task = Task { [weak self] in
            while let self, self.shouldContinue(generation: currentGeneration) {
                await self.pollOnce(generation: currentGeneration)
                guard self.shouldContinue(generation: currentGeneration) else { break }
                do {
                    try await Task.sleep(for: self.pollInterval)
                } catch {
                    break
                }
            }
            guard let self, self.generation == currentGeneration else { return }
            self.task = nil
            self.isRunning = false
        }
    }

    public func reconcile() {
        if settings.schedulerAutomationConfiguration.isEnabled, isCoreAvailable() {
            if !isRunning { start() }
        } else {
            stop()
        }
    }

    public func stop() {
        generation &+= 1
        task?.cancel()
        task = nil
        isRunning = false
    }

    public func pollOnce() async {
        await pollOnce(generation: nil)
    }

    private func pollOnce(generation expectedGeneration: UInt64?) async {
        guard settings.schedulerAutomationConfiguration.isEnabled, isCoreAvailable() else { return }
        if let expectedGeneration, !shouldContinue(generation: expectedGeneration) { return }
        do {
            async let attentionRequest = scheduler.fetchAttention()
            async let occurrenceRequest = scheduler.fetchPendingNotificationOccurrences()
            let (attention, occurrences) = try await (attentionRequest, occurrenceRequest)
            if let expectedGeneration, !shouldContinue(generation: expectedGeneration) { return }
            guard settings.schedulerAutomationConfiguration.isEnabled, isCoreAvailable() else { return }
            scheduler.applyAttention(attention)
            lastError = nil
            let acknowledgements = await notifications.notifyPendingOccurrencesIfAuthorized(
                occurrences
            ) { [weak self] in
                guard let self else { return false }
                if let expectedGeneration {
                    return self.shouldContinue(generation: expectedGeneration)
                }
                return self.settings.schedulerAutomationConfiguration.isEnabled
                    && self.isCoreAvailable()
            }
            var acknowledgementError: String?
            for acknowledgement in acknowledgements {
                if let expectedGeneration, !shouldContinue(generation: expectedGeneration) { return }
                guard settings.schedulerAutomationConfiguration.isEnabled,
                      isCoreAvailable() else { return }
                do {
                    try await scheduler.acknowledgeNotificationOccurrence(acknowledgement)
                } catch {
                    acknowledgementError = String(describing: error)
                }
            }
            lastError = acknowledgementError
        } catch {
            if let expectedGeneration, !shouldContinue(generation: expectedGeneration) { return }
            guard settings.schedulerAutomationConfiguration.isEnabled, isCoreAvailable() else { return }
            lastError = String(describing: error)
        }
    }

    private func shouldContinue(generation expectedGeneration: UInt64) -> Bool {
        !Task.isCancelled
            && generation == expectedGeneration
            && settings.schedulerAutomationConfiguration.isEnabled
            && isCoreAvailable()
    }
}
