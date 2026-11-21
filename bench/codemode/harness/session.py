import json
import re
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import bindings
import boundary
import evaluate
import model
import prompts
from config import MODEL, THINKING
from fixtures import snapshot, tree_effects
from session_tools import Session
from tasks import Task

SESSION_SCHEMA = "codemode-session/2"
MAX_TURNS = 8
MAX_SECONDS = 600
ACTION = re.compile(r"```([a-z]+)([^\n]*)\n(.*?)```", re.DOTALL)
FILE_ATTRIBUTE = re.compile(r"file=(\S+)")
DONE = re.compile(r"^\s*DONE\s*$", re.MULTILINE)
NO_ACTION = (
    "No action was found in your reply: tool-call markup is not read. Write each action as a fenced block, "
    "for example a block opened with ```api holding `language:core`, or a block opened with ```check."
)
INSTRUCTIONS = """You work in turns. Each reply may hold one or more actions, each a fenced block; they run in
order, and the next message shows their results. The actions:

- A block opened with ```xsh file=main.xsh holds a whole file and writes it to your workspace. Your
  program is `main.xsh`{module}; you may add other `.xsh` modules beside it.
- A block opened with ```api holds `xsht api` selectors on one line, such as `language:core`,
  `api:fs.walk` or `search:retry`, and returns the toolchain's reference text.
- An empty block opened with ```check runs `xsht check main.xsh`.
- A block opened with ```run holds the program's arguments, one per line (or nothing), and runs
  `xsh main.xsh -- ARGS...` with the working directory `/work`, the task's fixture. The fixture keeps
  every change your runs make{service}.

Python and Perl are not available, and there is no other network access. When the program is done,
reply with the single word DONE and no action.

You have at most {turns} replies and {minutes} minutes. When the budget ends, the `main.xsh` in your
workspace is the answer. It is then run once per test in fresh copies of the fixture, including tests
you cannot see, so make it follow the task's full contract, not just the visible fixture."""


@dataclass(slots=True)
class Turn:
    index: int
    prompt_file: str
    reply_file: str
    reply: dict
    interruptions: list[dict]
    actions: list[dict]
    done: bool


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
    turns: list[Turn]
    usage: dict
    tools: dict
    doc_bytes: int
    elapsed_seconds: float
    model_seconds: float
    tool_seconds: float
    stopped_by: str
    session_effects: dict
    final: dict | None
    verdict: str
    started: float
    finished: float
    protocol: str = "harness-mediated actions in fenced blocks; one model call per turn"
    notes: list[str] = field(default_factory=list)

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
            "declarations:", "", "```text", bindings.xsh_declarations(task.data).rstrip(), "```", "",
            "How the service behaves:", "", prompts.behavior_notes(), "",
        ]
    parts += ["## Your tools and budget", "", INSTRUCTIONS.format(
        module=", beside the read-only `svc.xsh`" if task.shared else "",
        service=", and the fixture service keeps its state" if task.shared else "",
        turns=MAX_TURNS, minutes=MAX_SECONDS // 60,
    )]
    return "\n".join(parts) + "\n"


def parse_actions(reply: str) -> list[dict]:
    actions = []
    for match in ACTION.finditer(reply):
        kind, attributes, body = match.group(1), match.group(2), match.group(3)
        if kind == "xsh":
            target = FILE_ATTRIBUTE.search(attributes)
            actions.append({"tool": "edit", "arguments": {"path": target.group(1) if target else "main.xsh", "content": body}})
        elif kind == "api":
            actions.append({"tool": "api", "arguments": {"query": body.split()}})
        elif kind == "check":
            actions.append({"tool": "check", "arguments": {}})
        elif kind == "run":
            actions.append({"tool": "run", "arguments": {"args": [line for line in body.splitlines() if line != ""]}})
    return actions


def turn_prompt(initial: str, turns: list[tuple[str, str]]) -> str:
    parts = [initial.rstrip(), ""]
    for index, (reply, results) in enumerate(turns, start=1):
        parts += [f"## Your reply {index}", "", reply.strip(), "", f"## Results of reply {index}", "", results, ""]
    parts.append("Reply with your next actions, or DONE.")
    return "\n".join(parts) + "\n"


def usage_totals(replies: list[model.Reply]) -> dict:
    totals = dict.fromkeys(("input", "output", "cache_read", "cache_write", "reasoning", "total"), 0)
    unknown = dict.fromkeys(totals, 0)
    cost = 0.0
    for reply in replies:
        for key in totals:
            value = getattr(reply.usage, key)
            if value is None:
                unknown[key] += 1
            else:
                totals[key] += value
        cost += reply.usage.cost_estimate or 0.0
    levels = sorted({r.thinking_level for r in replies if r.thinking_level})
    return {**totals, "unknown": unknown, "cost_estimate": cost, "cost_from_provider": False, "thinking_levels": levels}


def run_session(
    task: Task, condition: prompts.Condition, sample: int, out: Path, expected_context: str | None,
    backoff: model.Backoff,
) -> SessionRecord:
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
    setup = task.fixture.setup(task.variants[0], work)
    before = snapshot(work)
    service = None
    if task.shared:
        service = boundary.start_service(bindings.service_spec(task.data, setup.responses), directory / "service")
    tools = Session(state, service.name if service else None, float(task.data["timeout_s"]), setup.env)
    initial = session_prompt(task, condition)
    started = time.time()
    model_seconds = tool_seconds = 0.0
    turns: list[Turn] = []
    history: list[tuple[str, str]] = []
    replies: list[model.Reply] = []
    stopped_by = "turns"
    error = None
    try:
        for index in range(MAX_TURNS):
            if model_seconds + tool_seconds >= MAX_SECONDS:
                stopped_by = "time"
                break
            prompt = turn_prompt(initial, history) if history else initial
            prompt_file = directory / f"prompt-{index}.md"
            prompt_file.write_text(prompt)
            reply, interruptions = model.ask_with_backoff(prompt, directory, directory / f"events-{index}.jsonl", backoff)
            replies.append(reply)
            model_seconds += reply.wall_seconds
            reply_file = directory / f"reply-{index}.md"
            reply_file.write_text(reply.text)
            if reply.error is not None and not reply.text:
                error = reply.error
                stopped_by = "provider"
                turns.append(Turn(index, prompt_file.name, reply_file.name, reply.to_json(), interruptions, [], False))
                break
            actions = parse_actions(reply.text)
            results = []
            records = []
            for action in actions:
                if model_seconds + tool_seconds >= MAX_SECONDS:
                    results.append(f"### {action['tool']}\nnot run: the time budget is spent")
                    records.append({**action, "skipped": True})
                    continue
                clock = time.perf_counter()
                text, is_error = tools.call(action["tool"], action["arguments"])
                spent = time.perf_counter() - clock
                tool_seconds += spent
                results.append(f"### {action['tool']}\n{text}")
                records.append({"tool": action["tool"], "arguments": action["arguments"], "is_error": is_error,
                                "seconds": spent, "result_chars": len(text)})
            done = bool(DONE.search(reply.text)) and not actions
            turns.append(Turn(index, prompt_file.name, reply_file.name, reply.to_json(), interruptions, records, done))
            if done:
                stopped_by = "model"
                break
            history.append((reply.text, "\n\n".join(results) if results else NO_ACTION))
    except model.ProviderUnavailable as unavailable:
        error = str(unavailable)
        stopped_by = "provider"
    if service is not None:
        service.stop()
    after = snapshot(work)
    calls = service.calls() if service is not None else []
    effects = {"tree": tree_effects(before, after), "service_calls": len(calls),
               "duplicate_writes": sum(1 for c in calls if c.get("outcome") == "duplicate-write")}
    counts = {"api": 0, "edit": 0, "check": 0, "run": 0, "refused": 0}
    doc_bytes = 0
    log = state / "tools.jsonl"
    if log.exists():
        for line in log.read_text().splitlines():
            entry = json.loads(line)
            counts[entry["tool"]] += 1
            counts["refused"] += bool(entry.get("refused"))
            doc_bytes += entry.get("bytes", 0) if entry["tool"] == "api" else 0
    program = workspace / "main.xsh"
    final = None
    verdict = "no-program"
    if program.exists():
        result = evaluate.evaluate("xsh", task, program.read_text(), directory / "final")
        final = result.to_json()
        verdict = result.status
    record = SessionRecord(
        SESSION_SCHEMA, f"{task.id}/{condition.name}/{sample}", task.id, task.digest(), condition.name, digest, sample,
        MODEL, THINKING, "interrupted" if stopped_by == "provider" else "done", error, turns, usage_totals(replies),
        counts, doc_bytes, model_seconds + tool_seconds, model_seconds, tool_seconds, stopped_by, effects, final,
        verdict, started, time.time(),
    )
    (directory / "session.json").write_text(json.dumps(record.to_json(), indent=2))
    return record
