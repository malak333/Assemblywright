//! Deterministic test-only Codex stand-in. Never install as a product reviewer.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let model = arguments
        .windows(2)
        .find(|pair| pair[0] == "--model")
        .map(|pair| pair[1].clone());
    let reasoning_effort = arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("model_reasoning_effort=\"")
            .and_then(|value| value.strip_suffix('"'))
            .map(str::to_owned)
    });
    append_evidence(
        json!({"kind":"argv","model":model,"reasoning_effort":reasoning_effort,"arguments":arguments}),
    );
    let mut input = String::new();
    std::io::stdin()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut input)
        .unwrap();
    if let Some((_, raw)) = input.split_once("Untrusted canonical planning packet JSON follows:\n")
    {
        let packet: Value = serde_json::from_str(raw).unwrap();
        append_evidence(
            json!({"kind":"planning","packet_sha256":format!("{:x}", Sha256::digest(raw.as_bytes())),"skill_sha256":packet["skill_sha256"],"model_id":packet["model_id"],"reasoning_effort":packet["reasoning_effort"],"skill_present":input.contains("Understanding Lock") && input.contains("Turn raw ideas into")}),
        );
        let marker = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("planning-started.pid");
        std::fs::write(marker, std::process::id().to_string()).unwrap();
        let instruction = packet["instruction"].as_str().unwrap();
        if instruction.contains("[planning:wait]") {
            std::thread::sleep(std::time::Duration::from_secs(20));
        }
        if instruction.contains("[planning:malformed]") {
            print!("{{");
            return;
        }
        let mut output = planning_output(&packet, format!("{:x}", Sha256::digest(raw.as_bytes())));
        if instruction.contains("[planning:skip]") {
            output["response_kind"] = json!("ready");
        }
        if instruction.contains("[planning:stale]") {
            output["planning_packet_sha256"] = json!("0".repeat(64));
        }
        print!("{}", serde_json::to_string(&output).unwrap());
        return;
    }
    let packet_bytes = input
        .split_once("Untrusted canonical review packet JSON follows:\n")
        .unwrap()
        .1;
    let packet: Value = serde_json::from_str(packet_bytes).unwrap();
    append_evidence(
        json!({"kind":"review","model_id":packet["model_id"],"reasoning_effort":packet["reasoning_effort"],"approved_plan_sha256":packet["approved_plan_sha256"],"approved_plan_text_sha256":format!("{:x}", Sha256::digest(packet["approved_plan"].as_str().unwrap_or("").as_bytes()))}),
    );
    let mut digest = format!("{:x}", Sha256::digest(packet_bytes.as_bytes()));
    let files: Vec<Value> = packet["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| json!({"path":f["path"],"content_sha256":f["content_sha256"]}))
        .collect();
    std::fs::write(
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("started.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    let instruction = packet["instruction"].as_str().unwrap();
    if instruction.contains("[fixture:malformed]") {
        print!("{{");
        return;
    }
    if instruction.contains("[fixture:wait]") {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    if instruction.contains("[fixture:stale]") {
        digest = "0".repeat(64);
    }
    let rejected_file = if instruction.contains("[fixture:reject-zero]") {
        packet["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["content"].as_str().unwrap().contains("VALUE = 0"))
    } else {
        None
    };
    let findings = rejected_file.map(|file| vec![json!({"finding_id":"wrong-value", "path":file["path"], "message":"Implementation must set VALUE to 1 while preserving every other generated file."})]).unwrap_or_default();
    let decision = if findings.is_empty() {
        "approved"
    } else {
        "rejected"
    };
    let output = json!({"schema_version":1,"review_packet_sha256":digest,"provider_id":"openai.codex","model_id":packet["model_id"],"reasoning_effort":packet["reasoning_effort"],"decision":decision,"blocking_findings":findings,"non_blocking_findings":[],"validation_evidence_sha256":packet["validation_evidence_sha256"],"reviewed_files":files});
    std::io::stdout()
        .write_all(serde_json::to_string(&output).unwrap().as_bytes())
        .unwrap();
}

fn planning_output(packet: &Value, digest: String) -> Value {
    let key = packet["expected_response"].as_str().unwrap();
    let value = match key {
        "question_or_understanding" => {
            if packet["answers"].as_array().unwrap().is_empty() {
                r##"{"schema_version": 1, "planning_packet_sha256": "", "provider_id": "openai.codex", "model_id": "gpt-5.6-sol", "response_kind": "question", "question": {"text": "Should this feature preserve existing behavior outside the requested change?", "choices": ["Yes, preserve existing behavior", "Clarify the scope"]}, "understanding_summary": [], "assumptions": null, "open_questions": [], "approaches": [], "design_section": null, "design_complete": false, "decision_log": [], "implementation_plan": null}"##
            } else {
                r##"{"schema_version": 1, "planning_packet_sha256": "", "provider_id": "openai.codex", "model_id": "gpt-5.6-sol", "response_kind": "understanding", "question": null, "understanding_summary": ["Implement the requested feature in the selected project.", "Serve the project owner using the existing workflow.", "Preserve existing behavior outside the requested change.", "Use the configured validation command to verify the result.", "Do not introduce unrelated features or dependencies."], "assumptions": {"performance": "Keep this small feature responsive.", "scale": "One project owner.", "security_privacy": "No new network access or credentials.", "reliability_availability": "Report failures explicitly and preserve existing files.", "maintenance_ownership": "The owner maintains the project; prefer existing conventions.", "other": []}, "open_questions": [], "approaches": [], "design_section": null, "design_complete": false, "decision_log": [], "implementation_plan": null}"##
            }
        }
        "approaches" => {
            r##"{"schema_version": 1, "planning_packet_sha256": "", "provider_id": "openai.codex", "model_id": "gpt-5.6-sol", "response_kind": "approaches", "question": null, "understanding_summary": [], "assumptions": null, "open_questions": [], "approaches": [{"id": "minimal", "title": "Extend existing code", "summary": "Make the requested change using current conventions.", "tradeoffs": ["Small change with limited new dependencies."], "recommended": true}, {"id": "separate", "title": "Add a separate component", "summary": "Isolate the feature behind a new component.", "tradeoffs": ["More structure and maintenance for this small scope."], "recommended": false}], "design_section": null, "design_complete": false, "decision_log": [], "implementation_plan": null}"##
        }
        "design_section" => {
            r##"{"schema_version": 1, "planning_packet_sha256": "", "provider_id": "openai.codex", "model_id": "gpt-5.6-sol", "response_kind": "design_section", "question": null, "understanding_summary": [], "assumptions": null, "open_questions": [], "approaches": [], "design_section": {"id": "implementation", "title": "Implementation and verification", "body": "Implement the original request using the selected approach. Keep existing behavior and inputs intact. Handle errors explicitly. Run the configured validation command and request independent code review. Avoid unrelated dependencies and external services."}, "design_complete": false, "decision_log": [{"decision": "Use the owner-selected approach for the original request.", "alternatives": ["Introduce a separate component."], "reason": "Matches the confirmed scope and keeps maintenance small."}], "implementation_plan": null}"##
        }
        "design_section_or_ready" => {
            r##"{"schema_version": 1, "planning_packet_sha256": "", "provider_id": "openai.codex", "model_id": "gpt-5.6-sol", "response_kind": "ready", "question": null, "understanding_summary": [], "assumptions": null, "open_questions": [], "approaches": [], "design_section": null, "design_complete": true, "decision_log": [{"decision": "Use the owner-selected approach for the original request.", "alternatives": ["Introduce a separate component."], "reason": "Matches the confirmed scope and keeps maintenance small."}], "implementation_plan": "1. Read the existing project.\n2. Implement the original feature request and preserve existing behavior.\n3. Run the configured validation command.\n4. Address independent reviewer findings."}"##
        }
        _ => panic!("Unknown planning stage"),
    };
    let mut result: Value = serde_json::from_str(value).unwrap();
    result["planning_packet_sha256"] = json!(digest);
    result["model_id"] = packet["model_id"].clone();
    result["reasoning_effort"] = packet["reasoning_effort"].clone();
    result
}

fn append_evidence(value: Value) {
    let path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("review-input-evidence.jsonl");
    let mut output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(output, "{}", serde_json::to_string(&value).unwrap()).unwrap();
}
