# Replay commands

These commands copy the captured runners into fresh scratch directories. They use the source and product pins recorded in the [qualification report](README.md). The full run includes all 35 standard selectors and the namespace selector. The short-path command reruns only the `ss` selector.

Set the shared pins:

```sh
REPO=/home/josh/d/laputa-systems/xsh
EVIDENCE="$REPO/.work/wt/linux-residual-0deeb9c7/dev/consolidation/evidence/linux-residual-0deeb9c7"
export SOURCE_ROOT="$REPO/.work/wt/consolidation-final-7eb62cd99"
export PRODUCT_DIR="$REPO/.work/final-review-evidence/final-correctness-gates/correctness-7eb62cd99-20261017-r1/target/products/x86_64-unknown-linux-musl/verification"
export IMAGE=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
export SOURCE_COMMIT=7eb62cd993e578c13bf6befd68a6a6ac15133c78
export SOURCE_TREE=736b4bc16b1bcf07a0250b97e849506c19fb0753
export COMMON_ROOT="$REPO"
export PRODUCT_XSH_SHA256=d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af
export PRODUCT_XSHT_SHA256=4e0322037add1138f0e3a67f6696e3b27e7ae60ea794f198e0c5ea0d6bae8a6e
```

Copy the full-selection runners and selectors to owned scratch, then run them:

```sh
SCRATCH_ROOT="$REPO/.work/tmp/linux-residual-replay-full"
mkdir -p "$SCRATCH_ROOT/t"
cp "$EVIDENCE/attempt-2-full-selection/run-all.sh" "$SCRATCH_ROOT/linux-residual-run-all.sh"
cp "$EVIDENCE/attempt-2-full-selection/run-all-locked.sh" "$SCRATCH_ROOT/linux-residual-run-all-locked.sh"
cp "$EVIDENCE/attempt-2-full-selection/container-runner.sh" "$SCRATCH_ROOT/linux-residual-run-container.sh"
cp "$EVIDENCE/attempt-2-full-selection/native-runner.sh" "$SCRATCH_ROOT/linux-residual-run-native.sh"
cp "$EVIDENCE/standard-selectors.txt" "$SCRATCH_ROOT/linux-residual-selectors-default.txt"
cp "$EVIDENCE/sysadmin-selector.txt" "$SCRATCH_ROOT/linux-residual-selector-sysadmin.txt"
export SCRATCH_ROOT
bash "$SCRATCH_ROOT/linux-residual-run-all.sh"
```

Copy the short-path runner and single selector to a separate scratch directory, then acquire the shared native-suite lock:

```sh
SCRATCH_ROOT="$REPO/.work/tmp/linux-residual-replay-ss"
mkdir -p "$SCRATCH_ROOT/t"
cp "$EVIDENCE/attempt-3-ss-short-path/container-runner.sh" "$SCRATCH_ROOT/linux-residual-run-container.sh"
cp "$EVIDENCE/attempt-3-ss-short-path/native-runner.sh" "$SCRATCH_ROOT/linux-residual-run-native.sh"
cp "$EVIDENCE/attempt-3-ss-short-path/ss-selector.txt" "$SCRATCH_ROOT/linux-residual-selectors-default.txt"
export SCRATCH_ROOT
flock -n "$COMMON_ROOT/.work/consolidation/infra/native-suite.lock" \
  bash "$SCRATCH_ROOT/linux-residual-run-container.sh" standard
```
