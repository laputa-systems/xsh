import json
import random
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from statistics import mean

import evaluate
import manifest
import trial

ROUNDS = 4000
SEED = 7
TOKEN_FIELDS = ("input", "cache_read", "cache_write", "output", "reasoning")
CATEGORIES = (
    "syntax/binding", "missing/misunderstood API", "external-data/schema boundary", "result/optional/error handling",
    "process/argv/path", "stream/concurrency", "cleanup/partial effect", "task logic", "compiler/runtime defect",
    "harness/provider",
)


@dataclass(slots=True)
class Tokens:
    known: dict[str, int] = field(default_factory=lambda: dict.fromkeys(TOKEN_FIELDS, 0))
    unknown: Counter = field(default_factory=Counter)
    cost: float = 0.0
    cost_unknown: int = 0
    cost_from_provider: set = field(default_factory=set)

    def add(self, usage: dict) -> None:
        for name in TOKEN_FIELDS:
            value = usage.get(name)
            if value is None:
                self.unknown[name] += 1
            else:
                self.known[name] += int(value)
        if usage.get("cost_estimate") is None:
            self.cost_unknown += 1
        else:
            self.cost += float(usage["cost_estimate"])
        self.cost_from_provider.add(usage.get("cost_from_provider"))

    @property
    def total(self) -> int:
        return self.known["input"] + self.known["cache_read"] + self.known["cache_write"] + self.known["output"]


@dataclass(slots=True)
class Cell:
    task: str
    track: str
    condition: str
    sample: int
    state: str
    statuses: list[str]
    first: bool
    final: bool
    tokens: Tokens
    model_calls: int
    checks: int
    runs: int
    model_seconds: float
    program_seconds: float
    authoring_seconds: float
    prompt_input: int | None
    interruptions: int
    thinking: set[str]
    duplicate_writes: int
    rejected_inputs: int
    attempts: list[dict]


def final_commands(directory: Path, record: trial.TrialRecord) -> list[str]:
    if not record.attempts or record.attempts[-1].code_file is None:
        return []
    return evaluate.external_commands(record.arm, (directory / record.attempts[-1].code_file).read_text())


def load_cells(out: Path) -> tuple[dict, list[Cell], list[dict]]:
    loaded = manifest.load(out)
    cells, missing = [], []
    for item in loaded["schedule"]:
        record = trial.load(trial.trial_directory(out, item["task"], item["condition"], item["sample"]) / "trial.json")
        if record is None or record.state == "running":
            missing.append({**item, "state": "missing" if record is None else "incomplete"})
            continue
        cell = cell_of(record)
        if cell.attempts:
            cell.attempts[-1]["commands"] = final_commands(
                trial.trial_directory(out, item["task"], item["condition"], item["sample"]), record,
            )
        cells.append(cell)
    return loaded, cells, missing


def cell_of(record: trial.TrialRecord) -> Cell:
    tokens = Tokens()
    checks = runs = interruptions = duplicates = rejected = 0
    model_seconds = program_seconds = 0.0
    thinking: set[str] = set()
    attempts = []
    for attempt in record.attempts:
        reply = attempt.reply
        tokens.add(reply["usage"])
        model_seconds += reply["wall_seconds"]
        interruptions += len(attempt.interruptions)
        if reply.get("thinking_level"):
            thinking.add(reply["thinking_level"])
        evaluation = attempt.evaluation or {}
        if evaluation.get("check"):
            checks += 1
            program_seconds += evaluation.get("check_seconds", 0.0)
        for variant in evaluation.get("variants", []):
            runs += 1
            program_seconds += variant["execution"]["seconds"]
            duplicates += sum(1 for c in variant["calls"] if c.get("outcome") == "duplicate-write")
            rejected += sum(1 for c in variant["calls"] if c.get("outcome") in ("rejected-input", "unknown-operation"))
        attempts.append({
            "index": attempt.index, "status": attempt.status, "detail": attempt.detail,
            "codes": evaluation.get("diagnostic_codes", []), "commands": evaluation.get("commands", []),
        })
    statuses = [a.status for a in record.attempts]
    first_usage = record.attempts[0].reply["usage"] if record.attempts else {}
    return Cell(
        record.task, record.track, record.condition, record.sample, record.state, statuses,
        bool(statuses) and statuses[0] == "correct", bool(statuses) and statuses[-1] == "correct", tokens,
        len(record.attempts), checks, runs, model_seconds, program_seconds,
        (record.finished or record.started) - record.started, first_usage.get("input"), interruptions, thinking,
        duplicates, rejected, attempts,
    )


def bootstrap(tasks: list[str], statistic, rounds: int = ROUNDS) -> tuple[float, float] | None:
    if not tasks:
        return None
    rng = random.Random(SEED)
    values = []
    for _ in range(rounds):
        sample = rng.choices(tasks, k=len(tasks))
        value = statistic(sample)
        if value is not None:
            values.append(value)
    if not values:
        return None
    values.sort()
    return values[int(len(values) * 0.025)], values[min(int(len(values) * 0.975), len(values) - 1)]


def rate(cells: list[Cell], tasks: list[str], key: str) -> float | None:
    per_task = [mean(getattr(c, key) for c in cells if c.task == t) for t in tasks if any(c.task == t for c in cells)]
    return mean(per_task) if per_task else None


def per_solved(cells: list[Cell], tasks: list[str]) -> float | None:
    chosen = [c for t in tasks for c in cells if c.task == t]
    solved = sum(c.final for c in chosen)
    return sum(c.tokens.total for c in chosen) / solved if solved else None


def pct(value: float | None) -> str:
    return "n/a" if value is None else f"{100 * value:.1f}%"


def interval(bounds: tuple[float, float] | None, scale: float = 100.0, unit: str = "%") -> str:
    if bounds is None:
        return "n/a"
    low, high = bounds
    if unit == "%":
        return f"[{scale * low:.1f}, {scale * high:.1f}]"
    return f"[{low:,.0f}, {high:,.0f}]"


def condition_rows(cells: list[Cell], conditions: list[str], track: str | None) -> list[str]:
    rows = [
        "| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] "
        "| Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures "
        "| Completed but wrong | Mean programs/trial |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for condition in conditions:
        mine = [c for c in cells if c.condition == condition and (track is None or c.track == track)]
        if not mine:
            continue
        tasks = sorted({c.task for c in mine})
        first = rate(mine, tasks, "first")
        final = rate(mine, tasks, "final")
        first_ci = bootstrap(tasks, lambda s: rate(mine, s, "first"))
        final_ci = bootstrap(tasks, lambda s: rate(mine, s, "final"))
        tokens = per_solved(mine, tasks)
        tokens_ci = bootstrap(tasks, lambda s: per_solved(mine, s))
        solved = sum(c.final for c in mine)
        cost = sum(c.tokens.cost for c in mine)
        cost_text = f"${cost / solved:.4f}" if solved else "undefined (none solved)"
        statuses = Counter(s for c in mine for s in c.statuses)
        programs = sum(statuses[s] for s in ("correct", "wrong", "failed", "rejected"))
        rows.append(
            f"| {condition} | {len(mine)} | {pct(first)} {interval(first_ci)} | {pct(final)} {interval(final_ci)} "
            f"| {'undefined' if tokens is None else f'{tokens:,.0f}'} {interval(tokens_ci, unit='tokens')} "
            f"| {cost_text} | {mean(c.authoring_seconds for c in mine):.0f} "
            f"| {statuses['rejected']}/{programs} | {statuses['failed']}/{programs} | {statuses['wrong']}/{programs} "
            f"| {mean(len(c.statuses) for c in mine):.2f} |"
        )
    return rows


def paired_rows(cells: list[Cell], pairs: list[tuple[str, str]], track: str | None) -> list[str]:
    rows = [
        "| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] "
        "| Tokens per solved, ratio |",
        "|---|---|---|---|---|",
    ]
    for candidate, baseline in pairs:
        mine = [c for c in cells if track is None or c.track == track]
        a = [c for c in mine if c.condition == candidate]
        b = [c for c in mine if c.condition == baseline]
        tasks = sorted({c.task for c in a} & {c.task for c in b})
        if not tasks:
            continue

        def diff(sample: list[str], key: str) -> float | None:
            x, y = rate(a, sample, key), rate(b, sample, key)
            return None if x is None or y is None else x - y

        def ratio(sample: list[str]) -> float | None:
            x, y = per_solved(a, sample), per_solved(b, sample)
            return None if x is None or y is None else x / y

        first, final, r = diff(tasks, "first"), diff(tasks, "final"), ratio(tasks)
        ratio_ci = bootstrap(tasks, ratio)
        ratio_text = "undefined" if r is None else f"{r:.2f} [{ratio_ci[0]:.2f}, {ratio_ci[1]:.2f}]" if ratio_ci else f"{r:.2f}"
        rows.append(
            f"| {candidate} − {baseline} | {len(tasks)} | {pct(first)} {interval(bootstrap(tasks, lambda s: diff(s, 'first')))} "
            f"| {pct(final)} {interval(bootstrap(tasks, lambda s: diff(s, 'final')))} | {ratio_text} |"
        )
    return rows


def task_rows(cells: list[Cell], conditions: list[str]) -> list[str]:
    rows = ["| Task | Track | " + " | ".join(conditions) + " |", "|---|---|" + "---|" * len(conditions)]
    for task in sorted({c.task for c in cells}):
        track = next(c.track for c in cells if c.task == task)
        parts = []
        for condition in conditions:
            mine = sorted((c for c in cells if c.task == task and c.condition == condition), key=lambda c: c.sample)
            if not mine:
                parts.append("—")
                continue
            parts.append(" ".join("/".join(short(s) for s in c.statuses) or "∅" for c in mine))
        rows.append(f"| {task} | {track} | " + " | ".join(parts) + " |")
    return rows


def short(status: str) -> str:
    return {"correct": "✓", "wrong": "W", "failed": "F", "rejected": "R", "no-answer": "∅", "infra-error": "I",
            "provider-error": "P"}.get(status, status)


def accounting_rows(cells: list[Cell], conditions: list[str]) -> list[str]:
    rows = [
        "| Condition | Model calls | Checks | Program runs | Uncached input | Cache read | Cache write | Output "
        "| of which reasoning | Unknown usage fields | Mean first-prompt input | Model s | Program s | Provider interruptions "
        "| Duplicate writes | Rejected service inputs | Reported thinking |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for condition in conditions:
        mine = [c for c in cells if c.condition == condition]
        if not mine:
            continue
        known = {f: sum(c.tokens.known[f] for c in mine) for f in TOKEN_FIELDS}
        unknown = sum(sum(c.tokens.unknown.values()) for c in mine)
        prompts = [c.prompt_input for c in mine if c.prompt_input is not None]
        thinking = sorted({t for c in mine for t in c.thinking})
        rows.append(
            f"| {condition} | {sum(c.model_calls for c in mine)} | {sum(c.checks for c in mine)} | {sum(c.runs for c in mine)} "
            f"| {known['input']:,} | {known['cache_read']:,} | {known['cache_write']:,} | {known['output']:,} "
            f"| {known['reasoning']:,} | {unknown} | {mean(prompts) if prompts else 0:,.0f} | {sum(c.model_seconds for c in mine):,.0f} "
            f"| {sum(c.program_seconds for c in mine):,.0f} | {sum(c.interruptions for c in mine)} "
            f"| {sum(c.duplicate_writes for c in mine)} | {sum(c.rejected_inputs for c in mine)} | {', '.join(thinking)} |"
        )
    return rows


def diagnostics_rows(cells: list[Cell]) -> list[str]:
    seen: Counter = Counter()
    fixed: Counter = Counter()
    for cell in cells:
        for before, after in zip(cell.attempts, cell.attempts[1:]):
            for code in set(before["codes"]):
                key = (cell.condition, code)
                seen[key] += 1
                fixed[key] += code not in after["codes"]
    rows = ["| Condition | Diagnostic | Seen before a repair | Gone in the next program |", "|---|---|---|---|"]
    rows += [f"| {c} | `{code}` | {n} | {fixed[(c, code)]} |" for (c, code), n in seen.most_common()]
    return rows if seen else ["No repaired program carried a diagnostic code."]


def repair_rows(cells: list[Cell], conditions: list[str]) -> list[str]:
    rows = ["| Condition | Trials repaired | Correct after repair | Rejections then correct | Failures then correct |",
            "|---|---|---|---|---|"]
    for condition in conditions:
        mine = [c for c in cells if c.condition == condition and len(c.statuses) > 1]
        rows.append(
            f"| {condition} | {len(mine)} | {sum(c.final for c in mine)} "
            f"| {sum(c.final for c in mine if c.statuses[0] == 'rejected')} "
            f"| {sum(c.final for c in mine if c.statuses[0] == 'failed')} |"
        )
    return rows


def command_rows(cells: list[Cell], conditions: list[str]) -> list[str]:
    rows = ["| Condition | Final programs that start external commands | Commands (programs using them) |", "|---|---|---|"]
    for condition in conditions:
        mine = [c for c in cells if c.condition == condition and c.attempts]
        used: Counter = Counter()
        users = 0
        for cell in mine:
            commands = cell.attempts[-1]["commands"]
            users += bool(commands)
            used.update(set(commands))
        shown = ", ".join(f"`{name}` {n}" for name, n in used.most_common()) or "none"
        rows.append(f"| {condition} | {users}/{len(mine)} | {shown} |")
    return rows


def taxonomy_rows(taxonomy: dict, conditions: list[str]) -> list[str]:
    counts: Counter = Counter()
    for entry in taxonomy.get("attempts", []):
        counts[(entry["condition"], entry["category"])] += 1
    rows = ["| Category | " + " | ".join(conditions) + " |", "|---|" + "---|" * len(conditions)]
    for category in CATEGORIES:
        rows.append(f"| {category} | " + " | ".join(str(counts[(c, category)]) for c in conditions) + " |")
    return rows


def render(runs: list[Path], taxonomy_paths: list[Path] | None) -> str:
    cells: list[Cell] = []
    missing: list[dict] = []
    manifests = []
    for out in runs:
        loaded, found, absent = load_cells(out)
        manifests.append((out, loaded))
        cells += found
        missing += absent
    conditions = sorted({c.condition for c in cells}, key=lambda c: (c == "ts", c))
    lines = ["# Code mode report", ""]
    for out, loaded in manifests:
        lines.append(
            f"- `{out.name}`: phase {loaded['phase']}, split {loaded['split']}, model `{loaded['model']}`, thinking requested "
            f"`{loaded['thinking_requested']}`, pi {loaded['pi_version']}, {len(loaded['schedule'])} planned cells, "
            f"seed {loaded['seed']}, created {loaded['created']}"
        )
    lines += ["", f"Recorded cells: {len(cells)}. Missing or incomplete cells: {len(missing)}.", ""]
    if missing:
        lines += [f"- {m['cell']}: {m['state']}" for m in missing] + [""]
    discarded = [cell_of(r) for out in runs for p in sorted(out.glob("discarded/*/*/trial.json")) if (r := trial.load(p))]
    if discarded:
        tokens = sum(c.tokens.total for c in discarded)
        lines += [f"Discarded after an infrastructure failure and rerun: {len(discarded)} trials, "
                  f"{sum(c.model_calls for c in discarded)} model calls, {tokens:,} tokens (not in the tables below).", ""]
    infra = [c for c in cells if c.state == "infra-error" or "provider-error" in c.statuses]
    if infra:
        lines += ["Infrastructure or provider failures (kept in the counts as not correct):", ""]
        lines += [f"- {c.task}/{c.condition}/{c.sample}: {c.statuses}" for c in infra] + [""]
    pairs = [(a, b) for a, b in (
        ("xsh-candidate", "xsh-baseline"), ("xsh-revised", "xsh-baseline"), ("xsh-revised", "xsh-candidate"),
        ("xsh-selected", "xsh-baseline"), ("ts", "xsh-baseline"), ("ts", "xsh-candidate"), ("ts", "xsh-selected"),
    ) if a in conditions and b in conditions]
    for track, title in ((None, "All tasks"), ("shared", "Shared-capability track"), ("native", "Native track")):
        lines += [f"## {title}", "", *condition_rows(cells, conditions, track), "", *paired_rows(cells, pairs, track), ""]
    lines += [
        "Rates average samples within each task, then tasks; intervals bootstrap over tasks. Tokens per solved task "
        "divide every recorded token of the condition's trials, failed ones included, by its solved trials; "
        "uncached input + cache read + cache write + output (reasoning is inside output). Cost is pi's estimate "
        "(`totalFromProvider: false`), not a provider-reported cost.", "",
        "## Per task", "", "Each cell lists the attempts of each sample: ✓ correct, W completed but wrong, F runtime "
        "failure, R rejected by the checker, ∅ no answer, P provider error, I infrastructure error.", "",
        *task_rows(cells, conditions), "",
        "## Accounting", "", *accounting_rows(cells, conditions), "",
        "## Repairs", "", *repair_rows(cells, conditions), "", *diagnostics_rows(cells), "",
        "## External commands", "", "Literal command heads in each trial's last program (`run CMD` in XSH, "
        "`spawn`/`execFile` with a literal program in TypeScript).", "", *command_rows(cells, conditions), "",
    ]
    if taxonomy_paths:
        taxonomy = {"attempts": [e for p in taxonomy_paths for e in json.loads(p.read_text())["attempts"]]}
        lines += ["## Failure taxonomy", "", "Every program that was not correct, classified by hand "
                  "(`results/*-taxonomy.json` holds each one with a note).", "", *taxonomy_rows(taxonomy, conditions), ""]
    return "\n".join(lines)
