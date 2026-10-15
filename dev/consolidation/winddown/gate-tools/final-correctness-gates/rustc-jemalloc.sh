#!/usr/bin/env bash
set -euo pipefail

exec env LD_PRELOAD=/usr/lib/libjemalloc.so.2 /usr/local/bin/kache "$@"
