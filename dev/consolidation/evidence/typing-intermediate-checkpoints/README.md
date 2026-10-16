# Isolated inference intermediate checkpoints

These results belong to the inference branch. No inference implementation is
merged into consolidation main, and no end-to-end inference acceptance is claimed.

At `aef756e1a4dfbfdf6bd36c43c29f8637206e8bd3`, the retained-byte probe and both
then-existing scheme tests pass. The estimate includes owned names, row lacks,
schema vectors, and shared children without double counting within a scheme.
It estimates retained payload rather than exact allocator RSS: allocation headers
are omitted, names count per occurrence, and callers own cross-scheme Arc deduplication.

At `81481a573c0eae9095190ca2c7177f178385a38c`, tree
`f4df2139fd60ad46bcbca9a66cec418c4f79b546`, all 41 constraint tests and three
focused checker probes pass. The pinned musl XSHT all-target compile passes with
warnings denied. Source stays clean and the harness digest stays unchanged.

The scheme module has three passes and one failure. Its new map regression puts
a ground map inside one Ground leaf while its callback rewrites individual Int
leaves. The intended fixture is a SchemeType map containing two Ground leaves.
[Independent review](review-814.md) finds that the changed two-child mapping arm
and existing importer recursion meet their intended source contract. The scheme
gate remains open until the fixture is corrected and rerun.

The reviewed intermediate boundary stores one canonical scheme and closes the
Pending-to-Checked transition. Local provisional calls remain distinct from
generalized instantiations. Parser omissions, immutable generic body facts,
lowering, and capability acceptance remain unfinished. The retained loop review
distinguishes the intentional pre-fix red checkpoint from later green sources.

## Row evidence and export visibility

Source review of `afceaec9d` finds two gaps: row metavariables can retain an Any
payload, and the qualified scheme index includes private declarations. The
checkpoint also fails compilation on missing imports and a wrong type path;
`0e4f4c970` repairs those direct compiler errors.

Test-only `9ea2333e0` reproduces both Any failures: row binding returns success,
and RowEvidence accepts a nested Set(Any). The initial container fails before
Cargo because its read-only worktree lacks the target mountpoint. The corrected
launch builds and runs the actual red tests; both fail as expected.

At `935d7da01`, the actual nonfresh library build succeeds. Both exact Any
regressions pass, all 42 constraint tests pass, all five scheme tests pass,
and the synthetic compact export-index test passes. RowEvidence now validates
its private immutable payload once; CallInstantiation trusts that invariant
and validates its own binder correspondence. Explicit Any remains legal in
nonquantified ground signatures.

The real exported-identity probe fails first on an unrelated private helper's
unannotated signature. `fb537001b` annotates that helper to isolate visibility;
the private inferred identity capability remains required and unfinished.
[Independent review of 935](typing-935-source-review.md) finds that finalization
reads visibility after the Export wrapper restores it, so the export handoff
remains HOLD. The synthetic test alone cannot close it. `3edce3ab6` also has
an earlier compiler failure from assigning the compact scheme map to CheckOutput;
that failure and its source review remain separate. No generic feature or
export-visibility acceptance is claimed from these partial checkpoints.

At `e63689e1d`, export visibility is captured on the pending declaration's
identity and passed explicitly into checked publication. [Independent source
review returns GO](typing-e636-source-review.md). Compilation passes, and
the source-based probe failure is recorded below. The next lowering step
has a retained plan; implementation waits for that probe to pass.

## Source-based module export failure

The `e63689e1d` declaration-visibility repair compiles. Its actual module-source
probe fails on an unsupported constant initializer, missing documentation, and
an unresolved generic body expression. Test-only `534db019e` corrects the
initializer and documentation. After a real recompilation (`fresh:false`),
the exact probe still fails solely with `check.local-inference` on `value`.
The runtime uses the pinned Linux image without preload, and source and harness
identities match before and after.

The checker retains a symbolic scheme body fact while also finalizing the same
expression through its ground-type table. This ownership conflict is under
repair. The failed probe gives no lowerer, import, or feature acceptance credit.
