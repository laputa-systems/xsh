#!/usr/bin/env bash
set -Eeuo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly owner=$root/.work/wt/typing-host-native6
readonly source_tree=$root/.work/wt/typing-host-obligations
readonly products=$source_tree/.work/target-typing-host-obligations/x86_64-unknown-linux-musl/verification
readonly proof=$owner/dev/consolidation/evidence/typing-host-native6
readonly scratch=$owner/.work/tmp/typing-host-native6/run-5277
readonly native_lock=$root/.work/consolidation/infra/native-suite.lock
readonly cleanup_marker=$root/.work/consolidation/infra/native-suite.cleanup-unresolved
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly common_git=$(git -C "$source_tree" rev-parse --path-format=absolute --git-common-dir)
readonly expected_head=5277f62b7b8b81f9805cb0d8e33172db5c1d8c9f
readonly expected_tree=c5ffd8a209d8e6b59c7012c571d64ed5436f5290
readonly expected_xsh=ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154
readonly expected_xsht=c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c
readonly container_name=xsh-typing-host-native6-5277-$$
readonly cidfile=$scratch/container.cid

source_snapshot() {
  git -C "$source_tree" rev-parse HEAD 'HEAD^{tree}'
  git -C "$source_tree" status --porcelain=v1 --untracked-files=all
  [[ $(git -C "$source_tree" rev-parse HEAD) == "$expected_head" ]] || return 1
  [[ $(git -C "$source_tree" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || return 1
  [[ -z $(git -C "$source_tree" status --porcelain=v1 --untracked-files=all) ]] || return 1
}

product_snapshot() {
  sha256sum "$products/xsh" "$products/xsht"
  printf '%s  %s/xsh\n%s  %s/xsht\n' "$expected_xsh" "$products" "$expected_xsht" "$products" | sha256sum --check >&2
}

finish() {
  local primary=$?
  local cleanup_status=0
  local cid=
  trap - EXIT INT TERM
  set +e
  if [[ -s $cidfile ]]; then
    cid=$(cat "$cidfile")
    if [[ ! $cid =~ ^[0-9a-f]{64}$ ]]; then
      cleanup_status=75
    elif docker inspect "$cid" > "$scratch/container-at-cleanup.json" 2> "$scratch/container-at-cleanup.stderr"; then
      docker rm --force "$cid" > "$scratch/container-remove.stdout" 2> "$scratch/container-remove.stderr" || cleanup_status=75
    fi
  fi
  if docker ps --all --quiet --no-trunc > "$scratch/container-ids-after.txt" 2> "$scratch/container-ids-after.stderr"; then
    if [[ -n $cid ]] && awk -v cid="$cid" '$0 == cid { found=1 } END { exit !found }' "$scratch/container-ids-after.txt"; then cleanup_status=75; fi
  else
    cleanup_status=75
  fi
  if docker ps --all --quiet --no-trunc --filter "name=^/$container_name$" > "$scratch/container-name-after.txt" 2> "$scratch/container-name-after.stderr"; then
    [[ ! -s $scratch/container-name-after.txt ]] || cleanup_status=75
  else
    cleanup_status=75
  fi
  printf '%s\n' "$cleanup_status" > "$scratch/cleanup.status"
  source_snapshot > "$scratch/source-after.txt" 2> "$scratch/source-after.stderr" || cleanup_status=75
  product_snapshot > "$scratch/products-after.sha256" 2> "$scratch/products-after.stderr" || cleanup_status=75
  cmp "$scratch/source-before.txt" "$scratch/source-after.txt" || cleanup_status=75
  cmp "$scratch/products-before.sha256" "$scratch/products-after.sha256" || cleanup_status=75
  printf '%s\n' "$cleanup_status" > "$scratch/final-guards.status"
  if (( primary == 0 && cleanup_status != 0 )); then primary=$cleanup_status; fi
  printf '%s\n' "$primary" > "$scratch/runner.status"
  exit "$primary"
}

git -C "$owner" check-ignore --quiet .work/tmp/probe
[[ ! -L $owner/.work && ! -L $owner/.work/tmp && ! -e $scratch && ! -L $scratch ]]
mkdir "$scratch"
mkdir "$scratch/tmp" "$scratch/source-tmp"
source_snapshot > "$scratch/source-before.txt"
product_snapshot > "$scratch/products-before.sha256" 2> "$scratch/products-before.stderr"
[[ $(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image") == "$image linux amd64" ]]
[[ ! -e $cleanup_marker ]]
exec 9>"$native_lock"
if ! flock -n -x 9; then printf 'native suite busy; no runtime started\n' >&2; exit 75; fi
[[ ! -e $cleanup_marker ]]
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
args=(run --rm --init --name "$container_name" --cidfile "$cidfile" --platform linux/amd64 --user 1000:1000 --cpuset-cpus=12-15 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash
  --volume "$source_tree:/work:ro" --volume "$source_tree:$source_tree:ro" --volume "$common_git:$common_git:ro"
  --volume "$products:/target:ro" --volume "$scratch:/scratch:rw" --volume "$scratch/source-tmp:/work/.work/tmp:rw"
  --volume "$proof/container-native6.sh:/runner/container-native6.sh:ro"
  --workdir /work --env PATH=/target:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  --env TMPDIR=/scratch/tmp --env LC_ALL=C --env TZ=UTC
  "$image" /runner/container-native6.sh)
{ printf 'docker'; printf ' %q' "${args[@]}"; printf '\n'; } > "$scratch/docker.command"
if docker "${args[@]}" > "$scratch/docker.stdout" 2> "$scratch/docker.stderr"; then docker_status=0; else docker_status=$?; fi
printf '%s\n' "$docker_status" > "$scratch/docker.status"
exit "$docker_status"
