# Inference schemes and levels

These checkpoints remain isolated on `campaign/typing-rank1-integration`.
They do not enable omitted required parameter annotations on main.

Pre-fix `6a3b8f462fb7ffd20d3d1729a1c42cc1db22fa0b` reproduces three failures:
union leaves inherited record descendants above the outer level; the shared
recovery predicate misses Set elements and Module exports/signatures.
The corrected Module fixture uses the required SymbolOwner scope. Its failure
is in the predicate assertion. Scheme constructor rejection controls already
pass; an earlier review claim that the private scheme predicate omitted Set
was incorrect and is withdrawn in the retained review.

Fix `121d39242b9e7066a5a4dde8dbb15fb1336d022a`, tree
`9b5fa46779b2324ca9747ff290be1273800ed267`, lowers inherited bound record fields
and tails directly, extends the shared recovery predicate, and removes the
duplicate scheme traversal. Root library no-run and XSHT all-target checks
exit zero. All 41 constraint tests and nine exact probes pass without an
allocator preload. Source and harness identities match before and after.

The root independently verifies the harness SHA-256
`4b6efd2a2d595231b969317ee3a8e709438df48b3f69be092a07bbf49b93246e`.
Independent static review approves the source; a dedicated descendant-level
rollback test is an optional coverage improvement. No performance claim is made.
End-to-end scheme publication, immutable body facts, imports, lowering, and
the capability witnesses remain work for the ongoing implementation slice.
