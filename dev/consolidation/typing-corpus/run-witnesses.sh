#!/usr/bin/env bash
set -Eeuo pipefail
lane=$(git rev-parse --show-toplevel)
products=/home/josh/d/laputa-systems/xsh/.work/wt/typing-host-obligations/.work/target-typing-host-obligations/x86_64-unknown-linux-musl/verification
scratch=$lane/.work/tmp/corpus
selectors=$lane/dev/consolidation/typing-corpus/witnesses.tsv
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
check_products() {
  printf '%s  %s\n' ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154 "$products/xsh" c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c "$products/xsht" | sha256sum -c -
}
digest_views() {
  python3 "$lane/dev/consolidation/typing-corpus/view_digest.py" "$scratch/original" "$scratch/reduced-final"
}
attempt=$scratch/native-$(date -u +%Y%m%dT%H%M%SZ)-$$
mkdir "$attempt"
state=$attempt/state
mkdir "$state"
check_products >"$state/products.before"
digest_views >"$state/views.before"
cidfile=
prove_absence() {
  inspected_id=$1
  receipt=$2
  if docker inspect "$inspected_id" >"$receipt.stdout" 2>"$receipt.stderr"; then
    printf '0\n' >"$receipt.status"
    return 1
  else
    inspect_status=$?
    printf '%s\n' "$inspect_status" >"$receipt.status"
    [ "$inspect_status" = 1 ] && [ "$(wc -l <"$receipt.stderr")" = 1 ] && grep -Fxq "Error: No such object: $inspected_id" "$receipt.stderr"
  fi
}
cleanup() {
  status=$?
  trap - EXIT INT TERM
  if [ -n "$cidfile" ] && [ -f "$cidfile" ]; then
    container_id=$(cat "$cidfile")
    if ! prove_absence "$container_id" "$state/inspect-cleanup-before"; then
      [ "$(cat "$state/inspect-cleanup-before.status")" = 0 ] || status=1
      docker rm -f "$container_id" >>"$state/cleanup.log" 2>&1 || status=1
    fi
    if prove_absence "$container_id" "$state/inspect-cleanup-after"; then printf 'absent %s\n' "$container_id" >>"$state/cleanup.log"; else status=1; fi
  fi
  digest_views >"$state/views.after" || status=1
  cmp "$state/views.before" "$state/views.after" || status=1
  check_products >"$state/products.after" || status=1
  printf '%s\n' "$status" >"$state/exit.status"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
exec 9>/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/native-suite.lock
flock -n -x 9
for view in original reduced-final; do
  logs=$attempt/$view
  mkdir -p "$logs/tmp"
  cidfile=$state/$view-$$.cid
  container_name=xsh-typing-corpus-$$-$view
  docker run --rm --init --name "$container_name" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
    --volume "$scratch/$view:/views:ro" --volume "$products:/tools:ro" --volume "$logs:/scratch:rw" --volume "$selectors:/selectors.tsv:ro" \
    --env TMPDIR=/scratch/tmp --env PATH=/tools:/usr/local/bin:/usr/bin:/bin --env LC_ALL=C --env TZ=UTC \
    "$image" -c 'set -euo pipefail
    : >/scratch/summary.tsv
    index=0
    while IFS="$(printf "\t")" read -r project selector; do
      index=$((index + 1))
      cd "/views/$project"
      if timeout 120 /tools/xsht test --exact "$selector" -j 1 >"/scratch/$index.stdout" 2>"/scratch/$index.stderr"; then status=0; else status=$?; fi
      accepted=0
      if [ "$status" = 0 ] && [ "$(grep -c "^running 1 tests$" "/scratch/$index.stdout")" = 1 ] && [ "$(grep -c "^test result:" "/scratch/$index.stdout")" = 1 ] && grep -Fxq "test result: ok. 1 passed; 0 failed; 0 skipped" "/scratch/$index.stdout"; then accepted=1; fi
      printf "%s\t%s\t%s\t%s\n" "$status" "$accepted" "$project" "$selector" >>/scratch/summary.tsv
    done </selectors.tsv
    cat /scratch/summary.tsv
    [ "$index" = 13 ]
    [ "$(grep -c "^0$(printf "\t")1$(printf "\t")" /scratch/summary.tsv)" = 13 ]' >"$logs/docker.stdout" 2>"$logs/docker.stderr"
  container_id=$(cat "$cidfile")
  prove_absence "$container_id" "$state/inspect-$view-after"
  printf 'absent %s\n' "$container_id" >>"$state/cleanup.log"
done
printf '26 passed; 0 failed; 0 skipped\n' >"$state/summary.txt"
