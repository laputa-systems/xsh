from pathlib import Path

from fixtures import Outcome, Setup, Verdict, stdout_jsonl, symlink, write

LOG = "bin/encode.log"

ENCODER = r"""#!/bin/sh
# Fixture encoder: logs start and end, takes a while, and fails on corrupt input.
log="$(dirname "$0")/encode.log"
name=$(basename "$1")
echo "start $name" >> "$log"
sleep 0.4
if grep -q CORRUPT "$1"; then
  printf 'fLaC partial' > "$2"
  echo "encode: corrupt frame at offset 4096 in $name" >&2
  echo "encode: giving up" >&2
  echo "end $name" >> "$log"
  exit 2
fi
{ printf 'fLaC\n'; cat "$1"; } > "$2"
echo "encode: wrote $2"
echo "end $name" >> "$log"
"""


def wav(label: str) -> bytes:
    return f"RIFF WAVE {label}\n".encode()


VARIANTS = {
    "public": {
        "inputs": {"Track 01.wav": wav("t1"), "track 02.wav": wav("t2"), "intro.wav": wav("intro"),
                   "outro.wav": wav("outro"), "b-side.wav": wav("b")},
        "others": {"README.txt": b"drop recordings here\n", "cover.WAV": wav("upper")},
        "links": {}, "existing": {},
    },
    "partial": {
        "inputs": {"a.wav": wav("a"), "b.wav": b"RIFF CORRUPT b\n", "c d.wav": wav("cd"), "e.wav": wav("e"),
                   "f.wav": b"RIFF CORRUPT f\n", "g.wav": wav("g"), "intro.wav": wav("intro"),
                   ".take2.wav": wav("take2")},
        "others": {"notes.wav/readme": b"a directory\n"},
        "links": {"link.wav": "a.wav"},
        "existing": {"intro.flac": b"fLaC\nold intro\n", "h.flac": b"fLaC\nno input\n"},
    },
    "single": {
        "inputs": {".hidden take.wav": b"RIFF CORRUPT hidden\n"},
        "others": {"x.wav.bak": wav("bak")}, "links": {}, "existing": {},
    },
    "none": {
        "inputs": {"done.wav": wav("done")},
        "others": {}, "links": {}, "existing": {"done.flac": b"fLaC\nalready\n"},
    },
}


def corrupt(content: bytes) -> bool:
    return b"CORRUPT" in content


def expected(variant: str) -> list[dict]:
    spec = VARIANTS[variant]
    rows = []
    for name in sorted(spec["inputs"], key=str.encode):
        output = name[:-4] + ".flac"
        if output in spec["existing"]:
            rows.append({"input": name, "status": "skipped", "error": None})
        elif corrupt(spec["inputs"][name]):
            rows.append({"input": name, "status": "failed",
                         "error": f"encode: corrupt frame at offset 4096 in {name}"})
        else:
            rows.append({"input": name, "status": "encoded", "error": None})
    return rows


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    write(work, "bin/encode", ENCODER, mode=0o755)
    for name, content in {**spec["inputs"], **spec["others"]}.items():
        write(work, f"incoming/{name}", content)
    for name, target in spec["links"].items():
        symlink(work, f"incoming/{name}", target)
    for name, content in spec["existing"].items():
        write(work, f"out/{name}", content)
    return Setup()


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    rows = expected(variant)
    status = 4 if any(r["status"] == "failed" for r in rows) else 0
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    log = work / LOG
    events = log.read_text().splitlines() if log.exists() else []
    started = sorted(e[6:] for e in events if e.startswith("start "))
    wanted_runs = sorted(r["input"] for r in rows if r["status"] != "skipped")
    if started != wanted_runs:
        return Verdict.wrong(f"the encoder ran for {started}, expected {wanted_runs}")
    running, peak = 0, 0
    for event in events:
        running += 1 if event.startswith("start ") else -1
        peak = max(peak, running)
    if peak > 3:
        return Verdict.wrong(f"{peak} encoders ran at once")
    if peak < min(3, len(wanted_runs)):
        return Verdict.wrong(f"only {peak} encoders ran at once")
    for row in rows:
        output = f"out/{row['input'][:-4]}.flac"
        if row["status"] == "skipped":
            if outcome.after.get(output) != outcome.before.get(output):
                return Verdict.wrong(f"the existing {output} was touched")
        elif row["status"] == "failed":
            if output in outcome.after:
                return Verdict.wrong(f"the partial {output} was left behind")
        else:
            data = (work / output).read_bytes() if (work / output).is_file() else None
            if data != b"fLaC\n" + spec["inputs"][row["input"]]:
                return Verdict.wrong(f"{output} does not hold the encoded recording")
    if "out" not in outcome.after or outcome.after["out"]["kind"] != "dir":
        return Verdict.wrong("/work/out was not created")
    allowed = {"out", LOG} | {f"out/{r['input'][:-4]}.flac" for r in rows if r["status"] == "encoded"}
    extra = sorted(set(outcome.after) - set(outcome.before) - allowed)
    if extra:
        return Verdict.wrong(f"unexpected new files: {extra}")
    for rel, info in outcome.before.items():
        if rel.startswith("incoming") and outcome.after.get(rel) != info:
            return Verdict.wrong(f"{rel} was changed")
    printed, problem = stdout_jsonl(outcome)
    if problem:
        return Verdict.wrong(problem)
    if printed != rows:
        return Verdict.wrong("the printed lines differ from the expected results")
    return Verdict.ok(runs=len(started), peak=peak)
