set -u
repo=/home/josh/d/laputa-systems/xsh
task_dir=$repo/.work/tmp/tail-debug-profile-repair/lease2-controlled-red
lock=$repo/.work/consolidation/infra/native-suite.lock
exec 9<>"$lock"
flock -n 9 || exit 73
stat -c '%i %n' "$lock" > "$task_dir/lock-before.txt"
"$task_dir/run-container.sh"
container_status=$?
flock -u 9
stat -c '%i %n' "$lock" > "$task_dir/lock-after.txt"
exit "$container_status"
