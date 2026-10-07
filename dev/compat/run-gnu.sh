#!/bin/bash
# Gate 4: GNU coreutils test-suite differential, pinned uutils vs XSH.
#
#   UUTILS_ROOT=../ref/uutils-coreutils GNU_ROOT=../ref/gnu-coreutils \
#       dev/compat/run-gnu.sh [prepare|uutils|xsh|diff|all] [TEST...]
#
# TEST names are paths inside the GNU tree, e.g. tests/ls/dired.sh. With none,
# the complete suite runs (hours on a small machine; prefer per-utility groups).
#
# Design:
# - `prepare` builds one GNU tree with uutils' own util/build-gnu.sh (GNU
#   version pinned by uutils' util/fetch-gnu.sh at the locked commit). That
#   applies uutils' patches and test rewrites.
# - `uutils` and `xsh` run the *same* prepared tests, differing only in the
#   PATH entry that tests/local.mk prepends: the uutils multicall build
#   directory, or the XSH stage's gnu-bin/ (every GNU program name; names XSH
#   lacks are `false`). Because both sides share one harness, uutils'
#   patches cannot bias the differential. Which of those patches XSH should
#   adopt as policy for *absolute* GNU conformance is tracked separately in
#   dev/compat/gnu-patches.json.
# - The uutils baseline is cached per uutils commit in
#   dev/compat/results/gnu-uutils.json; XSH runs land in gnu-xsh.json, and
#   `diff` writes gnu-differential.json with the four cells.
#   COMPAT_RESULTS_DIR redirects all artifacts to scratch space.
# - Root test runs use an existing account selected by GNU_RUN_UID/GID
#   (default 1000); the prepared GNU tree must be writable by that account.
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
uutils=$(cd "${UUTILS_ROOT:?set UUTILS_ROOT}" && pwd)
gnu=${GNU_ROOT:-$(dirname "$uutils")/gnu-coreutils}
results=${COMPAT_RESULTS_DIR:-$repo/dev/compat/results}
stage=${XSH_COMPAT_STAGE:-$repo/target/compat-stage}
mode=${1:-all}
shift || true
mkdir -p "$results"

# Share the lock with the uutils integration runner, which temporarily disables
# the same multicall binary. Children must not keep the lock after this runner.
exec 9>"${UUTILS_SUITE_LOCK:-/tmp/uutils-suite.lock}"
flock 9
work=$(mktemp -d "$results/.gnu-run.XXXXXX")
restore_path=false
cleanup() {
	if "$restore_path"; then point_path_at "$uu_build"; fi
	rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["uutils"]["commit"])' "$repo/dev/compat/upstream.lock.json")
[ "$(git -C "$uutils" rev-parse HEAD)" = "$want" ] || { echo "UUTILS_ROOT is not at locked commit $want" >&2; exit 2; }

uu_build=${UUTILS_TARGET_DIR:-${CARGO_TARGET_DIR:-$uutils/target}}/debug

# GNU permission tests require a real unprivileged account. User namespaces are
# unavailable in ordinary container seccomp profiles and do not supply one.
run_uid=${GNU_RUN_UID:-${UUTESTS_RUN_UID:-1000}}
run_gid=${GNU_RUN_GID:-${UUTESTS_RUN_GID:-$run_uid}}
if [ "$(id -u)" -eq 0 ]; then
	for value in "$run_uid" "$run_gid"; do
		case "$value" in
			*[!0-9]* | "") echo "GNU test uid and gid must be numeric" >&2; exit 2 ;;
		esac
	done
	[ "$run_uid" -ne 0 ] && [ "$run_gid" -ne 0 ] || { echo "GNU tests require nonzero uid and gid" >&2; exit 2; }
	command -v setpriv >/dev/null || { echo "root runs require setpriv" >&2; exit 2; }
	getent passwd "$run_uid" >/dev/null || { echo "create an unprivileged GNU test account for uid $run_uid" >&2; exit 2; }
fi
as_user() {
	if [ "$(id -u)" -eq 0 ]; then
		setpriv --reuid="$run_uid" --regid="$run_gid" --clear-groups -- "$@" 9>&-
	else
		"$@" 9>&-
	fi
}

prepare() {
	# See run-uutils.sh: a missing docs/tldr.zip makes cargo rebuild every time.
	[ -e "$uutils/docs/tldr.zip" ] || : >"$uutils/docs/tldr.zip"
	if [ ! -f "$gnu/configure" ]; then
		mkdir -p "$gnu"
		(cd "$gnu" && bash "$uutils/util/fetch-gnu.sh" 9>&-)
	fi
	(cd "$uutils" && FORCE_UNSAFE_CONFIGURE=1 PROFILE=debug path_GNU="$gnu" bash util/build-gnu.sh 9>&-)
}

point_path_at() {
	local dir=$1 expr
	expr="s|^[[:blank:]]*PATH=.*|  PATH='${dir}\$(PATH_SEPARATOR)'\"\$\$PATH\" \\\\|"
	for f in Makefile tests/local.mk; do
		[ -f "$gnu/$f" ] && as_user sed -i "$expr" "$gnu/$f"
	done
	as_user touch "$gnu/Makefile.in" "$gnu/Makefile"
}

check_test_tree() {
	for path in "$gnu" "$gnu/tests" "$gnu/Makefile.in"; do
		as_user test -w "$path" || { echo "GNU test account must be able to write $path; fix tree ownership before running" >&2; return 1; }
	done
	as_user test -r "$gnu/Makefile" || { echo "GNU test account cannot read $gnu/Makefile" >&2; return 1; }
}

run_suite() {
	local out=$1 status=0
	shift
	# Ask the prepared Makefile for its complete selection, including generated
	# factor tests, before deleting evidence from earlier invocations.
	(cd "$gnu" && as_user make -s --no-print-directory \
		--eval 'compat-list-tests:;@printf "%s\n" $(TEST_LOGS)' \
		compat-list-tests ${1+TESTS="$*"} SUBDIRS=. gl_public_submodule_commit="" srcdir="$gnu") >"$work/expected"
	python3 - "$gnu" "$work/expected" <<'EOF'
import sys
from pathlib import Path
root = Path(sys.argv[1])
logs = Path(sys.argv[2]).read_text().splitlines()
if not logs or len(set(logs)) != len(logs):
    sys.exit("GNU harness produced an empty or duplicate test selection")
for name in logs:
    path = Path(name)
    if path.is_absolute() or ".." in path.parts or not name.startswith("tests/") or path.suffix != ".log":
        sys.exit(f"invalid GNU test log selection: {name!r}")
    (root / path).unlink(missing_ok=True)
    (root / path.with_suffix(".trs")).unlink(missing_ok=True)
(root / "tests/test-suite.log").unlink(missing_ok=True)
EOF
	(cd "$gnu" && as_user env -u TERM LC_ALL=C TZ=UTC timeout -sKILL 4h make -j "${GNU_JOBS:-3}" check \
		${1+TESTS="$*"} SUBDIRS=. RUN_EXPENSIVE_TESTS=yes RUN_VERY_EXPENSIVE_TESTS=yes \
		VERBOSE=no gl_public_submodule_commit="" srcdir="$gnu") || status=$?
	# Make returns 2 for failing tests, but also for broken builds. A complete
	# fresh summary and matching per-test evidence distinguish usable results.
	python3 - "$gnu" "$work/expected" "$status" "$out" "$want" <<'EOF'
import json, re, sys
from collections import Counter
from pathlib import Path
root, expected, status, output, commit = sys.argv[1:]
root = Path(root)
status = int(status)
if status not in (0, 2):
    sys.exit(f"GNU harness exited {status}; not updating results")
counts = Counter()
report = {}
for name in Path(expected).read_text().splitlines():
    log = root / name
    trs = log.with_suffix(".trs")
    if not log.is_file() or not trs.is_file():
        sys.exit(f"missing GNU test evidence for {name}; not updating results")
    results = re.findall(r"^:test-result: (PASS|FAIL|SKIP|ERROR|XFAIL|XPASS)$", trs.read_text(), re.M)
    if len(results) != 1:
        sys.exit(f"invalid GNU test result in {trs}; not updating results")
    result = results[0]
    # The driver appends its result without inserting a newline after test output.
    ending = re.search(r"(PASS|FAIL|SKIP|ERROR|XFAIL|XPASS) [^\n]* \(exit status: \d+\)\n?\Z", log.read_text(errors="replace"))
    if ending is None or ending.group(1) != result:
        sys.exit(f"GNU log/result disagreement for {name}; not updating results")
    counts[result] += 1
    current = report
    path = Path(name).relative_to("tests")
    for part in path.parent.parts:
        current = current.setdefault(part, {})
    current[path.name] = result
summary = root / "tests/test-suite.log"
if not summary.is_file():
    sys.exit("missing GNU harness summary; not updating results")
summary_counts = {}
for key in ("TOTAL", "PASS", "FAIL", "SKIP", "ERROR", "XFAIL", "XPASS"):
    values = re.findall(rf"^# {key}:\s*(\d+)\s*$", summary.read_text(), re.M)
    if len(values) != 1:
        sys.exit(f"missing or duplicate GNU summary count {key}; not updating results")
    summary_counts[key] = int(values[0])
if summary_counts != {"TOTAL": sum(counts.values()), **{key: counts[key] for key in summary_counts if key != "TOTAL"}}:
    sys.exit("incomplete GNU harness summary; not updating results")
failed = sum(counts[key] for key in ("FAIL", "ERROR", "XPASS"))
if status != (2 if failed else 0):
    sys.exit(f"GNU make exit {status} disagrees with test results; not updating results")
if Path(output).name == "gnu-uutils.json":
    report = {"uutils_commit": commit, "results": report}
output = Path(output)
temporary = output.with_suffix(".json.tmp")
temporary.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
temporary.replace(output)
EOF
}

run_uutils() {
	check_test_tree
	point_path_at "$uu_build"
	as_user test -x "$uu_build/coreutils" || { echo "GNU test account cannot execute $uu_build/coreutils" >&2; return 1; }
	run_suite "$results/gnu-uutils.json" "$@"
}

run_xsh() {
	check_test_tree
	(cd "$gnu" && ./build-aux/gen-lists-of-programs.sh --list-progs) >"$stage.gnu-programs"
	python3 "$repo/dev/compat/stage.py" --stage "$stage" --gnu-programs "$stage.gnu-programs"
	restore_path=true
	point_path_at "$stage/gnu-bin"
	run_suite "$results/gnu-xsh.json" "$@"
	point_path_at "$uu_build"
	restore_path=false
}

diff_results() {
	python3 - "$results/gnu-uutils.json" "$want" "$results/gnu-xsh.json" "$work/uutils-flat.json" <<'EOF'
import json, sys
data = json.load(open(sys.argv[1]))
if data.get("uutils_commit") != sys.argv[2]:
    sys.exit(f"cached uutils GNU baseline is for {data.get('uutils_commit')}, not {sys.argv[2]}; rerun `run-gnu.sh uutils`")
def flatten(report, prefix=""):
    if not isinstance(report, dict) or not report:
        sys.exit("missing or empty GNU differential input; rerun both suites")
    result = {}
    for name, value in report.items():
        path = prefix + name
        if isinstance(value, dict):
            result.update(flatten(value, path + "/"))
        elif value in ("PASS", "FAIL", "SKIP", "ERROR", "XFAIL", "XPASS"):
            result[path] = value
        else:
            sys.exit(f"invalid GNU differential result for {path}: {value!r}")
    return result
baseline = flatten(data["results"])
xsh = flatten(json.load(open(sys.argv[3])))
if baseline.keys() != xsh.keys():
    sys.exit("GNU differential test selections differ; rerun both suites with the same TESTS")
json.dump(data["results"], open(sys.argv[4], "w"))
EOF
	python3 "$repo/dev/compat/results.py" gnu "$work/uutils-flat.json" "$results/gnu-xsh.json" "$results/gnu-differential.json"
}

case $mode in
	prepare) prepare ;;
	uutils) run_uutils "$@" ;;
	xsh) run_xsh "$@" ;;
	diff) diff_results ;;
	all) prepare; run_uutils "$@"; run_xsh "$@"; diff_results ;;
	*) echo "unknown mode $mode" >&2; exit 2 ;;
esac
