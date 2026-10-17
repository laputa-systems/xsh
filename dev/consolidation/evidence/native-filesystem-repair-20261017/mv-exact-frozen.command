#!/usr/bin/env bash
set -Eeuo pipefail

lane=/home/josh/d/laputa-systems/xsh/.work/wt/native-filesystem-repair
frozen=/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-0dc9cff7c
products="$frozen/.work/target-final-canonical/x86_64-unknown-linux-musl/verification"
scratch="$lane/.work/tmp/mv-exact-frozen"
lock=/home/josh/d/laputa-systems/xsh/.work/consolidation/infra/native-suite.lock

mkdir -p "$scratch/tmp"

snapshot() {
  printf 'source_commit='
  git -C "$lane" rev-parse HEAD
  printf 'source_tree='
  git -C "$lane" rev-parse 'HEAD^{tree}'
  printf 'source_files_clean='
  if git -C "$lane" diff --quiet HEAD -- core/mv.xsh core/tests/test-gnu-mv.xsh; then
    printf 'yes\n'
  else
    printf 'no\n'
    return 1
  fi
  sha256sum "$lane/core/mv.xsh" "$lane/core/tests/test-gnu-mv.xsh"
  sha256sum "$products/xsh" "$products/xsht"
}

snapshot > "$scratch/before.txt"
grep -q '^source_commit=0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4$' "$scratch/before.txt"
grep -q '^source_tree=b4f9db3f8ee2f38a6c62fea876976619db424ed7$' "$scratch/before.txt"
grep -q '^c137d1d1ccff63c3ae492af1106aebb1872ae23b3d0e2af1d017968a4aa19a6c ' "$scratch/before.txt"
grep -q '^5dccc9f21f30b4b26e8f96a7815303dd3b7a13c94b132d421ad02cee3fada7e1 ' "$scratch/before.txt"

if flock -x -w 1800 "$lock" docker run --rm --init --platform linux/amd64 \
  --user 1000:1000 --cpus=12 --memory=8g --memory-swap=8g --network=none \
  --volume "$lane:$lane:ro" --volume /home/josh/d/laputa-systems/xsh/.git:/home/josh/d/laputa-systems/xsh/.git:ro \
  --volume "$products:/target:ro" --volume "$scratch:/scratch:rw" \
  --workdir "$lane" --env TMPDIR=/scratch/tmp --env LC_ALL=C --env TZ=UTC \
  --env PATH=/target:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  /target/xsht test --exact core/tests/test-gnu-mv.xsh::test_gnu_mv_leak_fd_log \
  > "$scratch/stdout.log" 2> "$scratch/stderr.log"; then
  docker_status=0
else
  docker_status=$?
fi

printf 'docker_exit_code=%s\n' "$docker_status" > "$scratch/status.txt"
snapshot > "$scratch/after.txt"
cmp "$scratch/before.txt" "$scratch/after.txt"
exit "$docker_status"
