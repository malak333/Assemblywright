import Foundation
import JarvisMacCore
import UserNotifications

actor MacSchedulerNotificationAdapter: JarvisSchedulerNotificationAdapter {
    func requestAuthorization() async throws -> Bool {
        try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
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
        try await UNUserNotificationCenter.current().add(notification)
    }
}
