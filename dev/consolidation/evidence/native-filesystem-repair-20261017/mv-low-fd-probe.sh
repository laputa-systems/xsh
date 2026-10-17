#!/bin/sh
set -eu
lane_path=/home/josh/d/laputa-systems/xsh/.work/wt/native-filesystem-repair
source_dir=/scratch/source
remote=/dev/shm/xsh-native-filesystem-repair-$$
mkdir -p "$source_dir" "$remote"
trap 'rm -rf "$source_dir" "$remote"' EXIT
set --
index=0
while [ "$index" -lt 96 ]; do
  name=$(printf 'source-%03d' "$index")
  mkdir "$source_dir/$name"
  : > "$source_dir/$name/f"
  set -- "$@" "$name"
  index=$((index + 1))
done
set -- "$@" "$remote"
cd "$source_dir"
ulimit -n 64
if strace -f -qq -e trace=openat,openat2,close -o /scratch/mv-fd.trace /target/xsh "$lane_path/core/mv.xsh" -- "$@"; then
  status=0
else
  status=$?
fi
printf 'xsh_status=%s
' "$status" > /scratch/mv-fd.status
exit 0
