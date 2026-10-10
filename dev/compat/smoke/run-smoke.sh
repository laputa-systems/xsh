#!/bin/sh
# Gate 8: build the clean smoke image from a release xsh and run it.
#
#   dev/compat/smoke/run-smoke.sh [--xsh PATH] [--userns] [--root] [--keep]
#
# The image is FROM scratch and holds only the static interpreter at /bin/xsh,
# the staged applets in /usr/bin (with their lib/ directory), account files and
# smoke.xsh. smoke.xsh runs inside it with PATH=/usr/bin and reports each
# workflow as PASS, FAIL or SKIP; the exit status is the container's, so it is
# non-zero when any workflow failed.
#
#   --xsh PATH   static musl xsh to install (default: $XSH_BIN, then
#                target/x86_64-unknown-linux-musl/release/xsh under the repo)
#   --userns     add CAP_SYS_ADMIN and an unconfined seccomp profile so the
#                namespace workflows (unshare, nsenter, lsns, setpriv) run
#   --root       run as root instead of the unprivileged account (uid 1000)
#   --keep       leave the image in place for inspection
#
# The build context, the staged tree and the interpreter copy live in one
# mktemp directory that is removed on exit, as are the container and (without
# --keep) the image.
set -eu

here=$(cd "$(dirname "$0")" && pwd -P)
repo=$(cd "$here/../../.." && pwd -P)

xsh=${XSH_BIN:-}
user="1000:1000"
caps=""
keep=0
while [ "$#" -gt 0 ]; do
	case "$1" in
		--xsh) xsh=${2:?--xsh needs a path}; shift 2 ;;
		--userns) caps="--cap-add SYS_ADMIN --security-opt seccomp=unconfined"; shift ;;
		--root) user="0:0"; shift ;;
		--keep) keep=1; shift ;;
		-h|--help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		*) echo "run-smoke.sh: unknown option $1" >&2; exit 2 ;;
	esac
done
[ -n "$xsh" ] || xsh=$repo/target/x86_64-unknown-linux-musl/release/xsh
[ -x "$xsh" ] || { echo "run-smoke.sh: $xsh is not executable; build a musl release xsh or pass --xsh" >&2; exit 2; }

# A dynamically linked interpreter cannot start in an image with no libc.
python3 - "$xsh" <<'PY' || { echo "run-smoke.sh: $xsh is not a static x86-64 ELF; use the x86_64-unknown-linux-musl release build" >&2; exit 2; }
import struct, sys
with open(sys.argv[1], "rb") as f:
    head = f.read(64)
    if head[:4] != b"\x7fELF" or head[4] != 2 or struct.unpack_from("<H", head, 18)[0] != 62:
        sys.exit(1)
    phoff, = struct.unpack_from("<Q", head, 32)
    phentsize, phnum = struct.unpack_from("<HH", head, 54)
    f.seek(phoff)
    for _ in range(phnum):
        p_type, = struct.unpack_from("<I", f.read(phentsize))
        if p_type == 3:  # PT_INTERP
            sys.exit(1)
PY

work=$(mktemp -d "${TMPDIR:-/tmp}/xsh-smoke.XXXXXX")
tag="xsh-smoke:$$"
name="xsh-smoke-$$"
cleanup() {
	docker rm -f "$name" >/dev/null 2>&1 || true
	[ "$keep" = 1 ] || docker rmi -f "$tag" >/dev/null 2>&1 || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM

# stage.py checks that the interpreter it is given is executable, and bakes
# that path into every shebang; build_rootfs.py rewrites it to /bin/xsh.
mkdir "$work/ctx"
cp "$xsh" "$work/xsh"
python3 "$repo/dev/compat/stage.py" --stage "$work/stage" --xsh "$work/xsh"
python3 "$here/build_rootfs.py" --stage "$work/stage" --xsh "$work/xsh" \
	--staged-shebang "$work/xsh" --smoke "$here/smoke.xsh" --out "$work/ctx/rootfs.tar"
cp "$here/Dockerfile.smoke" "$work/ctx/Dockerfile.smoke"

docker build --quiet --platform linux/amd64 -f "$work/ctx/Dockerfile.smoke" -t "$tag" "$work/ctx" >/dev/null

echo "== xsh-smoke ($tag): user $user${caps:+, $caps}"
status=0
# shellcheck disable=SC2086
docker run --rm --platform linux/amd64 --name "$name" --user "$user" $caps "$tag" || status=$?
exit "$status"
