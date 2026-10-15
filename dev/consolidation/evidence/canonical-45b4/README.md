# Canonical attempt at 45b4

The clean source freeze is `45b4c1cd8195f0f341c077a00a7b09dfa8c144ee`,
tree `242bab51de7abddee264170235c485658dfb354c`. The pinned Linux musl
batch exits 3. The receipt records the source checks, actual product paths
and hashes, container limits, and cleanup. The logs are copied verbatim.

- Process tests: 305 passed, four ignored.
- Debug library: 741 passed, three ignored.
- XSHT integration: 116 passed, five filtered. The complete desugar
  comparison passes, including the repaired AWK/curl fixture interaction.
- Soundness: 13 passed, one failed. The generated replacement-label test
  accepts the seed-4 program, then fails because its source has no `.replace(`.
  The failure is at `crates/xsh-fuzz/tests/soundness.rs:203`.

The privileged Rust, normal full native, repository, and documentation stages
are not reached. The batch has no timing or final acceptance credit.
The resource record reports no OOM or swap events. Inference compilation
overlapped test execution in a separate container.
