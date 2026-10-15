# W9 builder integration failures

Both attempts used a real schema-3 source-cohort receipt for the recovered
1,690-input corpus and invoked `w9_publication_adapter_runner.py build-harnesses`
with the pinned e240 reference. The qualified wrapper used pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
x86_64 Linux musl, verification profile, 12 CPUs, 24 GiB, no swap or network,
Kache, mold, static CRT, and the two isoc23 aliases. Preload applied to
compilation only. No capture or test corpus ran.

| Candidate source | Observed result |
|---|---|
| `77c545f42d51c57e7a76f24b6f10c3b6305bdd30`, tree `71e2f4f874323a344ab91b6e838393727b3f8e99` | Reference harness compilation failed because a generated helper imported an unused `OsStrExt` trait. Warnings remained denied. |
| `113a9ee7e3e581d1faf84a8e294a94e76f3a139f`, tree `b42ef8f5a14970199c166d4b0259d5fa8db759e9` | Reference harness compilation succeeded, then artifact validation rejected its path in the qualified shared target directory. The historical selector requires an artifact inside the source root. |

The one-line import repair is committed at `113a9ee7e`; all 65 adapter Python
tests pass afterward. The second failure requires a strict artifact boundary
for the modern builder's exact target directory while preserving historical
validation. Both complete failure outputs are retained here. Neither attempt
produced an accepted build receipt or established W9 parity. Candidate and
exact-owner harness qualification remain open. No timing result is claimed.
