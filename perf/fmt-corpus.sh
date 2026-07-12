#!/usr/bin/env bash
set -euo pipefail

root=$PWD
xsht=target/release/xsht
runs=10
warmup=3
operation=fmt
no_build=0
alloc=0

usage() {
  cat <<'EOF'
Usage: perf/fmt-corpus.sh [OPTIONS]

Benchmark read-only xsht operations over the repository XSH corpus.

Options:
  --root DIR       Repository root (default: current directory)
  --xsht PATH      xsht binary (default: target/release/xsht)
  --operation OP   fmt, check, lint, or all (default: fmt)
  --runs N         Hyperfine measured runs (default: 10)
  --warmup N       Hyperfine warmup runs (default: 3)
  --alloc          Build with perf-metrics and report counters for each operation
  --no-build       Use the existing xsht binary
  --help           Show this help
EOF
}

while (($# > 0)); do
  case $1 in
    --root)
      root=$2
      shift 2
      ;;
    --xsht)
      xsht=$2
      shift 2
      ;;
    --operation)
      operation=$2
      shift 2
      ;;
    --runs)
      runs=$2
      shift 2
      ;;
    --warmup)
      warmup=$2
      shift 2
      ;;
    --alloc)
      alloc=1
      shift
      ;;
    --no-build)
      no_build=1
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

root=$(cd "$root" && pwd)
if [[ $xsht != /* ]]; then
  xsht=$root/$xsht
fi

if ! command -v rg >/dev/null 2>&1; then
  printf 'rg is required to discover the XSH corpus\n' >&2
  exit 2
fi
if ! command -v hyperfine >/dev/null 2>&1; then
  printf 'hyperfine is required for timing; install it or use the existing perf workflow\n' >&2
  exit 2
fi

if ((no_build == 0)); then
  (cd "$root" && cargo build --release --bin xsht)
fi
if [[ ! -x $xsht ]]; then
  printf 'xsht binary not found or not executable: %s\n' "$xsht" >&2
  exit 2
fi

operations=()
case $operation in
  fmt|check|lint)
    operations=("$operation")
    ;;
  all)
    operations=(fmt check lint)
    ;;
  *)
    printf 'unknown operation: %s\n' "$operation" >&2
    exit 2
    ;;
esac

files=()
while IFS= read -r file; do
  files+=("$root/$file")
done < <(cd "$root" && rg --files core examples showcase tests/xsh tools -g '*.xsh' | sort)

if ((${#files[@]} == 0)); then
  printf 'no XSH files found under the corpus roots\n' >&2
  exit 2
fi

bytes=$(wc -c "${files[@]}" | awk 'END {print $1}')
stamp=$(date +%Y%m%d-%H%M%S)
results=$root/target/perf/xsht-corpus-$stamp
mkdir -p "$results"

run_timing() {
  local op=$1
  local command
  if [[ $op == fmt ]]; then
    command=$(printf '%q ' "$xsht" fmt --check "${files[@]}")
  else
    command=$(printf '%q ' "$xsht" "$op" "${files[@]}")
  fi
  hyperfine \
    --warmup "$warmup" \
    --runs "$runs" \
    --export-json "$results/$op-timing.json" \
    --command-name "xsht $op (${#files[@]} files)" \
    "$command" | tee "$results/$op-timing.txt"
}

run_allocation() {
  local op=$1
  if [[ $op == fmt ]]; then
    XSH_PERF_ALLOC=1 "$xsht" fmt --check "${files[@]}" \
      >"$results/$op-allocation.stdout" \
      2>"$results/$op-allocation.stderr"
  else
    XSH_PERF_ALLOC=1 "$xsht" "$op" "${files[@]}" \
      >"$results/$op-allocation.stdout" \
      2>"$results/$op-allocation.stderr"
  fi
  cat "$results/$op-allocation.stderr"
}

for op in "${operations[@]}"; do
  printf 'operation=%s\n' "$op"
  run_timing "$op"
done

printf 'corpus files=%s bytes=%s results=%s\n' "${#files[@]}" "$bytes" "$results"

if ((alloc == 1)); then
  (cd "$root" && cargo build --release --bin xsht --features perf-metrics)
  for op in "${operations[@]}"; do
    printf 'allocation_operation=%s\n' "$op"
    run_allocation "$op"
  done
fi
