import AppKit
import AssemblywrightMacCore
import SwiftUI
import UniformTypeIdentifiers

struct DeveloperModeSetupPresentation: Equatable {
  static let title = "Developer Mode Connection"
  static let setupRequired = "Setup required"
  static let helperReady = "Signed helper ready"
  static let pairingRequired = "Pair this Mac"
  static let masterOffline = "Windows master offline"
  static let connected = "Connected to Windows"

  let setupStatus: String
  let pairingStatus: String
  let connectionStatus: String
  let needsSetup: Bool
  let needsPairing: Bool
  let canRetry: Bool

  init(
    configurationState: AssemblywrightDeveloperBridgeConfigurationState,
    bridgeStatus: AssemblywrightDeveloperBridgeAppStatus,
    enrollmentInstalled: Bool?
  ) {
    switch configurationState {
    case .notConfigured:
      setupStatus = Self.setupRequired
      needsSetup = true
    case .invalidStore:
      setupStatus = "Saved setup needs repair"
      needsSetup = true
    case .configured:
      setupStatus = Self.helperReady
      needsSetup = false
    }

    let authenticated = bridgeStatus.deviceID != nil
      && bridgeStatus.masterEndpoint != nil
      && bridgeStatus.connectionEpoch != nil
    if authenticated || enrollmentInstalled == true {
      pairingStatus = "Mac identity installed"
      needsPairing = false
    } else if enrollmentInstalled == false {
      pairingStatus = Self.pairingRequired
      needsPairing = true
    } else {
      pairingStatus = "Check pairing status"
      needsPairing = false
    }

    switch bridgeStatus.phase {
    case .connected:
      connectionStatus = Self.connected
    case .maintenance:
      connectionStatus = "Windows maintenance"
    case .paused:
      connectionStatus = "Windows Emergency Pause"
    case .starting:
      connectionStatus = "Connecting…"
    case .masterOffline:
      connectionStatus = Self.masterOffline
    case .disabled:
      connectionStatus = Self.setupRequired
    case .stopped:
      connectionStatus = "Connection stopped"
    }
    canRetry = configurationState == .configured
      && bridgeStatus.phase != .starting
      && bridgeStatus.phase != .connected
  }
}

struct DeveloperModeSetupView: View {
  @ObservedObject var model: AssemblywrightDeveloperBridgeProcessLifecycle

  @State private var helperPath = ""
  @State private var teamIdentifier = ""
  @State private var choosingHelper = false
  @State private var enrollment: AssemblywrightDeveloperBridgeEnrollmentStatus?
  @State private var enrollmentInvitation = ""
  @State private var enrollmentReply = ""
  @State private var enrollmentReceipt = ""
  @State private var rotationInvitation = ""
  @State private var rotationReply = ""
  @State private var rotationReceipt = ""
  @State private var showingPairing = false
  @State private var showingRotation = false
  @State private var rotationInstalledAwaitingAcknowledgement = false
  @State private var rotationAcknowledgementGrantID: String?
  @State private var confirmsConfiguration = false
  @State private var editingConfiguration = false

  private var presentation: DeveloperModeSetupPresentation {
    DeveloperModeSetupPresentation(
      configurationState: model.bridgeConfigurationState,
      bridgeStatus: model.status,
      enrollmentInstalled: enrollment?.installed
    )
  }

  var body: some View {
    GroupBox(DeveloperModeSetupPresentation.title) {
      VStack(alignment: .leading, spacing: 12) {
        readinessRow("Helper", presentation.setupStatus,
          ready: model.bridgeConfigurationState == .configured)
        readinessRow("Pairing", presentation.pairingStatus,
          ready: enrollment?.installed == true || model.status.deviceID != nil)
        readinessRow("Windows", presentation.connectionStatus,
          ready: model.status.phase == .connected)

        if let endpoint = enrollment?.masterEndpoint ?? model.status.masterEndpoint {
          LabeledContent("Master", value: endpoint)
        }
        if let name = enrollment?.deviceName {
          LabeledContent("This Mac", value: name)
        }
        if let expiry = enrollment?.certificateNotAfterMilliseconds {
          LabeledContent("Certificate", value: certificateLabel(expiry))
        }

        if presentation.needsSetup || editingConfiguration {
          setupFields
        } else {
          HStack {
            Button("Check Pairing") { refreshEnrollment() }
              .disabled(model.ownerActionInProgress)
              .accessibilityIdentifier("developer-mode-check-pairing")
            if presentation.canRetry {
              Button("Retry Connection") {
                Task { await model.retryBridgeConnection() }
              }
              .disabled(model.ownerActionInProgress)
              .accessibilityIdentifier("developer-mode-retry-connection")
            }
            Button("Change Helper…") { confirmsConfiguration = true }
          }
        }

        if presentation.needsPairing || showingPairing {
          pairingFields
        }

        if enrollment?.installed == true || model.status.deviceID != nil {
          DisclosureGroup("Certificate recovery", isExpanded: $showingRotation) {
            rotationFields
          }
        }

        if let code = model.setupActionErrorCode {
          Text(guidance(for: code))
            .font(.caption)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("developer-mode-setup-error")
        }

        Text(
          "Windows remains authoritative. Setup stores only the helper location and expected Apple team. Pairing and recovery documents stay in memory and are never an approval or readiness claim."
        )
        .font(.caption)
        .foregroundStyle(.secondary)
      }
      .padding(.top, 6)
    }
    .fileImporter(
      isPresented: $choosingHelper,
      allowedContentTypes: [.item],
      allowsMultipleSelection: false
    ) { result in
      guard case let .success(urls) = result, let url = urls.first else { return }
      helperPath = url.path
    }
    .confirmationDialog(
      "Replace the saved signed helper configuration?",
      isPresented: $confirmsConfiguration,
      titleVisibility: .visible
    ) {
      Button("Continue") {
        showingPairing = false
        editingConfiguration = true
      }
      Button("Cancel", role: .cancel) {}
    } message: {
      Text("The replacement is validated before it is saved. Windows authority and the installed Mac identity are unchanged.")
    }
  }

  private var setupFields: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("1. Choose the separately signed Assemblywright bridge helper.")
        .font(.subheadline.bold())
      HStack {
        TextField("Signed helper", text: $helperPath)
          .textFieldStyle(.roundedBorder)
        Button("Choose…") { choosingHelper = true }
      }
      TextField("Verified 10-character Apple team ID", text: $teamIdentifier)
        .textFieldStyle(.roundedBorder)
      Button("Verify and Save") {
        Task {
          let configured = await model.configureBridge(
            helperURL: URL(fileURLWithPath: helperPath),
            expectedTeamIdentifier: teamIdentifier
          )
          if configured {
            editingConfiguration = false
            refreshEnrollment()
          }
        }
      }
      .disabled(!validSetupInput || model.ownerActionInProgress)
      .accessibilityIdentifier("developer-mode-save-helper")
      Text("The team ID is checked independently against the helper's Apple signature and Keychain entitlement.")
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }

  private var pairingFields: some View {
    VStack(alignment: .leading, spacing: 8) {
      Divider()
      Text("2. Pair this Mac")
        .font(.headline)
      Text(
        "On Windows, stop AssemblywrightMaster and run the confirmed `enrollment pair` command. Paste its public invitation below. The Windows process retains the secret and remains the only enrollment authority."
      )
      .font(.caption)
      .foregroundStyle(.secondary)
      documentEditor("Windows public invitation", text: $enrollmentInvitation)
      Button("Prepare Mac Reply") {
        Task {
          enrollmentReply = await model.prepareEnrollment(
            invitationData: Data(enrollmentInvitation.utf8)
          ).flatMap { String(data: $0, encoding: .utf8) } ?? ""
        }
      }
      .disabled(enrollmentInvitation.isEmpty || model.ownerActionInProgress)
      .accessibilityIdentifier("developer-mode-prepare-enrollment")
      if !enrollmentReply.isEmpty {
        documentOutput("Mac public reply", value: enrollmentReply)
        Text("Paste this reply into the waiting Windows command, then send EOF (Ctrl-Z) so Windows can issue the certificate receipt.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      documentEditor("Windows certificate receipt", text: $enrollmentReceipt)
      Button("Install and Connect") {
        Task {
          enrollment = await model.installEnrollment(
            receiptData: Data(enrollmentReceipt.utf8)
          )
          if enrollment?.installed == true {
            clearEnrollmentDocuments()
            showingPairing = false
          }
        }
      }
      .disabled(enrollmentReceipt.isEmpty || model.ownerActionInProgress)
      .accessibilityIdentifier("developer-mode-install-enrollment")
    }
  }

  private var rotationFields: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text(
        "Use the confirmed Windows `enrollment rotate-pair` ceremony for expiry or certificate recovery. Do not create a second grant if Windows issuance may already have committed; use its exact recovery command."
      )
      .font(.caption)
      .foregroundStyle(.secondary)
      documentEditor("Windows rotation invitation", text: $rotationInvitation)
      Button("Prepare Rotation Reply") {
        Task {
          rotationReply = await model.prepareCertificateRotation(
            invitationData: Data(rotationInvitation.utf8)
          ).flatMap { String(data: $0, encoding: .utf8) } ?? ""
        }
      }
      .disabled(rotationInvitation.isEmpty || model.ownerActionInProgress)
      if !rotationReply.isEmpty {
        documentOutput("Mac rotation reply", value: rotationReply)
      }
      documentEditor("Windows rotation receipt", text: $rotationReceipt)
      Button("Install Rotation and Reconnect") {
        Task {
          let receiptData = Data(rotationReceipt.utf8)
          enrollment = await model.installCertificateRotation(
            receiptData: receiptData
          )
          if enrollment?.installed == true {
            rotationAcknowledgementGrantID = rotationGrantID(from: receiptData)
            clearRotationDocuments()
            rotationInstalledAwaitingAcknowledgement = true
            showingRotation = true
          }
        }
      }
      .disabled(rotationReceipt.isEmpty || model.ownerActionInProgress)
      if rotationInstalledAwaitingAcknowledgement,
         let grantID = rotationAcknowledgementGrantID {
        Text(
          model.status.phase == .connected
            ? "Rotation is installed and Windows is authenticated. On Windows, run `enrollment rotate-recover-acknowledge --grant-id \(grantID) --confirm` to remove only the exact recovery journal."
            : "Rotation is installed. Wait for an authenticated Windows connection before running `enrollment rotate-recover-acknowledge --grant-id \(grantID) --confirm`."
        )
        .font(.caption.bold())
        .foregroundStyle(.orange)
        .accessibilityIdentifier("developer-mode-rotation-acknowledgement")
      }
    }
    .padding(.top, 8)
  }

  private var validSetupInput: Bool {
    helperPath.hasPrefix("/")
      && teamIdentifier.utf8.count == 10
      && teamIdentifier.utf8.allSatisfy {
        (0x41 ... 0x5a).contains($0) || (0x30 ... 0x39).contains($0)
      }
  }

  private func readinessRow(_ label: String, _ value: String, ready: Bool) -> some View {
    HStack {
      Image(systemName: ready ? "checkmark.circle.fill" : "circle")
        .foregroundStyle(ready ? .green : .secondary)
      LabeledContent(label, value: value)
    }
  }

  private func documentEditor(_ label: String, text: Binding<String>) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(label).font(.subheadline.bold())
      TextEditor(text: text)
        .font(.system(.caption, design: .monospaced))
        .frame(minHeight: 72, maxHeight: 120)
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
    }
  }

  private func documentOutput(_ label: String, value: String) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      HStack {
        Text(label).font(.subheadline.bold())
        Spacer()
        Button("Copy") {
          NSPasteboard.general.clearContents()
          NSPasteboard.general.setString(value, forType: .string)
        }
      }
      Text(value)
        .font(.system(.caption, design: .monospaced))
        .lineLimit(4)
        .textSelection(.enabled)
    }
  }

  private func refreshEnrollment() {
    Task {
      enrollment = await model.enrollmentStatus()
      if enrollment?.installed == false { showingPairing = true }
    }
  }

  private func clearEnrollmentDocuments() {
    enrollmentInvitation = ""
    enrollmentReply = ""
    enrollmentReceipt = ""
  }

  private func clearRotationDocuments() {
    rotationInvitation = ""
    rotationReply = ""
    rotationReceipt = ""
  }

  private func rotationGrantID(from receipt: Data) -> String? {
    guard let object = try? JSONSerialization.jsonObject(with: receipt) as? [String: Any],
      let value = object["grant_id"] as? String,
      let uuid = UUID(uuidString: value), uuid.uuidString.lowercased() == value
    else { return nil }
    return value
  }

  private func certificateLabel(_ milliseconds: UInt64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    return date.formatted(date: .abbreviated, time: .omitted)
  }

  private func guidance(for code: String) -> String {
    switch code {
    case "developer_bridge_configuration_store_invalid":
      "Saved setup is unsafe or malformed. Choose the signed helper and verified team again."
    case "developer_bridge_configuration_rejected", "developer_bridge_not_configured":
      "Choose the separately signed helper and enter its independently verified Apple team ID."
    case "invalid_helper_signature":
      "The selected helper no longer matches the verified Apple identity. Choose the correct signed helper."
    case "invalid_helper_path":
      "The selected helper path is no longer a safe executable file. Choose the signed helper again."
    case "enrollment_not_installed":
      "This Mac is not paired. Complete the public invitation and certificate receipt steps."
    case "enrollment_prepare_recovery_required":
      "Pairing preparation was interrupted. Retry with the same public Windows invitation."
    case "enrollment_install_recovery_required":
      "Certificate installation may have completed. Retry the same Windows receipt, then check pairing."
    case "invalid_enrollment_document", "invalid_helper_setup_response":
      "The public pairing document did not match the exact expected step. Paste the complete document and retry."
    case "helper_teardown_failed":
      "The helper could not be safely stopped. Quit Assemblywright before retrying setup."
    default:
      "Setup or recovery stopped safely (\(code)). Review the inputs and retry the exact step."
    }
  }
}
