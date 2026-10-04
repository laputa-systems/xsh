#!/bin/sh
# Gate 3: run the pinned uutils `tests/by-util` integration suite against XSH.
#
#   UUTILS_ROOT=../ref/uutils-coreutils dev/compat/run-uutils.sh [UTILITY...]
#
# With no utilities, runs every in-scope utility. Writes the raw JUnit report
# and dev/compat/results/uutils-integration.json.
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
# - Each applet runs under an address-space cap (XSH_COMPAT_MEM_KB, default 3 GiB)
#   and nextest runs UUTESTS_THREADS (default 3) tests at once: the host shares
#   one memory cgroup with builds, and an unbounded applet gets the suite killed.
# - Cross-utility calls (`scene.ccmd("touch")`) also dispatch to XSH. Tests that
#   spawn host programs directly are listed in dev/compat/host-deps.json.
set -eu

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
results=$repo/dev/compat/results
mkdir -p "$results"

python3 "$repo/dev/compat/stage.py" --stage "$stage"

target=${UUTILS_TARGET_DIR:-$uutils/target}
(cd "$uutils" && CARGO_TARGET_DIR=$target cargo nextest --version >/dev/null 2>&1) || {
	echo "cargo-nextest is required: cargo install cargo-nextest --locked" >&2
	exit 2
}

# Build the test binary once; nextest archives would also work but add a step.
(cd "$uutils" && CARGO_TARGET_DIR=$target cargo nextest run --no-run --release \
	--features feat_os_unix --test tests)

uubin=$target/release/coreutils
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

profile_dir=$uutils/.config
mkdir -p "$profile_dir"
cat >"$profile_dir/nextest-xsh.toml" <<EOF
[profile.xsh]
fail-fast = false
retries = 0
slow-timeout = { period = "30s", terminate-after = 4 }
[profile.xsh.junit]
path = "xsh-junit.xml"
store-success-output = false
store-failure-output = true
EOF

# Never convert a report this run did not produce: a killed nextest (the host's
# memory cgroup OOM-kills it) would otherwise leave the previous run's JUnit in
# place and publish its numbers as this run's.
junit=$target/nextest/xsh/xsh-junit.xml
rm -f "$junit"

set +e
(cd "$uutils" && \
	UUTESTS_BINARY_PATH=$stage/xsh-uutests \
	LC_ALL=C TZ=UTC \
	CARGO_TARGET_DIR=$target \
	cargo nextest run --release --features feat_os_unix --test tests \
		--config-file "$profile_dir/nextest-xsh.toml" --profile xsh \
		--no-fail-fast --test-threads "${UUTESTS_THREADS:-3}" -E "$filter")
status=$?
set -e

# nextest exits 0 when every test passed and 100 when some failed; anything else
# (137 after a kill, 101 for a build error) is a harness failure, not a result.
if { [ "$status" -ne 0 ] && [ "$status" -ne 100 ]; } || [ ! -s "$junit" ]; then
	echo "nextest exit status $status without a usable JUnit report; not updating results" >&2
	exit 1
fi
cp "$junit" "$results/uutils-integration.junit.xml"
python3 "$repo/dev/compat/results.py" uutils "$junit" "$results/uutils-integration.json" ${1+"$@"}
echo "nextest exit status $status (failures are expected until parity; see the JSON summary)"
