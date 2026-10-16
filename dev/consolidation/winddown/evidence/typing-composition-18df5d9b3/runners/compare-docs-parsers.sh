#!/usr/bin/env bash
set -euo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-composition
readonly scratch=$root/.work/tmp/typing-composition-debug/docs-parser-comparison
readonly old_target=$root/.work/target-typing-pipeline
readonly new_target=$root/.work/target-typing-composition
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
mkdir -p "$scratch/tmp"
cat > "$scratch/render-docs.xsh" <<'XSH'
use docs

docs.generate(
  p"/work",
  docs.DocTools(xsh: p"/tools/xsh", xsht: p"/tools/xsht"),
)?
XSH
flock "$root/.work/consolidation/infra/cargo.lock" docker run --rm --init \
  --platform linux/amd64 --cpuset-cpus=0-1 --cpus=2 --memory=2g --memory-swap=2g \
  --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:rw" \
  --volume "$scratch:/scratch:rw" \
  --volume "$old_target/x86_64-unknown-linux-musl/debug:/parser-old:ro" \
  --volume "$new_target/x86_64-unknown-linux-musl/debug:/parser-new:ro" \
  --volume "$new_target/x86_64-unknown-linux-musl/debug:/tools:ro" \
  --workdir /work \
  --env XSH_MODULE_PATH=/work/dev \
  --env TMPDIR=/scratch/tmp \
  "$image" -c 'set -u
for parser in old new; do
  if [ "$parser" = old ]; then binary=/parser-old/xsh; else binary=/parser-new/xsh; fi
  if "$binary" /scratch/render-docs.xsh >"/scratch/$parser.stdout" 2>"/scratch/$parser.stderr"; then status=0; else status=$?; fi
  printf "%s exit=%s\n" "$parser" "$status"
done'
