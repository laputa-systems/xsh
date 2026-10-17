#!/usr/bin/env bash
set -Eeuo pipefail
lane=$(git rev-parse --show-toplevel)
view=$1
logs=$2
products=/home/josh/d/laputa-systems/xsh/.work/wt/typing-host-obligations/.work/target-typing-host-obligations/x86_64-unknown-linux-musl/verification
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
case "$view:$logs" in "$lane/.work/"*:"$lane/.work/"*) ;; *) exit 2 ;; esac
mkdir -p "$logs/tmp"
check_products() {
  printf '%s  %s\n' ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154 "$products/xsh" c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c "$products/xsht" | sha256sum -c -
}
cleanup() {
  result=$?
  trap - EXIT INT TERM
  check_products >"$logs/products.after" || result=1
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
check_products >"$logs/products.before"
docker run --rm --init --platform linux/amd64 --user 1000:1000 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
  --volume "$view:/views:ro" --volume "$products:/tools:ro" --volume "$logs:/scratch:rw" \
  --env TMPDIR=/scratch/tmp --env PATH=/tools:/usr/bin:/bin --env LC_ALL=C --env TZ=UTC --workdir /views/xsh \
  "$image" -c 'set +e
  timeout 180 xsht check core/wc.xsh core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh >/scratch/xsh.stdout 2>/scratch/xsh.stderr
  echo $? >/scratch/xsh.status
  cd /views/laputa
  timeout 180 xsht check pm/plan_json.xsh >/scratch/laputa.stdout 2>/scratch/laputa.stderr
  echo $? >/scratch/laputa.status
  cat /scratch/xsh.status /scratch/laputa.status'
