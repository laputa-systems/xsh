#!/usr/bin/env bash
set -Eeuo pipefail
lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-corpus-b235
source_lane=/home/josh/d/laputa-systems/xsh/.work/wt/typing-set-candidate
products=/home/josh/d/laputa-systems/xsh/.work/target-typing-set-seed/x86_64-unknown-linux-musl/verification
original=$lane/.work/tmp/corpus/original
reduced=$lane/.work/tmp/corpus/mutable-set-plan/candidate
attempt=$lane/.work/tmp/corpus/set-check-$(date -u +%Y%m%dT%H%M%SZ)-$$
mkdir "$attempt"
mkdir "$attempt/state"
state=$attempt/state
cidfile=
source_guard() {
  [ "$(git -C "$source_lane" rev-parse HEAD)" = 7491228abdfd77e4be502247675b08682cbf16b8 ]
  [ "$(git -C "$source_lane" rev-parse 'HEAD^{tree}')" = e2332da7ce354110347efc6f3ba1ec052e1d9bec ]
  [ -z "$(git -C "$source_lane" status --porcelain)" ]
  git -C "$source_lane" rev-parse HEAD 'HEAD^{tree}'
}
product_guard() {
  printf '%s  %s\n' 1a4dae2d5cd783d666e3ae246fbd1bcbe0744b5617859b8ea0f32194798e4891 "$products/xsh" 1941396b50ace784c79cc851d001af41ea236a8102b378b4c32bc1258134f2c5 "$products/xsht" 5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d "$products/xsh-test-helper" | sha256sum -c -
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
result=0
for label in original reduced; do
  if [ "$label" = original ]; then view=$original; else view=$reduced; fi
  logs=$attempt/$label
  mkdir -p "$logs/tmp"
  cidfile=$state/$label.cid
  docker run --rm --init --name "xsh-typing-set-check-$$-$label" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpus=4 --cpuset-cpus=12,13 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
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
  grep -Eq '^xsht check: 1 files .*lower ' "$logs/laputa.stderr" || result=1
done
exit "$result"
