#!/bin/sh
# Gate 4 inside the `Dockerfile.test` image: same interface as run-gnu.sh.
#
#   UUTILS_ROOT=... GNU_ROOT=... XSH_BIN=... COMPAT_RESULTS_DIR=... \
#       dev/compat/run-gnu-docker.sh [prepare|xsh|diff|all] [TEST...]
#
# The container runs as root so the GNU tree can be built and the permission
# tests have a real unprivileged account (UID/GID 1000, the image's `compat`);
# run-gnu.sh drops each test to it with setpriv. After `prepare` the GNU tree is
# chowned to that account, which on the host is the invoking user, so no
# root-owned files remain. Paths are mounted at their host locations.
#
# The uutils baseline in dev/compat/results/gnu-uutils.json is cached per pinned
# commit; only `prepare`, `xsh` and `diff` normally run here. `prepare` fetches
# GNU coreutils (network required) and builds uutils' multicall binary into the
# `xsh-uutils-target` volume. The stage is built inside the container.
set -eu

repo=$(cd "$(dirname "$0")/../.." && pwd)
image=${XSH_TEST_IMAGE:-xsh-test}
uutils=$(cd "${UUTILS_ROOT:?set UUTILS_ROOT}" && pwd)
gnu=${GNU_ROOT:?set GNU_ROOT to the GNU tree directory (created if missing)}
xsh_bin=${XSH_BIN:?set XSH_BIN}
results=${COMPAT_RESULTS_DIR:?set COMPAT_RESULTS_DIR}
mkdir -p "$gnu" "$results"
gnu=$(cd "$gnu" && pwd -P)
results=$(cd "$results" && pwd -P)
bin_dir=$(cd "$(dirname "$xsh_bin")" && pwd -P)
nextest=${CARGO_NEXTEST_BIN:-$HOME/.cargo/bin/cargo-nextest}

inner='
set -u
# uutils build-gnu.sh needs GNU readlink -m and friends; BusyBox lacks them. Only
# `prepare` gets GNU coreutils, so test runs see the image as it is.
case "${1:-all}" in prepare|all)
	apk add -q --no-cache coreutils sed grep findutils gawk diffutils patch >/dev/null || exit 1
	[ -e /usr/bin/false ] || ln -sf /bin/false /usr/bin/false ;;
esac
dev/compat/run-gnu.sh "$@"
status=$?
chown -R "$HOST_UID:$HOST_GID" "$GNU_ROOT" "$COMPAT_RESULTS_DIR" "$UUTILS_ROOT/target" 2>/dev/null
exit $status
'

exec docker run --rm --init --platform linux/amd64 \
	-v "$repo:$repo" \
	-v "$bin_dir:$bin_dir:ro" \
	-v "$uutils:$uutils" \
	-v "$gnu:$gnu" \
	-v "$results:$results" \
	-v xsh-uutils-target:/uutils-target \
	-v xsh-cargo-registry:/root/.cargo/registry \
	-v "$nextest:/usr/local/bin/cargo-nextest:ro" \
	-w "$repo" \
	-e UUTILS_ROOT="$uutils" \
	-e GNU_ROOT="$gnu" \
	-e UUTILS_TARGET_DIR=/uutils-target \
	-e XSH_BIN="$bin_dir/$(basename "$xsh_bin")" \
	-e XSH_COMPAT_STAGE=/stage \
	-e COMPAT_RESULTS_DIR="$results" \
	-e GNU_RUN_UID=1000 -e GNU_RUN_GID=1000 \
	-e GNU_JOBS="${GNU_JOBS:-3}" \
	-e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
	-e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e GIT_CONFIG_VALUE_0='*' \
	-e CARGO_TERM_COLOR=never \
	-e RUSTFLAGS="-C target-feature=-crt-static" \
	-e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=clang-23 \
	"$image" sh -c "$inner" sh "$@"
