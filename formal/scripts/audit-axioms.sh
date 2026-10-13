#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

usage() {
  cat >&2 <<'EOF'
usage: scripts/audit-axioms.sh
       scripts/audit-axioms.sh --self-test

Builds and audits every Lean module under formal/ except audit scripts.
Only propext, Quot.sound, and Classical.choice are permitted dependencies.
EOF
}

load_project_modules() {
  local source module source_list
  source_list=$(rg --files -g '*.lean' -g '!scripts/**' | LC_ALL=C sort)
  [[ -n "$source_list" ]] || {
    printf 'no formal Lean modules found\n' >&2
    return 2
  }

  project_modules=()
  while IFS= read -r source; do
    module=${source%.lean}
    module=${module//\//.}
    if [[ ! "$module" =~ ^[A-Za-z_][A-Za-z0-9_.]*$ ]]; then
      printf 'unsupported Lean module path: %s\n' "$source" >&2
      return 2
    fi
    project_modules+=("$module")
  done <<<"$source_list"

  if [[ ! " ${project_modules[*]} " == *" Xsh "* ]]; then
    printf 'formal root module Xsh.lean is missing\n' >&2
    return 2
  fi
}

build_project_modules() {
  TMPDIR=../.work/tmp lake build "${project_modules[@]}"
}

audit_source() {
  local prefix=$1
  {
    printf 'import Lean\n'
    for module in "${project_modules[@]}"; do
      printf 'import %s\n' "$module"
    done
    printf 'def xshAuditProjectModules : Array Lean.Name := #[\n'
    for module in "${project_modules[@]}"; do
      printf '  `%s,\n' "$module"
    done
    printf ']\n'
    if [[ -n "$prefix" ]]; then
      printf '%s\n' "$prefix"
    fi
    cat scripts/AuditAxioms.lean
  } | TMPDIR=../.work/tmp lake env lean -j 4 -M 8192 --stdin
}

expect_rejected() {
  local label=$1
  local expected=$2
  local prefix=$3
  local output status=0

  output=$(audit_source "$prefix" 2>&1) || status=$?
  if (( status == 0 )); then
    printf 'negative control unexpectedly passed: %s\n' "$label" >&2
    return 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    printf 'negative control failed for another reason: %s\n%s\n' "$label" "$output" >&2
    return 1
  fi
  printf 'rejected %s negative control\n' "$label"
}

self_test() {
  local custom_control native_control sorry_control
  custom_control=$'namespace OutsideXsh\naxiom forbidden : True\ntheorem customRoot : True := forbidden\nend OutsideXsh'
  native_control=$'namespace OutsideXsh\ntheorem nativeRoot : 7 = 7 := by native_decide\nend OutsideXsh'
  sorry_control=$'namespace OutsideXsh\ntheorem sorryRoot : True := by sorry\nend OutsideXsh'

  expect_rejected custom 'project axiom declaration rejected: OutsideXsh.forbidden' "$custom_control"
  expect_rejected native 'project axiom declaration rejected: OutsideXsh.nativeRoot._native.native_decide.' "$native_control"
  expect_rejected sorry 'sorryRoot depends on forbidden axioms' "$sorry_control"
}

load_project_modules
build_project_modules

if [[ ${1:-} == --self-test ]]; then
  [[ $# -eq 1 ]] || { usage; exit 2; }
  self_test
  exit
fi

if [[ $# -ne 0 ]]; then
  usage
  exit 2
fi

audit_source ''
