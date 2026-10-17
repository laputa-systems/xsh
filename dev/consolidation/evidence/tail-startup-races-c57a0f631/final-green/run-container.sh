set -u
repo=/home/josh/d/laputa-systems/xsh
worktree=$repo/.work/wt/tail-debug-c-profile
profile=$repo/.work/consolidation/final-performance-gates/profile-cost-57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1/opt3
laputa=/home/josh/d/laputa-systems/laputa
task_dir=$repo/.work/tmp/tail-debug-profile-repair/lease4-final-green
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
docker run --cidfile "$task_dir/container.cid" --rm --init --platform linux/amd64 --cpuset-cpus 24,25,26,27 --cpus 4 --memory 8g --memory-swap 8g --tmpfs /tmp:rw,exec,nosuid,mode=1777 -v "$repo:$repo" -v "$repo/.git:$repo/.git:ro" -v "$laputa:$laputa:ro" -w "$worktree" -e LC_ALL=C -e TZ=UTC -e TMPDIR="$task_dir/tmp" -e CARGO_TARGET_DIR="$profile/target" -e CARGO_BUILD_JOBS=4 -e XSH_BIN="$profile/target/x86_64-unknown-linux-musl/release/xsh" -e XSHT_BIN="$profile/target/x86_64-unknown-linux-musl/release/xsht" -e TAIL_EVIDENCE_DIR="$task_dir" -e PATH="$profile/target/x86_64-unknown-linux-musl/release:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin" --user 1000:1000 -e HOME=/home/compat "$image" /bin/bash "$task_dir/run-exact.sh" > "$task_dir/container.stdout" 2> "$task_dir/container.stderr"
container_status=$?
printf '%s\n' "$container_status" > "$task_dir/container.exit"
exit "$container_status"
