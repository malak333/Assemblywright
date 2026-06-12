import Foundation
import JarvisMacCore
import UserNotifications

protocol JarvisUserNotificationCenter: Sendable {
    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool
    func add(_ request: UNNotificationRequest) async throws
}

private final class SystemUserNotificationCenter: JarvisUserNotificationCenter, @unchecked Sendable {
    private let notificationCenter: UNUserNotificationCenter

    init(notificationCenter: UNUserNotificationCenter = .current()) {
        self.notificationCenter = notificationCenter
    }

    func requestAuthorization(options: UNAuthorizationOptions) async throws -> Bool {
        try await notificationCenter.requestAuthorization(options: options)
    }

    func add(_ request: UNNotificationRequest) async throws {
        try await notificationCenter.add(request)
    }
}

actor MacSchedulerNotificationAdapter: JarvisSchedulerNotificationAdapter {
    private let notificationCenter: any JarvisUserNotificationCenter

    init(notificationCenter: any JarvisUserNotificationCenter = SystemUserNotificationCenter()) {
        self.notificationCenter = notificationCenter
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
