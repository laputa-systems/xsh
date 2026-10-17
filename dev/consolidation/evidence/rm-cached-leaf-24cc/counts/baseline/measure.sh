#!/bin/sh
set -eu
mkdir d
i=1
while [ "$i" -le 10000 ]; do
  : > "d/$i"
  i=$((i + 1))
done
test -f d/1
test -f d/10000
set +e
/usr/bin/strace --seccomp-bpf -f -qq -c -e trace=newfstatat,lstat,unlink,unlinkat -o /case/strace.summary /tools/xsh /src/core/tests/support/uu-launch.xsh '{"caller_keys":[],"umask":null}' /tools/xsh /src/core/rm.xsh -- -rf d
status=$?
set -e
printf '%s\n' "$status" > /case/rm.exit
if [ -e d ]; then
  printf 'tree_removed=0\n' > /case/scene.result
  exit 99
fi
printf 'tree_removed=1\n' > /case/scene.result
exit "$status"
