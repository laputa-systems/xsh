#!/bin/sh
# Run `xsht` inside the Dockerfile.test image from a checkout or lane worktree.
#
#   XSH_BIN=/path/to/release/xsh dev/compat/docker-xsht.sh [OPTIONS] -- XSHT_ARGS...
#
# The current directory (the checkout under test) and the directory holding
# XSH_BIN are mounted at their host paths, so every path is valid unchanged.
# Tests run as UID/GID 1000 (the image's `compat` account) with HOME and TMPDIR
# private to the container. Use it for tests that need what the host session
# lacks: a non-setgid scratch directory, a second network namespace, ICMP,
# loopback sockets, or capabilities.
#
# Options:
#   --root             run as root instead of UID 1000
#   --userns           SYS_ADMIN capability and an unconfined seccomp profile, so
#                      unshare(2), setns(2) and mount namespaces work
#   --netadmin         NET_ADMIN and NET_RAW capabilities (veth, links, raw ICMP)
#   --no-network       disable the container network entirely
#
# The container is removed on exit; nothing outside the checkout is written.
set -eu

user="1000:1000"
args="--rm --init --platform linux/amd64"
while [ "$#" -gt 0 ]; do
	case "$1" in
		--root) user="0:0"; shift ;;
		--userns) args="$args --cap-add SYS_ADMIN --security-opt seccomp=unconfined"; shift ;;
		--netadmin) args="$args --cap-add NET_ADMIN --cap-add NET_RAW"; shift ;;
		--no-network) args="$args --network none"; shift ;;
		--) shift; break ;;
		*) echo "docker-xsht.sh: unknown option $1" >&2; exit 2 ;;
	esac
done
[ "$#" -gt 0 ] || { echo "docker-xsht.sh: missing xsht arguments" >&2; exit 2; }

xsh_bin=${XSH_BIN:?set XSH_BIN to the release xsh binary}
bin_dir=$(cd "$(dirname "$xsh_bin")" && pwd -P)
here=$(pwd -P)

# shellcheck disable=SC2086
exec docker run $args --user "$user" \
	-v "$here:$here" -v "$bin_dir:$bin_dir:ro" -w "$here" \
	-e HOME=/home/compat -e LC_ALL=C -e TZ=UTC -e TMPDIR=/tmp \
	-e XSH_BIN="$bin_dir/$(basename "$xsh_bin")" \
	-e PATH="$bin_dir:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
	xsh-test "$bin_dir/xsht" "$@"
