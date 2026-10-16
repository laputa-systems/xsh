#!/bin/bash
set -u
repo=/home/josh/d/laputa-systems/xsh
work="$repo/.work/tmp/procps-exit-main787/attempt3"
products="$repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification"
receipt="$repo/.work/tmp/acceptance-products-exit-state.XXliGcHd"
expected_xsh=d0588020f3adbdb041e07ed83a4ac33ddf957969365674e7e09fd8227308a809
expected_xsht=feded7ddcb55ee73bf524abb45455902d785ab77000eabdb6d2b18ec4cf3744a
cd "$repo" || exit 90
if [ "$(git rev-parse HEAD)" != 787ba37a43e808c6b63b85b17cc912fe36352b22 ] || [ "$(git rev-parse HEAD^{tree})" != 31681c63f02b825b4352017482230a33e78a819d ] || [ -n "$(git status --porcelain=v1)" ]; then
  printf '%s\n' 'source pin or clean-tree preflight failed' > "$work/preflight.error"
  exit 91
fi
actual_xsh=$(sha256sum "$products/xsh" | cut -d ' ' -f 1)
actual_xsht=$(sha256sum "$products/xsht" | cut -d ' ' -f 1)
if [ "$actual_xsh" != "$expected_xsh" ] || [ "$actual_xsht" != "$expected_xsht" ]; then
  printf 'product hash mismatch: xsh=%s xsht=%s\n' "$actual_xsh" "$actual_xsht" > "$work/preflight.error"
  exit 92
fi
{
  git status --short --branch
  git rev-parse HEAD HEAD^{tree}
  sha256sum src/modules/process.rs core/tests/test-procps.xsh tests/xsh/stdlib/linux_sampling.xsh
} > "$work/source-before.txt"
printf '%s\n' "$actual_xsh  $products/xsh" "$actual_xsht  $products/xsht" > "$work/products-before.sha256"
printf 'receipt_dir=%s\n' "$receipt" > "$work/build-receipt-location.txt"
printf '%s\n' 'docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-13 --cpus=2 --memory=4g --memory-swap=4g --network=none --tmpfs /tmp:rw,nodev,exec,mode=1777 --volume /home/josh/d/laputa-systems/xsh:/work:ro --volume /home/josh/d/laputa-systems/xsh/.work/target-final-canonical/x86_64-unknown-linux-musl/verification:/artifact:ro --volume /home/josh/d/laputa-systems/xsh/.work/tmp/procps-exit-main787/attempt3:/scratch:rw --workdir /work --env TMPDIR=/scratch/runtime-tmp --env HOME=/scratch/home --env TARGET=x86_64-unknown-linux-musl sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c -c "unset LD_PRELOAD; exec /bin/bash /scratch/integration-runtime.sh"' > "$work/docker.argv.txt"
docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-13 --cpus=2 --memory=4g --memory-swap=4g --network=none --tmpfs /tmp:rw,nodev,exec,mode=1777 --entrypoint /bin/bash --volume "$repo:/work:ro" --volume "$products:/artifact:ro" --volume "$work:/scratch:rw" --workdir /work --env TMPDIR=/scratch/runtime-tmp --env HOME=/scratch/home --env TARGET=x86_64-unknown-linux-musl sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c -c 'unset LD_PRELOAD; exec /bin/bash /scratch/integration-runtime.sh' > "$work/container.stdout" 2> "$work/container.stderr"
container_status=$?
printf '%s\n' "$container_status" > "$work/container.status"
{
  git status --short --branch
  git rev-parse HEAD HEAD^{tree}
  sha256sum src/modules/process.rs core/tests/test-procps.xsh tests/xsh/stdlib/linux_sampling.xsh
} > "$work/source-after.txt"
sha256sum "$products/xsh" "$products/xsht" > "$work/products-after.sha256"
if ! cmp -s "$work/source-before.txt" "$work/source-after.txt" || ! cmp -s "$work/products-before.sha256" "$work/products-after.sha256"; then
  printf '%s\n' 'source or product identity changed during runtime' > "$work/identity-check.error"
  exit 93
fi
printf 'container status: %s\n' "$container_status"
cat "$work/container.stdout"
cat "$work/container.stderr"
exit "$container_status"
