# W9 formatting memory diagnosis

The never-merge diagnostic starts from reference source `e240` and preserves
the original input, default profile, one CPU, 1 GiB memory, and no swap.

The marker build is recorded in [pretty-build](pretty-build/source.txt).
The [original-format run](pretty-oom/stderr.log) reaches the checker return
and diagnostic formatting, then dies before full pretty-Debug formatting
finishes. Docker reports exit 137 and an OOM kill. The cgroup reaches its
1,073,741,824-byte bound.

The [compact diagnostic](compact-diagnostic/stderr.log) reaches row-ready and
exits 0 at the same cap. Its peak is 902,017,024 bytes with no OOM kill.
The run keeps a named raw Debug string live longer than the original helper's
temporary. It is a diagnostic experiment, not an accepted helper or capture.

Pretty and compact whitespace-normalized hashes differ. Pretty formatting
adds trailing commas that the canonicalizer removes; that explanation still
requires qualification. This run supplies no publication-equivalence proof,
full-corpus parity, timing result, or acceptance credit. Large generated raw
records are not tracked. Receipts, marker logs, source and executable hashes,
commands, statuses, and cgroup observations are retained here.
