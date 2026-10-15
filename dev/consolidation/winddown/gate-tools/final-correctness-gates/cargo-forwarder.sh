#!/usr/bin/env bash
set -euo pipefail

: "${XSH_NESTED_CARGO_TRACE:?nested Cargo trace path is required}"
: "${XSH_NESTED_CARGO_STDOUT:?nested Cargo stdout path is required}"
: "${CARGO_TARGET_DIR:?nested Cargo target directory is required}"
: "${XSH_GATE_RUSTC_WRAPPER:?compiler wrapper path is required}"

{
  printf 'cargo_path=/root/.cargo/bin/cargo\n'
  printf 'argv=%s' "${1-}"
  for arg in "${@:2}"; do
    printf ' %s' "$arg"
  done
  printf '\n'
  printf 'target_dir=%s\n' "$CARGO_TARGET_DIR"
  printf 'cargo_net_offline=true\n'
} > "$XSH_NESTED_CARGO_TRACE"

export CARGO_NET_OFFLINE=true
export RUSTC_WRAPPER="$XSH_GATE_RUSTC_WRAPPER"
set -o pipefail
/root/.cargo/bin/cargo "$@" | tee "$XSH_NESTED_CARGO_STDOUT"
