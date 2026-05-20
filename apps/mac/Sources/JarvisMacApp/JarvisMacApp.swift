import JarvisMacCore
import SwiftUI

@main
struct JarvisMacApp: App {
    @StateObject private var model = CommandConsoleModel()

    var body: some Scene {
        WindowGroup("Jarvis") {
            CommandConsoleView(model: model)
                .task {
                    await model.refreshHealth()
                }
        }
        .commands {
            CommandMenu("Jarvis") {
                Button("Refresh Health") {
                    Task { await model.refreshHealth() }
                }
                .keyboardShortcut("r", modifiers: [.command])
            }
        }
    }
}

struct CommandConsoleView: View {
    @ObservedObject var model: CommandConsoleModel
    @State private var input = ""

    var body: some View {
        VStack(spacing: 0) {
            statusBar

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
        guard let health = model.health else {
            return "Core status unknown"
        }

        return "\(health.status) | \(health.commandRuntime) | jobs: \(health.schedulerJobs)"
    }

    private func send() {
        let command = input
        input = ""
        Task {
            await model.submit(input: command)
        }
    }
}
