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
    if let Some(output) = scalable_review(&input, &arguments) {
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
        .map(|f| {
            json!({
                "path": f["path"],
                "content_sha256": f["content_sha256"],
                "classification": f["classification"]
            })
        })
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
    let source_regression = instruction.contains("[fixture:source-regression]");
    let rejected_file = if source_regression || instruction.contains("[fixture:reject-zero]") {
        packet["files"].as_array().unwrap().iter().find(|file| {
            file["content"].as_str().unwrap().contains("VALUE = 0")
                && (!source_regression || file["path"] == "app.py")
        })
    } else {
        None
    };
    let findings = rejected_file.map(|file| vec![json!({
        "finding_id": if source_regression { "publisher-regression" } else { "wrong-value" },
        "path":file["path"],
        "message": if source_regression {
            "Correct the publisher output and add the regression assertion."
        } else {
            "Implementation must set VALUE to 1 while preserving every other generated file."
        }
    })]).unwrap_or_default();
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

fn scalable_review(input: &str, arguments: &[String]) -> Option<Value> {
    let marker = [
        "Untrusted canonical review batch JSON follows:\n",
        "Untrusted aggregate review evidence JSON follows:\n",
    ]
    .into_iter()
    .find(|marker| input.contains(marker))?;
    let packet: Value = serde_json::from_str(input.split_once(marker)?.1).unwrap();
    let context = input
        .split_once("Trusted host-generated review context JSON follows:\n")
        .and_then(|(_, value)| value.split_once("\nUntrusted "))
        .map(|(value, _)| serde_json::from_str::<Value>(value).unwrap())
        .unwrap_or_else(|| json!({}));
    let mut output = json!({
        "decision": "approved",
        "blocking_findings": [],
        "non_blocking_findings": []
    });
    let observed = arguments
        .windows(2)
        .filter(|pair| pair[0] == "--image")
        .map(|pair| format!("{:x}", Sha256::digest(std::fs::read(&pair[1]).unwrap())))
        .collect::<Vec<_>>();
    let assets = packet["assets"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        observed,
        assets
            .iter()
            .map(|asset| asset["content_sha256"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    let instruction = packet["instruction"]
        .as_str()
        .or_else(|| packet["shared_candidate"]["instruction"].as_str())
        .unwrap_or("");
    if packet["batch_index"] == 0 {
        append_evidence(json!({"kind":"review", "model_id":packet["model_id"],
            "reasoning_effort":packet["reasoning_effort"], "approved_plan_sha256":packet["approved_plan_sha256"],
            "approved_plan_text_sha256":format!("{:x}", Sha256::digest(packet["approved_plan"].as_str().unwrap_or("").as_bytes()))}));
    }
    std::fs::write(
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("started.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    let gate = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("staged-review.gate");
    if instruction.contains("staged-automatic:") && packet["batch_index"] == 0 && gate.exists() {
        std::fs::write(
            gate.with_extension("started"),
            std::process::id().to_string(),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while gate.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            !gate.exists(),
            "native fixture review gate was not released"
        );
    }
    if instruction.contains("[fixture:malformed]") {
        return Some(json!("malformed"));
    }
    if instruction.contains("[fixture:wait]") && packet.get("batch_index").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
    let source_regression = instruction.contains("[fixture:source-regression]");
    let rejected = packet["files"].as_array().and_then(|files| {
        files.iter().find(|file| {
            (source_regression || instruction.contains("[fixture:reject-zero]"))
                && file["content"].as_str().unwrap_or("").contains("VALUE = 0")
                && (!source_regression || file["path"] == "app.py")
        })
    });
    output["decision"] = json!(if rejected.is_some() {
        "rejected"
    } else {
        "approved"
    });
    output["blocking_findings"] = rejected.map(|file| json!([{
        "finding_id": if source_regression { "publisher-regression" } else { "wrong-value" },
        "path":file["path"],
        "message": if source_regression {
            "Correct the publisher output and add the regression assertion."
        } else {
            "Implementation must set VALUE to 1."
        }
    }])).unwrap_or_else(|| json!([]));
    output["non_blocking_findings"] = json!([]);
    if let Some(receipts) = packet["ordered_batch_receipts"].as_array() {
        for receipt in receipts {
            if receipt["decision"] == "rejected" {
                output["decision"] = json!("rejected");
                output["blocking_findings"].as_array_mut().unwrap().extend(
                    receipt["blocking_findings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned(),
                );
            }
        }
    }
    let schema_path = arguments
        .windows(2)
        .find(|pair| pair[0] == "--output-schema")
        .unwrap();
    let schema: Value = serde_json::from_slice(&std::fs::read(&schema_path[1]).unwrap()).unwrap();
    if schema["properties"].get("review_summary").is_some() {
        output["review_summary"] = json!(
            "Fixture verified exact entries and attachment hashes; no unresolved interfaces."
        );
    }
    if schema["properties"]
        .get("interfaces_and_dependencies")
        .is_some()
    {
        output["interfaces_and_dependencies"] = json!([]);
    }
    if (instruction.contains("[fixture:stale-batch]") || instruction.contains("[fixture:stale]"))
        && packet.get("batch_index").is_some()
    {
        output["review_batch_sha256"] = json!("0".repeat(64));
    }
    if instruction.contains("[fixture:omit-batch-entry]") && packet.get("batch_index").is_some() {
        output["reviewed_entries"] = context["reviewed_entries"].clone();
        output["reviewed_entries"].as_array_mut().unwrap().pop();
    }
    if instruction.contains("[fixture:stale-aggregate]")
        && packet.get("ordered_batch_receipts").is_some()
    {
        output["review_packet_sha256"] = json!("0".repeat(64));
    }
    append_evidence(json!({
        "kind": if packet.get("ordered_batch_receipts").is_some() { "review_aggregate" } else { "review_batch" },
        "candidate_sha256":packet["aggregate_candidate_sha256"],
        "batch_index":packet["batch_index"],"batch_count":packet["batch_count"],
        "entries":context.get("reviewed_entries").unwrap_or(&packet["candidate_manifest"]),"image_sha256s":observed
    }));
    Some(output)
}
