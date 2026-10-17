#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
commit=$1
scratch="$root/.work/tmp/integrated-$commit"
products="$root/.work/consolidation/final-performance-gates/profile-cost-24cc0d70-20261017-r2/opt3/target/x86_64-unknown-linux-musl/release"
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
label="campaign.integrated-$commit"
[[ $(git -C "$root" rev-parse HEAD) == "$commit" ]]
[[ -z $(git -C "$root" status --porcelain) ]]
[[ ! -e "$scratch" ]]
mkdir -m 700 "$scratch"
exec 9<>"$root/.work/consolidation/infra/native-suite.lock"
flock -n -x 9
[[ $(stat -Lc %i /proc/$$/fd/9) == 62012470 ]]
[[ $(cat "$root/.work/consolidation/infra/native-suite.lock") != cleanup-unresolved:* ]]
git -C "$root" rev-parse HEAD 'HEAD^{tree}' > "$scratch/source.txt"
sha256sum "$root/core/sort.xsh" "$root/core/rm.xsh" "$root/core/tests/test-gnu-rm.xsh" > "$scratch/source.before"
sha256sum "$products/xsh" "$products/xsht" "$products/xsh-test-helper" > "$scratch/products.before"
cleanup() {
  status=$?
  trap - EXIT INT TERM HUP
  set +e
  if [[ -s "$scratch/container.cid" ]]; then
    timeout -k 1s 5s docker inspect --format '{{.State.OOMKilled}} {{.HostConfig.CpusetCpus}} {{.HostConfig.Memory}} {{.HostConfig.MemorySwap}}' "$(cat "$scratch/container.cid")" > "$scratch/container-state.txt" 2> "$scratch/inspect.stderr"
    timeout -k 1s 5s docker rm -f "$(cat "$scratch/container.cid")" > "$scratch/cleanup.stdout" 2> "$scratch/cleanup.stderr"
    cleanup_status=$?
  else
    cleanup_status=0
  fi
  timeout -k 1s 5s docker ps -aq --filter "label=$label" > "$scratch/containers.remaining"
  list_status=$?
  sha256sum "$root/core/sort.xsh" "$root/core/rm.xsh" "$root/core/tests/test-gnu-rm.xsh" > "$scratch/source.after"
  sha256sum "$products/xsh" "$products/xsht" "$products/xsh-test-helper" > "$scratch/products.after"
  cmp -s "$scratch/source.before" "$scratch/source.after"; source_status=$?
  cmp -s "$scratch/products.before" "$scratch/products.after"; product_status=$?
  [[ $(git -C "$root" rev-parse HEAD) == "$commit" && -z $(git -C "$root" status --porcelain) ]]; tree_status=$?
  printf 'command=%s cleanup=%s list=%s source=%s products=%s tree=%s\n' "$status" "$cleanup_status" "$list_status" "$source_status" "$product_status" "$tree_status" > "$scratch/result.txt"
  if ((cleanup_status || list_status || source_status || product_status || tree_status)) || [[ -s "$scratch/containers.remaining" ]]; then exit 70; fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
timeout -k 3s 180s docker run --init --cidfile "$scratch/container.cid" --label "$label" --platform linux/amd64 --cpuset-cpus=28-31 --cpus=4 --memory=8g --memory-swap=8g --network=none --user=1000:1000 --mount "type=bind,src=$root,dst=/repo,readonly" --mount "type=bind,src=$products,dst=/tools,readonly" --mount "type=bind,src=$scratch,dst=/case" -e TMPDIR=/case -e LC_ALL=C --workdir /repo "$image" /bin/sh -ec '
for selector do
  /tools/xsht test --jobs 1 --exact "$selector"
done
' sh core/tests/test-sort.xsh::test_sort_key_option_suffixes_and_blank_boundaries core/tests/test-sort.xsh::test_sort_general_numeric_prepares_spilled_and_merge_keys core/tests/test-gnu-rm.xsh::test_gnu_rm_r_symlink_directory_log > "$scratch/stdout.log" 2> "$scratch/stderr.log"
