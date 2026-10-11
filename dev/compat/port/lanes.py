#!/usr/bin/env python3
"""Create port lanes: one per (suite, utility, chunk) of the frozen tests.

    lanes.py uutils UTIL [UTIL...] [--chunk N]      create lanes (not spawned)
    lanes.py list [--suite S]                       utilities with unmapped tests

Each lane's brief lists the exact test identifiers, the upstream source file,
the output file it owns, and the transcription rules. Lane names are
`port-<suite>-<util>[-<n>]`. Spawning is the coordinator's job (Haiku 5.5 high).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
LANE = REPO / ".claude/skills/xsh-compat-campaign/scripts/lane.py"
UUTILS = REPO.parent / "ref/uutils-coreutils"

RULES = """Transcribe each listed upstream test into a native XSH test. Rules:
1. One XSH test per upstream test, named test_uu_<util>_<upstream name without its leading test_>, in your owned file, with the comment line `# origin: uutils <exact id>` immediately above the `test` line. Keep the upstream order.
2. Use the helper module: `use support.uu as uu` (read core/tests/support/uu.xsh and core/tests/test-uu-basename.xsh first). Map the Rust builder one to one: new_ucmd!/scene.ucmd().args(..).pipe_in(..).env(..) -> uu.invoke(s, "<util>", [..], stdin: b"..", vars: {..}); .succeeds()/.fails()/.fails_with_code(n) -> uu.succeeds/uu.fails/uu.fails_with_code; .stdout_is/.stdout_only/.stderr_is/.stderr_only/.stdout_contains/.stderr_contains/.no_stdout/.no_stderr/.no_output likewise; at.touch/write/read/mkdir/file_exists/dir_exists/symlink_file/symlink_dir/hard_link/is_symlink/append/set_mode/remove/rename/mkfifo/truncate -> uu.touch(s, ..) etc. (see the file for every helper); fixtures from tests/fixtures/<util>/ are copied once to core/tests/data/uutils/<util>/ and used with uu.fixture(s, "<util>", name, as_name). A helper you need that does not exist: do not edit support/uu.xsh; write a small local proc in your file, and list it under Requests.
3. The assertions are the upstream assertions, unchanged: same arguments, same inputs, same expected bytes and statuses. Never weaken one to make it pass. Where the upstream test is #[cfg]-gated, transcribe the branch that applies to x86_64 Linux musl.
4. Run your file with `XSH_BIN=/home/josh/d/laputa-systems/xsh/target/x86_64-unknown-linux-musl/release/xsh /home/josh/d/laputa-systems/xsh/target/x86_64-unknown-linux-musl/release/xsht test core/tests/<your file>`. Every test must pass. If a transcribed test fails, first check your transcription against the Rust source; if the transcription is faithful and the applet differs, report the id and the output under Findings and leave the test out of the file.
5. A test that cannot be a native XSH test (it needs strace, a pseudo-terminal, root, rlimits, real timing, SELinux, or an interactive terminal) goes into dev/compat/port/exceptions.json as {"uutils <id>": "reason"} instead; keep the file's existing entries. Use sparingly: most tests are plain command runs.
6. Finish with `python3 dev/compat/port/check_port.py --suite uutils --util <util>`: every id of your list must be mapped (a tag or an exception). Commit only your owned files."""

def freeze() -> dict:
    return json.loads((REPO / "dev/compat/port/freeze.json").read_text())

def tagged() -> set[str]:
    import re
    out: set[str] = set()
    pat = re.compile(r"^\s*#\s*origin:\s*(\S+)\s+(.+?)\s*$")
    for root in (REPO / "core/tests", REPO / "tests/xsh"):
        for path in root.rglob("*.xsh"):
            for line in path.read_text(errors="replace").splitlines():
                m = pat.match(line)
                if m:
                    out.add(f"{m.group(1)} {m.group(2)}")
    exc = REPO / "dev/compat/port/exceptions.json"
    if exc.exists():
        out |= set(json.loads(exc.read_text()))
    return out

def create(suite: str, util: str, chunk: int) -> list[str]:
    ids = freeze()[suite][util]
    done = tagged()
    ids = [i for i in ids if f"{suite} {i}" not in done]
    names = []
    parts = [ids[i:i + chunk] for i in range(0, len(ids), chunk)]
    for n, part in enumerate(parts, 1):
        suffix = f"-{n}" if len(parts) > 1 else ""
        lane = f"port-{suite}-{util}{suffix}"
        owned = f"core/tests/test-uu-{util}{suffix}.xsh"
        goal = (f"Port {len(part)} upstream tests of the {suite} suite for `{util}` to native XSH tests.\n"
                f"Upstream source (read-only): {UUTILS}/tests/by-util/test_{util}.rs and fixtures under {UUTILS}/tests/fixtures/{util}/.\n"
                f"You own: {owned}, core/tests/data/uutils/{util}/, dev/compat/port/exceptions.json.\n\n{RULES}\n\nThe tests (ids):\n"
                + "\n".join(part))
        cmd = [sys.executable, str(LANE), "new", lane, "--own", owned, f"core/tests/data/uutils/{util}/",
               "dev/compat/port/exceptions.json", "--goal", goal]
        subprocess.run(cmd, check=True, capture_output=True, text=True, cwd=REPO)
        names.append(lane)
    return names

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action")
    parser.add_argument("utils", nargs="*")
    parser.add_argument("--chunk", type=int, default=80)
    parser.add_argument("--suite", default="uutils")
    args = parser.parse_args()
    if args.action == "list":
        done = tagged()
        for util, ids in freeze()[args.suite].items():
            left = [i for i in ids if f"{args.suite} {i}" not in done]
            if left:
                print(f"{util}: {len(left)}/{len(ids)}")
        return
    if args.action != "uutils":
        sys.exit("only the uutils suite has a generator so far")
    for util in args.utils:
        for name in create("uutils", util, args.chunk):
            print(name)

if __name__ == "__main__":
    main()
