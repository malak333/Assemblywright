import AppKit
import JarvisMacCore
import SwiftUI

enum JarvisMenuBarContract {
    static let mainWindowID = "assemblywright-main"
    static let title = "Assemblywright"
}

struct JarvisMenuBarPresentation: Equatable {
    let statusLine: String
    let systemImage: String

    /// Whether the menu bar draws a state badge beside the proofmark. A
    /// connected bridge shows the mark alone; every other phase earns an
    /// indicator.
    let showsStateBadge: Bool

    init(status: JarvisDeveloperBridgeAppStatus) {
        switch status.phase {
        case .disabled:
            statusLine = "Developer Mode disabled"
            systemImage = "circle"
            showsStateBadge = true
        case .starting:
            statusLine = "Bridge starting"
            systemImage = "circle.dotted"
            showsStateBadge = true
        case .connected:
            statusLine = "Bridge connected"
            systemImage = "checkmark.circle.fill"
            showsStateBadge = false
        case .masterOffline:
            statusLine = "Master offline"
            systemImage = "exclamationmark.triangle.fill"
            showsStateBadge = true
        case .maintenance:
            statusLine = "Master maintenance"
            systemImage = "wrench.and.screwdriver.fill"
            showsStateBadge = true
        case .paused:
            statusLine = "Bridge paused"
            systemImage = "pause.circle.fill"
            showsStateBadge = true
        case .stopped:
            statusLine = "Bridge stopped"
            systemImage = "circle"
            showsStateBadge = true
        }
    }
}

struct JarvisMenuBarView: View {
    @ObservedObject var developerBridge: JarvisDeveloperBridgeProcessLifecycle
    @Environment(\.openWindow) private var openWindow

    private var presentation: JarvisMenuBarPresentation {
        JarvisMenuBarPresentation(status: developerBridge.status)
    }

    var body: some View {
        Button("Open Assemblywright") {
            openWindow(id: JarvisMenuBarContract.mainWindowID)
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
        }

        Divider()

        Text(presentation.statusLine)

        Divider()

        Button("Quit Assemblywright") {
            NSApplication.shared.terminate(nil)
        }
    }
}
