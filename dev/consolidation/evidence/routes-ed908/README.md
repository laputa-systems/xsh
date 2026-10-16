# Expression route acceptance at ed908

All 184 source-pinned probes pass: 92 expression tags, each in normal and forced
mode. Each execution emits exactly one authenticated witness artifact. Independent
review verifies all mappings, IDs, modes, routes, ordinals, source and artifact
hashes, logs, and 368 clean source checks. The existing route collector accepts
all 184 tuples.

Source is `ed90857bdd825439159b7d788ea5d57cd9652920`, tree
`efc56a4a81c7993e35b2fd76a2549742eb8d8847`. The receipt binds actual verification
products before and after. This closes the mapped expression route gap only;
it gives no full-native, non-Expr route, or source-order coverage credit.

The first 184-run attempt also passed its probes but used a binary path outside
the collector's required target layout. Its independent HOLD is retained.
The corrected launcher mounts the same verified products read-only beneath
`/artifacts/target`. These are fresh executions with fresh receipts; the old
receipt was not edited. The retained driver SHA-256 is
`9b76d6503a5bbd9653d0bb57de0f71151ba5c945a4fb97659eb70cdad40ddafe`.

The intervening privileged-feature build changed the plain xsh artifact. A
cached default-feature Cargo build restored its exact original hash before
the accepted replay. The accepted receipt verifies that restored hash in every
probe. No performance claim is made.
