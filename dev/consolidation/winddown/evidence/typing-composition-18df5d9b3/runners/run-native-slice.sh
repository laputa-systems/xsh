#!/usr/bin/env bash
set -Eeuo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-composition
readonly scratch=$root/.work/tmp/typing-composition-debug/native-final
readonly target_dir=$root/.work/target-typing-composition/x86_64-unknown-linux-musl/debug
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly native_lock=$root/.work/consolidation/infra/native-suite.lock
mkdir -p "$scratch/tmp"
exec 9>"$native_lock"
flock -w 1800 -x 9
docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-15 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$target_dir:/target:ro" \
  --volume "$scratch:/scratch:rw" \
  --workdir /work \
  --env PATH=/target:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  --env TMPDIR=/scratch/tmp \
  --env LC_ALL=C --env TZ=UTC \
  "$image" -c 'set -euo pipefail
xsht=/target/xsht
cases=(
  "file:tests/xsh/typing-capability-witnesses.xsh"
  "file:tests/xsh/rank1-inference.xsh"
  "exact:tests/xsh/size-literals.xsh::test_size_literal_is_a_plain_unsigned_integer"
  "exact:tests/xsh/size-literals.xsh::test_size_literal_binding_is_unsigned"
)
: >/scratch/summary.tsv
index=0
for case in "${cases[@]}"; do
  index=$((index + 1))
  test_log=$(printf "%02d" "$index")
  case_kind=${case%%:*}
  test_selector=${case#*:}
  if [ "$case_kind" = file ]; then
    if "$xsht" test "$test_selector" >"/scratch/$test_log.stdout" 2>"/scratch/$test_log.stderr"; then status=0; else status=$?; fi
  else
    if "$xsht" test --exact "$test_selector" >"/scratch/$test_log.stdout" 2>"/scratch/$test_log.stderr"; then status=0; else status=$?; fi
  fi
  printf "%s\t%s\n" "$status" "$case" >>/scratch/summary.tsv
done
cat /scratch/summary.tsv
pass=0
failures=0
while read -r status selector; do
  if [ "$status" = 0 ]; then pass=$((pass + 1)); else failures=$((failures + 1)); fi
done </scratch/summary.tsv
printf "passed=%d failed=%d total=%d\n" "$pass" "$failures" "$((pass + failures))"
[ "$failures" -eq 0 ]
' >"$scratch/docker.stdout" 2>"$scratch/docker.stderr"
