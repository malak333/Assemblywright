import Foundation
import JarvisMacCore
import UserNotifications

protocol JarvisUserNotificationCenter: Sendable {
    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool
    func add(_ request: UNNotificationRequest) async throws
}

private final class SystemUserNotificationCenter: JarvisUserNotificationCenter, @unchecked Sendable {
    private let notificationCenter: UNUserNotificationCenter

    init(notificationCenter: UNUserNotificationCenter) {
        self.notificationCenter = notificationCenter
    }

    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool {
        try await notificationCenter.requestAuthorization(options: options)
    }

    func add(_ request: UNNotificationRequest) async throws {
        try await notificationCenter.add(request)
    }
}

private struct UnavailableUserNotificationCenter: JarvisUserNotificationCenter {
    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool {
        false
    }

    func add(_ request: UNNotificationRequest) async throws {
        throw CocoaError(.featureUnsupported)
    }
}

actor MacSchedulerNotificationAdapter: JarvisSchedulerNotificationAdapter {
    private let notificationCenter: any JarvisUserNotificationCenter

    init(notificationCenter: any JarvisUserNotificationCenter = MacSchedulerNotificationAdapter.defaultNotificationCenter()) {
        self.notificationCenter = notificationCenter
    }

    static func defaultNotificationCenter(bundleURL: URL = Bundle.main.bundleURL) -> any JarvisUserNotificationCenter {
        guard bundleURL.pathExtension == "app" else {
            return UnavailableUserNotificationCenter()
        }

        return SystemUserNotificationCenter(notificationCenter: .current())
    }

    func requestAuthorization() async throws -> Bool {
        try await notificationCenter.requestAuthorization(options: [.alert, .sound])
    }

    func deliver(_ request: JarvisSchedulerNotificationRequest) async throws {
        let content = UNMutableNotificationContent()
        content.title = request.title
        content.body = request.body
        content.sound = .default
        content.threadIdentifier = request.threadIdentifier
        content.userInfo = [
            "scheduler_job_id": request.schedulerJobId.uuidString,
            "notification_kind": request.notificationKind
        ]

        let notification = UNNotificationRequest(
            identifier: request.id,
            content: content,
            trigger: nil
        )
        try await notificationCenter.add(notification)
    }
}
