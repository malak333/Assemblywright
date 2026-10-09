"""Test-only batch reviewer. Exercises bindings and real attachment transport."""
import hashlib
import json
import os
import sys
import time
from pathlib import Path


def scalable_review(input_text, arguments, evidence):
    markers = ("Untrusted canonical review batch JSON follows:\n",
               "Untrusted aggregate review evidence JSON follows:\n")
    marker = next((value for value in markers if value in input_text), None)
    if marker is None:
        return None
    raw = input_text.split(marker, 1)[1]
    packet = json.loads(raw)
    context = {}
    if "Trusted host-generated review context JSON follows:\n" in input_text:
        context_text = input_text.split(
            "Trusted host-generated review context JSON follows:\n", 1)[1]
        context = json.loads(context_text.split("\nUntrusted ", 1)[0])
    assets = packet.get("assets", [])
    paths = [arguments[i + 1] for i, value in enumerate(arguments[:-1]) if value == "--image"]
    observed = [hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in paths]
    assert observed == [asset["content_sha256"] for asset in assets], (observed, assets)
    instruction = packet.get("instruction", packet.get("shared_candidate", {}).get("instruction", ""))
    if packet.get("batch_index") == 0:
        evidence({"kind": "review", "model_id": packet.get("model_id"),
                  "reasoning_effort": packet.get("reasoning_effort"),
                  "approved_plan_sha256": packet.get("approved_plan_sha256"),
                  "approved_plan_text_sha256": hashlib.sha256((packet.get("approved_plan") or "").encode()).hexdigest()})
    source_regression = "[fixture:source-regression]" in instruction
    bad = next((file for file in packet.get("files", [])
                if "VALUE = 0" in file.get("content", "")
                and (not source_regression or file["path"] == "app.py")), None)
    findings = []
    if bad and (source_regression or "[fixture:reject-zero]" in instruction):
        findings = [{"finding_id": "publisher-regression" if source_regression else "wrong-value",
                     "path": bad["path"],
                     "message": ("Correct the publisher output and add the regression assertion."
                                 if source_regression else
                                 "Implementation must set VALUE to 1.")}]
    for receipt in packet.get("ordered_batch_receipts", []):
        if receipt["decision"] == "rejected":
            findings.extend(receipt["blocking_findings"])
    Path(arguments[0]).with_name("started.pid").write_text(str(os.getpid()))
    gate = Path(arguments[0]).with_name("staged-review.gate")
    if "staged-automatic:" in instruction and packet.get("batch_index") == 0 and gate.exists():
        gate.with_suffix(".started").write_text(str(os.getpid()))
        deadline = time.monotonic() + 120
        while gate.exists() and time.monotonic() < deadline:
            time.sleep(.05)
        assert not gate.exists(), "native fixture review gate was not released"
    if "[fixture:malformed]" in instruction:
        print("{")
        sys.exit(0)
    if "[fixture:wait]" in instruction and "batch_index" in packet:
        time.sleep(20)
    output = dict(decision="rejected" if findings else "approved",
                  blocking_findings=findings, non_blocking_findings=[])
    schema_path = arguments[arguments.index("--output-schema") + 1]
    properties = json.loads(Path(schema_path).read_text())["properties"]
    if "review_summary" in properties:
        output["review_summary"] = "Fixture verified the exact assigned entries and attachment hashes; no unresolved interfaces."
    if "interfaces_and_dependencies" in properties:
        output["interfaces_and_dependencies"] = []
    if ("[fixture:stale-batch]" in instruction or "[fixture:stale]" in instruction) and "batch_index" in packet:
        output["review_batch_sha256"] = "0" * 64
    if "[fixture:omit-batch-entry]" in instruction and "batch_index" in packet:
        output["reviewed_entries"] = context.get("reviewed_entries", [])[:-1]
    if "[fixture:stale-aggregate]" in instruction and "ordered_batch_receipts" in packet:
        output["review_packet_sha256"] = "0" * 64
    evidence({"kind": "review_aggregate" if "ordered_batch_receipts" in packet else "review_batch",
              "candidate_sha256": packet.get("aggregate_candidate_sha256"),
              "batch_index": packet.get("batch_index"), "batch_count": packet.get("batch_count"),
              "entries": context.get("reviewed_entries", packet.get("candidate_manifest", [])), "image_sha256s": observed})
    return output
