import hashlib
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

import bindings
from config import CONTEXTS, TOOLCHAIN
from tasks import Task

FENCE = re.compile(r"```([^\n`]*)\n(.*?)```", re.DOTALL)
TAGS = {"xsh": ("xsh",), "ts": ("ts", "typescript", "mts")}
REPLY = {
    "xsh": "Reply with one XSH program in a ```xsh code block, and nothing else.",
    "ts": "Reply with one TypeScript module in a ```ts code block, and nothing else.",
}
REPAIR = {
    "xsh": "Reply with a corrected program in one ```xsh code block, and nothing else.",
    "ts": "Reply with a corrected module in one ```ts code block, and nothing else.",
}
EXTRACTION_RULE = (
    "Take the first fenced code block whose info string's first word is the arm's tag "
    "(xsh: `xsh`; ts: `ts`, `typescript` or `mts`); otherwise the first fenced block of any tag; "
    "otherwise the whole reply. Strip surrounding whitespace. An empty result is no answer."
)


@dataclass(frozen=True, slots=True)
class Condition:
    name: str
    arm: str
    context: str | None


CONDITIONS = {
    "xsh-baseline": Condition("xsh-baseline", "xsh", "baseline.md"),
    "xsh-candidate": Condition("xsh-candidate", "xsh", "candidate.md"),
    "xsh-revised": Condition("xsh-revised", "xsh", "revised.md"),
    "xsh-selected": Condition("xsh-selected", "xsh", "selected.md"),
    "ts": Condition("ts", "ts", None),
}


def context_text(condition: Condition) -> str | None:
    if condition.context is None:
        return None
    return (CONTEXTS / condition.context).read_text()


def context_digest(condition: Condition) -> str | None:
    text = context_text(condition)
    return None if text is None else hashlib.sha256(text.encode()).hexdigest()


def extract(reply: str, arm: str) -> tuple[str, str]:
    blocks = [(m.group(1).strip().split(" ")[0].lower(), m.group(2)) for m in FENCE.finditer(reply)]
    for tag, body in blocks:
        if tag in TAGS[arm]:
            return body.strip(), f"fence:{tag}"
    if blocks:
        tag, body = blocks[0]
        return body.strip(), f"first-fence:{tag or 'untagged'}"
    return reply.strip(), "whole-reply"


def xsh_api_facts(selectors: list[str]) -> str:
    if not selectors:
        return ""
    completed = subprocess.run(
        [str(TOOLCHAIN / "xsht"), "api", "--strict", *selectors],
        capture_output=True, text=True, check=False, timeout=60,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"xsht api --strict {' '.join(selectors)} exited {completed.returncode}: {completed.stderr}")
    return completed.stdout.strip()


def behavior_notes() -> str:
    return "\n".join(f"- {note}" for note in bindings.BEHAVIOR_NOTES)


def xsh_facts(task: Task) -> str:
    parts = []
    facts = xsh_api_facts(task.data["xsh_api"])
    if facts:
        parts += ["Standard-library and language entries, as `xsht api` prints them:", "", "```text", facts, "```", ""]
    if task.shared:
        parts += [
            "The service module `svc` (the file `svc.xsh` beside `main.xsh`; import it with `use svc` and call "
            "`svc.NAME(...)`, `svc.ServiceError.Timeout`, and so on). Its exported declarations:", "",
            "```xsh", bindings.xsh_declarations(task.data).rstrip(), "```", "",
            "How the service behaves:", "", behavior_notes(), "",
        ]
    return "\n".join(parts).rstrip()


def ts_facts(task: Task) -> str:
    modules = ", ".join(f"`{m}`" for m in task.data["ts_modules"]) or "none"
    parts = [f"Node.js built-in modules relevant to this task: {modules}.", ""]
    if task.shared:
        parts += [
            "The service module (the file `svc.mts` beside `main.mts`; import it with "
            '`import * as svc from "./svc.mts";`). Its exported declarations:', "",
            "```ts", bindings.ts_declarations(task.data).rstrip(), "```", "",
            "How the service behaves:", "", behavior_notes(), "",
        ]
    return "\n".join(parts).rstrip()


def run_section(task: Task, arm: str) -> str:
    if arm == "xsh":
        lines = [
            "Your program is the file `main.xsh`. It is checked with `xsht check main.xsh` and then run as "
            "`xsh main.xsh -- ARGS...` with the working directory `/work`.",
        ]
    else:
        lines = [
            "Your program is the TypeScript ES module `main.mts`. It is type-checked with `tsc` 7.0.2 (strict, "
            "`module: nodenext`, `erasableSyntaxOnly`, `@types/node` 26) and then run as `node main.mts ARGS...` "
            "on Node.js 26, which strips the types, with the working directory `/work`. Only Node.js built-in "
            "modules are available; there are no npm packages.",
        ]
    lines.append(
        "The program runs once per test in a fresh copy of the fixture. It has no network access"
        + (" other than the fixture service" if task.shared else "")
        + ", and Python and Perl are not available."
    )
    return "\n".join(lines)


def initial_prompt(task: Task, condition: Condition) -> str:
    arm = condition.arm
    parts: list[str] = []
    context = context_text(condition)
    if context is not None:
        parts += [context.rstrip(), ""]
    facts = xsh_facts(task) if arm == "xsh" else ts_facts(task)
    parts += ["## Your task", "", task.data["description"].strip(), ""]
    if facts:
        parts += ["## Facts for this task", "", facts, ""]
    parts += ["## How your program runs", "", run_section(task, arm), "", REPLY[arm]]
    return "\n".join(parts) + "\n"


def repair_prompt(original: str, attempts: list[tuple[str, str]], arm: str) -> str:
    tag = TAGS[arm][0]
    parts = [original.rstrip(), ""]
    for code, feedback in attempts:
        parts += ["Your previous answer:", "", f"```{tag}", code, "```", "", "It failed:", "", feedback, ""]
    parts.append(REPAIR[arm])
    return "\n".join(parts) + "\n"


def prompt_sections(prompt: str) -> dict[str, int]:
    marks = ["## Your task", "## Facts for this task", "## How your program runs"]
    positions = [prompt.find(m) for m in marks]
    context_end = positions[0] if positions[0] >= 0 else 0
    task_end = positions[1] if positions[1] >= 0 else positions[2]
    facts_end = positions[2]
    original_end = prompt.find("Your previous answer:")
    original_end = len(prompt) if original_end < 0 else original_end
    return {
        "context_chars": context_end,
        "task_chars": max(task_end - context_end, 0),
        "facts_chars": max(facts_end - task_end, 0) if positions[1] >= 0 else 0,
        "run_chars": max(original_end - facts_end, 0),
        "feedback_chars": len(prompt) - original_end,
    }


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
