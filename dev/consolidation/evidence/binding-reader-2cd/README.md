# Binding-reader checks

The reviewed reader repair is integrated at
`2cd2baa86d60ec5bbeb62e06fcc062159535450f`, tree
`c258e9153935347e26fcd5b51362e720677a9cd2`.
The checkout stayed clean through the verification build and native checks.

The lane's warning-free debug compile and 18 exact unit selections pass.
Their raw outputs are `unit-compile.log` and `unit-tests.log`. The lane commit
is `c73115cf5685edcaa34d7bf317ff39caf4359823`. Its checker files match the
integrated source byte for byte. These checks cover identity selection,
missing bindings, proof reads, command capture, stream classification, and
the seven existing reader filters. The ordinary nested-item probe was temporary.
Existing native tests retain ownership of that language behavior.

`verification-build.log` records the merged-source `xsh` and `xsht` build.
All runs use the pinned x86_64 Linux musl image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The build uses 8 CPUs, 16 GiB, no swap, Kache, mold, the current musl linker,
and Dockerfile.test's static-musl and isoc23 flags. Jemalloc is preloaded only
during compilation. Runtime uses UID/GID 1000 and no preload. `/scratch`
binds the repository's disk-backed `.work/tmp`.

The verification products are preserved under
`.work/consolidation/resume-candidate-2cd`:

| Product | Bytes | SHA-256 |
|---|---:|---|
| xsh | 31,593,528 | `74ee558fea6098b78becb30ec74cd8db9001d015b961e423615a0cf97575debb` |
| xsht | 38,740,320 | `779ec6ae29ca09ce4ef1f4600eebbd03f93f81b4ecf0c8d953a6246bb40c1a4c` |

`native.stdout` records four passing exact selections, one test each:

- `item-shorthand.xsh::test_implicit_item_reads_like_a_named_parameter`
- `item-shorthand.xsh::test_nested_callbacks_have_their_own_item`
- `item-shorthand.xsh::test_error_handler_takes_the_error_as_its_item`
- `lexical-blocks.xsh::test_bare_blocks_preserve_implicit_stream_item_values`

The final `2` is the constructor-precedence probe. It defines a record schema,
binds a same-named integer, calls the constructor, and checks the resulting
field. Baseline 2688 and this candidate both exit zero and print `2`.
`native.stderr` is empty. Product hashes are unchanged after these checks.

The shared native lock covers the four selections and the bounded probe.
These results establish no canonical-suite, W9, route-matrix, release, or
performance acceptance. Timing printed by tools is not a performance result.
