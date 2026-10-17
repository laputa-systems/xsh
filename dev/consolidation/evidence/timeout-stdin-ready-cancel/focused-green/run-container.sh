set -u
repo=/home/josh/d/laputa-systems/xsh
worktree=$repo/.work/wt/timeout-stdin-c-profile
profile=$repo/.work/consolidation/final-performance-gates/profile-cost-57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1/opt3
laputa=/home/josh/d/laputa-systems/laputa
task_dir=$repo/.work/tmp/timeout-stdin-profile-repair/ready-fifo-fix
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
lock=$repo/.work/consolidation/infra/native-suite.lock
test "$(stat -c '%i' "$lock")" = 62012470 || exit 70
printf '%s  %s\n' \
  e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b "$profile/target/x86_64-unknown-linux-musl/release/xsh" \
  daa6c47667f65e8a2fe06a04ff179ee99e6286496c6d441b193889b3b9fec7fa "$profile/target/x86_64-unknown-linux-musl/release/xsht" \
  cf8005649749a43cc22aa5e40092a8f90365efc370b857fce5de2004adb2e621 "$profile/target/x86_64-unknown-linux-musl/release/xsh-test-helper" \
  | sha256sum -cs /dev/stdin || exit 71
set +e
flock -n "$lock" docker run --cidfile "$task_dir/container.cid" --rm --init --platform linux/amd64 --cpuset-cpus 28,29,30,31 --cpus 4 --memory 8g --memory-swap 8g --tmpfs /tmp:rw,exec,nosuid,mode=1777 \
  -v "$repo:$repo" -v "$repo/.git:$repo/.git:ro" -v "$laputa:$laputa:ro" \
  -w "$worktree" -e LC_ALL=C -e TZ=UTC -e TMPDIR="$task_dir/tmp" \
  -e CARGO_TARGET_DIR="$profile/target" -e CARGO_BUILD_JOBS=4 \
  -e XSH_BIN="$profile/target/x86_64-unknown-linux-musl/release/xsh" \
  -e XSHT_BIN="$profile/target/x86_64-unknown-linux-musl/release/xsht" \
  -e TIMEOUT_EVIDENCE_DIR="$task_dir" \
  -e PATH="$profile/target/x86_64-unknown-linux-musl/release:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
  --user 1000:1000 -e HOME=/home/compat "$image" /bin/bash "$task_dir/run-exact.sh" \
  > "$task_dir/container.stdout" 2> "$task_dir/container.stderr"
status=$?
printf '%s\n' "$status" > "$task_dir/container.exit"
exit "$status"
