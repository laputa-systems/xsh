#!/bin/sh
exec /usr/bin/clang-23 --ld-path=/usr/local/bin/mold -B/usr/lib/xsh-crt/x86_64-unknown-linux-musl "$@"
