set -eu
mode=$1
root=$2
xsh=$3
core=$4
compare() {
  set +e
  /bin/mountpoint "$@" > "$root/ref.out" 2> "$root/ref.err"
  ref=$?
  "$xsh" "$core/mountpoint.xsh" -- "$@" > "$root/cand.out" 2> "$root/cand.err"
  cand=$?
  set -e
  [ "$ref" = "$cand" ] && cmp "$root/ref.out" "$root/cand.out" && cmp "$root/ref.err" "$root/cand.err"
}
if [ "$mode" = chroot ]; then
  mount -t tmpfs fixture "$root"
  trap 'cd /; umount -l "$root"' EXIT
  mkdir -p "$root/r" "$root/r$core" "$(dirname "$root/r$xsh")"
  for d in bin lib lib64 usr etc dev sbin proc; do
    [ -d "/$d" ] && [ ! -L "/$d" ] || continue
    mkdir -p "$root/r/$d"
    mount --rbind "/$d" "$root/r/$d"
  done
  mkdir -p "$root/r/plain" "$root/r/tmp"
  mount --bind "$core" "$root/r$core"
  touch "$root/r$xsh"
  mount --bind "$xsh" "$root/r$xsh"
  PATH="$PATH:/usr/sbin:/sbin" chroot "$root/r" /bin/sh "$core/tests/data/mountpoint-review6.sh" inside /tmp "$xsh" "$core"
elif [ "$mode" = inside ]; then
  for path in / /plain /tmp /usr; do
    compare "$path"
    compare -d "$path"
    compare -q "$path"
    compare --show "$path"
  done
elif [ "$mode" = hidden ]; then
  mount -t tmpfs fixture "$root"
  trap 'cd /; umount -l "$root"' EXIT
  mkdir "$root/a"
  mount -t tmpfs inner "$root/a"
  mkdir "$root/a/b"
  mount -t tmpfs nested "$root/a/b"
  cd "$root/a/b"
  mount -t tmpfs cover "$root/a"
  for path in . ..; do
    compare "$path"
    compare -d "$path"
    compare --show "$path"
  done
fi
