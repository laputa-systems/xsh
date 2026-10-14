#!/bin/bash
arguments=()
for argument in "$@"; do
  case "$argument" in
    --defsym=*) arguments+=("-Wl,$argument") ;;
    *) arguments+=("$argument") ;;
  esac
done
exec clang-23 --ld-path=/usr/local/bin/mold --target="$TARGET" -B"/usr/lib/xsh-crt/$TARGET" "${arguments[@]}"
