# Frozen 0dc9 supplemental reference record

This is a reference copy of the approved supplemental run, not a new execution. The receipt and selected combined logs were copied byte-for-byte from the ignored attempt at `.work/final-review-evidence/final-correctness-gates/correctness-0dc9cff7-20261016-r1/`; the R6 manifest was copied from its sibling output directory. Paths inside the receipt still point to the original attempt. No build targets or binaries are included. The R6 manifest keeps its original filename; the receipt's `source_expected` and `tool_expected` fields bind this run to commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`, tree `b4f9db3f8ee2f38a6c62fea876976619db424ed7`.

The receipt records clean source and tool checks and exit code zero for all 13 steps. The verification product build, three harness builds, warning-denied workspace Clippy, xsht staging, and five Linux contexts passed. The five contexts reported:

- `core-compat-boundaries`: 23 passed, 0 failed.
- `core-compat-extra`: 58 passed, 0 failed, 3 ignored, 2 filtered.
- split context: 2 passed, 0 failed, 61 filtered, under the planned 0.01 CPU and 1 GiB memory limits.
- privileged cp context: 2 passed, 0 failed, 88 filtered, with the actual `.work/tmp` mounted as the required 1 GiB nodev tmpfs.
- uid-1000 traceroute context: 18 passed, 0 failed, 2 capability skips.

The receipt status remains `commands-passed-needs-privilege-skip-and-close-review`; it was not rewritten after review. Root's separate manual review confirmed the two traceroute skips are expected because the uid-1000 container lacks `CAP_NET_RAW` and raw ICMP access. Root also independently verified seven QEMU cases on the 0dc9 source and disk `951eb22d`: three XSH date tests, three GNU date tests, and one BTRFS reflink test passed. That review satisfies the supplemental privilege and QEMU skips; canonical, W9, route, and resource closure remain pending.

The receipt records that product hashes remained unchanged across harness builds and tests. It binds the verification binaries, harness variants, xsht source and staged destination, pinned `x86_64-unknown-linux-musl` image, resource limits, and exact commands.

The retained run inputs and qualification outputs are:

- [Exact run receipt](receipt.json)
- [Approved R6 manifest](manifest-correctness-bb809e1a-20261016-nodev-tmpdir-r6.json)
- [Product build](logs/002-build-product-01-xsh-xsht-xshi-xsh-fuzz.log)
- [Boundary harness build](logs/003-build-harness-core-compat-boundaries.log)
- [Extra harness build](logs/004-build-harness-core-compat-extra.log)
- [Privileged harness build](logs/005-build-harness-core-compat-extra-privileged.log)
- [Workspace warning-denied Clippy](logs/006-warning-deny-01.log)
- [xsht staging](logs/007-stage-core-compat-extra-xsht.log)
- [Context 1: compatibility boundaries](logs/008-linux-special-01-core-compat-boundaries.log)
- [Context 2: compatibility extra](logs/009-linux-special-02-core-compat-extra.log)
- [Context 3: split](logs/010-linux-special-03-core-compat-extra.log)
- [Context 4: privileged cp](logs/011-linux-special-04-core-compat-extra-privileged.log)
- [Context 5: unprivileged traceroute](logs/012-linux-special-05-traceroute-unprivileged.log)
