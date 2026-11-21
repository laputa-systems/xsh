import json
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, symlink, write


TOOL = r"""#!/bin/sh
# Fixture migration tool: records each call, then behaves as the migration file says.
{
  printf 'call %s\n' "$#"
  for a in "$@"; do printf 'arg[%s]\n' "$a"; done
  printf 'cwd[%s]\n' "$(pwd)"
  printf 'env[%s]\n' "${MIGRATION_ID-unset}"
  printf 'state[%s]\n' "$(tr '\n' ' ' < /work/state/applied 2>/dev/null)"
} >> "$(dirname "$0")/calls.log"
eval "file=\${$#}"
if [ ! -f "$file" ]; then
  echo "dbtool: cannot open $file" >&2
  exit 66
fi
echo "NOTICE:  applying $file"
code=$(sed -n 's/^-- fixture: fail //p' "$file")
if [ -n "$code" ]; then
  echo "ERROR:  relation \"widgets\" does not exist"
  printf 'ERROR:  relation "widgets" does not exist\nLINE 3: ALTER TABLE widgets ADD COLUMN "size %%" int;\n        ^\n' >&2
  exit "$code"
fi
echo "warning: $file took a while" >&2
printf 'OK %s\n' "$file"
"""

OK_SQL = "CREATE TABLE IF NOT EXISTS t (id int);\n"


def fail_sql(code: int) -> str:
    return f"-- fixture: fail {code}\nALTER TABLE widgets ADD COLUMN size int;\n"


VARIANTS = {
    "public": {
        "tool": "/work/bin/dbtool",
        "args": ["--database", "inventory", "--schema", ""],
        "migrations": {"0001-create items.sql": OK_SQL, "0002-add-price.sql": OK_SQL, "0003-add index.sql": OK_SQL},
        "others": {"README.md": "Run the migrator.\n"},
        "applied": None, "last_error": None, "log": None,
    },
    "resume": {
        "tool": "/work/bin/dbtool",
        "args": ["--database", "inventory", "--schema", ""],
        "migrations": {"0001-init.sql": OK_SQL, "0002-users.sql": OK_SQL, "0003-orders.sql": OK_SQL,
                       "0004-widgets.sql": fail_sql(3), "0005-later.sql": OK_SQL},
        "others": {"0006-draft.sql.disabled": OK_SQL, "009-short.sql": OK_SQL, ".0010-hidden.sql": OK_SQL,
                   "0007-dir.sql/keep": "not a migration\n"},
        "links": {"0008-link.sql": "0003-orders.sql"},
        "applied": "0001\n0002", "last_error": "stale error from yesterday\n", "log": "old log line\n",
    },
    "uptodate": {
        "tool": "/work/bin/dbtool",
        "args": ["--database", "inventory"],
        "migrations": {"0001-init.sql": OK_SQL, "0002-users.sql": OK_SQL},
        "others": {},
        "applied": "0001\n0002\n", "last_error": "stale error\n", "log": "previous runs\n",
    },
    "spaced": {
        "tool": "/work/tools/db tool",
        "args": ["--dsn", "postgres://db/main?sslmode=disable", "", "--label", "nightly run"],
        "migrations": {"0010-a.sql": OK_SQL, "0002-b.sql": OK_SQL, "0100-c.sql": fail_sql(7),
                       "0050-d d.sql": OK_SQL},
        "others": {"0011-UPPER.SQL": OK_SQL, "0012-x.sql.bak": OK_SQL},
        "applied": "0002\n", "last_error": None, "log": None,
    },
}


def tool_path(variant: str) -> str:
    return VARIANTS[variant]["tool"].removeprefix("/work/")


def calls_path(variant: str) -> str:
    return tool_path(variant).rsplit("/", 1)[0] + "/calls.log"


def pending(variant: str) -> list[tuple[str, str]]:
    spec = VARIANTS[variant]
    done = (spec["applied"] or "").split()
    found = sorted((name[:4], name) for name in spec["migrations"] if name[:4] not in done)
    return found


def outcome_of(variant: str, name: str) -> tuple[int, str, str]:
    content = VARIANTS[variant]["migrations"][name]
    if content.startswith("-- fixture: fail "):
        code = int(content.split("\n")[0].rsplit(" ", 1)[1])
        out = f"NOTICE:  applying {name}\nERROR:  relation \"widgets\" does not exist\n"
        err = 'ERROR:  relation "widgets" does not exist\nLINE 3: ALTER TABLE widgets ADD COLUMN "size %" int;\n        ^\n'
        return code, out, err
    return 0, f"NOTICE:  applying {name}\nOK {name}\n", f"warning: {name} took a while\n"


def expected(variant: str) -> dict:
    spec = VARIANTS[variant]
    before = (spec["applied"] or "").split()
    applied = list(before)
    runs, lines, log, error = [], [], spec["log"] or "", None
    for ident, name in pending(variant):
        code, out, err = outcome_of(variant, name)
        runs.append({"id": ident, "name": name, "state": list(applied)})
        log += out
        if code == 0:
            applied.append(ident)
            lines.append(f"applied {ident}")
        else:
            error = err
            lines.append(f"failed {ident}: exit {code}")
            break
    return {"runs": runs, "lines": lines, "log": log, "error": error,
            "applied": "".join(f"{i}\n" for i in applied), "status": 4 if error is not None else 0}


def setup(variant: str, work: Path) -> Setup:
    spec = VARIANTS[variant]
    write(work, "migrate.json", json.dumps({"tool": spec["tool"], "args": spec["args"]}, indent=2) + "\n")
    write(work, tool_path(variant), TOOL, mode=0o755)
    for name, content in {**spec["migrations"], **spec["others"]}.items():
        write(work, f"migrations/{name}", content)
    for name, target in spec.get("links", {}).items():
        symlink(work, f"migrations/{name}", target)
    for key, rel in (("applied", "state/applied"), ("last_error", "state/last-error"), ("log", "state/migrate.log")):
        if spec[key] is not None:
            write(work, rel, spec[key])
    return Setup()


def parse_calls(text: str) -> list[dict] | None:
    lines = text.split("\n")
    calls, i = [], 0
    while i < len(lines) and lines[i]:
        if not lines[i].startswith("call "):
            return None
        count = int(lines[i][5:])
        args = []
        for line in lines[i + 1:i + 1 + count]:
            if not (line.startswith("arg[") and line.endswith("]")):
                return None
            args.append(line[4:-1])
        rest = lines[i + 1 + count:i + 4 + count]
        if len(rest) != 3:
            return None
        cwd, env, state = (r[r.index("[") + 1:-1] for r in rest)
        calls.append({"args": args, "cwd": cwd, "env": env, "state": state.split()})
        i += 4 + count
    return calls


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    spec = VARIANTS[variant]
    wanted = expected(variant)
    if outcome.exit != wanted["status"]:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {wanted['status']}")
    log_path = work / calls_path(variant)
    calls = parse_calls(log_path.read_text()) if log_path.exists() else []
    if calls is None:
        return Verdict.wrong("the tool's call log is malformed (arguments with line breaks?)")
    if len(calls) != len(wanted["runs"]):
        return Verdict.wrong(f"the tool ran {len(calls)} times, expected {len(wanted['runs'])}")
    for call, run in zip(calls, wanted["runs"]):
        if call["args"] != [*spec["args"], "apply", run["name"]]:
            return Verdict.wrong(f"migration {run['id']} was run with the wrong arguments")
        if call["cwd"] != "/work/migrations":
            return Verdict.wrong(f"migration {run['id']} was run in {call['cwd']!r}")
        if call["env"] != run["id"]:
            return Verdict.wrong(f"migration {run['id']} saw MIGRATION_ID={call['env']!r}")
        if call["state"] != run["state"]:
            return Verdict.wrong(f"when migration {run['id']} started, the applied list was not up to date")
    state = work / "state"
    applied = state / "applied"
    if (applied.read_text() if applied.exists() else "") != wanted["applied"]:
        return Verdict.wrong("state/applied does not hold the expected ids")
    log = state / "migrate.log"
    if (log.read_text() if log.exists() else "") != wanted["log"]:
        return Verdict.wrong("state/migrate.log does not hold the tool's output")
    error = state / "last-error"
    if wanted["error"] is None:
        if error.exists():
            return Verdict.wrong("state/last-error exists after a run without failures")
    elif not error.exists() or error.read_text() != wanted["error"]:
        return Verdict.wrong("state/last-error does not hold the failing run's standard error")
    for rel, info in outcome.before.items():
        if rel.startswith("state/") or rel == "state":
            continue
        if outcome.after.get(rel) != info:
            return Verdict.wrong(f"{rel} was changed")
    allowed = {"state", "state/applied", "state/migrate.log", "state/last-error", calls_path(variant)}
    extra = sorted(set(outcome.after) - set(outcome.before) - allowed)
    if extra:
        return Verdict.wrong(f"unexpected new files: {extra}")
    if outcome.stdout.splitlines() != wanted["lines"]:
        return Verdict.wrong("the printed lines differ from the expected progress")
    return Verdict.ok(runs=len(calls))
