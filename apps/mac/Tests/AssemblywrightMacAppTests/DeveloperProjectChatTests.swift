import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer project chat")
struct DeveloperProjectChatTests {
  @Test
  func preservesProjectAndContextDisclosure() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let snapshot = try decoder.decode(DeveloperChatSnapshot.self, from: Data("""
      {"project":"temperature-demo","messages":[
        {"role":"user","content":"How do I open the GUI?"},
        {"role":"assistant","content":"On Windows run python temperature_gui.py."}],
       "running":false,"request_id":"request","error":null,
       "context_limit":262144,"context_tokens":1234,
       "context_files":["README.md","temperature_gui.py"],"omitted_messages":2}
      """.utf8))
    #expect(snapshot.project == "temperature-demo")
    #expect(snapshot.messages.map(\.role) == ["user", "assistant"])
    #expect(snapshot.messages[1].content.contains("Windows"))
    #expect(snapshot.contextLimit == 262144)
    #expect(snapshot.contextTokens == 1234)
    #expect(snapshot.contextFiles == ["README.md", "temperature_gui.py"])
    #expect(snapshot.omittedMessages == 2)
    #expect(!snapshot.running)
  }

  @Test
  func startConfirmationRetainsTargetAndCheckpoint() {
    let feature = DeveloperRunnerFeature(id: "original", project: "example", instruction: "change",
      validation: "tests", status: "paused", checkpoint: "applied", message: "", changedFiles: [],
      repairAttempts: 0, modelTarget: "windows")
    #expect(feature.startBinding["expected_feature_id"] as? String == "original")
    #expect(feature.startBinding["expected_model_target"] as? String == "windows")
    #expect(feature.startBinding["expected_status"] as? String == "paused")
    #expect(feature.startBinding["expected_checkpoint"] as? String == "applied")
  }
}
