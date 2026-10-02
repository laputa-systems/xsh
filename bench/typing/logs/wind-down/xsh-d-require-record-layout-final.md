Owned files: src/runtime/eval/require.rs, tests/xsh/stdlib/json.xsh, docs/JSON.md.

Record validation produces a RecordVec with the exact prepared-schema prefix and retains all extra fields and decoded nested values. Schema/type checks reject reordered physical layouts. Compact Stats and StatsBlob fields retain their visible values. Existing nonempty FsEntry decoder rejection is unchanged.

Verification: three owned decoder tests passed after actual failures were captured. The original inferred-require frontend-drop test passed on both runtime routes. Targeted debug xsh/xsht builds passed.

Native verification is pending: the canonical JSON module fails existing source checks; the same new native test isolated in a temporary tests directory fails generic_evidence_verification before execution. Existing wire-enum preparation also fails before decoding because a direct native argument disagrees with its selected signature. No native or broader runtime acceptance is claimed.

User requested wind-down. No mixed encode_lines production fix or source test was started. The exact baseline/current source reproduction and root cause are recorded in the JSON handoff. Original baseline-v2 accepts the mixed literal and emits four JSON lines; current source checking trains the first Int item and rejects Str/Null/Bool.
