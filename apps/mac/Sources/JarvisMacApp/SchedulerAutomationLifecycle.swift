import Combine
import JarvisMacCore

@MainActor
final class SchedulerAutomationLifecycle: ObservableObject {
    private var cancellables: Set<AnyCancellable> = []

    init(
        supervisor: JarvisCoreSupervisor,
        settings: SchedulerAutomationSettingsModel,
        coordinator: SchedulerAttentionCoordinator
    ) {
        Publishers.CombineLatest3(
            supervisor.$mode,
            supervisor.$activeSchedulerAutomationConfiguration,
            settings.$isEnabled
        )
        .sink { [weak coordinator] _, _, _ in
            coordinator?.reconcile()
        }
        .store(in: &cancellables)
    }
}
