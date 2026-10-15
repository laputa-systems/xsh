# W9 build qualification at 6198

Source: `619810fb96ca5dbcb4b521012e6fe5a43d0eb0c2`, tree
`43822131f809f792feb33c8b794872a77fadb896`. The source receipt binds the
recovered 1,690-file cohort and pinned builder, linker, image, profile, and flags.

Fresh reference and candidate adapter builds pass, including Cargo artifact
selection and harness listing. Their schema-2 build receipts are retained here.
The reference is e240, and the candidate is 6198. Both bind the same source
receipt. Builds use 12 CPUs, 24 GiB with no swap, and no network. Actual CPU
and memory use were not sampled. No timing result is claimed.

The exact-owner reference build fails with warnings denied. Its generated
helper contains three unused imports: `Read` inside the test function,
`OFlags` inside directory hashing, and `OsStrExt` inside directory hashing.
The complete error and status are retained. No exact-owner build receipt or
binary is produced. The repair must preserve the historical helper source pin.

The adapter capture is stopped with SIGINT because that repair requires a
new qualified source freeze. The runner exits 130 and records incomplete
status. Four reference ranges finish, with 400 profile outcomes. Four more
ranges remain partial, and the candidate never starts. Top-level completed
ranges remain zero. Cleanup finds no owned workers or Docker containers.
The complete CLI output and status are retained. Partial raw files remain
disposable under `.work/consolidation/w9-final-6198/adapter-capture` and grant
no capture or comparison credit.

The source and reference remain clean at their pins through build completion
and capture cleanup. This attempt establishes no parity result. A fresh build
and capture on the repaired freeze remain required.
