#!/bin/sh
# Gate 3: run the pinned uutils `tests/by-util` integration suite against XSH.
#
#   UUTILS_ROOT=../ref/uutils-coreutils dev/compat/run-uutils.sh [UTILITY...]
#
# With no utilities, runs every in-scope utility. Writes the raw JUnit report
# and uutils-integration.json under COMPAT_RESULTS_DIR.
#
# Environment: UUTILS_ROOT (required), XSH_BIN (interpreter for the stage; default
# target/release/xsh of this checkout), XSH_COMPAT_STAGE, COMPAT_RESULTS_DIR,
# UUTESTS_THREADS (default 3), UUTILS_SUITE_LOCK, UUTESTS_RUN_UID/GID (root runs).
#
# Build notes:
# - The uutils test crate needs `env!("CARGO_BIN_EXE_coreutils")` and gates each
#   test module on its utility feature, so the minimum build is
#   `--features feat_os_unix`. That compiles the uutils implementations, but
#   UUTESTS_BINARY_PATH routes every invocation to XSH instead.
# - Tripwire: after the build the uutils `coreutils` binary is made
#   non-executable, so any test that bypasses UUTESTS_BINARY_PATH fails loudly
#   instead of passing on uutils' behavior. It is restored on exit.
# - Only the `tests` target runs; `test_util_name` and `test_uudoc` exercise the
#   uutils binary itself and do not apply.
# - The framework clears the child environment, so the adapter is installed
#   inside the stage and locates the stage from its own path.
# - Each test process (nextest wrapper) and each applet (adapter) runs under an
#   address-space cap, because a buggy applet can emit gigabytes that the test
#   process then buffers (XSH `date +%99999999999c` wrote 2 GiB). Applet cap (XSH_COMPAT_MEM_KB, default 3 GiB)
#   and nextest runs UUTESTS_THREADS (default 3) tests at once: the host shares
#   one memory cgroup with builds, and an unbounded applet gets the suite killed.
# - Cross-utility calls (`scene.ccmd("touch")`) also dispatch to XSH. Tests that
#   spawn host programs directly are listed in dev/compat/host-deps.json.
set -eu

# The uutils test crate embeds the build-time PATH (`env!("PATH")`), so a different
# PATH between invocations makes cargo recompile the whole test crate (five
# minutes). Pin it: rustup shims first, then the standard system directories.
PATH="${CARGO_HOME:-$HOME/.cargo}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
export PATH

# Concurrent invocations share the uutils checkout and its nextest config, so
# they serialize on one lock even when lanes use separate build directories.
lock=${UUTILS_SUITE_LOCK:-/tmp/uutils-suite.lock}
exec 9>"$lock"
flock 9
# Every child below runs with 9>&-: a test that leaves a background process behind
# (the sleep tests leave `sleep 999d`) would otherwise inherit this descriptor and
# hold the suite lock until it is killed.

repo=$(cd "$(dirname "$0")/../.." && pwd)
uutils=${UUTILS_ROOT:?set UUTILS_ROOT to the pinned uutils checkout}
uutils=$(cd "$uutils" && pwd)
want=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["uutils"]["commit"])' "$repo/dev/compat/upstream.lock.json")
have=$(git -C "$uutils" rev-parse HEAD)
if [ "$want" != "$have" ]; then
	echo "UUTILS_ROOT is at $have; dev/compat/upstream.lock.json pins $want" >&2
	exit 2
fi

# uutils' build.rs declares docs/tldr.zip (a gitignored docs input for uudoc) as a
# rerun trigger. While the file is missing cargo treats the test crate as dirty
# on every invocation, which costs five minutes per run. An empty placeholder
# keeps the fingerprint stable and changes no test behavior.
[ -e "$uutils/docs/tldr.zip" ] || : >"$uutils/docs/tldr.zip"

stage=${XSH_COMPAT_STAGE:-$repo/target/compat-stage}
# Lanes point COMPAT_RESULTS_DIR at scratch space: results/ is integrator-owned.
results=${COMPAT_RESULTS_DIR:-$repo/dev/compat/results}
mkdir -p "$results"
results=$(cd "$results" && pwd -P)
junit=$results/uutils-nextest.xml

python3 "$repo/dev/compat/stage.py" --stage "$stage"

target=${UUTILS_TARGET_DIR:-$uutils/target}
export CARGO_TARGET_DIR=$target
(cd "$uutils" && cargo nextest --version >/dev/null 2>&1 9>&-) || {
	echo "cargo-nextest is required: cargo install --debug cargo-nextest --locked" >&2
	exit 2
}

# The xsh-test image targets musl, where the uutils stdbuf fixture needs a
# dynamically linked cdylib. Build this test harness through the C driver with
# crt-static disabled, without changing the static flags used for XSH itself.
host_target=$(rustc -vV | sed -n 's/^host: //p')
case "$host_target" in
	*-musl)
		target_key=$(printf '%s' "$host_target" | tr '[:lower:]-' '[:upper:]_')
		linker_env="CARGO_TARGET_${target_key}_LINKER"
		ref_rustflags=${RUSTFLAGS:-}
		RUSTFLAGS="${ref_rustflags:+$ref_rustflags }-C target-feature=-crt-static"
		export RUSTFLAGS
		cargo_nextest() {
			env "$linker_env=cc" cargo nextest "$@"
		}
		;;
	*)
		cargo_nextest() {
			cargo nextest "$@"
		}
		;;
esac

# Build the test binary once; nextest archives would also work but add a step.
(cd "$uutils" && cargo_nextest run --target-dir "$target" --no-run \
	--features feat_os_unix --test tests 9>&-)

uubin=$target/debug/coreutils
restore() { [ -e "$uubin" ] && chmod 755 "$uubin"; }
trap restore EXIT INT TERM
chmod 000 "$uubin"

filter=""
if [ "$#" -gt 0 ]; then
	for util in "$@"; do
		filter="${filter:+$filter | }test(/^test_${util}::/)"
	done
else
	filter="all()"
fi

# Reference tests assume an unprivileged caller, including permission-denied
# fixtures and attempts to install over protected device nodes.
test_runner=""
if [ "$(id -u)" -eq 0 ]; then
    run_uid=${UUTESTS_RUN_UID:-1000}
    run_gid=${UUTESTS_RUN_GID:-$run_uid}
    for value in "$run_uid" "$run_gid"; do
        case "$value" in
            *[!0-9]* | "") echo "test uid and gid must be numeric" >&2; exit 2 ;;
        esac
    done
    [ "$run_uid" -ne 0 ] && [ "$run_gid" -ne 0 ] || { echo "reference tests require nonzero uid and gid" >&2; exit 2; }
    command -v setpriv >/dev/null || { echo "root runs require setpriv" >&2; exit 2; }
    getent passwd "$run_uid" >/dev/null || { echo "create an unprivileged test account for uid $run_uid" >&2; exit 2; }
    test_runner="setpriv --reuid=$run_uid --regid=$run_gid --clear-groups --"
fi

profile_dir=$uutils/.config
mkdir -p "$profile_dir"
cat >"$profile_dir/nextest-xsh.toml" <<'EOF'
experimental = ["wrapper-scripts"]

[scripts.wrapper.xsh-memcap]
command = ["sh", "-c", "ulimit -v @TEST_MEM_KB@; exec @TEST_RUNNER@ \"$@\"", "sh"]

[[profile.xsh.scripts]]
filter = "all()"
run-wrapper = "xsh-memcap"

[profile.xsh]
fail-fast = false
retries = 0
slow-timeout = { period = "30s", terminate-after = 4 }
[profile.xsh.junit]
path = @JUNIT_PATH@
store-success-output = false
store-failure-output = true
EOF
sed -i "s/@TEST_MEM_KB@/${UUTESTS_TEST_MEM_KB:-4194304}/" "$profile_dir/nextest-xsh.toml"
sed -i "s|@TEST_RUNNER@|$test_runner|" "$profile_dir/nextest-xsh.toml"
python3 - "$profile_dir/nextest-xsh.toml" "$junit" <<'PY'
import json
from pathlib import Path
import sys

config = Path(sys.argv[1])
contents = config.read_text()
contents = contents.replace("@JUNIT_PATH@", json.dumps(sys.argv[2], ensure_ascii=False))
config.write_text(contents)
PY

# Never convert a report this run did not produce: a killed nextest (the host's
# memory cgroup OOM-kills it) would otherwise leave the previous run's JUnit in
# place and publish its numbers as this run's.
rm -f "$junit"

set +e
(cd "$uutils" && \
	UUTESTS_BINARY_PATH=$stage/xsh-uutests \
	LC_ALL=C TZ=UTC \
	cargo_nextest run --target-dir "$target" --features feat_os_unix --test tests \
		--config-file "$profile_dir/nextest-xsh.toml" --profile xsh \
		--no-fail-fast --test-threads "${UUTESTS_THREADS:-3}" -E "$filter" 9>&-)
status=$?
set -e

# nextest exits 0 when every test passed and 100 when some failed; anything else
# (137 after a kill, 101 for a build error) is a harness failure, not a result.
if { [ "$status" -ne 0 ] && [ "$status" -ne 100 ]; } || [ ! -s "$junit" ]; then
	echo "nextest exit status $status without a usable JUnit report; not updating results" >&2
	exit 1
fi
cp "$junit" "$results/uutils-integration.junit.xml"
XSH_COMPAT_STAGE=$stage python3 "$repo/dev/compat/results.py" uutils "$junit" "$results/uutils-integration.json" ${1+"$@"}
echo "nextest exit status $status (failures are expected until parity; see the JSON summary)"
