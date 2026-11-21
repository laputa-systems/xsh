import json
import re
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "harness"))

import tasks
from config import CONTEXTS, RUNS

WORD = re.compile(r"[A-Za-z0-9][A-Za-z0-9_./-]{3,}")
DISTINCT = re.compile(r"[0-9_./-]")


def variant_words(task: tasks.Task, variant: str) -> set[str]:
    with tempfile.TemporaryDirectory(dir=RUNS) as scratch:
        work = Path(scratch) / "work"
        work.mkdir()
        setup = task.fixture.setup(variant, work)
        text = json.dumps({"argv": setup.argv, "env": setup.env, "responses": setup.responses})
        for path in work.rglob("*"):
            text += "\n" + path.name
            if path.is_file() and not path.is_symlink():
                text += "\n" + path.read_bytes().decode(errors="replace")
    return set(WORD.findall(text))


def main() -> int:
    runs = [RUNS / name for name in sys.argv[1:]]
    background = set()
    for context in CONTEXTS.glob("*.md"):
        background |= set(WORD.findall(context.read_text()))
    good = True
    for task in tasks.load("dev"):
        public = variant_words(task, task.variants[0]) | set(WORD.findall(json.dumps(task.data)))
        hidden = set()
        for variant in task.variants[1:]:
            hidden |= variant_words(task, variant)
        secret = {w for w in hidden - public - background if DISTINCT.search(w)}
        prompts = [p for run in runs for p in (run / "trials" / task.id).glob("*/prompt-*.md")]
        leaked = {}
        for prompt in prompts:
            found = sorted(w for w in secret if w in prompt.read_text())
            if found:
                leaked[str(prompt.relative_to(RUNS))] = found[:5]
        good &= not leaked
        print(f"{task.id:20} {len(secret):4} hidden-only tokens, {len(prompts):3} prompts, leaked: {leaked or 'none'}")
    print("hidden-answer separation:", "holds" if good else "FAILS")
    return 0 if good else 1


if __name__ == "__main__":
    raise SystemExit(main())
