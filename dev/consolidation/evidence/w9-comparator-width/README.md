# W9 comparison worker memory

The comparator retains its three-worker cap and pending results stay bounded to one batch.
Results and exceptions retain input-index order across batch boundaries.
Each worker receives at most one input per batch and compares all four profiles.
The active comparison controller and its captures remain unchanged.

The parent disables cyclic collection while workers own the inherited context.
Each child freezes its inherited objects and restores the caller's collection policy for new objects.
The parent restores its original collection policy on success, error, and cancellation.
The parent's frozen objects remain unchanged.
Worker termination releases inherited objects when the process exits.
The parent's scheduler creates bounded batches of acyclic protocol records.

Cancellation signals stay blocked during process creation and handle registration.
Each child restores the inherited signal mask after installing its signal handlers.
The parent restores its mask after registering the child.
Existing abort, kill fallback, join, pipe-close, and handler restoration rules remain in force.

## Functional verification

[The raw unittest output](unittest.txt) records 64 passing tests:

```sh
python3 -m unittest \
  dev.consolidation.test_w9_publication_adapter_runner.SourceQualifiedAdapterReceiptTests \
  dev.consolidation.test_w9_publication_adapter_runner.AdapterCorrectedReferenceInputTests \
  dev.consolidation.test_w9_publication_adapter_runner.DerivedCaptureClaimTests \
  dev.consolidation.test_w9_publication_adapter_runner.AdapterReferenceReuseTests
```

The wider cases cover strict serial equality, mismatch bytes, both exact-owner roles,
input order, three concurrent children, 19 unique assignments, and counts below three.
Startup failure and cancellation cases check child reaping, pipe closure, signal masks,
signal handlers, and parent collection state.
New child cycles remain collectible after inherited objects freeze.

## Resource diagnostic

[The memory diagnostic](memory_probe.py) retains all 1,690 exact-owner maps and assignment proofs.
It calls the official per-input comparator for the eight largest candidate publication inputs.
The diagnostic preserves original input indexes, four profiles, owner projections,
assignment bindings, and original native-hash pairs.
It uses the admitted captures and native hash logs from the frozen `0dc9cff7c` cohort.
It does not rerun complete exact-owner or native source-audit admission.
It creates no parity receipt and grants no source acceptance.

The diagnostic runs in the pinned verification image with eight CPUs, 16 GiB,
no swap, no network, a read-only repository mount, and a writable lane scratch mount.
Host and image use the same Python 3.14.8 executable bytes.
The first attempt rejected source admission because container Git lacked the host's global ignore file.
The corrected attempt mounts that ignore file read-only, matching host Git behavior.

The rejected eight-worker diagnostic reached the 16 GiB cgroup limit.
[The raw samples](eight-worker-memory.txt) report five OOM kills.
[The failure log](eight-workers.txt) reports the lost worker outcome.
[The Docker status](eight-worker-status.txt) records exit 1 and OOM termination.
The comparator aborted and reaped its remaining workers.
The three-worker regression diagnostic is pending.
No throughput improvement is claimed.

## Remaining acceptance

Eight-worker scheduling is rejected under the 16 GiB budget.
The original three-worker bound remains in force.
The next complete admitted comparison still requires its resource acceptance.
The next controller freeze requires fresh source-bound captures and admissions.
Existing receipts bind their controller identity and cannot be relabeled for this source.
The full comparison still requires all 1,690 inputs, 6,760 outcomes, unchanged strict
publication equality, exact-owner proofs, source audits, and assignment-facts proof.
No full comparison or capture is authorized by this diagnostic.
