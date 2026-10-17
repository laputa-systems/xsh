# Direct-IR ExprIf proof on 7eb62cd9

The strict route matrix is green for frozen source `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The earlier red matrix and 428-step replay remain unchanged in the parent directory.

## Matrix result

The matrix contains 152 rows, zero route gaps, and zero unmapped paths. It retains all 188 exact XSH route-artifact proofs. It admits one separate normal `ExprIf` proof for `in_place_evaluator_path` from the Rust test `runtime::eval::indexed::full::tests::direct_if_children_are_marked_and_selected`.

The Rust proof does not relabel an XSH artifact. The normal and forced XSH `ExprIf` artifacts still report `explicit_frame_control_path`. The 60 unique source-order selectors ran in both modes for 120 executions. The matrix records 252 tag-row links across 119 rows because selectors can apply to several rows. The 60 behavior-only selectors also ran in both modes for 120 executions.

`matrix-7eb62cd99-direct-ir-20261017.json` has SHA-256 `68f2b3fb2ce32798cf75bbcf6ebb7ed1ef25e05a5a133decb272620924f8191b`. It records source-lock status `matched`, a clean source snapshot, and the direct-IR proof separately from the XSH route artifacts.

## Direct-IR receipt

`direct-ir-proof/runner/receipt.json` has SHA-256 `f36639d28e59ab4b75f7ae32a4a5e3cc47a5fd397d770d3fd25f1d12f24f1f06`. Its step log has SHA-256 `a034fdeecf351585f5615b2fcb9a3d9b167f9fc3f645b5ad3c0f9a158926ea10`. The test passed with one test run, zero failures, and 750 filtered tests.

The receipt records source commit and tree `7eb62cd993e578c13bf6befd68a6a6ac15133c78` and `736b4bc16b1bcf07a0250b97e849506c19fb0753` before and after the test. The harness digest was `045f2690bf1de99ec55324f458ab2be454e4a581c6dd5d8d1e43b7d110da7d22` before and after. `build-receipt.json` preserves the canonical debug-harness receipt with SHA-256 `5f316e45b2d716b6272046adbafce80c94bb3180b0e2d5dbd66152092a3c5714`.

The debug harness came from build source `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree `08402ddd079228e072f24642fdb82db5196fff11`. The direct-IR validator binds that build separately from selected source `7eb62cd99` and checks the reviewed source and fixture hashes. The collector records the exact permitted fixture spelling difference in its direct-IR receipt policy.

`direct-ir-proof/runner/` preserves the unchanged receipt, test output, runner inputs, source snapshots, harness guards, and cleanup records. `direct-ir-proof/controller-path-attempt/` preserves a captured repeat of the failed controller-path invocation. Its stdout is empty. Its traceback and exit status show that `OPERAND_MANIFEST.relative_to(root)` received a path from a sibling controller worktree.

## Collector and scratch mirror

The accepted collector implementation is in commits `30bebbc445657c19ba21d976f7d38823170fa824` and `9f5d127252fb0a56a47cbb3802f650c123fbdf73` on branch `campaign/route-direct-ir-admission-20261017`. Controller tip `9c8898b240ff2b027308755b9cc7e0ba5e6d62e4` adds documentation after those code commits. Root reviewed its 79 Python controls and strict source checks. This evidence commit contains no collector or runtime changes.

The run copied `generate.py`, `evidence.json`, `operand-order-manifest.tsv`, and `ast-extractor/` from the controller worktree to the frozen root's ignored `.work/tmp/route-tool/dev/consolidation/winddown/gate-tools/semantic-route-matrix/` path. `collector-mirror/` preserves those exact input bytes and also preserves the controller's `selftest.py`. Its `evidence.json` and operand-order manifest match the frozen tracked files byte for byte. Their SHA-256 values are `2578a15dddb7b247959406b5bd174bab799e6ac1c5361f3e0a77200b8075e209` and `87cb2cfdcd13ade7b1a3d22f7cbbe22a726f080bebd90b9a2557fb9f9fed1665`.

The successful generator command was:

```sh
flock -n /home/josh/d/laputa-systems/xsh/.work/consolidation/infra/cargo.lock \
  env PATH=/home/josh/.cargo/bin:/home/josh/.local/bin:/usr/local/bin:/usr/bin:/bin \
  TMPDIR=/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99/.work/tmp \
  CARGO_BUILD_JOBS=4 \
  CARGO_BUILD_TARGET=x86_64-unknown-linux-musl \
  RUSTFLAGS='-C linker=clang -C link-arg=-fuse-ld=mold' \
  python3 -B /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99/.work/tmp/route-tool/dev/consolidation/winddown/gate-tools/semantic-route-matrix/generate.py \
    --root /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99 \
    --direct-ir-receipt /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99/.work/tmp/direct-if-route-7eb62/receipt.json \
    --output /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99/.work/tmp/route-matrix-7eb62cd99-direct-ir.json
```

The matrix records the mirrored manifest path and its SHA-256 `87cb2cfdcd13ade7b1a3d22f7cbbe22a726f080bebd90b9a2557fb9f9fed1665`. The source-fingerprinted AST helper binary has SHA-256 `25c1e75dad46d147bf430fd00114ce2edb107272052b5a9396ea0d9ca4462c03`. `generate.py` sets `RUSTC_WRAPPER` to `/home/josh/.cargo/bin/kache`. The generator used the existing helper target under the shared Cargo lock. No XSH product build or full native suite ran for this matrix generation.
