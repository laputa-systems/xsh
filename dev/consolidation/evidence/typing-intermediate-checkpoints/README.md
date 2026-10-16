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
