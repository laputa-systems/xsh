# R5 resource observer interruption

This archive preserves the interrupted R5 attempt for clean source commit
`b1bc741fcd512d1c71b43116853278a4a3f1521a`, tree
`6d2a7d2fc8506ca5de8e312ded27b5548385116e`.

The isolated repository-lint stage completed. The canonical
`cargo dev gate --batch` stage recorded exit status 3 after
`lint_format_invariance::layout_perturbation_preserves_lints_on_the_repository_corpus`
failed. The ordinary formatter-preservation invariant passed. The root sent
SIGINT while the embedded desugared-corpus test was running. The full
`test-xsh` suite had not started. R5 remains interrupted and earns no resource
or timing credit. The additive cleanup files do not rewrite the original
receipt.

The resource capture recorded a cgroup memory peak of 4,075,487,232 bytes and
a sampled process RSS peak of 3,231,641,600 bytes. It recorded zero OOM,
OOM-kill, and swap events. These diagnostic measurements do not count as a
resource pass. Timing was discarded.

Cleanup confirmed that the container and process group were absent. It
confirmed that the original Cargo, performance, and native lock inodes were
unchanged and free. It also confirmed that the native cleanup marker was
removed. The archive preserves the first native-marker bytes and container
ID.

[`r5-raw.tar.gz`](r5-raw.tar.gz) contains the original attempt and stage
receipts, stage stdout and stderr, the canonical resource capture and Docker
command records, and the cleanup results and marker snapshot. Each archived
file matches its preserved original byte-for-byte. The archive contains no
build products or caches.
