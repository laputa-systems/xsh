#!/usr/bin/env bash
set -Eeuo pipefail
lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-corpus-b235
native_lock=/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/native-suite.lock
products=/home/josh/d/laputa-systems/xsh/.work/target-typing-set-seed/x86_64-unknown-linux-musl/verification
source_lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-set-proof
scratch=$lane/.work/tmp/corpus/mutable-set-plan
original=$lane/.work/tmp/corpus/original
reduced=$scratch/candidate
selectors=$lane/dev/consolidation/typing-corpus/witnesses.tsv
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
corpus_guard() {
  [ "$(git -C "$lane" rev-parse HEAD)" = 03b0aad7394f351408a72c2360a23e9fbf936d18 ]
  [ "$(git -C "$lane" rev-parse 'HEAD^{tree}')" = 51787d9420c922484a8a779a57d3f2357aea82e3 ]
  [ -z "$(git -C "$lane" status --porcelain)" ]
  git -C "$lane" rev-parse HEAD 'HEAD^{tree}'
  printf '%s  %s\n' a6e8c5aa651584592a800edc7d9b1983e11e71bf0c15b686c137a3ea8fcd0102 "$selectors" 0212c5bb08d7faa5948fdeb6f26cc1329440bf4532d3473d896ab0be6b8a0154 "$lane/dev/consolidation/typing-corpus/view_digest.py" | sha256sum -c -
}
lock_guard() {
  [ -f "$native_lock" ] && [ ! -L "$native_lock" ]
  [ "$(stat -c '%i' "$native_lock")" = 62012470 ]
  stat -c '%d:%i %a' "$native_lock"
}
source_guard() {
  [ "$(git -C "$source_lane" rev-parse HEAD)" = 020f9c0c799a0e17c2b0006ced779e416a3a8052 ]
  [ "$(git -C "$source_lane" rev-parse 'HEAD^{tree}')" = 1f67a4262727c8779e3135bf0fa49132ef9f7c12 ]
  [ -z "$(git -C "$source_lane" status --porcelain)" ]
  git -C "$source_lane" rev-parse HEAD 'HEAD^{tree}'
}
check_products() {
  printf '%s  %s\n' 171025d26de98b7eafed618e0ddbf0e8bbfc0792030fa6c4c09c8a16fa9e772a "$products/xsh" 4060fbfee92d74c9492465a1c95dbe6a70a0cde1fd25d4f8dfc21a7eb097623f "$products/xsht" 5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d "$products/xsh-test-helper" | sha256sum -c -
}
digest_views() {
  python3 "$lane/dev/consolidation/typing-corpus/view_digest.py" "$original" "$reduced"
}
attempt=$scratch/native-$(date -u +%Y%m%dT%H%M%SZ)-$$
mkdir "$attempt"
state=$attempt/state
mkdir "$state"
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
    [ "$inspect_status" = 1 ] && [ "$(wc -l <"$receipt.stderr")" = 1 ] && { grep -Fxq "Error: No such object: $inspected_id" "$receipt.stderr" || grep -Fxq "error: no such object: $inspected_id" "$receipt.stderr"; }
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
  corpus_guard >"$state/corpus.after" || status=1
  lock_guard >"$state/lock.after" || status=1
  digest_views >"$state/views.after" || status=1
  cmp "$state/views.before" "$state/views.after" || status=1
  source_guard >"$state/source.after" || status=1
  check_products >"$state/products.after" || status=1
  printf '%s\n' "$status" >"$state/exit.status"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
corpus_guard >"$state/corpus.before"
lock_guard >"$state/lock.before"
source_guard >"$state/source.before"
check_products >"$state/products.before"
digest_views >"$state/views.before"
printf 'original\t14287\te3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899\ncandidate\t14287\t43ad0beddeb95ef7f695598664156df29605831df3156433a11ea356d39d8c35\n' | cmp - "$state/views.before"
exec 9<"$native_lock"
[ "$(stat -Lc '%i' /proc/$$/fd/9)" = 62012470 ]
flock -n -x 9
for view in original reduced; do
  if [ "$view" = original ]; then view_path=$original; else view_path=$reduced; fi
  logs=$attempt/$view
  mkdir -p "$logs/tmp"
  cidfile=$state/$view-$$.cid
  container_name=xsh-typing-set-corpus-$$-$view
  docker run --rm --init --name "$container_name" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpus=4 --cpuset-cpus=12,13 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
    --volume "$view_path:/views:ro" --volume "$products:/tools:ro" --volume "$logs:/scratch:rw" --volume "$selectors:/selectors.tsv:ro" \
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
