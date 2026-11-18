#!/bin/sh
set -eu
[ "$#" -eq 1 ] || { echo "usage: $0 FIXTURE_DIR" >&2; exit 2; }
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$script_dir/oracle.sh" --root --mount "$1" --rw -- sh -c '
set -eu
umask 0
gnu_version=$(tar --version | head -n 1)
bsd_version=$(bsdtar --version)
case "$gnu_version" in "tar (GNU tar) 1.35"*) ;; *) echo "unexpected GNU tar: $gnu_version" >&2; exit 1 ;; esac
case "$bsd_version" in "bsdtar 3.8."*) ;; *) echo "unexpected bsdtar: $bsd_version" >&2; exit 1 ;; esac
printf "%s\n%s\n" "$gnu_version" "$bsd_version"
mkdir src gnu bsd
printf data > src/file
mkdir src/dir
mknod src/char c 1 7
mknod src/block b 7 42
mkfifo src/fifo
chown 1234:2345 src/*
chmod 4755 src/file
chmod 2755 src/dir
chmod 6751 src/char
chmod 2640 src/block
chmod 1654 src/fifo
tar --numeric-owner --format=ustar -cf archive.tar -C src file dir char block fifo
tar --numeric-owner -xpf archive.tar -C gnu
bsdtar --numeric-owner -xpf archive.tar -C bsd
'
