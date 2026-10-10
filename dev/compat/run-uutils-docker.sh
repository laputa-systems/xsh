#!/bin/sh
# Gate 3 inside the `Dockerfile.test` image: same interface as run-uutils.sh.
#
#   UUTILS_ROOT=... XSH_BIN=... COMPAT_RESULTS_DIR=... TMPDIR=... \
#       dev/compat/run-uutils-docker.sh [UTILITY...]
#
# The container runs as root so tests can use chroot, mknod and ownership
# changes, and run-uutils.sh drops each test to the image's `compat` account
# (UID/GID 1000) with setpriv. Everything the run writes on the host is chowned
# back to the invoking user, so no root-owned files are left behind.
#
# Mounts use the host's own paths, so every path in the environment is valid in
# the container unchanged. The uutils checkout is read-only; its test crate is
# built into the `xsh-uutils-target` volume, which keeps the (minutes-long) first
# build across runs. The stage is built inside the container and discarded.
# cargo-nextest comes from the host (CARGO_NEXTEST_BIN); the image has none.
#
# Environment: UUTILS_ROOT, XSH_BIN (directory also holds xsht), COMPAT_RESULTS_DIR
# and TMPDIR (all required), XSH_TEST_IMAGE (default xsh-test), UUTESTS_THREADS.
set -eu

repo=$(cd "$(dirname "$0")/../.." && pwd)
image=${XSH_TEST_IMAGE:-xsh-test}
uutils=$(cd "${UUTILS_ROOT:?set UUTILS_ROOT to the pinned uutils checkout}" && pwd)
xsh_bin=${XSH_BIN:?set XSH_BIN to the release xsh binary}
results=${COMPAT_RESULTS_DIR:?set COMPAT_RESULTS_DIR}
scratch=${TMPDIR:?set TMPDIR to a directory that is not setgid}
nextest=${CARGO_NEXTEST_BIN:-$HOME/.cargo/bin/cargo-nextest}
[ -x "$nextest" ] || { echo "cargo-nextest not found at $nextest (set CARGO_NEXTEST_BIN)" >&2; exit 2; }
docker image inspect "$image" >/dev/null 2>&1 || {
	echo "image $image is missing; build it with: docker build --platform linux/amd64 -t $image -f Dockerfile.test <empty directory>" >&2
	exit 2
}

mkdir -p "$results" "$scratch"
results=$(cd "$results" && pwd -P)
scratch=$(cd "$scratch" && pwd -P)
bin_dir=$(cd "$(dirname "$xsh_bin")" && pwd -P)

# Runs the same runner the host does; ownership is repaired even when it fails.
inner='
set -u
dev/compat/run-uutils.sh "$@"
status=$?
chown -R "$HOST_UID:$HOST_GID" "$COMPAT_RESULTS_DIR" "$TMPDIR" 2>/dev/null
exit $status
'

exec docker run --rm --init --platform linux/amd64 \
	-v "$repo:$repo" \
	-v "$bin_dir:$bin_dir:ro" \
	-v "$uutils:$uutils:ro" \
	-v "$results:$results" \
	-v "$scratch:$scratch" \
	-v xsh-uutils-target:/uutils-target \
	-v xsh-cargo-registry:/root/.cargo/registry \
	-v "$nextest:/usr/local/bin/cargo-nextest:ro" \
	-w "$repo" \
	-e UUTILS_ROOT="$uutils" \
	-e UUTILS_TARGET_DIR=/uutils-target \
	-e XSH_BIN="$bin_dir/$(basename "$xsh_bin")" \
	-e XSH_COMPAT_STAGE=/stage \
	-e COMPAT_RESULTS_DIR="$results" \
	-e TMPDIR="$scratch" \
	-e UUTESTS_THREADS="${UUTESTS_THREADS:-3}" \
	-e UUTESTS_RUN_UID=1000 -e UUTESTS_RUN_GID=1000 \
	-e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
	-e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e GIT_CONFIG_VALUE_0='*' \
	-e CARGO_TERM_COLOR=never \
	"$image" sh -c "$inner" sh "$@"
