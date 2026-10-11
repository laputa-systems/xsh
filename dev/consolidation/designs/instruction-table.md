# Design: one instruction schema

Status: internal design established 2026-10-11. Implements workstream 1 of
`../CAMPAIGN.md`; it introduces no language or encoding contract.

## Authority

One declarative entry owns each instruction's variant, tag, ordered payload
fields, child ownership, verifier field constraints, effect classification,
and runtime listing. Generate construction rows, tags, encoding, typed
borrowed decoding, and mechanical field verification from those entries.
Rust's exhaustive matches enforce behavior coverage in consumers. The table
does not infer behavior from a catch-all or invent a second executable IR.

The existing 158 instructions, 18 patterns, and 46 stages are the initial
inventory. Preserve tag ordinals, optional sentinels, field order, locations,
and packed Text-pattern layout. Explicit custom field codecs belong to the
same schema entry; they must not leave an independent handwritten row mirror.
Stage and aggregate argument views borrow the verified payload and extra
tables. Decoding performs no allocation and exposes precise IDs and fields.

## Verification and execution

Keep the executable artifact private until verification succeeds. Generated
field checks prove payload bounds, typed child IDs, ownership, and schema
shape. Semantic checks such as callable identity, declared types, control
flow, and recomputed return behavior remain explicit. Sharing a semantic view
must retain independent verifier recomputation and corruption tests.

Execution destructures a typed instruction view. Remove its manual
`indexed_raw`, `indexed_decode`, `indexed_optional_raw`, and `indexed_finish`
sequences after the generated view covers their exact fields. No decoder may
silently supply a missing field or recover a malformed artifact.

Generate runtime support listing from the same operation inventory. Distinguish
native-test operations, script-backed operations, and specialized host
operations; a difference between old lists alone is not an unsupported API.
The coverage tool reads the generated observable listing rather than parsing
Rust visibility or enum source text.

## Acceptance

Retain encoding round-trip, malformed-payload, owner, cycle, callee, return,
and runtime behavior oracles. Require focused native coverage for previously
uncovered fields before changing their reader. Record structural count
reductions and allocation deltas, then run the shared integration gate.
Partition moves are measured separately from schema changes. No strict new
timing threshold is introduced before functional close.
