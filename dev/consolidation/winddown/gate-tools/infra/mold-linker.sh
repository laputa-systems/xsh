#!/bin/sh
exec /usr/bin/clang-23 --ld-path=/usr/local/bin/mold "$@"
