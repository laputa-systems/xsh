#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
source_root=$root/.work/wt/consolidation-final-7eb62cd99
scratch=$root/.work/tmp/direct-if-route-7eb62
harness=$root/.work/wt/consolidation-final-b235a1a43/.work/target-final-canonical/x86_64-unknown-linux-musl/debug/build/xsh/692c8c084af82ba2/out/xsh-692c8c084af82ba2
test_id=runtime::eval::indexed::full::tests::direct_if_children_are_marked_and_selected
name=xsh-direct-if-route-7eb62-$$
cidfile=$scratch/container.cid
snapshot() {
  git -C "$source_root" rev-parse HEAD 'HEAD^{tree}'
  git -C "$source_root" status --porcelain=v1 --untracked-files=all
  [[ $(git -C "$source_root" rev-parse HEAD) == 7eb62cd993e578c13bf6befd68a6a6ac15133c78 ]]
  [[ $(git -C "$source_root" rev-parse 'HEAD^{tree}') == 736b4bc16b1bcf07a0250b97e849506c19fb0753 ]]
  [[ -z $(git -C "$source_root" status --porcelain=v1 --untracked-files=all) ]]
}
finish() {
  local primary=$?
  local cleanup_status=0
  trap - EXIT INT TERM
  set +e
  if [[ -s $cidfile ]]; then
    cid=$(cat "$cidfile")
    if [[ $cid =~ ^[a-f0-9]{64}$ ]]; then
      docker rm --force "$cid" > "$scratch/cleanup.stdout" 2> "$scratch/cleanup.stderr" || cleanup_status=1
      docker ps --all --quiet --no-trunc --filter "id=$cid" > "$scratch/container-after.stdout" 2> "$scratch/container-after.stderr" || cleanup_status=1
      [[ ! -s $scratch/container-after.stdout ]] || cleanup_status=1
    else
      cleanup_status=1
    fi
  else
    cleanup_status=1
  fi
  snapshot > "$scratch/source-after.txt" || cleanup_status=1
  sha256sum "$harness" > "$scratch/harness-after.sha256" || cleanup_status=1
  cmp "$scratch/source-before.txt" "$scratch/source-after.txt" || cleanup_status=1
  cmp "$scratch/harness-before.sha256" "$scratch/harness-after.sha256" || cleanup_status=1
  printf '%s\n' "$cleanup_status" > "$scratch/cleanup.status"
  if (( primary == 0 && cleanup_status != 0 )); then primary=$cleanup_status; fi
  printf '%s\n' "$primary" > "$scratch/runner.status"
  exit "$primary"
}
exec 9>"$root/.work/consolidation/infra/native-suite.lock"
flock -n -x 9
[[ ! -e $root/.work/consolidation/infra/native-suite.cleanup-unresolved ]]
snapshot > "$scratch/source-before.txt"
sha256sum "$harness" > "$scratch/harness-before.sha256"
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
args=(run --rm --init --name "$name" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpus=4 --memory=8g --memory-swap=8g --network=none --volume "$harness:$harness:ro" --volume "$source_root:/work:ro" --volume "$scratch:/scratch:rw" --workdir /work --env TMPDIR=/scratch/tmp --env LC_ALL=C --env TZ=UTC sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c "$harness" "$test_id" --exact)
{ printf 'docker'; printf ' %q' "${args[@]}"; printf '\n'; } > "$scratch/docker.command"
if timeout 120 docker "${args[@]}" > "$scratch/stdout.log" 2> "$scratch/stderr.log"; then status=0; else status=$?; fi
printf '%s\n' "$status" > "$scratch/process.status"
{ printf '\n\n[stdout]\n'; cat "$scratch/stdout.log"; printf '\n[stderr]\n'; cat "$scratch/stderr.log"; } > "$scratch/step.log"
[[ $status == 0 ]]
[[ $(grep -c '^running 1 test$' "$scratch/stdout.log") == 1 ]]
[[ $(grep -Fc "test $test_id ... ok" "$scratch/stdout.log") == 1 ]]
grep -Eq '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; [0-9]+ filtered out;' "$scratch/stdout.log"
