#!/usr/bin/env bash
set -euo pipefail

if rg -n \
  'xsh::(source|symbol|syntax|sema|loader|runner|runtime|modules|parse_script_with_module_roots)' \
  crates/xshi/src crates/xsht/src crates/xsht/tests tests src/entrypoints \
  --glob '*.rs'
then
  echo 'deprecated libxsh implementation import found' >&2
  exit 1
fi
