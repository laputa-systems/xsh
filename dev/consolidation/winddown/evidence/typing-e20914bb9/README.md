# Parked first-slice observation

Source is e20914bb9c9091cab36bfe219184e2f9dbb5dca1, tree
1cae8ed274160dcb0df3e4168e28012ff1681647. The source tree is preserved by
campaign/typing-pipeline-integration; its later e2cf1fec2 commit adds a handoff.

The selected Rust group passed 286, including 215 FullVerifier cases, 47
constraints, nine schemes, eight frontend statistics, six metadata and one
explicit prepared execution selection. The prepared test is also in the
verifier group; these are selections, not necessarily unique tests. Its raw
stdout is retained here. Both indexed routes execute after frontend disposal.
This does not prove the newly required module-value ownership control.

Native results are 19/20, with all six named tests present in the selectors.
The importer-independence case fails the exact module export set check;
reading a value exposes the missing quantified pure exports. The separate rank1 imported-call control and ordinary identity/projection,
diagnostic, annotated and dynamic controls pass. The strengthened module-value
importer witness fails separately as well as jointly. The complete native child streams and selected Rust summary
remain byte-exact. This is not accepted first-slice qualification.

The three shell files are the actual historical build/run commands. Recreate
the isolated worktree before reuse, requalify product paths and hashes from
actual Cargo output, and choose new output directories. Their old paths are
observations, not a current executable approval. The raw selection-count file
contains literal separator escapes; selectors and per-test summary are the
records to inspect. No failed output was normalized or rewritten.
