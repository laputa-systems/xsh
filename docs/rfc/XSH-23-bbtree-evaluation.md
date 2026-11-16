# XSH-23: ordered-map profile

Decision: keep the standard library map and leave XSH-23 open. At base revision
`db3316403daae381a7748ca08923a900752289fe`, two isolated changes to dynamic
spelling lookup failed to improve the 512-unit witness and were discarded. An
`#[inline]` on `DynamicSpellings::load` left its five-run mean at 12.426 billion
instructions, essentially the baseline's 12.426 billion. Splitting its miss
path into a cold function with an always-inlined fast path raised the witness
mean to 12.730 billion (+2.45%), while reducing `core/sort.xsh` from 1.8433 to
1.8377 billion (-0.30%). Five additional individual runs ranged from 12.4252 to
12.4271 billion on the baseline and 12.7294 to 12.7299 billion on the split
candidate; the corresponding sort ranges were 1.8415–1.8434 and 1.8377–1.8379
billion. Both sides used the release profile and forced frame pointers. These
are user-mode instruction counts, not wall-time comparisons. The record-map
change was not attempted, and the required final correctness and gate checks
were not run. The attempt limit was reached without an accepted candidate.

The discarded split candidate's flat profile had 6,332 samples: `Name::cmp`
6.12% self, `memcmp` 11.84%, and the inlined dynamic-spelling comparison closure
6.76%; `DynamicSpellings::load` had no separately attributed symbol. Record-map
bulk node construction was 4.92%, insert 2.04%, and tree drop 1.99% self.
Inlining changes attribution, so these shares are not directly comparable to
the earlier profile below.

At revision `c334c791f77ae9e7f767fecdbf51e8256bde6d88`, the reduced
`named-generic-constructors-independent-512` source from
`dev/performance/inference_workloads.py` constructs 512 independent binders and
checks their results. This is a generalization-copy witness; the inference
document describes multiple 512-unit families rather than identifying one
unique witness. `xsht check` succeeded. The host was loaded (32 CPUs, load
averages 10.99, 13.17, 15.97); these are sampled CPU shares, not wall-time
comparisons or a performance gate.

```sh
mkdir -p .work/tmp/xsh-23
TMPDIR="$PWD/.work/tmp/xsh-23" CARGO_TARGET_DIR="$PWD/.work/tmp/xsh-23/target" RUSTFLAGS='-C force-frame-pointers=yes' cargo build --profile release -p xsh -p xsht
TMPDIR="$PWD/.work/tmp/xsh-23" python3 - <<'PY'
from pathlib import Path
from dev.performance.inference_workloads import _named_generic_constructors
root = Path('.work/tmp/xsh-23/witness')
root.mkdir(parents=True, exist_ok=True)
for name, source in _named_generic_constructors(512, 'independent').reduced_sources.items():
    (root / name).write_text(source)
PY
TMPDIR="$PWD/.work/tmp/xsh-23" perf record -F 4000 --call-graph fp -o .work/tmp/xsh-23/witness.data -- .work/tmp/xsh-23/target/release/xsht check .work/tmp/xsh-23/witness/main.xsh
perf report -i .work/tmp/xsh-23/witness.data --stdio --children --no-call-graph --percent-limit 0
perf report -i .work/tmp/xsh-23/witness.data --stdio --no-children --percent-limit 0 -g caller
```

The profile recorded 6,727 samples. In the `xsht-check` thread,
`BTreeMap<Name, Type>::insert` alone accounted for **24.20% inclusive** and
2.72% self. Its callers include `SemanticPools::to_type_inner`, which rebuilds
record fields in `src/runtime/eval/indexed/semantic.rs`; that owner accounts for
28.48% inclusive. Another map operation, `BTreeMap<Name, Type>::from_iter`
instantiated for `CompactLowerConstructProbe::canonicalize_checked_type`,
accounts for 23.39% inclusive. These percentages overlap, so they cannot be
summed. The first symbol alone exceeds the 5% threshold; an aggregate across
all B-tree stacks was not needed for the stop decision. Optimized inlining and
frame-pointer stack coverage limit attribution, and the result does not isolate
map machinery from work called by map operations.

## Flat profile

A second run of the same command (`perf record -F 4000 -g`, then
`perf report --no-children -g none --sort symbol`) puts the self time in key
comparison and map construction rather than in tree search:
`Name::cmp` 8.57%, `memcmp` 8.45%, `DynamicSpellings::load` 5.58%, B-tree node
construction and search 4.77% and 3.85%, `Vec<(Name, Type)>` collection 4.49%,
`BTreeMap::insert` 3.54%, and B-tree drop 2.48%. Name ordering by spelling
(22.6% together) costs more than the tree itself, and most of the tree work
comes from record field maps that `SemanticPools::to_type_inner` and
`canonicalize_checked_type` rebuild for every decoded type. A different
ordered map would not remove either cost, so the next step is to make `Name`
ordering cheap and stop rebuilding those maps; a map swap is evaluated only if
tree operations still dominate afterwards.
