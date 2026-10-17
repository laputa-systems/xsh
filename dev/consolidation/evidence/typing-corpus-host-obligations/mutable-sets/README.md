# Mutable Set corpus proof

The corpus source stays at `03b0aad7394f351408a72c2360a23e9fbf936d18`, tree
`51787d9420c922484a8a779a57d3f2357aea82e3`, through both the 020f native guard and
the 76caa check guard. The later archive commit changes proof and documentation
only. The 9,194 frozen AST sites, 693 eligible locals and 2,190 eligible internal
annotations retain their original categories.

## Set candidate

`prepare_mutable_sets.py` selects all 109 eligible mutable empty Set initializers
from the accepted 877 retained table. The candidate removes 457 locals and 529
internal annotations, totaling 986. Internal counts remain 165 parameters and
364 returns. All 202 effects remain explicit. Schemas, public interfaces, entry
contracts and source algorithms are unchanged. The fixture-only real-flock
correction outside the 48 program sources applies to both views.

The original view contains 14,287 entries with digest
`e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899`.
The 986 reduced view contains 14,287 entries with digest
`43ad0beddeb95ef7f695598664156df29605831df3156433a11ea356d39d8c35`.
The original 877 reduced view remains unchanged.

The top-level `original/`, `reduced/`, `state/` and `run-check.sh` retain the failed
7491228ab attempt. Original checks pass. Reduced checks reject 102 XSH and seven
Laputa empty Set initializers. The runner's `1 files` summary typo is preserved.
Its failure is separate from the actual reduced compiler status 2.

`020f-check/` retains the fresh successful ordinary checks after the seed-context
repair. Both views check ten XSH roots and one Laputa root. All four batches exit
0 with an actual lower-stage summary and no diagnostics. The unchanged candidate
receives no annotation restoration between the failed and successful attempts.

`020f-native-26/` retains the actual native run and hardened runner. Each view
selects and passes thirteen exact witnesses. All 26 invocations select exactly
one test, pass one, fail zero and skip zero. The wrapper exits 0. Source, products,
selector and digest-helper hashes, both complete view digests and native-lock
inode guards match before and after. Both exact owned container IDs are absent.
The original lock inode is 62012470. The lock is free after process exit.

The execution compiler is `020f9c0c799a0e17c2b0006ced779e416a3a8052`, tree
`1f67a4262727c8779e3135bf0fa49132ef9f7c12`. Verification products are read-only:

| Product | SHA-256 |
|---|---|
| xsh | `171025d26de98b7eafed618e0ddbf0e8bbfc0792030fa6c4c09c8a16fa9e772a` |
| xsht | `4060fbfee92d74c9492465a1c95dbe6a70a0cde1fd25d4f8dfc21a7eb097623f` |
| helper | `5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d` |

The native runner SHA-256 is
`f8c8e0122558643692602f6219ef2123b66ad427c10d373367c2ff4c701b7394`.
Its selector authority is the archived `witnesses.tsv`, SHA-256
`a6e8c5aa651584592a800edc7d9b1983e11e71bf0c15b686c137a3ea8fcd0102`.
The archived `view_digest.py` SHA-256 is
`0212c5bb08d7faa5948fdeb6f26cc1329440bf4532d3473d896ab0be6b8a0154`.

Execution uses pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
UID 1000, no network, four CPUs, affinity 12–13, 8 GiB memory and no extra swap.
The native selector deadline remains 120 seconds. No timing or broad corpus
acceptance follows from this run. The existing assertions compare outputs,
failures, operation order, target propagation and cleanup. They do not exercise
all 109 accumulators independently. Full accepted removal credit remains 877
pending the Set feature's API fixture correction and documentation gate.

## Unqualified collection plan

`collection-plan/prepare.py` reproduces the separate 54 List plus 25 conditional
or mutable-null annotation removals atop 986. Source validation protects all
48 loaded source transformations and all 149 Map annotations on `{}` initializers.
The plan contains 536 removed locals and 529 removed internal annotations,
totaling 1,065. Its 14,287-entry digest is
`b3c7baba21066818d06b42b7e7e6ab7c7745fb5b7ae0e878ee0dbe4de7743b17`.
No behavior run or successful whole-candidate inference claim exists for it.

`collection-plan/020f-red/` preserves reduced XSH status 2 with 93 diagnostics and
Laputa status 0 with ordinary lowering. The unchanged annotated 020f result is
in `020f-check/original/`. `collection-plan/76caa-red/` preserves a fresh original
11-root pass, reduced XSH status 2 with 92 diagnostics, and reduced Laputa status
0 with ordinary lowering. Both trials use two CPUs, affinity 12–13 and 8 GiB with
no extra swap. They take no native or Cargo lock. All source, product and view
guards hold, and every owned container is absent.

The latter compiler is `76caa6920b43861df386d4f208a3a1bdff2f9974`, tree
`8d032d81485eb0dd1d01603d38aa7827ae1ebf50`. Its xsh hash is
`b8b6f9530dc1c5a97ac9e1469148cf1d4a6b23f8715e0a844b1a0a2791ba5179`;
its xsht hash is `7d1344167e20651887dbe093f34724939ff56f2fb6babab0265c3ac2bf9ede05`.
The helper hash is unchanged. The literal pins remain in each actual runner.

`diagnostic-delta.tsv` removes only the Path/RelPath mismatch at site 2963,
`sys_mount.usage_path`, and adds no diagnostic location. The 92 remaining
locations retain their original function and candidate-site associations in
`diagnostic-sites.tsv`. Those associations identify candidates in the same AST
function, rather than independently proving each site's necessity or cause.
No restoration, denominator change or additional removal credit follows.

The user retains `{}` as a record. All 149 Map annotations on `{}` initializers
stay required inside the original eligible denominator. The maximum possible
local score under that rule is 544 of 693 (78.50%); the 95% target remains unmet. The 85% internal
target also remains unmet. Individual effect proofs do not establish joint
elision authority, so all 202 effects remain explicit.

All archived raw files and runners were compared byte-for-byte with their scratch
originals during archival. Previous failed attempts remain unchanged.
