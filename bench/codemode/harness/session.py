import json
import os
import signal
import subprocess
import sys
import threading
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import bindings
import boundary
import evaluate
import model
import prompts
from config import MODEL, PI, THINKING
from fixtures import snapshot, tree_effects
from tasks import Task

SESSION_SCHEMA = "codemode-session/1"
MAX_TURNS = 8
MAX_SECONDS = 600
SERVER = Path(__file__).resolve().parent / "session_server.py"
SYSTEM_PROMPT = (
    "You write one XSH program that solves the task, using only the tools you are given. "
    "When the program is finished, reply with a short final message and stop calling tools."
)
PI_SESSION = (
    "-p", "--mode", "json", "--approve", "--no-session", "--no-extensions", "--no-skills", "--no-context-files",
    "--no-prompt-templates", "--offline",
)
INSTRUCTIONS = (
    "You have four tools. `api` runs `xsht api QUERY...` and returns the toolchain's reference text. `edit` writes "
    "a whole file in your workspace; your program is `main.xsh`{module}. `check` runs `xsht check main.xsh`. `run` "
    "runs `xsh main.xsh -- ARGS...` with the working directory `/work`, the task's fixture; the fixture keeps every "
    "change your runs make{service}. Python and Perl are not available, and there is no other network access.\n\n"
    "You have at most {turns} model turns and {minutes} minutes. When the budget ends, the `main.xsh` in your "
    "workspace is the answer. It is then run once per test in fresh copies of the fixture, including tests you "
    "cannot see, so make it follow the task's full contract, not just the visible fixture."
)


@dataclass(slots=True)
class SessionRecord:
    schema: str
    cell: str
    task: str
    task_digest: str
    condition: str
    context_digest: str | None
    sample: int
    model: str
    thinking_requested: str
    state: str
    error: str | None
    turns: int
    usage: dict
    thinking_levels: list[str]
    tools: dict
    doc_bytes: int
    doc_tokens_estimate: int
    elapsed_seconds: float
    stopped_by: str
    session_effects: dict
    final: dict | None
    verdict: str
    started: float
    finished: float
    interruptions: list[dict] = field(default_factory=list)

    def to_json(self) -> dict:
        return asdict(self)


def session_directory(out: Path, task_id: str, condition: str, sample: int) -> Path:
    return out / "sessions" / task_id / f"{condition}-{sample}"


def session_prompt(task: Task, condition: prompts.Condition) -> str:
    parts = []
    context = prompts.context_text(condition)
    if context:
        parts += [context.rstrip(), ""]
    parts += ["## Your task", "", task.data["description"].strip(), ""]
    if task.shared:
        parts += [
            "## Facts for this task", "",
            "The service module `svc` (the file `svc.xsh` beside `main.xsh`; import it with `use svc`). Its exported "
            "declarations:", "", "```xsh", bindings.xsh_declarations(task.data).rstrip(), "```", "",
            "How the service behaves:", "", prompts.behavior_notes(), "",
        ]
    instructions = INSTRUCTIONS.format(
        module=", beside the read-only `svc.xsh`" if task.shared else "",
        service=" and the fixture service keeps its state" if task.shared else "",
        turns=MAX_TURNS, minutes=MAX_SECONDS // 60,
    )
    parts += ["## Your tools and budget", "", instructions]
    return "\n".join(parts) + "\n"


def usage_totals(messages: list[dict]) -> tuple[dict, list[str]]:
    totals: dict = {k: 0 for k in ("input", "output", "cache_read", "cache_write", "reasoning", "total")}
    unknown: dict = {k: 0 for k in totals}
    cost = 0.0
    from_provider = set()
    levels = []
    for message in messages:
        usage = model.Usage.from_message(message.get("usage"))
        for key in totals:
            value = getattr(usage, key)
            if value is None:
                unknown[key] += 1
            else:
                totals[key] += value
        cost += usage.cost_estimate or 0.0
        from_provider.add(usage.cost_from_provider)
        if message.get("thinkingLevel"):
            levels.append(message["thinkingLevel"])
    return {**totals, "unknown": unknown, "cost_estimate": cost, "cost_from_provider": sorted(map(str, from_provider))}, levels


def tool_summary(state: Path) -> tuple[dict, int]:
    counts = {"api": 0, "edit": 0, "check": 0, "run": 0, "refused": 0}
    doc_bytes = 0
    log = state / "tools.jsonl"
    if log.exists():
        for line in log.read_text().splitlines():
            entry = json.loads(line)
            counts[entry["tool"]] += 1
            counts["refused"] += bool(entry.get("refused"))
            doc_bytes += entry.get("bytes", 0) if entry["tool"] == "api" else 0
    return counts, doc_bytes


def run_session(task: Task, condition: prompts.Condition, sample: int, out: Path, expected_context: str | None) -> SessionRecord:
    directory = session_directory(out, task.id, condition.name, sample)
    if (directory / "session.json").exists():
        raise FileExistsError(f"{directory} already holds a session")
    digest = prompts.context_digest(condition)
    if digest != expected_context:
        raise RuntimeError(f"{condition.name}: context digest {digest} differs from the manifest's {expected_context}")
    state = directory / "state"
    workspace, work = state / "workspace", state / "work"
    workspace.mkdir(parents=True)
    work.mkdir(parents=True)
    if task.shared:
        (workspace / "svc.xsh").write_text(bindings.xsh_module(task.data))
    public = task.variants[0]
    setup = task.fixture.setup(public, work)
    before = snapshot(work)
    service = None
    if task.shared:
        service = boundary.start_service(bindings.service_spec(task.data, setup.responses), directory / "service")
    project = directory / "project"
    (project / ".pi").mkdir(parents=True)
    server_args = [str(SERVER), "--state", str(state), "--timeout", str(task.data["timeout_s"]), "--env", json.dumps(setup.env)]
    if service is not None:
        server_args += ["--service", service.name]
    mcp = {"mcpServers": {"xsh": {"command": sys.executable, "args": server_args, "exposure": "direct", "timeout": 300}}}
    (project / ".pi" / "mcp.json").write_text(json.dumps(mcp, indent=2))
    prompt = session_prompt(task, condition)
    (directory / "prompt.md").write_text(prompt)
    command = [
        str(PI), *PI_SESSION, "--tools", "mcp__xsh__*", "--model", model.allowed_model(MODEL), "--thinking", THINKING,
        "--system-prompt", SYSTEM_PROMPT,
    ]
    (directory / "command.json").write_text(json.dumps(command, indent=2))
    environment = {**os.environ, "PYTHONPATH": str(SERVER.parent)}
    started = time.time()
    clock = time.monotonic()
    messages: list[dict] = []
    stopped_by = "model"
    error = None
    child = subprocess.Popen(
        command, cwd=project, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=(directory / "stderr.txt").open("w"), text=True, start_new_session=True,
    )
    assert child.stdin is not None and child.stdout is not None
    child.stdin.write(prompt)
    child.stdin.close()

    def close(reason: str) -> None:
        nonlocal stopped_by
        if stopped_by == "model":
            stopped_by = reason
        (state / "closed").write_text(reason)
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass

    timer = threading.Timer(MAX_SECONDS, close, args=("time",))
    timer.start()
    with (directory / "events.jsonl").open("w") as sink:
        for line in child.stdout:
            sink.write(json.dumps({"time_ns": time.time_ns(), "line": line.rstrip("\n")}) + "\n")
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            message = event.get("message") if isinstance(event, dict) else None
            if event.get("type") == "message_end" and isinstance(message, dict) and message.get("role") == "assistant":
                messages.append(message)
                if message.get("stopReason") in ("error", "aborted"):
                    error = f"the model stopped with {message.get('stopReason')}: {message.get('errorMessage', '')}"
                if len(messages) >= MAX_TURNS and message.get("stopReason") != "stop":
                    close("turns")
    child.wait()
    timer.cancel()
    elapsed = time.monotonic() - clock
    if service is not None:
        service.stop()
    after = snapshot(work)
    usage, levels = usage_totals(messages)
    tools, doc_bytes = tool_summary(state)
    calls = service.calls() if service is not None else []
    effects = {"tree": tree_effects(before, after), "service_calls": len(calls),
               "duplicate_writes": sum(1 for c in calls if c.get("outcome") == "duplicate-write")}
    program = workspace / "main.xsh"
    final = None
    verdict = "no-program"
    if program.exists():
        result = evaluate.evaluate("xsh", task, program.read_text(), directory / "final")
        final = result.to_json()
        verdict = result.status
    if error is None and child.returncode not in (0, None) and stopped_by == "model":
        error = f"pi exited {child.returncode}: {(directory / 'stderr.txt').read_text()[-800:]}"
    state_name = "interrupted" if error is not None and model.looks_like_provider(error) else "done"
    record = SessionRecord(
        SESSION_SCHEMA, f"{task.id}/{condition.name}/{sample}", task.id, task.digest(), condition.name, digest, sample,
        MODEL, THINKING, state_name, error, len(messages), usage, levels, tools, doc_bytes, doc_bytes // 4, elapsed,
        stopped_by, effects, final, verdict, started, time.time(),
    )
    (directory / "session.json").write_text(json.dumps(record.to_json(), indent=2))
    return record
