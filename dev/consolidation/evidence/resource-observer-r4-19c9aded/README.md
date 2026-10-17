# R4 resource observer interruption

This archive preserves the failed R4 attempt for clean source commit
`19c9aded0ea42c979fbc6be32162320d29ded3b1`, tree
`956ea0056c53b5f006ede4fd980fd7ae364970e2`.

The isolated repository-lint stage completed. The canonical `cargo dev gate --batch`
stage recorded exit status 3 after
`lint_format_invariance::formatting_preserves_lints_on_the_repository_corpus`
failed. The root then interrupted the run with SIGINT. No full XSH native
suite started. The original receipt remains interrupted, and R4 earns no
resource or timing credit.

The resource capture recorded a cgroup memory peak of 3,429,998,592 bytes and
a sampled process RSS peak of 3,170,050,048 bytes. It recorded zero OOM and
OOM-kill events. These diagnostic measurements do not count as a resource
pass. Timing was discarded.

The later cleanup reconciliation proves the container absent, clears the
performance marker, and confirms the original Cargo, performance, and native
lock inodes remain unchanged. It preserves the first unresolved container ID
and lock-marker snapshots. Cleanup did not rewrite the original receipt.

[`r4-raw.tar.gz`](r4-raw.tar.gz) contains the original R4 and stage receipts,
stage stdout and stderr, the canonical resource capture and Docker command
records, and the additive cleanup receipt and snapshots. Each archived file
matches its preserved original byte-for-byte. The archive contains no build
products or caches.

The later corpus fixture-discovery repair is recorded in the
[successor functional close](../successor-b1bc741f/README.md). That focused
red/green result is subsequent evidence. It does not change R4's failed status
or grant R4 resource or timing credit.
