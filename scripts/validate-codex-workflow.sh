#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
from pathlib import Path
import sys

try:
    import tomllib
except ImportError as exc:
    raise SystemExit("error: Python 3.11+ with tomllib is required") from exc

root = Path.cwd()
config_path = root / ".codex" / "config.toml"
agents_dir = root / ".codex" / "agents"
instructions_path = root / "AGENTS.md"
workflow_doc_path = root / "docs" / "development-agent-workflow.md"

expected = {
    "assemblywright-explorer": ("gpt-5.6-terra", "medium", "read-only"),
    "assemblywright-quick-worker": ("gpt-5.6-luna", "low", "workspace-write"),
    "assemblywright-worker": ("gpt-5.6-sol", "medium", "workspace-write"),
    "assemblywright-high-risk-worker": ("gpt-5.6-sol", "high", "workspace-write"),
    "assemblywright-reviewer": ("gpt-5.6-terra", "high", "read-only"),
    "assemblywright-high-risk-reviewer": ("gpt-5.6-sol", "high", "read-only"),
}

def load(path: Path) -> dict:
    if not path.is_file():
        raise SystemExit(f"error: missing required file: {path.relative_to(root)}")
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except tomllib.TOMLDecodeError as exc:
        raise SystemExit(f"error: invalid TOML in {path.relative_to(root)}: {exc}") from exc

config = load(config_path)
agents_config = config.get("agents")
if not isinstance(agents_config, dict):
    raise SystemExit("error: .codex/config.toml must define [agents]")
if agents_config.get("max_depth") != 1:
    raise SystemExit("error: agents.max_depth must remain 1")
max_threads = agents_config.get("max_threads")
if not isinstance(max_threads, int) or not 1 <= max_threads <= 4:
    raise SystemExit("error: agents.max_threads must be an integer from 1 through 4")

found = {}
for path in sorted(agents_dir.glob("*.toml")):
    data = load(path)
    name = data.get("name")
    if not isinstance(name, str) or not name:
        raise SystemExit(f"error: {path.relative_to(root)} requires a non-empty name")
    if name in found:
        raise SystemExit(f"error: duplicate agent name: {name}")
    found[name] = path
    for field in ("description", "developer_instructions"):
        if not isinstance(data.get(field), str) or not data[field].strip():
            raise SystemExit(f"error: {path.relative_to(root)} requires {field}")
    for forbidden in ("approval_policy", "danger-full-access"):
        if forbidden in data or forbidden in data.get("developer_instructions", ""):
            raise SystemExit(f"error: forbidden policy override in {path.relative_to(root)}: {forbidden}")

missing = sorted(set(expected) - set(found))
extra = sorted(set(found) - set(expected))
if missing or extra:
    raise SystemExit(f"error: agent inventory mismatch; missing={missing}, extra={extra}")

for name, (model, effort, sandbox) in expected.items():
    data = load(found[name])
    actual = (data.get("model"), data.get("model_reasoning_effort"), data.get("sandbox_mode"))
    if actual != (model, effort, sandbox):
        raise SystemExit(
            f"error: {name} route mismatch; expected={(model, effort, sandbox)}, actual={actual}"
        )

if not instructions_path.is_file():
    raise SystemExit("error: missing AGENTS.md")
instructions = instructions_path.read_text(encoding="utf-8")
for name in expected:
    if f"`{name}`" not in instructions:
        raise SystemExit(f"error: AGENTS.md does not route {name}")
if len(instructions.splitlines()) > 100:
    raise SystemExit("error: AGENTS.md exceeds the 100-line instruction limit")

def normalized(text: str) -> str:
    return " ".join(text.split())

required_instructions = (
    "new implementation work is delegated to GLM-5.3-Flash, the single bounded low/normal-risk implementation lane",
    "the local Qwen/local-AI worker lane is retired and must not be used",
    "The frontier parent retains planning, independent complete-diff review, integration, and every commit, push, merge, publication action, and final evidence judgment",
    "Treat GLM-5.3-Flash output as an untrusted proposal",
)
norm_instructions = normalized(instructions)
for required in required_instructions:
    if required not in norm_instructions:
        raise SystemExit(f"error: AGENTS.md is missing delegation policy text: {required}")
for forbidden in (
    "local Qwen proposal worker",
    "local_codex_worker.py",
    "local-worker",
):
    if forbidden in norm_instructions:
        raise SystemExit(f"error: AGENTS.md must not keep the retired local Qwen/local-AI lane text: {forbidden}")

if not workflow_doc_path.is_file():
    raise SystemExit("error: missing docs/development-agent-workflow.md")
workflow_doc = normalized(workflow_doc_path.read_text(encoding="utf-8"))
for required in (
    "## GLM-5.3-Flash Implementation Delegation",
    "new implementation work is delegated to GLM-5.3-Flash, the single bounded low- and normal-risk implementation lane",
    "the local Qwen/local-AI worker lane is retired and must not be used",
    "The frontier parent retains planning, independent complete-diff review, integration, and every commit, push, merge, publication action, and final evidence judgment",
    "they are never routed to GLM-5.3-Flash",
):
    if required not in workflow_doc:
        raise SystemExit(f"error: docs/development-agent-workflow.md is missing delegation policy text: {required}")
for forbidden in (
    "## Local Qwen Proposal Worker",
    "local Qwen proposal worker",
    "local_codex_worker.py",
    "qwen36-local",
    "local_qwen",
    "proposal or cross-check",
):
    if forbidden in workflow_doc:
        raise SystemExit(f"error: docs/development-agent-workflow.md must not keep the retired local Qwen/local-AI lane text: {forbidden}")

print(f"Assemblywright Codex workflow validation: ok ({len(expected)} agents, max_threads={max_threads})")
PY
