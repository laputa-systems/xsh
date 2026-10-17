#!/usr/bin/env bash
set -Eeuo pipefail
lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-corpus-b235
source_lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-collection-contributions
products=/home/josh/d/laputa-systems/xsh/.work/target-typing-collection-contributions/x86_64-unknown-linux-musl/verification
original=$lane/.work/tmp/corpus/original
reduced=$lane/.work/tmp/corpus/collection-plan/candidate
attempt=$lane/.work/tmp/corpus/collection-76caa-check-$(date -u +%Y%m%dT%H%M%SZ)-$$
mkdir "$attempt"
mkdir "$attempt/state"
state=$attempt/state
cidfile=
source_guard() {
  [ "$(git -C "$lane" rev-parse HEAD)" = 03b0aad7394f351408a72c2360a23e9fbf936d18 ]
  [ "$(git -C "$lane" rev-parse 'HEAD^{tree}')" = 51787d9420c922484a8a779a57d3f2357aea82e3 ]
  [ -z "$(git -C "$lane" status --porcelain)" ]
  [ "$(git -C "$source_lane" rev-parse HEAD)" = 76caa6920b43861df386d4f208a3a1bdff2f9974 ]
  [ "$(git -C "$source_lane" rev-parse 'HEAD^{tree}')" = 8d032d81485eb0dd1d01603d38aa7827ae1ebf50 ]
  [ -z "$(git -C "$source_lane" status --porcelain)" ]
  git -C "$source_lane" rev-parse HEAD 'HEAD^{tree}'
}
product_guard() {
  printf '%s  %s\n' b8b6f9530dc1c5a97ac9e1469148cf1d4a6b23f8715e0a844b1a0a2791ba5179 "$products/xsh" 7d1344167e20651887dbe093f34724939ff56f2fb6babab0265c3ac2bf9ede05 "$products/xsht" 5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d "$products/xsh-test-helper" | sha256sum -c -
}
view_guard() {
  python3 "$lane/dev/consolidation/typing-corpus/view_digest.py" "$original" "$reduced"
}
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
  result=$?
  trap - EXIT INT TERM
  if [ -n "$cidfile" ] && [ -f "$cidfile" ]; then
    container_id=$(cat "$cidfile")
    if ! prove_absence "$container_id" "$state/inspect-cleanup-before"; then
      [ "$(cat "$state/inspect-cleanup-before.status")" = 0 ] || result=1
      docker rm -f "$container_id" >>"$state/cleanup.log" 2>&1 || result=1
    fi
    if prove_absence "$container_id" "$state/inspect-cleanup-after"; then printf 'absent %s\n' "$container_id" >>"$state/cleanup.log"; else result=1; fi
  fi
  source_guard >"$state/source.after" || result=1
  product_guard >"$state/products.after" || result=1
  view_guard >"$state/views.after" || result=1
  cmp "$state/views.before" "$state/views.after" || result=1
  printf '%s\n' "$result" >"$state/exit.status"
  printf 'attempt %s exit %s\n' "$attempt" "$result"
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
source_guard >"$state/source.before"
product_guard >"$state/products.before"
view_guard >"$state/views.before"
printf 'original\t14287\te3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899\ncandidate\t14287\tb3c7baba21066818d06b42b7e7e6ab7c7745fb5b7ae0e878ee0dbe4de7743b17\n' | cmp - "$state/views.before"
result=0
for label in original reduced; do
  if [ "$label" = original ]; then view=$original; else view=$reduced; fi
  logs=$attempt/$label
  mkdir -p "$logs/tmp"
  cidfile=$state/$label.cid
  docker run --rm --init --name "xsh-typing-collection-check-$$-$label" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpus=2 --cpuset-cpus=12,13 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
    --volume "$view:/views:ro" --volume "$products:/tools:ro" --volume "$logs:/scratch:rw" \
    --env TMPDIR=/scratch/tmp --env PATH=/tools:/usr/bin:/bin --env LC_ALL=C --env TZ=UTC --workdir /views/xsh \
    sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c -c 'set +e
    timeout 180 xsht check core/wc.xsh core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh >/scratch/xsh.stdout 2>/scratch/xsh.stderr
    echo $? >/scratch/xsh.status
    cd /views/laputa
    timeout 180 xsht check pm/plan_json.xsh >/scratch/laputa.stdout 2>/scratch/laputa.stderr
    echo $? >/scratch/laputa.status
    [ "$(cat /scratch/xsh.status)" = 0 ] && [ "$(cat /scratch/laputa.status)" = 0 ]' >"$logs/docker.stdout" 2>"$logs/docker.stderr" || result=1
  container_id=$(cat "$cidfile")
  prove_absence "$container_id" "$state/inspect-$label-after" || result=1
  printf 'absent %s\n' "$container_id" >>"$state/cleanup.log"
  [ "$(cat "$logs/xsh.status")" = 0 ] || result=1
  [ "$(cat "$logs/laputa.status")" = 0 ] || result=1
  grep -Eq '^xsht check: 10 files .*lower ' "$logs/xsh.stderr" || result=1
  grep -Eq '^xsht check: 1 file .*lower ' "$logs/laputa.stderr" || result=1
done
exit "$result"
