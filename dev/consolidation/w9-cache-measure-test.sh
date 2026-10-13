#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_root=$(cd -- "$script_dir/../.." && pwd -P)
mkdir -p -- "$repo_root/.work/tmp"
test_root=$(mktemp -d "$repo_root/.work/tmp/w9-cache-measure-test.XXXXXX")
trap 'rm -rf -- "$test_root"' EXIT

make_fixture() {
  local name=$1 fixture xsh laputa
  fixture="$test_root/$name"
  xsh="$fixture/xsh"
  laputa="$fixture/laputa"
  mkdir -p -- "$xsh/dev/consolidation" "$xsh/.work" "$laputa/.git/info"
  mkdir -p -- "$xsh/.git/info"
  cp -- "$script_dir/w9-cache-measure.sh" "$xsh/dev/consolidation/"
  cp -- "$script_dir/w9-cache-trace-validate.awk" "$xsh/dev/consolidation/"
  cat > "$xsh/dev/consolidation/w9-cache-build.sh" <<'BUILD_STUB'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" == 3 && "$1" == verify ]]
BUILD_STUB
  chmod +x -- "$xsh/dev/consolidation/w9-cache-build.sh"
  cat > "$xsh/.work/xsht" <<'XSHT_STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'synthetic xsht output\n'
if [[ -n "${XSH_W9_CACHE_TRACE:-}" ]]; then
  printf 'W9CACHE\t2\t%s\t0\t1\tshared\tcache\t-\t-\t-\t-\t-\t-\n' "$$" > "$XSH_W9_CACHE_TRACE"
  printf 'W9CACHE\t2\t%s\t1\t1\tshared\trequested\t"<entry>"\tSourceId(1)\t0\t1\tEntry\tStr\n' "$$" >> "$XSH_W9_CACHE_TRACE"
  printf 'W9CACHE\t2\t%s\t2\t1\tshared\tprepared\t"<entry>"\tSourceId(1)\t0\t1\tEntry\tStr\n' "$$" >> "$XSH_W9_CACHE_TRACE"
  printf 'W9CACHE\t2\t%s\t3\t1\tshared\tchecked\t"<entry>"\tSourceId(1)\t0\t1\tEntry\tStr\n' "$$" >> "$XSH_W9_CACHE_TRACE"
fi

case "${SYNTHETIC_MUTATION:-}" in
  xsh-dirty|xsh-tree) target=$SYNTHETIC_XSH_ROOT ;;
  laputa-dirty|laputa-tree) target=$SYNTHETIC_LAPUTA_ROOT ;;
  *) target= ;;
esac
if [[ -n "$target" && "$PWD" == "$target" && ! -e "$SYNTHETIC_MUTATION_MARKER" ]]; then
  : > "$SYNTHETIC_MUTATION_MARKER"
  printf 'concurrent change\n' >> "$target/source.xsh"
  if [[ "$SYNTHETIC_MUTATION" == *-tree ]]; then
    git -C "$target" add source.xsh
    git -C "$target" -c user.name=synthetic -c user.email=synthetic@example.invalid commit -qm 'concurrent source change'
  fi
fi
XSHT_STUB
  chmod +x -- "$xsh/.work/xsht"
  printf 'synthetic build receipt\n' > "$xsh/.work/build.receipt"
  printf 'source\n' > "$xsh/source.xsh"
  printf '.work/\n' > "$xsh/.git/info/exclude"
  printf 'source\n' > "$laputa/source.xsh"
  printf '.work/\n' > "$laputa/.git/info/exclude"
  for root in "$xsh" "$laputa"; do
    git -C "$root" init --quiet
    git -C "$root" config user.name synthetic
    git -C "$root" config user.email synthetic@example.invalid
    git -C "$root" add .
    git -C "$root" commit --quiet -m 'initial synthetic source'
  done
}

run_measurement() {
  local name=$1 mutation=$2 fixture xsh laputa output log
  fixture="$test_root/$name"
  xsh="$fixture/xsh"
  laputa="$fixture/laputa"
  output="$xsh/.work/measurement"
  log="$fixture/measurement.log"
  if env \
      SYNTHETIC_MUTATION="$mutation" \
      SYNTHETIC_MUTATION_MARKER="$fixture/mutated" \
      SYNTHETIC_XSH_ROOT="$xsh" \
      SYNTHETIC_LAPUTA_ROOT="$laputa" \
      bash "$xsh/dev/consolidation/w9-cache-measure.sh" \
        "$xsh/.work/xsht" "$xsh/.work/build.receipt" "$laputa" "$output" > "$log" 2>&1; then
    return 0
  fi
  return 1
}

make_fixture baseline
baseline_xsh="$test_root/baseline/xsh"
baseline_laputa="$test_root/baseline/laputa"
baseline_output="$baseline_xsh/.work/measurement"
xsh_commit=$(git -C "$baseline_xsh" rev-parse HEAD)
xsh_tree=$(git -C "$baseline_xsh" rev-parse 'HEAD^{tree}')
laputa_commit=$(git -C "$baseline_laputa" rev-parse HEAD)
laputa_tree=$(git -C "$baseline_laputa" rev-parse 'HEAD^{tree}')
run_measurement baseline '' || { cat "$test_root/baseline/measurement.log" >&2; exit 1; }
rg -F -q "xsh_commit=$xsh_commit" "$baseline_output/run.txt"
rg -F -q "xsh_tree=$xsh_tree" "$baseline_output/run.txt"
rg -F -q "laputa_commit=$laputa_commit" "$baseline_output/run.txt"
rg -F -q "laputa_tree=$laputa_tree" "$baseline_output/run.txt"

for mutation in xsh-dirty xsh-tree laputa-dirty laputa-tree; do
  make_fixture "$mutation"
  if run_measurement "$mutation" "$mutation"; then
    printf 'expected rejection of concurrent source drift: %s\n' "$mutation" >&2
    exit 1
  fi
  rg -F -q 'source changed during measurement:' "$test_root/$mutation/measurement.log"
  rg -F -q "${mutation%%-*} source changed during measurement:" "$test_root/$mutation/measurement.log"
done

printf 'W9 cache measurement source-freeze checks passed\n'
