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
set -euo pipefail

repo=$(cd "$(dirname "$0")/../.." && pwd)
uutils=$(cd "${UUTILS_ROOT:?set UUTILS_ROOT}" && pwd)
gnu=${GNU_ROOT:-$(dirname "$uutils")/gnu-coreutils}
results=$repo/dev/compat/results
stage=${XSH_COMPAT_STAGE:-$repo/target/compat-stage}
xsh_bin=${XSH_BIN:-${CARGO_TARGET_DIR:-$repo/target}/release/xsh}
mode=${1:-all}
shift || true
mkdir -p "$results"

want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["uutils"]["commit"])' "$repo/dev/compat/upstream.lock.json")
[ "$(git -C "$uutils" rev-parse HEAD)" = "$want" ] || { echo "UUTILS_ROOT is not at locked commit $want" >&2; exit 2; }

uu_build=${UUTILS_TARGET_DIR:-$uutils/target}/release

# GNU configure refuses to run as root, and many GNU tests change behavior (or
# skip) when they do. Preparation bypasses the root check for configure only.
# The test suites run in a user namespace that maps GNU_RUN_UID (default 1000)
# onto the invoking user: tests see an unprivileged uid and permission checks
# refuse as they would for a normal account, while the kernel identity still owns
# the files and devices (sandboxes often leave /dev/null unusable by other
# accounts). Nothing is chowned and no account is created.
run_uid=${GNU_RUN_UID:-1000}
as_user() {
	if [ "$(id -u)" -eq 0 ]; then
		unshare --user --map-user="$run_uid" --map-group="$run_uid" -- "$@"
	else
		"$@"
	fi
}

prepare() {
	# See run-uutils.sh: a missing docs/tldr.zip makes cargo rebuild every time.
	[ -e "$uutils/docs/tldr.zip" ] || : >"$uutils/docs/tldr.zip"
	if [ ! -f "$gnu/configure" ]; then
		mkdir -p "$gnu"
		(cd "$gnu" && bash "$uutils/util/fetch-gnu.sh")
	fi
# This pinned uutils revision places its external libstdbuf under
# target/release/build/uu_stdbuf/*/out instead of target/release/deps. Run a
# temporary copy that links that built library (and tolerates a build without
# the optional artifact); keep the pinned reference checkout itself untouched.
	local build_helper helper_status gnu_tools
	local -a prepare_env
	gnu_tools=${XSH_TOOLS_ROOT:-$(dirname "$repo")/.tools}/gnu-env
	prepare_env=(FORCE_UNSAFE_CONFIGURE=1 PROFILE=release "path_GNU=$gnu")
	if [ -d "$gnu_tools/usr/lib/x86_64-linux-gnu/pkgconfig" ]; then
		prepare_env+=(
			"PKG_CONFIG_SYSROOT_DIR=$gnu_tools"
			"PKG_CONFIG_PATH=$gnu_tools/usr/lib/x86_64-linux-gnu/pkgconfig:$gnu_tools/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
			"CPPFLAGS=${CPPFLAGS:+$CPPFLAGS }-I$gnu_tools/usr/include"
			"LDFLAGS=${LDFLAGS:+$LDFLAGS }-L$gnu_tools/usr/lib/x86_64-linux-gnu -L$gnu_tools/usr/lib64"
		)
	fi
	# A previous preparation removes generated factor-test names from
	# tests/local.mk and leaves an empty continued assignment. If GNU needs to
	# be reconfigured later, normalize only that already-empty list so
	# autoreconf accepts the file; a fresh list with test names is unchanged.
	if [ ! -f "$gnu/gnu-built" ] && [ -f "$gnu/tests/local.mk" ]; then
		python3 - "$gnu/tests/local.mk" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text().splitlines(keepends=True)
for index, line in enumerate(lines):
    if not line.startswith("factor_tests = \\"):
        continue
    end = index + 1
    saw_continuation = False
    while end < len(lines) and lines[end].strip() == "\\":
        saw_continuation = True
        end += 1
    if saw_continuation and end < len(lines) and not lines[end].strip():
        lines[index] = "factor_tests =\n"
        del lines[index + 1:end + 1]
    break
path.write_text("".join(lines))
PY
	fi
	build_helper=$(mktemp "$uutils/util/build-gnu.xsh.XXXXXX")
	if ! cp "$uutils/util/build-gnu.sh" "$build_helper"; then
		unlink "$build_helper"
		return 2
	fi
	if ! python3 - "$build_helper" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
source = path.read_text()
old = '    ln -vf "${UU_BUILD_DIR}"/deps/libstdbuf.* -t "${UU_BUILD_DIR}"\n'
new = '''    stdbuf_lib_found=false
    for lib in "${UU_BUILD_DIR}"/deps/libstdbuf.*; do
        [ -e "$lib" ] || continue
        ln -vf "$lib" -t "${UU_BUILD_DIR}"
        stdbuf_lib_found=true
    done
    if [ "$stdbuf_lib_found" = false ]; then
        for lib in "${UU_BUILD_DIR}"/build/uu_stdbuf/*/out/libstdbuf.so; do
            [ -e "$lib" ] || continue
            ln -vf "$lib" -t "${UU_BUILD_DIR}"
        done
    fi
'''
if source.count(old) != 1:
    raise SystemExit("pinned build-gnu.sh no longer has the expected libstdbuf glob")
path.write_text(source.replace(old, new))
PY
	then
		unlink "$build_helper"
		return 2
	fi
	if (cd "$uutils" && env "${prepare_env[@]}" bash "$build_helper"); then
		unlink "$build_helper"
	else
		helper_status=$?
		unlink "$build_helper"
		return "$helper_status"
	fi
}

point_path_at() {
	local dir=$1 expr
	expr="s|^[[:blank:]]*PATH=.*|  PATH='${dir}\$(PATH_SEPARATOR)${gnu}/src\$(PATH_SEPARATOR)'\"\$\$PATH\" \\\\|"
	for f in Makefile tests/local.mk; do
		[ -f "$gnu/$f" ] && sed -i "$expr" "$gnu/$f"
	done
	touch "$gnu/Makefile.in" "$gnu/Makefile"
}

clear_logs() {
	if [ "$#" -gt 0 ]; then
		for t in "$@"; do rm -f "$gnu/${t%.*}.log" "$gnu/${t%.*}.trs"; done
	else
		find "$gnu/tests" \( -name '*.log' -o -name '*.trs' \) -delete
	fi
}

run_suite() {
	local out=$1
	shift
	clear_logs "$@"
	(cd "$gnu" && as_user env -u TERM LC_ALL=C TZ=UTC timeout -sKILL 4h make -j "${GNU_JOBS:-3}" check \
		${1+TESTS="$*"} SUBDIRS=. RUN_EXPENSIVE_TESTS=yes RUN_VERY_EXPENSIVE_TESTS=yes \
		VERBOSE=no gl_public_submodule_commit="" srcdir="$gnu") || true
	python3 "$uutils/util/gnu-json-result.py" "$gnu/tests" >"$out"
	python3 - "$gnu/tests" "$out" <<'PY'
import json
import sys
from pathlib import Path

test_root = Path(sys.argv[1])
report_path = Path(sys.argv[2])
report = json.loads(report_path.read_text())
for result_path in test_root.rglob("*.trs"):
    status = None
    for line in result_path.read_text(errors="replace").splitlines():
        if line.startswith(":test-result: "):
            status = line.removeprefix(":test-result: ")
    if status not in {"PASS", "FAIL", "SKIP", "ERROR"}:
        continue
    current = report
    for part in result_path.parent.relative_to(test_root).parts:
        current = current.setdefault(part, {})
    current[result_path.stem + ".log"] = status
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY
}

run_uutils() {
	point_path_at "$uu_build"
	run_suite "$results/gnu-uutils.json" "$@"
	python3 - "$results/gnu-uutils.json" "$want" <<'EOF'
import json, sys
path, commit = sys.argv[1], sys.argv[2]
data = json.load(open(path))
json.dump({"uutils_commit": commit, "results": data}, open(path, "w"), indent=2, sort_keys=True)
EOF
}

run_xsh() {
	(cd "$gnu" && ./build-aux/gen-lists-of-programs.sh --list-progs) >"$stage.gnu-programs"
	python3 "$repo/dev/compat/stage.py" --stage "$stage" --xsh "$xsh_bin" --gnu-programs "$stage.gnu-programs"
	point_path_at "$stage/gnu-bin"
	run_suite "$results/gnu-xsh.json" "$@"
	point_path_at "$uu_build"
}

diff_results() {
	python3 - "$results/gnu-uutils.json" "$want" <<'EOF'
import json, sys
data = json.load(open(sys.argv[1]))
if data.get("uutils_commit") != sys.argv[2]:
    sys.exit(f"cached uutils GNU baseline is for {data.get('uutils_commit')}, not {sys.argv[2]}; rerun `run-gnu.sh uutils`")
json.dump(data["results"], open(sys.argv[1] + ".flat", "w"))
EOF
	python3 "$repo/dev/compat/results.py" gnu "$results/gnu-uutils.json.flat" "$results/gnu-xsh.json" "$results/gnu-differential.json"
	rm -f "$results/gnu-uutils.json.flat"
}

case $mode in
	prepare) prepare ;;
	uutils) run_uutils "$@" ;;
	xsh) run_xsh "$@" ;;
	diff) diff_results ;;
	all) prepare; run_uutils "$@"; run_xsh "$@"; diff_results ;;
	*) echo "unknown mode $mode" >&2; exit 2 ;;
esac
