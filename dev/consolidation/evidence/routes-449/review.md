# Route evidence review: `routes-449320`

## Finding

**GO for the 184 Expr route tuples in this receipt.** The existing collector accepts exactly the 92 mapped Expr tags in normal and forced modes. This is route-witness evidence only; it does not establish full-native acceptance or route coverage for the 60 non-Expr mappings.

## Checks

- Receipt `.work/final-review-evidence/final-correctness-gates/routes-449320/receipt.json` is `completed`, has 184 unique steps, 184 successful non-timeout results, one artifact and one logged output per step, and 368 clean source checks.
- Source freeze is HEAD `449320b1844cb5c7cc361df2fe27e387b37b12d1`, tree `b1f2632d9fc3b48ceb5747843a6631db6e0aeb1c`; the checkout is clean and every before/after check matches that freeze.
- Schema and mapping validation found 152 schema rows/mappings and 92 Expr child-source mappings. For every child mapping, the retained source-byte, ID, tag, ordinal, route-policy, and artifact validators accepted both modes.
- Each tuple has the exact `xsht test --exact <ID>` tail, the correct forced marker, a matching artifact and log digest, and a one-record witness matching test ID, child hash, tag, ordinal, expected route, and mode. Container commands use the pinned `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c` image, network `none`, and read-only source/product mounts.
- Receipt and on-disk verification products agree before and after: `xsh` `7fa9a4e517e7375af102b1e539429d1ba055201b2018f0028a8cbb079576567a`; `xsht` `7fb101044a56b27be71debb5283a88302e244555c1b1eec46110098b00835267`.
- The existing `route_witness_receipts` collector returns exactly 184 tuples: 92 normal and 92 forced. It accepts these records without validator changes or relaxed checks.

Driver SHA-256: `9b76d6503a5bbd9653d0bb57de0f71151ba5c945a4fb97659eb70cdad40ddafe`. Mapping TSV SHA-256: `d0f4084f9f3eccbdb7192cdc815ce47858156cd2eb00043bb99c9f0ac9f75e7b`. Receipt SHA-256: `23886161b4543e79a720d0154a49c079c6ef5d2afa44770cc63c069ccb88e44b`.

No route replay, build, or tests were run during this review; I used the existing pure receipt and artifact validators and read-only checks.
