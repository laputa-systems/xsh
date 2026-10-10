#!/bin/sh
# Run a reference tool in a throwaway container, as an oracle for a native test.
# The tool is only ever a test reference, never a runtime dependency.
#
#   dev/compat/oracle.sh [OPTIONS] -- COMMAND [ARG...]
#
# The `xsh-oracle` image (Alpine edge, built on first use) carries GNU coreutils,
# util-linux (taskset chrt ionice prlimit setpriv nsenter unshare lsns ...),
# iproute2 (ip ss), iputils (ping tracepath), net-tools (ifconfig route arp),
# traceroute, ethtool, iw, smartmontools, nvme-cli, efibootmgr, cpio, curl, wget,
# netcat-openbsd, eudev (udevadm), procps-ng, psmisc, findutils, e2fsprogs and
# dosfstools. COMMAND runs as UID 1000 in an empty working directory.
#
# Options:
#   --mount DIR        bind DIR at /fixture (read-only) and start there
#   --rw               make the --mount directory writable
#   --network          keep the default bridge network (default: none)
#   --userns           SYS_ADMIN capability and an unconfined seccomp profile,
#                      so unshare(2) and setns(2) work, still inside the container
#   --netadmin         NET_ADMIN and NET_RAW capabilities
#   --root             run as root (default: UID 1000)
#
# The container is removed on exit and shares no host mount except --mount.
set -eu

image=${XSH_ORACLE_IMAGE:-xsh-oracle}
mount_dir=""
mount_mode=ro
network=none
caps=""
user="1000:1000"
while [ "$#" -gt 0 ]; do
	case "$1" in
		--mount) mount_dir=$2; shift 2 ;;
		--rw) mount_mode=rw; shift ;;
		--network) network=bridge; shift ;;
		--userns) caps="$caps --cap-add SYS_ADMIN --security-opt seccomp=unconfined"; shift ;;
		--netadmin) caps="$caps --cap-add NET_ADMIN --cap-add NET_RAW"; shift ;;
		--root) user="0:0"; shift ;;
		--) shift; break ;;
		*) echo "oracle.sh: unknown option $1" >&2; exit 2 ;;
	esac
done
[ "$#" -gt 0 ] || { echo "oracle.sh: missing command after --" >&2; exit 2; }

if ! docker image inspect "$image" >/dev/null 2>&1; then
	context=$(mktemp -d)
	cat >"$context/Dockerfile" <<'EOF'
FROM alpine:edge
RUN apk add --no-cache coreutils util-linux util-linux-misc util-linux-login iproute2 \
    iproute2-ss iputils iputils-ping iputils-tracepath net-tools traceroute ethtool iw \
    smartmontools nvme-cli efibootmgr cpio curl wget netcat-openbsd eudev udev procps-ng \
    psmisc findutils e2fsprogs e2fsprogs-extra dosfstools gzip bzip2 xz zstd lzip sysstat
EOF
	docker build --platform linux/amd64 -t "$image" "$context" >&2
	rm -rf "${context:?}"
fi

args="--rm --init --platform linux/amd64 --network $network --user $user $caps"
if [ -n "$mount_dir" ]; then
	mount_dir=$(cd "$mount_dir" && pwd -P)
	args="$args -v $mount_dir:/fixture:$mount_mode -w /fixture"
else
	args="$args -w /tmp"
fi

# shellcheck disable=SC2086
exec docker run $args "$image" "$@"
