# Modern exact-helper projection checks

Merged repair: `5347f665a`. The two-file Python change adds 245 lines and
removes 12. Product Rust and the approved exact-owner helper are unchanged.
The helper source retains SHA-256
`33c77273d9f9096d3e6478622d0b29a3f0f1f79dee772cdea12b06f2ac8200d8`.

Source-cohort builds remove three known unused imports from a compiled copy
of that source. Historical builds use its original bytes. Build, capture, and
proof validation select the same payload, reject unknown schemas, and check
the combined helper hash. The source path and hash keep their original meaning.

All five merged-source Python suites pass:

| Module | Tests |
|---|---:|
| Publication runner | 51 |
| Adapter runner | 67 |
| Exact-owner runner | 53 |
| Reference correction | 8 |
| Source builder | 2 |

The complete five logs retain all 181 results. Projection tests preserve the
used `Read` imports and reject missing or duplicate import snippets. Artifact
tests reject an unprojected modern suffix, a projected historical suffix, and
an incorrect combined hash. Shell syntax and diff checks pass.

The [failed real build](../w9-preflight-6198/README.md) remains retained.
These pure checks establish no repaired Rust build or capture result.
