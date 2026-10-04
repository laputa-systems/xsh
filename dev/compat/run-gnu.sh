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
mode=${1:-all}
shift || true
mkdir -p "$results"

want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["uutils"]["commit"])' "$repo/dev/compat/upstream.lock.json")
[ "$(git -C "$uutils" rev-parse HEAD)" = "$want" ] || { echo "UUTILS_ROOT is not at locked commit $want" >&2; exit 2; }

uu_build=${CARGO_TARGET_DIR:-$uutils/target}/release

prepare() {
	if [ ! -f "$gnu/configure" ]; then
		mkdir -p "$gnu"
		(cd "$gnu" && bash "$uutils/util/fetch-gnu.sh")
	fi
	(cd "$uutils" && PROFILE=release path_GNU="$gnu" bash util/build-gnu.sh)
}

point_path_at() {
	local dir=$1 expr
	expr="s|^[[:blank:]]*PATH=.*|  PATH='${dir}\$(PATH_SEPARATOR)'\"\$\$PATH\" \\\\|"
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
	(cd "$gnu" && LC_ALL=C TZ=UTC timeout -sKILL 4h make check \
		${1+TESTS="$*"} SUBDIRS=. RUN_EXPENSIVE_TESTS=yes RUN_VERY_EXPENSIVE_TESTS=yes \
		VERBOSE=no gl_public_submodule_commit="" srcdir="$gnu") || true
	python3 "$uutils/util/gnu-json-result.py" "$gnu/tests" >"$out"
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
	python3 "$repo/dev/compat/stage.py" --stage "$stage" --gnu-programs "$stage.gnu-programs"
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
