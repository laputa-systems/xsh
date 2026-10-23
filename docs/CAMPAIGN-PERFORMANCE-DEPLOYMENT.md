# Make XSH fast to run and dependable in Laputa

Status: active, 2026-10-17. The owner authorized bounded isolated edits,
local commits and integration, pinned fetches, and owned Docker/KVM operations.
Pushing remains forbidden.

The owner's 2026-10-21 direction authorizes incremental merges to `master`
and removal of completed, integrated worktrees. Unfinished compiler work stays
on `campaign/full-inference-integration`. Pushing remains forbidden.

The owner's 2026-10-23 closeout order retains inference qualification, local
`master` integration and cleanup before Pyramid closes. The subsequent request
expands independent Pyramid implementation now, alongside inference, using five
expert lanes and one shared two-build limit. Pyramid qualification still waits
for the accepted inference source. Its isolated integration branch is
`campaign/pyramid-integration`; no pushes are authorized.

Provisional inference source `a1fa6bcb2` integrates scoped mutable argument
proofs, deferred sort-key checks, closed Null returns and pure-return validation.
Focused controls pass. The actual historical system-report checker goes from
two sort errors to zero. The mutable argument repair passes twelve controls
plus strengthened writer positives. The `wc` boundary now authenticates the
exact ground callee's physical UInt parameter validation while proving the
caller's actual stored type and all writers. Sixteen focused controls pass,
including signed runtime rejection, missing/wrong guards, forged initializers
and charged validation of a wide cached source. Pure Str/Error joins now fail
during checking, while typed Str results and nominal errors remain valid.
The shared Optional return relation now accepts a payload or Null under its
matching Optional scheme while rejecting foreign binders. Arithmetic call
arguments derive evidence from their physical operands through the existing
producer proof. Its twelve focused controls pass; the exact historical gates
and internal modules prepare and verify; the exact historical `wc` module also
prepares and verifies. The shared Result return relation now preserves symbolic
success binders while applying existing error-family widening; ten focused
controls and independent review pass. A scoped integer producer bridge reuses
the already verified same-owner arithmetic result, with fifteen focused
controls. The exact historical date-parser module now fully prepares and
verifies. Broader joint corpus checks remain open.

List compound assignment now relates an unresolved target to a known List
right operand through the existing constraints. Eight focused controls pass,
including both execution routes and retained unsigned checks. On the actual
broader system-report candidate, all compound-operator errors disappear; five
generic-body eligibility errors remain. Those identify missing nominal variant
producer evidence and are not a module pass or new removal credit.

Matching verification products for frozen `a1fa6bcb2`, tree
`ec65b4deec4eadffc83d062e11bae93609c0ea20`, are building in
`.work/wt/full-inference-historical-product-qualification/` for the next corpus
replay. No result is accepted until that owned command terminates and its
source/products are checked.

Frozen source `f1a8e1a4c`, tree
`faba8d431c89c211076b5e8155ff21b2ba280b91`, passes the complete library command:
2,235 passes, zero failures and three ignores. The owned command exits 0 and
cleanup completes. The source stays clean. Its log is
`.work/wt/full-inference-arithmetic-library-qualification/.work/tmp/arithmetic-full-library.log`.
This source includes Optional and arithmetic-argument repairs, and excludes
the later `wc` proof change. Matching-product native and corpus qualification
remain pending.

Frozen source `bdb9b165b` completes the full library command with 2,232 passes,
one failure and three ignores; the command exits 101. The sole failure expects
Null data returns to remain rejected, contrary to the campaign's explicit
return rule. Correction `4b4e4cf57` replaces that obsolete expectation with
typed Null calls, retaining its other rejection controls; the exact test passes.
The failed full run remains evidence, not a successful full-library gate.
Its log is
`.work/wt/full-inference-domain-library-qualification/.work/tmp/domain-full-library.log`.

Matching `bdb9b165b` verification products build without warnings. The Null,
deferred-sort and two pure-return selectors each pass once in normal and
forced-frame modes: eight passes, zero failures or skips. Matching products and
native logs are under
`.work/wt/full-inference-domain-product-qualification/.work/`.
These products exclude the later Optional and arithmetic repairs. The XSH
product is 42,455,328 bytes; XSHT is 50,810,944 bytes. No speedup is claimed.

Frozen source `a6178b9f5` has matching warning-free verification products.
Its new Null-return native selector passes once in each normal and forced-frame
mode, with zero skips. Logs and products are under
`.work/wt/full-inference-null-pattern-qualification/.work/`.
This earlier source excludes the later sort, mutable-writer and pure-return
repairs and supplies no qualification credit for them.

The earlier full-library checkpoint is frozen source `654fe6e99`: 2,214 pass,
zero fail, and three remain ignored. Matching XSH and XSHT verification products
build without warnings. The source tree is
`d39903e9fe794a7fd811da1fd3c8a26c83290e36`. The full-library log is
`.work/wt/full-inference-integration/.work/tmp/parameter-context-full-library.log`;
matching build and products are in
`.work/wt/full-inference-parameter-context-qualification/.work/`.

The XSH product is 42,430,688 bytes with SHA-256
`2cdbaa455fa16693fded208c7e7185ecdbe698681a592255e601d45f73c8f9da`.
The XSHT product is 50,785,576 bytes with SHA-256
`ac69cff66fa4f12054f0ad831b873c26a5089aa08bd7ced2687e8b722252e713`.
Normal and forced-frame capability runs each pass all 143 selectors with zero
failures or skips; the preceding 140-selector boundary is retained. All six API
catalog selectors pass. Their logs and detail directories are under that
checkout's `.work/tmp/parameter-context-native-{normal,forced,api}`. These
results establish this capability boundary, not corpus or final qualification.

The original corpus passes the require-target check. Ground RecordUpdate
repair `3d62c73c2`, integrated as `b1c26aeb3`, proves the actual base and every
replacement against existing field paths. Eight focused controls pass and
independent review is clear. Nominal-field repair `a589a1583`, integrated as
`c637b226e`, preserves exact source Tag identities through checked fields and
charges the complete source-type validation. All twelve focused checks pass,
including cold preparation of the complete original system-report module.
Independent review is clear. Runtime corpus qualification remains open. The
historical 1,040-removal cohort alone also fails on the preceding `f87c6d69a`
products with seven diagnostics: one list splice, one arithmetic generalization,
two sort keys and three cold generic-argument builds. Additional Proc/effect
erasures are not needed to reproduce those seven failures. Their raw log is
`.work/wt/full-inference-effect-corpus/.work/tmp/frozen-joint-f87c6d69-historical1843-utility9-diagnostic.log`.
The list-splice fix `70ee168f6`, integrated as `efa83632e`, resolves the canonical
inferred operand shape before enforcing List-only splicing. Six controls pass,
including Result/Stream rejection; independent review is clear. Other causes
are under investigation. This later repair needs broader combined qualification.

The preceding frozen source `4fafeefbb` has 2,193 library passes, six failures
and three ignores. Its matching system-report selector also fails before its
body with missing ground index receiver evidence. Preserve its logs under
`.work/wt/full-inference-qualified-uint/.work/tmp/`; it carries no runtime or
joint-corpus credit.

Repairs `39eca66cd`, `be4a6fe93` and `addf396ac` close those six failures.
The first skips receiver work only when the declared error channel already
accepts Error. The latter two preserve verified selected/forwarded index
bindings and prefer the existing scoped Optional narrowing proof before
structural initializer work. Independent reviews are clear. Combined source
`c8384fe30` first passed thirteen focused controls; the full-library run above
also includes the typed callback repair. Deterministic proof limits and unsafe
UInt-to-Str rejection remain unchanged.

The previous combined replay uses source `2da01f69b` and completes the full Rust library run:
2,183 pass, one fails, and three remain ignored. Its log is
`.work/wt/full-inference-producer-obligation-close/.work/tmp/joint-2da-full-library.log`.
Its sole failure is the UInt-map error-contract control. The earlier
`e80adaf31` run has four failures. All three other failures now pass on the
combined source. These results do not qualify the compiler.

At `2da01f69b`, matching verification products build without warnings. Normal and forced-frame
native capability runs each pass all 143 selectors with no skips. Each run
retains the earlier 140-selector boundary and adds three Proc-capture cases.
All six API catalog selectors pass. Logs are under
`.work/wt/full-inference-producer-obligation-close/.work/tmp/joint-2da-native-{normal,forced,api}`.
The `xsh` product is 42,098,456 bytes with SHA-256
`c6a6ffa4eeb8c81c95cc45237037bd6a29b280239684a908e6068c4e6a39aeed`.
The `xsht` product is 50,484,864 bytes with SHA-256
`a5b5c22a919a62a519470b37e1a38092a82e2949af6e88ba6530460d76923f48`.
These are capability and API results, not full canonical or performance acceptance.

UInt repair `8b4cfd5be` and follow-up `a433792b3` are integrated as `5e6778bce`
and `c9f5abbc2`. The final focused group passes all fifteen controls, including
the former library failure, stored-call captures, conditional Proc completion,
and original completion-source mutations. Independent review is clear.
Callee boundaries remain authoritative: caller capture cannot permit a Schema
error under a Str contract. Local completion evidence retains actual producers
and conditional operation references. The function-owned verifier rejects a
TopLevel boundary; its source invariant is reviewed, without mutation coverage
credit. Combined library and matching-product qualification remain open.

The earlier failures are the UInt-map error-contract control, two cross-output
scheme pointer assertions, and a lost callback argument relationship in a
generic Proc call. Constructor error defaults close correctly. The application
solver failed to revisit the callback relationship after that closure.
Repair `2d123efa6`, integrated as `516b522ef`, retains pending argument
constraints and passes five focused controls, including the original failure.
It preserves selected argument layouts, diagnostics and transaction rollback.

Published outputs intentionally rebase schemes into fresh owners. Test repair
`027c585cd`, integrated as `67c1ffd1e`, replaces the two pointer assertions
with complete contract comparisons and passes five publication controls.
Cached-source identity checks remain unchanged. The UInt repair must preserve
callee capture boundaries through direct and materialized calls. A caller's
outer `try` must not permit a callee to publish a Schema error under `Str`.
The combined library run includes both repairs.

Contextual-list repair `6c021dfcb`, integrated as `4832662a5`, and producer
repair `e80adaf31` have clear independent source reviews. Producer proof reuses
the shared owned Expression-fact query. Uniform native method effects remain
independently checked. Receiver-dependent methods retain their selected effect
proof. Immutable imported captures retain their actual binding and export
identity. Combined products, native replay, corpus parity and final gates remain
open. Tooling repair `d7ab77545`, integrated as `81894a393`, retains the
published callable-relationship context in `CheckOutput` contract views.
Comparison memoizes completed relationship pairs. Rendering expands each
relationship once and uses local labels for later references. Nine XSH library
controls and two XSHT comparison controls pass. Independent comparison review
and parent rendering review are clear. `docs/XSHT.md` documents the new
declaration accessor and diagnostic labels.

Materialized callable repair `2675cfb92`, integrated as `dce4fed2c`, passes
twelve focused controls and the unchanged pure-forwarding regression. Named
references retain actual value-instantiation proof, pending application
relationships forward until settled, and independently supported inputs establish
dependent callback binders. Runtime controls execute nested Results on both
routes after frontend disposal. Cold ownership, target, substitution, cycle,
Null, Any and phantom-binder mutations reject. Independent review is clear.
The corrected forwarding proof closes the original pure-forwarder provenance
failure through existing application evidence. This integration needs a combined replay.
Selected-callback repair `5bc6d5a6d`, integrated as `25cbdb225`, passes three
focused controls and independent review. It preserves exact left/right stream
projections for same-signature callbacks, finite known-target unions, mixed
unknown forwarding and imported stream aliases. Selection uses the actual
settled layout and unique formal identities, without signature matching.
Typed-argument follow-up `7a1bd555d`, integrated as `08323a734`, passes five
focused controls, including local and imported explicit argument types. Both
new controls fail on the preceding source and pass after the repair. Exact
source/formal binding identities preserve their stream projections; immutable
lookup maps share storage across checker probes. Independent review is clear.
Rest follow-up `387fd3207`, integrated as `80634fd4f`, passes six focused
checker controls. It preserves exact ordinary-rest and list-splice projections
through the settled argument layout and actual AST bindings. Independent review
is clear. Cold index repair `1b9cbc8fd`, integrated as `8c28d61d3`, derives the
field receiver from the existing checked index-result proof; its six controls
pass and independent review is clear. The combined source passes three focused
controls, including a new frontend-disposed runtime case for both rest forms
and both left/right targets on both execution routes. Its log is
`.work/wt/full-inference-integration/.work/tmp/combined-rest-runtime3.log`.
These later repairs still need broader combined qualification.
From-end follow-up `44ce19b10`, integrated as `0fe66fc5d`, uses the same
existing proof for `rows[-1].field`. Six controls pass, including distinct
first/last values on both routes. Default-origin repair `58374d818`, integrated
as `22f34e420`, uses only the exact selected target's checked defaults for
omitted slots. Eight controls pass, including direct-call parity and both-route
execution after frontend disposal. Both repairs have clear independent reviews.
The combined default/rest/from-end group passes all five controls in
`.work/wt/full-inference-integration/.work/tmp/combined-callback-default-rest5.log`.
The pristine system-report fixture exposed a separate record-field proof
failure before execution. Repair `e385c1fb7`, integrated as `464ad3c27`, reuses
the existing structural initializer proof beneath an actual complete storage
check and proves each conditional record branch. Both new regressions and
seven existing controls pass; independent review is clear. The original test
module passes static preparation. Matching-product runtime evidence follows.
The orchestration test is included as test-only integration `f6642254c`. Its
exact selector passes with the frozen `f87c6d69a` products: one selected, one
pass, zero skips. All six API catalog selectors also pass with those products.
Corpus extraction repair `fbe597d4a`, integrated as `3a21d10e0`, reproduces
both accepted view digests inside Docker. Root tar extraction had restored
0664/0775 modes; explicit umask 022 and `--no-same-permissions` retain the
accepted modes, including inherited setgid directories. The existing adapter
regression fails under umask 077 before the fix and passes afterward. No digest
guard changed.

The full 21-selector replay reaches original-root checks and the first nine
utility selectors, then fails while loading the first system-report test
module at `fallback.target.require()`. It has no reduced-view or parity credit.
Parameter-context repair `d9d43aecd`, integrated as `3b9bac2ea`, retains the
selected parameter's structural schema context, preserving actual nominal and
rest-item schemas. All six controls pass, including Any/callable/unresolved
target rejections and nominal-instance retention. Independent review is clear;
the matching-product replay passes that typing boundary but stops at the
separate RecordUpdate proof gap described above.

A nine-utility diagnostic reaches the jointly reduced ten-XSH-root check and
fails with 1,501 diagnostics. These are not independent causes. One source-backed
class is deliberate Proc return semantics: 39 of the 758 candidate Proc sites
are written plain returns. Charter section 7 gives their erased value bodies a
Result envelope. Site 2061's plain `collectors.SourceRead` return and site 111's
`Input` return demonstrate that the annotations preserve meaning. A new
scratch-only diagnostic retains these 39 original rows and still erases 719
Proc annotations and all 202 effects jointly: 922 retained rows and 921
additional candidate erasures. Its tables and source views validate, but no
new removal or parity credit is accepted. Source resolution confirms that all
39 selected return types have plain outer shapes, including the misleadingly
named `InputResult` record. The class probe removes the site-2061 caller field
failures, but the reduced ten-root check still fails with 765 diagnostics.
Leading examples include `date_parse.multiply` generalization and wrapper
fallback relationships. The historical 1,040-only cohort must be checked on
the same compiler to distinguish regressions from additional-erasure causes;
diagnostic totals are not independent failure counts.

Generic defer/assert and `error.fail` repair `17210eae3`, integrated as
`facda4a85`, passes one focused regression with both execution routes.
Terminal deferred actions retain Unit completion; assertion messages run only
on failure, and Int/Str instantiations preserve exact failure payloads. Cold
mutations of condition/message contracts and Error permission reject.
Independent source review is clear. This closes the demonstrated ordinary-body
omissions; combined library/native and remaining capability qualification stay
open.

Frozen corpus adapter repair `19132f32b`, integrated as `4fb050b06`, allows
selected historical Proc erasures with either zero or all 202 private effect
clauses erased. Partial effect groups, foreign sites and changed retained
reasons reject. Fifteen frozen-witness tests and two existing adapter controls
pass; shell syntax and diff checks pass. No annotations were removed and no
new corpus equivalence result is claimed. Product-bound corpus runs wait for
the combined compiler.

Aggregate callable storage at `ffeb2a0c9` and sharing correction `40fd5264f`,
integrated through `c75deec72`, passes eleven focused controls. The private
value-evidence contract now carries owned structural conversions; mutable
record fields retain one monotype, while independent fields and opaque generic
siblings preserve their contracts. Actual source paths, target substitutions,
mutable writers and complete evidence ownership remain cold-verified.
Missing/foreign/shared evidence, changed signatures and noncanonical facts
reject. Both execution routes retain invocation bindings, and unchanged shared
containers keep their storage. Independent source reviews are clear. Combined
source `a7900d1f1` passes 26 focused aggregate, ownership, Path, TempDir, tail
and defer controls with no failures or ignored tests. Matching verification
products build successfully at `96a932e04`, including the XSHT static API
catalog consumer. The catalog rejects owned callable relationships because
its registry signatures cannot contain them. Matching normal and forced native
runs each execute 138 selectors: 124 pass, fourteen fail, and none skip. Their
failure sets match. Against `b5989a1e7`, compatible callable selection now
passes, while two formerly passing imported-module selectors fail with
`checked try producer is missing its operand type`. The missing evidence lies
between `module.get(...)` and its outer propagation. Six API catalog selectors
fail before execution because successful assertions do not establish non-null
authority in the ground producer proof. Both gaps require repair. These results
do not qualify the combined compiler. Test-only import and helper scoping at
`27aed9509` follows the product freeze and awaits a production rebuild.
The TempDir native fixture itself used invalid bare field syntax in its two
print statements. Adding the required `$` preserves every assertion and the
expected output. Its extracted program passes with the same `96a932e04`
verification binary in the pinned offline container, printing exactly `5` and
`2`. A complete newer native boundary has not yet run.

Assertion proof repair `8632f9d4a`, integrated as `ef9a942ad`, passes two
focused controls with independent review clear. Successful assertions now
contribute their actual expression condition to the existing charged guard
census. The unchanged API helper executes on both routes. Mandatory
typed-callable proof rejects a foreign binding and an assertion moved after
its read. Writes still invalidate checker narrowing. Primitive checked reads
can remain valid through runtime validation and are not rejection controls for
this invariant. Matching verification products and the API selectors still
require replay.

Callable ownership repair `2b107877a`, integrated as `b4175a43d`, passes all
thirteen focused callable-value controls with independent source review clear.
Checked nested signatures rebase into the final owner, and module `get`
retains the actual Result success contract through propagation. Both execution
routes preserve prepared relationships after frontend disposal. Publication
rejects foreign programs, instantiated values regaining quantified authority,
and a real two-target relationship whose visible identity disagrees with its
prepared function. Matching combined products and native regression replay
remain pending.

Constructor error inference repair `2ed526d67`, integrated as `c69597e25`,
passes four initial controls, three existing wrapper controls, and eleven
expanded controls with independent source review clear. Constructor-owned
phantom error cells wait for all invocation arguments; declaration closure
preserves input-supported relationships. Bare `Ok` retains its `Error`
default. Rejected calls roll back bindings and close argument checkpoints.
No runtime evidence or solver is added. Matching native qualification remains
open; the SPEC template records the behavior and awaits combined generation.

Callable-list repair `41452fb0c`, integrated as `db81ac347`, passes six
focused controls. The checker joins actual target families, and list producer
proof preserves the exact physical target union while authenticating each
child's own contract. Duplicates, aliases and contextual empty branches work
on both execution routes. Canonical foreign children, forged references,
erased members and extra parent targets reject. Parent source review is clear;
matching native replay remains pending.

Proc completion and effect repair `b4664014b` plus `b44499d01`, integrated
through `911fd9bb7`, passes twelve focused controls with independent review
clear. Owned completion groups distinguish synthetic lifts, original Results
and callback application returns. Symbolic data instantiated with a Result
remains nested data. Scoped producer and effect evidence retain their existing
authority. Mutation controls preserve table ownership while changing actual
source roles or contracts; rejection of orphan rows is not credited as source
authentication. Forwarding, effect ceilings and both execution routes pass.
Combined library, native and performance qualification remain open.

Grounded callable branches at `685811709`, integrated as `6110c9d61`, pass
six focused controls with independent review clear. Named function aliases
retain their checked signatures when leaving a branch. Ground target, ABI,
immutable origin and writer checks remain strict; generic branches retain
independent instantiation. Matching native replay remains pending.

Combined source `15c71c323` executes 64 focused Rust selectors: 63 pass and
one existing typed-alias storage control fails. The same control fails on the
unchanged grounded-branch base, so it is not attributed to that repair.
Alias repair `16176899a`, integrated as `f3f308e4a`, passes five focused
controls with independent review clear. After authenticating an instantiated
read, storage proof retains its actual lexical contract instead of replacing
the quantified relationship with a bare function kind. Original driver,
captured, local and nested aliases execute on both routes; existing cold
mutations reject. An additional imported-alias control exposed the same earlier structural
producer defect on both sources. Repair `997205e28`, integrated as
`2fc9d81ab`, authenticates that Param contract with the existing immutable
import proof. Six focused controls pass; a strengthened one-test replay
rejects an owned, same-signature alternate target for the precise structural
producer reason. Independent source review is clear.

Combined source `30d36decd` passes all 93 focused Rust selectors, with no
failures, ignored tests or warnings. This includes the original 64, grounded
callable branches, Index/Slice, lexical aliases and imported aliases. Two
mechanical module-index adaptations at that source distinguish ordinary keys
from negative literal operands. Matching verification products and complete
native/API replay remain pending; this is not global compiler qualification.

Matching `30d36decd` products subsequently execute 140 native selectors in
each mode: 135 pass, five fail, none skip, and both failure sets match. All
prior 138 selectors remain, with two new Index/Slice positives. Five of six
API selectors pass. The remaining API inventory omits the already integrated
`crypto.ed25519_verify` and `net.validate_root_bundle`; `669396bf9` refreshes
only those entries and their counts against the matching tool's exact output.
The five native failures concern optional empty-list fallback composition,
generic guards/loops, open-record overrides, an Index diagnostic code, and
returned mutable-callable inference. New matching-product replay remains
required after their repairs.

Inferred Proc returns at `a1d299c7c`, integrated as `8da279d6e`, preserve
callable values when the expected type is only an inference placeholder.
Authored annotations and constructor error contexts remain binding. A checker
control and expanded Full IR control pass; the latter executes returned
captured Procs, both generic return branches and explicit contracts on both
routes. Independent source review is clear. The mutable-capture fixture now
supplies its monomorphic element type from actual initial state and explicitly
returns the Proc, avoiding an unrelated empty-cell and command-syntax failure.
The SPEC template is updated; combined generation and native replay remain.

Open-record overrides at `4fd5c2400`, integrated as `5be8c8cd6`, pass all ten
focused controls with independent review clear. The existing operation authority
retains ordered replacements and a distinct output remainder, preserving unknown
fields and changed-type collisions without restricting input labels. Cold proof
derives known fields and exact closed results independently. A coordinated
uncalled-body forgery was accepted before that proof repair and now rejects.
Full rebuilds and duplicate/shorthand evaluation order pass on both routes.
The private operation enum and physical rows retain their previous maximum
sizes; owned field arrays are included in solver and scheme memory accounting.
SPEC-template and Architecture changes are integrated. Full gates remain open.

The full Rust library attempt at `30d36decd` selects 2,157 tests but aborts on
a test-thread stack overflow after reporting twelve failures; it has no completed
suite result. Fixture repair `71a743b79`, integrated as `ab4fc9aa9`, passes its
eleven focused corruption controls across bounded runs. They target the actual
type payload after the check discriminator, preserve canonical table ordering,
and reject real source/contract mutations. The aborting fixture passes with the
suite's existing scoped stack helper. No production policy is relaxed. The
separate valid imported-function fixture and a complete library run remain open.

Generic Index and Slice at `48b66343f`, integrated as `8cfbceb34`, pass all
twenty focused controls with independent review clear. Inputs select List,
Map, constant Record/Module and slice domains; negative literals retain their
different List and Map meanings. Known open records use the existing journaled
field projection. UInt validation preserves lexical captures and uses the
authenticated substituted return envelope at erased boundaries. Deliberate
Result data remains data on both routes. Operation payloads grow by eight
bytes, physical operation rows grow from 16 to 24 bytes, and ordinary Index
encoding grows by four bytes. `Type` and `SchemeType` remain 24 and 56 bytes.
Native, memory/scaling and performance qualification remain open.

The Index parity matrix also demonstrates existing safety debt: a negative
Int key in a UInt Map can publish a Schema error under a Str error envelope
in both ordinary and generic functions. This is not credited as correct
behavior. Its origin repair must account for the implicit key validation in
the checked error contract while preserving input-driven operation selection.

Callable-value checkpoints `06f79b98c` and `06af00b5a`, integrated as
`ec62fe981` and `85e99b3fb`, pass nine focused controls on the combined base.
Record and conditional values instantiate independently; mutable bindings
retain one concrete instance. Factory calls retain separate invocation and
value evidence at the same span. Both execution routes pass after frontend
disposal. Independent review finds no demonstrated false acceptance in the
checked target, effect, binder mapping or runtime paths. These initial
checkpoints lacked module transport, complete retained accounting and nested
mutable storage; the subsequent focused repairs cover those gaps. Generic
composition and the remaining storage families still require coverage. No
full-library or native qualification is claimed for these checkpoints.

Module ownership repair `5b5fb951f`, integrated as `25cba55dc`, passes its
module-execution and retained-table controls. Four additional controls at
`17bdcb3a7`, integrated as `14c099c3b`, cover foreign semantic handles, checker
probe snapshots, shared allocation accounting, and nested cached imports into
independent entry roots. Source review is clear. Imported signatures and outputs
rebase into one fresh final relationship owner before publication. Cached
sources stay unchanged, and the new relationship pool participates in retained
accounting. Aggregate materialization is covered by the later focused repair.

Path execution repairs `9525dbc69` and `dd253674b`, integrated as `cc26cd72a`
and `af9e56aae`, pass three focused controls. Checked generic Path and RelPath
reads, queries and writes use existing canonical host operations. Byte reads
share the existing reader. Both execution routes preserve binary data,
diagnostics, explicit file modes and receiver/argument evaluation order;
operation, operand and injected-plan corruptions reject. Source review is
clear. These focused results do not qualify every registered Path operation or
the combined compiler.

The complete native Path host class at `2cf3a8f0c` and correction `799b67c21`,
integrated as `b5db0ddc6` and `617d8cda1`, passes four focused controls.
Canonical checked methods use the existing receiver-first native ABI. Ordinary
method rows forward only when all registered overloads share one host operation;
there is no runtime overload selection. Ordinary/generic comparisons cover
relative cwd, defaults, mutations, lazy reads, stream cleanup and error behavior;
cold operation forgeries reject. Independent review is clear. A scheduling
overlap exceeded the two-Cargo bound during the first corrective run; its
replacement passed all four selectors within the bound. Combined qualification
remains open. Registered Result[Unit] tail repair `bb4fceea3`, integrated as
`29845306b`, passes one focused selector. Generic-body admission now accepts
the existing checked registry plan as well as an operation obligation. Ordinary
and generic `[fs]` tails return failed writes as Result data; explicit returns
retain that behavior. A separate non-tail discard still requires Error, and
removing that permission rejects cold. Both execution modes pass. A frozen
ordinary-compiler probe confirms the data-forwarding contract; no new lowering
or blanket return-effect rule was retained.

TempDir repair `5b214e7ed` and its controls, integrated through `5c650d1ab`,
pass seven distinct focused selectors. Generic fresh and at-path scopes share
ordinary lowering, reconstruct entry errors from their payloads, and retain
owned expression and declaration facts. Both execution routes preserve entry
ordering, body failure, early return, nested Results and cleanup. Six cold
corruptions reject. Returned independent roots stay live until caller close in
ordinary and generic scopes; no speculative transfer change was needed.
Independent source review is clear. Cleanup-effect repair `d69676bec`,
integrated as `be40c6fcd`, passes one additional focused selector. TempDir
requires `fs` and `error` unless lexical `try` or `retry` captures Error.
Required checked Result contracts survive on fresh Match and at-path ValueBlock
sources; their existing structural producer proofs remain mandatory. The test
strips auxiliary contexts and rejects missing, incompatible and three Any-widened
contracts. Independent review is clear. Combined library/native qualification
remains open.

Source `d54631649` builds matching verification binaries and completes the
138-selector native inference boundary in normal and forced-frame modes.
Each mode reports 123 passes, fifteen failures and no skips; every selector
executes exactly once. All previously passing selectors remain passing, and
the formerly failing legacy wrapper selector now passes. Six new selectors
add four passes and two failures: generic Any-return cleanup with monomorphic
captured counters, and guarded access to a nullable record field. The fifteen
failures remain mandatory work; callable storage, open-row rebuilding and
ordinary-body inference are not qualified. No new practical-corpus or full
library result is claimed from this run.

Capture/Retry repair `6c8150dff`, integrated as `487ea1476`, passes four focused
regressions and five existing controls with independent review clear. Required
checked contracts survive publication for ground and driver boundaries,
including explicit Any payloads; scheme boundaries retain their owned facts.
Actual mixed error contributions join before comparison and normalize through
the checked Error boundary. Empty captures preserve declared phantom error
types. The added verifier traversal is charged against the existing budget.
Combined library and native qualification remain open. The next wave combines
this repair with the optional-record argument boundary; callable-value storage
and ordinary-body inference remain active, while open-row rebuilding waits for
the scheme representation to settle.

Optional-record repair `0d2ed9c74`, integrated as `f2c1bae75`, passes its
both-route regression and two constructor controls with independent review
clear. Explicit closed source annotations preserve nullable record arguments
when a null field prevents independent type derivation. Existing writer proof
and complete declaration validation remain authoritative: stale validation
rejects cold, and malformed physical data behind a valid complete validator
fails at the declaration before the generic call on both routes. Combined
source `f2c1bae75` completes the unfiltered Rust library check: 2,078 selected,
2,061 pass, fourteen fail and three are ignored. Eight failures hit the existing
work bound; one synthetic resource-cleanup capture lacks its required contract.
Those remain active repairs. Four stale controls are corrected at `5c70eff57`
and pass their focused rerun: explicit Any preserves Result data, supported
fallback relationships do not require the old recovery error, known Unit
propagation retains ProcessError, and required capture proof derives a fold
without an auxiliary temporary row. The reduced fold still rejects a forged
writer and executes on both routes. A fifth stale diagnostic assertion remains
with the capture repair. No full-library pass or newer native result is claimed.
The d546 verification products do not qualify this newer source.

Capture discovery repair `c4270b66c`, integrated as `e624551a6`, passes eleven
focused controls with independent review clear. The existing charged owner
census now collects actual Capture/Retry roots, avoiding a second traversal of
every instruction. All eight work-bound failures from the full run pass their
focused controls. A synthetic resource-cleanup Capture retains its actual
checked source type; its missing-contract failure also passes. The remaining
stream-carrier failure was the expected diagnostic text, now updated while
preserving rejection. Full-library qualification remains open.

Bare-propagation repair `320d777b9`, integrated as `cd8713574`, passes two
focused controls with independent review clear. `proc unwrap(value) { value? }`
infers a Result input and preserves its success and error parameters. Direct,
captured and retried calls retain Str and nominal failures on both execution
routes after frontend disposal; wrong input and effect mutations reject cold.
Plain inferred Proc returns now join actual propagated errors instead of always
defaulting to Error. Compatibility checks remain required for that contract
change. Phantom constructor context and generic tempdir support remain separate
mandatory work.
The combined compatibility check selects twelve controls: ten pass and two
normalizer fixtures still assume every inferred error becomes Error. Their
test-only repair `a1e6d312a`, integrated as `484fef6d8`, passes both focused
controls. Actual Error inputs now exercise inferred mixed-error normalization,
while a separate all-Str callback relay preserves its specific error. Existing
traceback, authored-return and cleanup controls remain. The ten compatible
passes and this two-test receipt cover the affected group; an unfiltered replay
and matching native products remain required.

Ordinary-body repair `b60f64f8b`, integrated as `b5989a1e7`, passes four focused
controls with independent review clear. Successful `error.failure` calls publish
checked argument evidence for generalized bodies. Explicit-return Procs use
their actual checked Error permission for cleanup propagation, including
arbitrary authenticated error payloads and independently checked callback
effects. Captured generic calls substitute the exact invocation's effects;
Ground script calls retain their existing signature authority. Missing effect
or invocation evidence and a retargeted script call reject. The worktree was
moved onto the current integration base before the final check; an earlier
mixed-capture failure came from its old base and earns no regression claim.
Source `b5989a1e7` completes the unfiltered Rust library run: 2,083 selected,
2,073 pass, seven fail and three are ignored. All fourteen failures from the
previous full run pass. Four mutation controls encounter earlier rejection
diagnostics; a stream owner-boundary mutation unexpectedly accepts. Two wrapper
fixtures disagree with the inferred error types. These seven cases require
diagnosis and focused repair before another full replay. Matching native
qualification remains open.

Matching verification products for `b5989a1e7` complete the 138-selector native
boundary in both modes: each has 125 passes, thirteen failures and no skips.
The dynamic-return cleanup and optional-record-field selectors now pass; every
previous pass remains passing. Each mode includes all eight legacy selectors,
all passing. The other thirteen failures remain unchanged. These products
precede the provisional callable-value checkpoints and do not qualify them.
Test repair `06caa3779`, integrated as `3b07df9ea`, passes five focused controls
and one separately checked field-corruption control. Mutations retain rejection
under the earlier call-target checks. Concrete and generic Procs returning
Stream fail eagerly under Error permission, while genuine stream bodies fail
on iteration. Removing Error permission still rejects. Two remaining wrapper
failures expose a real generic Proc completion gap: retained guarded projection
still defaults its return error to Error instead of joining actual conditional
error contributions. That relationship is active work, not a fixture waiver.

Method-family repair `824bf0de8`, integrated as `f730f31f7`, passes four focused
XSH controls and its registry control with independent review clear. Reversed
named mode/data arguments reproduce a cold-verifier failure before the repair;
text and byte writes, exact modes, failed writes and both execution routes now
pass. The private layout adds a 28-byte method row and an 8-byte binding, with
eight bytes added to each scheme and call-instantiation row. Frozen combined
source passes nine focused library controls covering methods and the existing
Proc-envelope relationships. Full-library and complete native replay remain
required; the earlier selection-mismatch attempt earns no qualification credit.

Wrapper repair `dcb02e1b1`, integrated as `872f7d2bf`, passes focused capture,
handler, stored-callable and Any-provenance controls with independent review
clear. Actual dynamic payloads execute on both routes after frontend disposal.
Constructor payload and coordinated fact forgeries reject; a deep constructor
alias chain retains the existing depth rejection. Complete ordered capture
proof replaces uncharged quadratic searches. Combined source `872f7d2bf` passes
31 of 33 focused controls. A retained corruption case rejects earlier than its
old expected diagnostic; the corrected assertion passes at `250631339`.
The other failure is a genuine contextual UInt constructor false rejection,
repaired by `609a0feda`, integrated as `74c7ff885`. Eight affected combined
controls pass, including removal of the required runtime validator and a
negative payload failing at declaration before the call. Compiler layout
assertions pass at 120 bytes per scheme
and 60 bytes per call-instantiation row. Full-library, native and corpus
qualification remain required.

Any-return repair `966af869b`, integrated as `f52452ae5`, passes four focused
controls and independent review. Explicit Any returns preserve raw Result data;
actual propagation and failed cleanup retain their failure channels. The
regression also checks LIFO errdefer behavior. Unit-Proc repair `089eb98fa`,
integrated as `b4ced1a2b`, infers known propagation errors instead of retaining
the default Error placeholder. Two corrected controls and nine unchanged
compatibility controls pass. The former conditional-propagation rejection was
the same inference deficiency; its preserved test now asserts Result[Unit, Str],
while an explicit incompatible contract still rejects. Omitted dynamic returns
retain the existing grounded annotation policy.

The inferred-Proc error repair `83634b225`, integrated as `016e925aa`, passes
nine focused controls and final independent review. Ground normalization derives
from the checked return and signature; defaulted Scheme Procs use their actual
prepared invocation substitution, including callback-result parameters. Both
routes preserve nested Result data, authored custom errors, cleanup and original
traceback locations. Its flag reuses existing metadata storage. Combined-source
qualification remains required.
Callable-value storage and record remainder
implementation follow these representation changes. Provisional focused results
do not advance full campaign acceptance.

Source `3ee4d8b92` builds matching verification products and passes the complete
four-return practical comparison: twelve XSH roots and three Laputa roots in
each view, six runtime selectors per view, signature comparisons, 1,991 callable
interfaces and all three negative controls. Source, product, pinned-input and
final view guards pass, and owned cleanup completes. The historical cohort,
final migration and inherited acceptance remain open.

Its complete native inference boundary executes every selector exactly once:
118 pass, fourteen fail and none skip. Both producer witnesses now pass,
including forced frames; every previously passing selector remains passing.
The remaining failures cover wrappers, host method families, callable-value
storage and open-row spread. The earlier passing unfiltered Rust receipt is the
`865b8aac2` result below; it does not qualify these newer sources.

Repair `a6fff7537`, integrated as `3ee4d8b92`, proves the missing Result shape
from the same callable instance's own checked proc-return envelope. It resolves
both return identities before exact comparison and retains effect permission
checks. Five focused controls pass, including actual FullBuilder/FullVerifier
execution on both routes, alias identity and wrong-instance rejection. Both
reviews are clear. Arbitrary unresolved returns without this proof retain their
existing diagnostics; no early generalization or new IR state is introduced.

Matching verification products at the preceding `5fa4a4b6b` execute the complete
132-selector native inference boundary: 116 pass, sixteen fail and none skip. Every selector
executes exactly once and all previously passing selectors remain passing.
The one resolved failure is the diagnostic fixture correction described below;
the fifteen main-family failures are unchanged.

Repair `e8e50ba94`, integrated as `5fa4a4b6b`, stops requiring a settled return
shape when invocation effects prove Error impossible. The causal regression
and retained ticket/error-flow control pass: two selected, two passed. Effects
that may contain Error still require the existing shape proof. The matching
native replay still rejects the two producer witnesses whose effect contracts
remain symbolic. Those cases require the retained invocation-flow relationship;
this narrower repair earns no producer-capability closure.

Source `7d9f4c6ac` builds matching verification products and passes the complete
four-return practical comparison. Both original and reduced views check all
twelve XSH roots and three Laputa roots. Six runtime selectors execute in each
view; signature comparisons, all 1,991 callable interfaces and three retained
wc negative controls pass. Source, product, pinned-input and final view guards
pass, and owned cleanup completes. This qualifies only that practical slice;
the historical 1,040-site cohort and final migration remain required.

The causal Proc repair `b67962c51`, integrated as `7d9f4c6ac`, checks a partial
tail `if` through the existing statement path when completing a Proc. Unit
fallthrough and Result[Unit] propagation survive early error returns. Ten
focused controls pass across compatible receipts, with both reviews clear.
Value-context conditionals, exhaustive branches, ignored non-Unit values and
authored Unit return restrictions retain their existing checks.
The completed Proc checkout is removed after patch-equivalence and ownership
checks; its branch and four small failure/test logs remain available.

The combined unfiltered library replay at `865b8aac2` selects 2,024 tests:
2,021 pass, zero fail and three are ignored. Matching verification products
build successfully. The complete native inference family executes 132 selectors
exactly once: 115 pass, 17 fail and none skip. All previously passing selectors
remain passing. One failure is a stale recursive-conflict diagnostic expectation:
the two declaration orders now report operator-type and type-mismatch errors,
respectively. Keeping both invalid programs and their status-2 assertions, the
corrected exact selector passes through normal and forced-frame execution.
That focused result does not replace the complete 115/17 receipt.

The matching practical replay passes all twelve original XSH roots, including
the previously failing `sort` check. The original Laputa check stops with nine
diagnostics, beginning with a partial tail `if` incorrectly requiring `else` in
an inferred Proc. Reduced views, runtime selectors, interfaces and comparisons
do not run; no migration credit is claimed. The minimal completion repair and
retained wrapper relationships are the next deliveries. Full qualification,
performance gates and deployment acceptance remain open.

The preceding unfiltered library replay at `6d67d40f3` selects 2,023 tests:
2,012 pass, eight fail and three are ignored. Five failures use expectations or
fixture premises from earlier contracts. Corrections pass all five focused
controls, retaining forged-input rejection, dynamic-key rejection and concrete
recursive SCC assertions. The recursive fixture now explicitly requires Int;
integer arithmetic alone also admits UInt and can retain a generalized domain.

The other three failures expose an alias initializer probing `env` as a runtime
module value before ordinary `env.PATH` checking. Repair `1bee8e87f`, integrated
as `a09ad8b49`, passes those three controls and two retained module/environment
controls, plus independent review. Standard namespace-as-value rejection remains.
The subsequent combined replay above covers both repairs without filters.

Iteration evidence repair `42588ef3b`, integrated as `fe094a17c`, passes six
focused controls and independent review. A checked `List[Any]` source supplies
its derived item evidence through the canonical iteration contract. Independent
result forgeries still reject Int-to-Any item widening; bare Any sources still
require validation, and non-Any refinements retain their directional behavior.

Proc-origin repair `a95f2e801`, integrated as `05c54556e`, passes eight focused
controls and independent review. Inferred Proc completions already record each
actual origin; finalization validates those facts without adding the aggregate
tail's success envelope again. The original `sort` failure was reproduced with
equal declared/resolved return types and an invalid double-success projection.
The causal minimal regression goes from fail to pass. Matching products and
practical replay remain pending.

Checked-dynamic repair `38eaa017c`, integrated as `8a40f9df8`, passes eleven
distinct focused Rust controls across compatible runs and independent review.
Passive generic calls retain checked opaque arguments, including typed rest
splices; empty-list and null inference requirements stay unchanged. Cold ground
and symbolic calls require exact input-binder evidence. Callable references use
the existing exact declared-signature proof. Ownership, forged Any/Int arguments
and results, and dynamic-operation rejection remain covered. Combined library
and native qualification are pending. The new symbolic evidence walk's repeated
subtree scan remains a bounded-work risk for the mandatory scaling gates.

The completed module-projection checkout is removed, with its branch and small
scratch artifacts retained. Docker-owned cache permissions required privileged
removal after restoring Git's registration; active checkouts remain intact.

Matching verification products built from `ce2c89e34` execute all 121 enumerated
full-inference native controls and eight changed legacy controls: 110 pass and
19 fail. Every selector executes exactly once, with no previously passing
selector regressing. Constant-key module projections and their legacy witness
also pass through forced frames. Stream iteration cleanup passes; the caught
iteration-error control reaches execution but lets the error escape its `try`.
Repair `e20fe99e4`, integrated as `30e2f6ac7`, routes comprehension Result errors
through the existing checked propagation flow. Its focused Rust regression
fails before the repair and passes afterward on both runtime routes, preserving
nominal error identity and stream cleanup. Independent review is clear; matching
native qualification remains pending.

The practical four-return comparison with these products passes its source,
product and view guards, then stops in the original XSH check at
`core/sort.xsh:2298:159`: `buffered_sort` exceeds the checked stream-return
projection boundary. Reduced-source checks, Laputa, runtime selectors,
interfaces and final comparisons do not run. This failed replay earns no
migration credit. A preceding wrong-product-path invocation fails before any
source check and remains separate from the actual checker failure.

Arithmetic repair `74bf33f00`, integrated as `f2cc01697`, passes fifteen new
focused Rust controls and three existing effect controls across compatible
runs, plus independent review. Sealed input/result relationships survive
generalization, forwarding and cold verification without runtime inference.
Directional list refinements and fresh Set-key obligations retain rollback
proof. Encoded requirements occupy 16 bytes and bindings 8 bytes; scheme and
call-instantiation rows each grow by 8 bytes. With matching `f8d7ffb16`
verification products, all five arithmetic
native selectors pass on both runtime routes: ten exact executions, including
the two original failures. Documentation regeneration also passes. Memory and
binary-size acceptance remain pending.

Matching verification products built from `e885b64c5` execute all 121 enumerated
full-inference native controls and seven changed legacy controls: 105 pass and
23 fail. Every selector executes exactly once. All previously passing selectors
remain passing, and six previous failures are resolved. Four new controls fail
on fixture mistakes. Correcting their assertions, reserved names and error
constructor calls gives two focused passes and exposes two cold driver-annotation
validation failures. Keep those focused receipts separate from the complete run.
The existing invalid-comparison-domain control also passes with these products.
Documentation regeneration succeeds through the existing checked generator.

The unfiltered Rust library replay at `92e7cabf6` selects 1,998 tests: 1,992 pass,
three fail and three are ignored. Two exact alias-work counters omit the new
iteration guard's constant representative lookup. Corrected counters pass both
focused controls, retaining the linear slope, zero compression writes and all
constraint counts. The remaining failure exposes lost explicit-Any provenance
through a generic relay. Iteration over that result reports unresolved inference
instead of the required dynamic boundary. This production class remains open.

Driver-source repair `9631019d8`, integrated as `92e7cabf6`, passes four focused
Rust controls and independent review. Incomplete runtime annotations require
proof of the exact initializer. The original local `RelPath`, annotated Stream
and nominal error-family programs build; forged path bytes, stream items and
same-owner error constructors are rejected. The combined library replay covers
the final budget-check ordering change. Native cleanup qualification is pending.

Static module projection repair `6ec8a0d52`, integrated as `6a0497396`, passes
three focused Rust controls and independent review. Constant-key Get and Index
retain the imported generic callable's scheme and authenticate its source through
the existing export proof. Get preserves its registered Result error type.
Runtime qualification on both routes awaits matching products.

The preceding verification products built from `2137456d9` execute all 113 enumerated
full-inference native controls and seven changed legacy controls: 95 pass and
25 fail. Every listed selector executes exactly once. This resolves eight
failures from the preceding `f8771f5b2` products with assertions through
`d88180b53` (87 pass, 33 fail), with no passing selector regressing. The earlier
`7b57e094d` products with `2aaefda86` assertions passed 80 and failed 40.
Structural and operator relationships and callable composition remain open;
these results do not establish full capability completion.

Fixture correction `3eb7eda14` passes its exact native selector using the
`2137456d9` products. Its three explicitly `Int`-returning Proc calls wrongly
used Result propagation. Direct calls preserve all mutable reassignment,
effect, runtime value and output assertions. Keep this focused receipt separate
from the complete 95/25 run.

Imported-callable repair `698e166ca`, integrated as `b5c3c4fd2`, passes four
focused Rust controls and independent review. Immutable imported aliases resolve
through the actual Use binding, canonical child namespace and exported function
contract, without inventing a value binding or an opaque target. Execution after
frontend disposal covers both routes; existing forged-export and immutable
alias controls remain. Matching combined native qualification is pending.

Iteration repair `728ddc1e3`, integrated as `44fc9212c`, passes eleven distinct
focused Rust controls across compatible runs and independent review. Source,
item and error relationships survive schemes and cold verification. The review
reproduced a rollback leak and verified its repair without increasing budgets.
Representative storage grows from 128 to 136 bytes; encoded type-parameter rows
grow from 12 to 20 bytes. Five new native controls pass across compatible runs;
two expose the driver-source failures addressed above. Their matching runtime
qualification remains pending.
Ordering repair `c97388857`, integrated as `4e6f3d4b0`, passes four focused Rust
controls. Ordering constrains and resolves operands before checking their
domains. Polymorphic arithmetic remains open.

Nominal producer repair `73ce589a9`, integrated as `54f03d86d`, passes three
focused Rust controls and independent review. The unchanged nominal example
verifies and executes through both routes. Forged nominal targets remain
rejected. Documentation regeneration passes with matching `e885b64c5` products.

Documentation regeneration through the existing `docs.generate` API with
`2137456d9` tools fails on the unchanged `70-nominal-type.xsh` example: a physical
record constructor is rechecked as its nominal result. No generated files were
written. The nominal repair above resolves this failure, and regeneration passes
with `e885b64c5` products.
Iteration review exposed two separate refinement programs: an annotated local
`RelPath` passed through an inferred iterator, and a grounded
`List[RelPath]`-to-`List[Path]` helper called with a contextual `p"relative"`
literal. Repairs `df8a204b9` and `92e7cabf6` now verify and execute these programs
on both routes, with physical literal-forgery rejection.

Open-row record spreads remain unsupported. Their result must retain unknown
input fields while later explicit fields replace colliding labels. Resolving
only the currently known fields would erase that relationship.

Structural repair `2eda90c75`, integrated as `f8771f5b2`, preserves open-row
relationships during destructuring and seeds omitted rest parameters as lists
of fresh item variables. All twelve focused Rust controls pass. The native
source-order and rest cases pass, including explicit JSON rendering of the
concrete list results. Iterable and Optional/Result relationships remain open.
Producer repair `e21dcb4b5`, integrated as `acc280757`, passes nine focused
controls. Cold proof retains runtime validation for unsigned defaults and
checks fresh aggregate payloads without exempting nested calls or operands.

Alias repair `3d2e04eaa`, integrated as `82914c14d`, passes all 23 focused Rust
controls and independent review. Driver lowering selects actual immutable reads
before allocating dense input frames; mutable and signal-hook inputs remain.
Cold alias authentication is iterative and retains every physical type claim,
owner check and cycle guard. The controls include the original 400-alias case
and 64/128/256/512-link cold verification and execution on both routes. No work
limit or stack size increases. The rejected post-encoding compaction mode is
absent. The combined unfiltered library replay at `82914c14d` selects 1,957 tests:
1,947 pass, seven fail and three are ignored. Four failures are stale mutation
fixture assumptions corrected by `c85e728fe`, integrated as `b0f8a8c88`; all four
focused controls pass. Three optional-receiver producer controls remain open.
These three passed on the earlier `c21be8076` source. Repair `e4560be76` moves
typed-callee signature proof to the mandatory owner-evidence phase while keeping
structural call checks in place. All twelve focused controls pass, including
guard-forgery negatives and the original alias work bounds; independent review
is clear. Broader acceptance remains open.

Effect repair `6b8e61495`, integrated as `4cd251358`, constrains pending recursive
effects before generalization and retains owned rejected-call tickets. Already
diagnosed calls skip duplicate effect reporting while still invalidating their
static call facts. All eleven focused controls pass, including finite empty
effects, real I/O rejection, diagnostic multiplicity and resource provenance.
Independent review is clear.

Unary conversion repair `86c970133`, integrated as `c8d79c2e7`, retains Display
and the existing distinct Argv requirements in schemes and cold proof. The
type-parameter metadata uses a reserved byte without changing its size. All six
latest focused controls pass, including natural work bounds through 512
parameters, and all three matching native controls pass. Independent review is
clear. Lists remain outside Display; the affected list fixture uses JSON while
retaining its value assertions. Bare capturing-command tails and direct run
returns remain separate mandatory gaps. Generated SPEC output and combined
full Rust/native qualification of this assembly are pending.

The unfiltered library replay at `0b78c8d9f` selects 1,972 tests: 1,957 pass,
twelve fail and three are ignored. Ten failures expect later verifier rejection
messages; the actual structural producer/receiver checks reject the unchanged
mutations earlier. All ten corrected controls pass focused rechecks, preserving
the strict multiple-parent assertion. The other two count one additional alias
hop for the unary conversion guard; their exact counts pass at `11ebd4ced`, with
the original linear slope and zero compression writes. This failed unfiltered
receipt remains distinct from the focused corrections.

Storage repair `40fac80ec`, integrated as `4e6260b6c`, verifies unannotated
initializers under an authenticated read contract when no initializer context
exists. Exact declaration checks remain unchanged. Both capture and Map
comprehension controls pass; disabling the repair reproduces the Map failure,
and mutating its actual iterator item is rejected. Imported generic-callable
source proof remains separate and open.

Command-return repair `e2d3d39f0`, integrated as `feea025ba`, passes all twelve
focused controls and independent review. Bare command tails complete with Unit;
explicit run returns retain captured output and use the checker's actual Return
resource-transfer fact. Coverage includes nested completion, early returns,
Bytes, Stream ownership, failure cleanup and cold output/permission corruption
on both execution routes. Combined native qualification remains pending.

Stream repair `c2149de8e`, integrated as `deb26fe9e`, retains the authenticated
stream owner and admits the runtime's existing `Str`-to-`Error` return-boundary
conversion. A small script reproduces the original rejection. All five focused
Rust controls pass, including late failure and exactly-once nested cleanup on
both execution routes, nominal-error preservation and forged consumer-boundary
rejections. This does not establish other raw-error conversion domains.

Fixture repairs `51d41a12e` and `071787144` pass all five focused native
executions. Three corpus controls needed required public documentation and
valid print interpolation. The cleanup negative incorrectly treated print as
a capability: XSH print is effect-free. Its corrected Env-read rejection,
permitted Env construction and original print-only acceptance all pass, as
does the existing print-effect control. No production capability bug is
established by the original cleanup receipt. Generic Display fixtures retain
their value-containing output assertions and require actual inference support.

Static literal-splice repair `10fce8c8d`, integrated as `c06d930d0`, passes the
matching native source-order/refinement selector, rest-element rejection and
default controls. Independent review is clear. Iterator-source repair
`c024801ea`, integrated as `7b57e094d`, resolves established cells before loop
and comprehension item projection. Ten Rust controls and its native rejection
pass; the public positive still fails at downstream prepared storage proof.
Keep that unchanged positive mandatory. Active source work addresses reusable
Display/Argv requirements and authenticated prepared storage/callable sources.

Provisional assembly `e0a97e754` passes 662 of 670 selected Rust controls.
The replay preserves all 668 preceding selectors and adds named-spread Stream
provenance and dormant callable-record guard controls. Eight failures remain:
three invalid-Proc diagnostic cascades, three producer-contract disagreements,
one callback receiver contract, and the flat immutable-alias work bound.
This selected boundary does not qualify the full suite or campaign.

Repair `2ec846442`, integrated as `5938baae2`, rejects an invalid Proc owner
before generalization. All eight focused diagnostic, sibling-isolation and
single-body controls pass; independent review reports no P1/P2. Combined
qualification of this newer assembly remains pending.

Guarded-read repair `7d6c8130e`, integrated as `e0a97e754`, passes 27 focused
controls and independent review. Ordinary records retain their ordering proofs;
callable-bearing records retain precise optional-receiver guard proofs even
without invocation. Repeated field reads reuse the existing bounded traversal
without per-read visited-set allocation or a larger work limit. Nine stale
fixtures are corrected at `6d20e11c4`; all nine pass while preserving corruption,
cleanup and independent-diagnostic assertions.

The preceding `3c98fe04d` passes 639 of 668 selected controls, with 29 failures.
The unfiltered library run at `c21be8076` passes 1,917 tests, fails 25, and ignores
three. Those failures expanded the selected boundary; no failed receipt becomes
acceptance credit. The earlier `8bb093c79` replay passes 629 of 630 controls,
with the nested optional-guard failure. Its 21 focused Proc controls pass,
including the six regressions introduced by `4e90fafe1`.

The preceding `4e90fafe1` passes 623 of 630 selected Rust controls, with six
Proc regressions and the nested optional-guard failure. Preserve that failed
receipt. Neither selected replay qualifies the full suite or campaign.

The preceding `353cf3c8d` passes 624 of 625 selected Rust controls, with only
the nested optional-guard failure. Preserve both failed-source receipts; neither
qualifies the full suite or the campaign.

The preceding `ec65747bb` replay passed 623 of 625 controls and exposed a
bounded-work regression in repeated guarded record reads. Repair `ad78fe079`,
integrated as `353cf3c8d`, reuses normal method proofs and searches for a consumer
only when its receiver contract remains absent. All six focused controls and
independent review pass. Unresolved roots still require independent receiver,
argument, result and effect proof; the mandatory final worklist drain remains.

Union checkpoint `af20162d8`, integrated as `1f500a409`, passes fourteen focused
controls and independent source review. Every native method family requires an
actual receiver proof. Uniform effects remain unchanged, and exact generic plans
reuse the mandatory body proof. Completed union-read proofs share owner-local work.
Capture checkpoint `05238aa81`, integrated as `85700af0d`, passes five focused
controls and independent source review. Actual immutable literal initializers
qualify record captures, and a separate cold proof checks every writer value.
The unread-Stream corruption reaches the intended payload rejection. Literal
computation repair `92966b18c`, integrated as `f51af1668`, passes seven focused
controls across compatible runs. The cold traversal now proves finite literal
arithmetic, field access and canonical Boolean negation. The next combined
replay retains all 622 selectors and adds its two new behavior controls.

Native-method demand repair `2ca5a91d4`, integrated as `f88cb1bfd`,
passes four focused controls, including a definition without calling code and
the original receiver mutations. Independent review found that deferred method
checks discard the return-context flag needed for implicit Result tails.
Repair `ac218e8ab`, integrated as `ec65747bb`, passed five focused controls and
independent review. The final phase-reuse repair preserves that Result-tail
regression while removing the duplicate method check that caused it.

Earlier runtime assembly `fcdf567fd` passes 604 of 608 selected Rust controls.
Every previous selector remains selected and every prior passing control passes.
The four remaining failures cover literal record capture, two union receiver
cases and nested optional-guard verification. Master remains unchanged.
This selected regression boundary does not qualify the full campaign.

Builtin assembly `dd3f8be86` first passed 600 of 607 controls and exposed an
inferred-`Any` return regression. Repair `276d53bf1` rejects the actual dynamic
return candidate before recursive generalization. All five focused controls pass,
including authored `Any` and genuine generic recursion. The incidental body-scheme
diagnostic becomes the direct return-inference diagnostic.
Result-producer repair `67c1f3f15`, integrated as `fcdf567fd`, passes all four
focused controls. It specializes the canonical empty-Set constructor and derives
pipeline output from its actual input and stages. Both execution routes pass.
The Set test now targets the actual dedicated operation instead of assuming an
empty-list conversion. Its receiver, item, alias and method mutations still reject.

The reviewed optional-record and immutable callable-alias repairs are now
integrated. Active source work addresses static literal positional splices,
the remaining producer/callback verifier contracts, and repeated alias proof
work. Final-source native, corpus, performance and inherited deployment gates
remain open. Pyramid remains frozen until inference qualifies.

The generic-body fixture now asserts runtime behavior for an independent generic
value alongside a live ground mutable capture, and for activation-local mutation.
Commit `ca80a5964`, integrated as `c53e7ac44`, passes both positive selectors and
the two retained shared-state rejection controls. Matching final-source native
qualification remains required. Three other native controls retain diagnostic
or module-verification failures in the earlier product run.

The fixed-Int mutable-capture rejection was already failing on the preceding
source and conflicts with the charter's independent-variable generalization.
Its corrected positive checks two instantiations. Actual shared-state
polymorphism rejection controls remain selected and pass.
Pyramid remains frozen until inference qualifies. No full-suite or timing
acceptance follows from these focused results.

Earlier assembly `e2b76eaac` passes 584 of 589 selected Rust controls.
Its five failures belong to the earlier full-library failure set. Every prior
passing control remains selected and passes. The 570-control `e8f4a17bf`
boundary and all nineteen added controls remain selected. Nominal callable
pattern ownership is repaired; generic capture, union method receivers, named
stage configuration and nested optional-guard verification remain open.
The assembly includes actual host-call selection publication, ordinary generic
cleanup, With/Guard scopes and immutable environment-path snapshots. Authored
recursive records retain their dynamic Map values and settled provenance.
All three original Tokei regressions pass. Master has documentation-only changes
after `d2453760a`; the assembly is not qualified for master.

Checker checkpoint `6008d4a79`, integrated as `6a65887ad`, closes original
collecting owners and retains exact pending-call return-flow tickets. Its first
merged replay exposed twenty-two previously passing controls. Repair `d6ecf0bc2`,
integrated as `e2b76eaac`, restores them through canonical constructor roles,
source-derived child types, owner-scoped host finalization, deferred callable
compatibility and the original unresolved-return diagnostic. All forty-two
focused controls and the expanded replay pass their previously passing scope;
no test assertion changed. Broader single-body authority remains incomplete.
Nominal-pattern checkpoint `e8a60d2eb`, integrated as `6eadbfc40`, retains
canonical enum/error payload contracts and exact structured-error projections.
Thirteen focused controls and the expanded replay preserve actual target,
parameter-label, nominal-field and source-identity rejection checks.

Ownership-test checkpoint `1015ffd0e` corrects a disproved test premise:
rejected owners intentionally lose latent Stream facts, while a valid owner
retains the unique call fact. A fallback return makes that valid fixture
independent of the fact for builder acceptance. Pipeline repair `e8f4a17bf`
recognizes a physical empty List as a non-Stream source even without an item
type. Stage-demand analysis and all other producer obligations remain checked.
Valid empty and typed inputs capture the expected allocation error and produce
the expected ordinary range output on both execution routes. Each checkpoint
passes four focused controls; the combined replay preserves the existing
pipeline and Stream capture mutations and all previous passing controls.

MapKey checkpoint `8808c9281` retains the existing finite key-domain constraint
through generalization, imports, callbacks and prepared execution. Empty
List/Map/Set owners retain their actual solved initializer and binding types.
Thirteen focused controls pass. The first combined replay exposed unnecessary
representative lookups and two obsolete verifier-test premises. Repair
`7707b65f0`, integrated as `e53f3e47f`, restores both original exact alias-work
assertions, preserves canonical `set.empty()` and empty-list conversion
mutations, and expects the same List-to-Set forgery's earlier missing-key-bound
rejection. All seventeen focused controls and the combined replay pass their
previously passing scope. No work limit or forged-input guard is relaxed.


Snapshot checkpoint `02a87cb84`, integrated as `1682fc689`, retains immutable
read-time environment-path entries with a sealed Rust Value payload. Ordinary
lists cannot forge that retained contract or prime its callable cache. Actual
Any observations, list consumers and active-environment mutation behavior remain
unchanged. Authenticated type-test evidence supplies guarded List reads without
changing the stored Any contract; For and Index source/type/polarity mutations
remain rejected. Equality reuses the existing concrete-domain rule and retains
the Any and Optional/Null exceptions. Twenty focused controls and the combined
569-control replay pass their previously passing scope. The public Rust payload
change is breaking; XSH syntax and the documented runtime List behavior remain.
Diagnostic checkpoint `c38df2edc` displays MapKey bounds without producing source
annotation syntax; its five focused controls and the combined replay pass.
Dedicated generic Path/env/conversion/require admission remains open.

Scoped-body checkpoint `73596f399` passes seventeen controls and a separate
symbolic resource control, then the combined replay. Ordinary generic guarded
commands, retry, error contexts and context scopes reuse the existing executor
and retain actual values, nominal errors, effect boundaries and restoration.
Known Stream escape remains statically rejected; symbolic output retains the
runtime escape guard. Equality reads existing resolved operand types without
relaxing storage. Dedicated Path/env and authored conversion/require forms
and single-body authority remain open.

Host checkpoint `3dd1df93f` retains actual checked argument values, binding and
schema context through canonical method selection. All twenty-three focused
controls pass, followed by the combined replay. The duplicated inner argument
effect diagnostic is gone. Original call and named-record-field identities,
receiver schemas and contextual requirement targets remain authenticated.
Probe adoption and single-body checking are separate pending work.

Guard checkpoint `b1305521e` prepares earlier Guard success bindings before
capturing callables through canonical Guard validation. Preparation does not
advance statement flow. Four focused controls and the combined replay pass,
including later-binding and fallthrough rejection and the unchanged physical
capture-writer forgery. Scalar, Optional, destructured and Stream-origin
captures are checked; no runtime cleanup claim follows from checker controls.

Stream checkpoint `ecfeed5a3` authenticates actual ErrorContext carriers and
proved noncompleting Capture bodies. Completing captures still need a value
tail. Its four focused controls pass, including both execution routes and a
physical false-to-true mutation that removes noncompletion. Parameter checkpoint
`34f281746` reuses the mandatory checked narrowing at the same ground parameter
read only when the checked slot matches the declared signature. Five focused
controls pass; declared call/slot contracts and forged-path guards remain.
Both checkpoints pass the combined replay without increasing work limits.

Diagnostic checkpoint `3f839a3fa` suppresses only a recovery-type generalization
cascade with an existing primary error on the same owned recovery-bearing
expression. Earlier local validation and other scheme failures remain intact.
The new declaration-order and same-body controls and three existing controls
pass, followed by the 541-control combined replay. The unchanged List-to-Set
forgery at that checkpoint expects its earlier declaration/initializer
rejection; its original source executes on both routes. These changes grant no corpus removal credit.

Local-fact checkpoint `7351fd426` publishes the original empty-list owner's
existing inference type to its initializer and binding. Constructor publication
resolves its own pending suffix while preserving earlier pending expressions.
Four phase controls and 48 family controls pass, followed by the combined
540-control replay. Nested constructors and error cleanup retain their guards.
Full-source accounting still records four performed body checks against one
published body; this is not single-body authority acceptance.

Practical comparison checkpoint `f6d22586c` compares interfaces and private
bodies within each selected entry context, preserving nominal identity and
old/new contracts. Seven comparator controls pass in isolation and on integrated
`25d47071a`. Matching products pass every original/reduced XSH and Laputa root,
the twelve selected behavior executions, and both interface qualifications.
The first negative control exposed the recovery cascade repaired by
`3f839a3fa`. Signature probing then exposed input-mode drift from restoring
modified frozen files. Adapter checkpoint `bd1a6b280` probes scratch copies;
its new regression and existing adapter family pass. The guarded practical
replay on integrated `d225795ad` passes both views' twelve XSH and three Laputa
roots, twelve exact behavior executions, all three negative controls, six
signatures per view, interface comparison, unchanged-view postconditions and
cleanup. The accumulated interface count is 1,991 callable and zero CLI
contracts. This qualifies the named four-return view, not the historical cohort
or annotation-removal targets. One preceding invocation exited silently before
stages; its exact argv was not retained, so its failing predicate is unknown.
The successful traced run records the exact inputs and complete guards.

Four test-premise corrections pass the combined replay: scalar reads and local
storage execute against annotated references on both routes; supported default
arguments construct and verify; forged default calls retain the complete effect
rejection; discarded container arguments use their actual lowered statement.
The original sources and forgeries remain. No production guard is weakened.

Path-like checkpoint `55c681c25` passes eighteen focused controls and the
combined replay. It reuses the canonical Path-like validator for all three
methods, retaining contextual path literals, stored-Str rejection, exact
arguments and original effect order. A unique registered receiver remains
valid inference evidence; no artificial missing-annotation rule is introduced.
Diagnostic repair `2321053f2` stops retrying already rejected callable
interfaces. Both original regression assertions pass unchanged. Independent
inference errors remain visible in both declaration orders, and genuine
output-only and unresolved-call controls pass. Five focused controls pass
before the combined replay. No global error suppression is introduced.

Result-context checkpoint `9f1d5939b` retains the documented one- or
two-argument contract, selected receiver and error family, defaults and effect
ownership. Twenty focused controls pass, followed by the combined replay.
Three arguments now receive the existing arity diagnostic instead of reaching
an unsupported lowering boundary. No runtime or registry signature changes.

Fresh-list checkpoint `8479c7d45` repairs the diagnosed statement-tail join:
exact empty literals can inherit another literal branch's element type in
value-producing `if` and `match`, including nested block tails. Ten focused
controls and the inherited stored-domain control pass. Stored aliases and
incompatible known element types remain constrained. The SPEC template and
generated paragraph match. It passes the combined replay. Matching `6cf8c3a25`
verification products build without warnings. The guarded practical replay
passes all twelve XSH and three Laputa source roots, and six exact selectors
in each view. The earlier `cut` comparison failure does not recur. XSH
interface qualification stops at `date_parse.xsh::absolute`, whose declaration
is inconsistent across entry programs. Laputa interfaces, negative controls
and signatures remain unrun. Source, product and cleanup guards pass. No new
removal credit is claimed; diagnosis precedes another product build.

Interface checkpoint `ccc954553` authenticates empty-list and divergent-block
operands from their actual checked AST and flow. Exact empty-list branch tails
may take a peer list literal's item type; stored aliases keep one shared type.
The SPEC source and generated paragraph match. Exit publication belongs to
actual ValueBlock expressions, preserving declaration-body comparison across
annotation removal. All 45 body-contract controls pass. Nine nullable controls
pass; the extra generalization diagnostic was still a required failure at
that checkpoint. Diagnostic repair `2321053f2` subsequently closes it. Both
families retain their passing controls in the latest combined replay. No whole-doc generator was run because its
only entry point also builds release products and renders all documents.
Matching `08e824b91` verification products build without warnings. The guarded
practical replay passes every source root and behavior selector, then fails at
`cut.xsh:147` during interface comparison. Product/source post-guards and owned
cleanup pass. Later Laputa interfaces, signature and negative comparisons do
not run. A separate in-process diagnosis owns the remaining comparison gaps;
no repeated product build is queued until that source class is repaired.

Host checkpoint `e99bb2cc5` also finalizes argument-dependent Standard method
overloads from committed argument evidence. Actual before/after argument
contexts preserve each canonical selection phase and effect owner. All sixteen
focused controls pass, followed by the combined replay. Schema-bearing and
specialized method continuations still use their existing checking paths.
Body probes remain separate work; these checkpoints do not establish one-body
authority or a performance result.

A subsequent test-only correction retains nested-schema rejection while checking
supported closed Stream callable identity and both cross-kind rejections. All
21 callable-contract controls pass. The old assertion treated Stream projection
as unsupported, contrary to the implemented callable-kind contract. This
corrects one remaining test premise; no production admission changes.

Three existing valid-first forgery controls also pass after correcting obsolete
rejection expectations. Generic tail and tail-less block forgeries retain their
actual type guards. Filesystem scans and walks reject forged Result envelopes
through the registry signature inside capture and the effect ceiling outside
capture. Their source programs and mutations are unchanged. No production
verification rule changes. All four test corrections pass in the 448-control
combined replay and remain passing in the expanded 493-control replay.
Required full regression gates remain open.

Export checkpoint `2580811ac` unifies the public Rust ground-export variants as
`ModuleExportType::Callable { kind, sig, optional }`. Eight isolated controls
pass, including actual ground and generic Stream exports on both routes,
owner and signature mutations, strict codecs, and existing module contracts.
Initial assembly `1255cf0d3` exposed two dynamic module-call regressions.
Repair `3ef7f5532` preserves common checking while publishing typed-value facts
only for typed-value calls. All ten focused controls pass, including the
unchanged purity mutation. Both regressions pass in the combined replay.
Both libraries compile; this run selects XSH tests and establishes no XSHT
test coverage.

Test-first checkpoint `06a950205` preserves the earlier parked terminal Proc
command control in committed source. It now passes in the combined replay.
Body checkpoint `426157ef3` passes all 26 isolated controls. Host-continuation
checkpoint `01ae27f43` passes all 15 isolated controls. The initial combined
replay exposed a test adapter supplying Pure for an actual Proc; correction
`a6ac1b984` derives the checked scheme kind and retains both capture assertions.
All three focused controls pass, followed by the corrected combined replay.
The completed export checkout is retired from the active path and Git
registration. Its scratch and generated artifacts remain in the ignored
retired-lane archive.

The preceding full XSH library run on `0cc8dc0ff` finishes with 1,754 passes,
44 failures and three existing ignores. Twenty-four failures are repaired or their obsolete test premises corrected in
the combined replay. The two diagnostic cascades are also repaired on
`2321053f2`. Four further test-premise corrections leave fourteen required
failures on `6cf8c3a25`. Guard scheduling, Stream carriers, parameter narrowing
and the generic-alias test premise subsequently reduce this to ten on
`6f354caab`. The actual checked-value constructor forgery fixture and solved
local-fact checkpoint subsequently reduce this to nine on `6a082af02`.
Remaining failures include producer gaps, malformed mutation
fixtures and obsolete unsupported-policy assertions. Each needs a scoped repair
or a demonstrated correction of its former premise. The broad binding-fact
corpus sweep passes.

The practical four-return paired run on matching `97a0b32dd` products passes
all 12 XSH roots and three Laputa roots in both unchanged source views.
All six selected behavior tests pass in each view: 12 passes, zero failures.
The earlier exported-constant interface ambiguity is repaired. XSH interface
qualification now stops at a null-safe method receiver in `sort.xsh`.
Laputa interface comparison, negative controls, signatures and final view
comparison do not run. Source and product post-guards pass. Owned container
cleanup resolves. These partial results grant no new removal credit.

Repair `cb07cbd79` captures actual checked null-safe receiver payloads without
changing ordinary field selection. Eleven isolated controls pass, including
wrong and missing receiver facts. Repair `995ae6054` preserves the established
Unit boundary of valueless recursive Procs. Three isolated controls pass,
including the original recursive body-contract failure and value-returning
controls. Both repairs pass the combined replay. The next matching practical
pair remains pending.

Repair `058f05738` restores conservative failure capture for an opaque Proc.
Unknown effects may include `error`; treating them as empty incorrectly permits
a narrow nominal error promise. Both existing source controls pass, including
their pure, finite-effect, Result and unpulled Stream counterparts.
The combined replay retains every earlier passing control and adds no failure.

Runtime checkpoint `3b96ee9e7` is integrated. One canonical callable kind and
invocation environment survive aliases, suspension, resume and cleanup.
The unchanged captured-write and Repeat witness passes on both execution
routes. Consumer writes before first pull, between pulls and before close are
preserved. Repeated and never-started cancellation retain their contracts.
Exact capture-storage, actual declaration-kind error boundaries, delegation,
UInt driver validation and actual dynamic export target controls pass.
The dynamic adapter repair transports program, function and kind together.
Ordinary script calls and the actual adapter pass their separate controls.
The obsolete mandatory Stream return-annotation diagnostic is removed, and its
existing emission-inventory test now passes. No authored syntax is added.

The remaining generic-body families and single-body checking authority stay
open, alongside 18 baseline failures. The first body checkpoint does not cover
run-initialized Guard, Retry/context/error-context, dedicated builtins or the
remaining conversion forms. The next lanes own those body forms, canonical
host-call continuations and paired interface completion. Full Rust, native, memory, performance and inherited
Laputa acceptance remain pending. The last matching native replay is the earlier
`7f4dccc80` pair: 27 passes with unchanged source and product guards.

Test-only follow-ups are integrated through `4baec41cf`. On `3924bb34e`,
all 14 independent type, row and finite-effect oracle controls pass, including
rollback and interrupted-trial continuations. The effect comparison is bounded
to two variables; it does not establish complete solver or scheme-publication
correctness. XSHT's original 12-selector replay also passes on `0c9d8d1f2`
with test correction `f29cdd313`. The corrected test first demonstrates that
the real body proof permits a redundant effect-clause warning, then removes
one checked declaration while retaining its effect fact and requires removal
to fail closed. Its former implicit unsupported-body premise is now false.
Neither follow-up changes production behavior or qualifies a full gate.

The callable-kind foundation changes the internal Rust type API while preserving
authored Pure and Proc syntax. It passes 68 isolated controls and the non-test
XSH and XSHT library check. Runtime Stream carriers and dispatch still reject
unsupported paths explicitly. New runtime probes expose the remaining Stream
transport and captured-write limitations. These are mandatory open work.
Structured recursive return repair `4aa2a44eb` is integrated and covered by
the combined group. Both runtime routes preserve application envelopes and
formal Result data. Contextual empty-list returns, live-resource escape rejection
and missing-leaf resource-proof rejection pass. The escape repair checks the
actual leaf before Unit consumption. Corpus
classification and the inherited full acceptance gates remain open.
The counter-getter build warning is fixed. Compiler-setting repair `4563d2c3f`
lets the pinned compiler complete XSHT's recursive Send proof. The targeted
verification-profile library check passes with compiler warnings denied.
XSH solver and verifier budgets remain unchanged. No performance qualification follows.

Single-body authority remains open. Local constraint collection checks a cloned
body and retains constraints without the selected host-call facts needed by
execution. Deferred host solving must publish those facts through canonical
finalization before the second traversal can disappear. Total inference-work
counters must qualify this requirement. Output-only body counts omit probe work.
Proc return-shape probing needs separate treatment.

Source review also identifies five remaining generic-body admission classes:
Proc defer and errdefer, With and Guard scopes, retry and context expressions,
nonregistry builtins, and authored conversions or require targets. These are
existing-language implementation gaps. Their completion remains mandatory.
The deferred host-call continuation must retain its committed instantiation,
checked argument layout and original effect scope before the probe can disappear.

Master includes the four foundation commits through `c44e47016` and the
independent effect-analysis repair `d6d71cb4f`. A fresh isolated replay passes
all 46 canonical-launcher tests. The earlier pinned warning-denied check passed
for the warning repair. The effect repair reproduces and fixes an external
unknown-summary panic. Its private graph solver can omit a selected set of
clauses without changing ordinary solving or authored contracts. All 12 merged
effect and importer controls pass. No corpus clauses are removed.
Independent diagnostic repair `62e65d2dc` also lands on master after its exact
pinned regression passes. Spanless indexed-build errors identify the failing
entry source; explicit error spans remain intact. This ports campaign repair
`80fea2f8d` without the unfinished compiler changes. The isolated checkout is
clean and its owned build has finished.
Master also includes the independent performance adapter through `353ebeca0`,
ported with its three generator prerequisites. All seven files match campaign
source `354d9215c`; fresh isolated checks pass 26 workload/measurement tests
and 42 adapter tests on both sources. The optional `inference_performance`
configuration and module command cover section 12 measurements only. They
require explicit reference products and reject missing canonical counters;
full inference acceptance remains closed. No actual timing result follows.
Master now includes the published HTTP dependency updates through `25592b9a8`.
The h12tiny family is at 0.4.0, with `rustls-webpki` 0.103.16 and both
WebPKI root packages at 1.0.10. The first update passes 25 focused networking
checks. The second passes a pinned offline verification-profile `xsh-net`
check. All dependencies come from the registry. Both updates are integrated
into the campaign branch.
No fresh full Rust, native, or performance gate accompanies these merges.
Later inference commits remain on the campaign branch: even the focused-green
`8f80bc2f5` checkpoint still fails existing XSH corpus preparation and is not
ready for master.

Twelve completed lane worktrees are removed after confirming campaign
integration, clean tracked files, and no open handles. Their generated Cargo
outputs are discarded. Branches remain. The temporary foundation and diagnostic
qualification checkouts are also removed. Active work, qualified products, failed-run evidence,
and checkouts containing useful probes or corpus data remain intact.
Both completed performance-adapter checkouts are removed after their merged
checks pass; their accepted commits remain on master and the campaign branch.
Seven superseded assignment, comprehension, context, iteration, operator,
paired-comparison and repaired qualification checkouts are also removed.
Their raw qualification logs remain under
`.work/tmp/full-inference-retired-qualification/`; their commits remain available.
Five completed body, compiler, contract-comparison and driver-projection
checkouts are also removed after clean ancestry and ownership checks.
Their complete scratch trees remain under
`.work/tmp/full-inference-retired-lanes/`. Their branches remain available.
Four completed interface, record-form, scalar-contract and graph-verification
checkouts are removed under the same checks. Their complete scratch trees remain
in that directory. The imported-source reproducer and dirty iteration prototype
remain untouched. The prototype's patch lines match the accepted iteration edits
at `c0a282df8`, apart from seven later added stream-error verification lines.
Two completed XSHT checkouts are removed after clean patch-equivalence checks
and confirmation that no agent or process uses them. Their complete ignored
working trees remain under `.work/tmp/full-inference-retired-lanes/`.
The superseded argument and budget qualification checkouts are also removed
after clean ancestry checks. Their complete scratch trees remain in the same
directory. Matching collector, NonEmpty and optional products remain retained.

Finish the remaining acceptance work, integrate the accepted inference capabilities,
and improve ordinary invocation and substantial workloads. Qualify complete Laputa
operations on the existing Linux platform. Add no syntax-feature batches.

The deliverables are:

1. One accepted compiler baseline with reproducible gates and a concise everyday reference.
2. Integrated inference, a classified retained-annotation corpus, and joint behavior parity on real utilities and Laputa code.
3. Reproducible measurements for startup, streaming, algorithms, and filesystem/process orchestration.
4. A measured decision on prepared shipped modules, plus useful improvements to shared runtime mechanisms.
5. Disposable Laputa package, process, rooted-filesystem, and real boot/shutdown acceptance workflows.
6. A bounded release candidate with all applicable gates passing and accurate formal-evidence claims.


Reviewed producer repair `512779e63` is integrated at `3e11fe729`.
Its final focused boundary passes all 63 controls, with independent approval
and unchanged work limits. Nineteen new portable tests cover the repaired
relationships. Review reproduced a wrong-family direct error transfer accepted
by the verifier; mandatory physical propagation proofs now reject it at the
nearest applicable function boundary while preserving local captures.
The census assertion now counts physical statements independently and compares
complete writer and return sets with the old traversal. Its old exact count
described the removed expression walk. The first merged run passes 702 of 703
controls; the remaining old test requires resolver calls that unique bindings
no longer need. Test-only repair `4ec6ac8a8` asserts exact zero for those bindings
and retains nonvacuous ordered-query bounds for actual mutable replacements.

Fresh source `7b6a9629c` passes all 703 controls, eight corpus diagnostic checks
and twelve runtime controls, with zero failures, ignores or native skips.
Matching verification products are 36,406,008 bytes for `xsh` and 44,412,944
bytes for `xsht`. Source and products remain unchanged. The corpus candidate
is still the 202-effect diagnostic probe; no historical removal or paired runtime
credit follows. The full canonical attempt stops at 206 root integration passes
and 25 failures; downstream native stages do not run. Fifteen failures share
incorrect Result-returning function-tail lowering. The remaining primary causes
are compacted process defaults, a declaration tag, driver binding ownership,
loaded standard-library effect contracts and a stale private-proc rejection.
Four poisoned-lock failures follow the driver failure. Repair each cause through
its exact controls before retrying the full boundary on a fresh source.
The failed receipt remains in `full-inference-producers-qualified-r2`; owned
cleanup resolves and no combined acceptance follows.
The private-proc test correction and Result-tail repair are integrated. The
original Pure parameter test remains, and both exact declaration tests pass.
The tail repair preserves actual Result forwarding and cleanup order on both
execution routes; all fifteen networking regressions pass with matching products.
Library effect repair `f467bb0e0` is also integrated. Both original loaded-module
regressions pass. Two new controls reject forged reserved callees, enforce
implementation effect contracts and retain concrete Result error transport.
On Linux, `unix.uptime_seconds` now requires `fs`, matching its shipped file
reader; other platforms retain `process`. Fresh combined qualification remains
required before any compiler-prefix merge to master.
Process default and driver binding repairs pass their two focused controls.
Configured Range proof repair `d1b5534ba` passes seven focused controls and
all seven original canonical process and stack controls. These repairs close
the individually reproduced causes of all 25 failures. A fresh combined
canonical run remains required.
Named-cause repair `4d4c93a81` passes 67 focused controls on its isolated source
and is integrated. The verifier authenticates the exact binding subjects,
payload and cause without inventing a success type. Merged test extension
`f91ef3f8d` covers both explicit returns and the original implicit tail with
the same five physical mutations. Fresh merged replay exposes a real remaining
failure: the earlier tail repair handles expression tails but misses bare
binding tails. Repair `de8e288dd` uses the exact resolved binding and its checked
slot type. Both tail forms now reject all five mutations. All 68 focused
controls pass, including parameter and captured Result forwarding, cleanup
order and broader-error retarget rejection. The repair is integrated.
Obsolete transfer helpers are removed at `6b8ef0329`; the warning-denied
non-test check and all 68 controls pass. Frozen source `d17f140ce` passes
fresh canonical bootstrap, then 231 root integration tests. All 25 earlier
failures pass. The remaining failure is `libc_hygiene::no_migratable_libc_calls`,
which cannot parse its `cargo metadata` output. Native stages do not run.
The failed receipt remains in `full-inference-producers-qualified-r5`.
Source stays clean, cleanup resolves, and peak memory is 4,343,279,616 bytes
with no OOM events. This run supplies no timing or combined acceptance credit.
The metadata failure comes from an uncached locked `webpki-root-certs` 1.0.10.
Fetching that package makes the original unfiltered offline metadata command
and unchanged hygiene test pass. No source, lockfile or audit policy changes.
The fresh R4 attempt on the same source passes bootstrap and all 232 root
integration tests. Ambient-filesystem, symbol and compatibility-boundary groups
also pass. Core compatibility then stops at 31 passes, 27 failures, three
ignores and two filters. Failures include stream provenance in `cp`, record
spread proofs in `expr`, a producer mismatch in the selected `ls` test and
an unproved binary expression in `btop`. Group the remaining terminal and
startup failures by reproduced cause before another full attempt.
Preserve R4 under the same frozen checkout. Later native stages remain unrun.

The owner also requested isolated Laputa certificate hardening. Laputa commit
`2549eb0` removes unauthenticated live certificate downloads, updates packaged
roots and runtimes, and makes CA package dependencies explicit. Seven source
checks and the focused CA recipe test pass. XSH master includes Ed25519 host
verification at `21ce1ed25`, with one registry and two native controls passing.
XSH distrust-policy enforcement and trust-generation invalidation are integrated
through `be7009bb7`. Fifteen Rust controls, two native controls and validation
of the unchanged curl bundle pass. Laputa local master now includes signed
trust-release composition and staging repairs through `f7da5b523`.
Two pre-fix regressions reproduce deletion of caller-owned overlays and
temporary paths. The repairs use exclusively owned private temporary
roots and atomic no-replace publication. Initial privacy follows the existing
mode-at-creation host primitives and independent source review.
The final five-file check passes under both umask `000` and `002`.
All six trust-release and fourteen root-composition tests pass under each
mask, including preservation, publication permissions and one-winner composition.
No forced publication-race test is claimed. The fast-forwarded master is clean
and exactly matches the reviewed source. Nothing is pushed or installed.
The production signing public key remains unprovisioned; authenticated update
deployment still requires an explicitly provisioned key.
These security changes do not supply inference or deployment acceptance credit.

Isolated carrier checkpoint `f506a237b` passes three runtime controls on both
post-disposal routes. Generic callback materialization, polymorphic aliases at
Int and Str in both client orders, and Result-as-data all pass. Three cold
controls reject forged targets, layouts, kinds and nested effects. Independent
review approves that bounded prefix. The warning-denied non-test check still
needs the canonical reuse API's SCC production consumer. Host `.call` and
`utils.cache` adapters, SelfCall completion, recursion and Stream remain open.
This checkpoint is not qualified for master.
Assembly `0c75290cf` merges that checkpoint with the campaign source.
The reviewed resolution preserves the registry's declaration-effect ceiling
and the scoped per-call effect proof. All seven carrier/forgery controls and
all 66 verifier-effect controls pass on that assembly, with no failures or
ignores. The slice is integrated into the campaign branch; master is unchanged.
These targeted passes do not qualify the combined regression boundary.
A later host checkpoint `2518b8e01` passes eighteen focused controls and is
integrated through `b87587160`. Actual Module signatures include parameter
labels. Stream contracts retain their actual program, producer and closed
instance without pulling. Ground parameter validation uses the same recursive
matcher. Cold yield proofs retain lazy UInt checks and delegation behavior.
Independent review approves this bounded prefix. Remaining factory routes,
generic Stream environments and the combined regression boundary remain open.
Range checkpoint `a6d119581` passes ten focused controls and is integrated through
`a57c98136`. It preserves eager allocation and error timing while returning the
declared Stream carrier. Actual declaration kind controls producer suspension.
Bound, item and captured-error forgeries reject. Both expr programs now pass
actual preparation and their native cases on matching verification products.

Recursive command checkpoint `f017423e9` passes nine focused controls and is
integrated through `6d6e2e4b3`. These cover checked command classes, argument
authority, original Stream inference cells, callback instances and atomic
failure. Its combined replay passes 120 of 124 controls with no ignores.
The later constructor-phase repair clears two original Proc callback failures.
Checkpoint `c6565bad5`, integrated through `bdf3f43f0`, also prepares omitted-return
Procs with authored parameters through the existing authoritative body path.
Its seven-control replay passes six, including the real-program counter fixture
and both original cold application checks. Completion checkpoint `d16610e26`
then passes all twelve focused controls. Each terminal branch retains its actual
application and Proc envelope through the existing completion operation.
Formal Result values remain nested data. Integration `7137953cb` passes 202 of
203 combined controls with no ignores. The sole failure is the retained
Stream-function alias positive. Mutual Proc completion, the real-program counter,
both mutable runtime routes and the original cold application checks all pass.
The inherited collecting probe also remains separate work from the authoritative
body check. Stream aliases, terminal commands and native-order checks remain open.

Constructor checkpoint `3342bf3a9` passes all 43 focused checks with no ignores,
including disposal, effect order, physical overwrite proofs and paired bodies.
Its packed body fact occupies sixteen bytes. Authored interfaces now use the
existing declaration schedule before raw type resolution. Capture-order and
atomic-publication controls pass. Integration `13c7d119f` preserves the command
and producer proofs. Explicit `List[Any]` and heterogeneous literals stay dynamic.
The subsequent combined group passes 192 of 197 controls with no ignores.
Three failures expose the ordinary Proc preparation gap repaired by `c6565bad5`.
Mutual Proc completion and the retained Stream-alias positive also fail.
The later 203-control replay above clears those Proc failures.
The native witnesses integrated at `6397a90cf` both reproduce real failures.
Prepared-constant repair `85d6f607a` is now integrated through `6fd59da4b`.
Its 89-test checker/runtime group and four imported-read/nominal controls pass.
The final payload and type forgery control passes, and matching verification
products pass the unchanged native constant witness. Prepared expressions keep
their ordinary leaf semantics and a separate authenticated data-work budget.
The ordinary generic proof bound is unchanged. Mutable-callable checkpoint
`09af29f84` is integrated through `719480599`. Ground and generic reassignment
pass on both execution routes, and forged writer instances reject. Its wider
isolated group passes 27 of 30 controls. Recursive inference and the retained
Stream-alias positive still require combined qualification.

The reviewed producer repair passes all 74 verifier-effect controls, including
both original 128-use bounds, without exclusions or raised limits.
It obtains statement ancestry during the existing ownership walk and retains
the original resolver for owners with expression-region bindings.
The repair is integrated through `489b304cb`. All 88 selected controls pass on
that assembly. Matching isolated products build, and the original 27 native
failures now yield 17 passes and ten failures, with no ignores. The remaining
failures are eight btop cases and two expr cases. Follow-up `ee22c3cc8` passes all
78 verifier controls and is integrated through `586d4bf0c`. Its actual btop
preparation passes after fresh-record, empty-set and immutable-loop guard fixes.
An earlier complete-Optional allowance failed seven unchanged corruption tests
and was withdrawn. Matching verification products build on `ee22c3cc8`.
The original 27 native failures yield 25 passes and two expr failures on that
checkpoint, with no ignores. All eight btop cases pass. On combined `a57c98136`,
matching products pass both expr cases but newly fail all five cp cases during
checking. The resulting 22-pass, five-failure replay has no ignores. Authored
plain-payload Procs reject valid propagation before inferred error effects settle.
This regression blocks qualification. The matching build also emits an unused
counter-getter warning and an XSHT Send-proof recursion-depth warning.
Test adapter repair `bc7e8fffe` preserves the concrete effect assertions after
the checked-effect representation change. All five affected integration tests pass.
These isolated results do not qualify the combined regression boundary.
Work-counter repair `ab26826b9` passes nine focused controls and a non-test
check. It counts discarded diagnostics and nested retained type nodes.
Independent review finds no blocker in that checkpoint. Adaptation `d5ecb43f1`
counts retained generic body evidence, including proc-return sites, without
inventing output types. All fifteen focused counter controls pass on its merged
source. Stream and signal-hook body entries now pass their regression alongside
all sixteen focused counter controls. The accounting prefix is integrated through
`9bd8fe67d`. Repair `6da02f160` counts each constructor role as evidence without
inventing an output type. Its actual constructor fixture passes in the combined
197-control run. The later Proc repair also passes the real-program evidence
fixture. Performance qualification remains open.

The superseded failed producer-qualification checkout is removed after preserving
its complete scratch tree and checking clean ancestry, handles and registration.
Thirty-nine completed checkouts have now been retired. R3 removal partially
fails on generated-file permissions after Git unregisters it. Its orphaned
directory is preserved intact under the retired-qualification archive.
R4's ignored artifacts are also preserved before ordinary removal succeeds.
Both original paths and registrations are absent. The completed constant-assembly
checkout is also removed after confirming clean integration and no consumers.
Its scratch data and ignored artifacts remain in the retired-lane directory.
The completed mutable-callable checkout is removed under the same checks after
the combined runtime controls pass. Its full ignored tree remains archived there.
The active failed
canonical checkout and all unfinished source remain retained.

Previous focused source `50b9d543e` passes 684 merged controls with zero failures
or ignores. Matching verification products build: `xsh` is 36,324,192 bytes
and `xsht` is 44,329,432 bytes. All twelve runner, resource, constructor and
comprehension selectors pass, each selecting one test with zero skips and
unchanged source/product pins. The fresh diagnostic replay passes all four
Laputa source checks. Four XSH checks still fail: two slice-contract and six
checked-input-disagreement diagnostic instances. The frozen candidate remains
the 202-effect probe. No paired runtime, interface or removal credit follows.

Previous focused source `9f66593f3` passes 676 merged controls, with zero failures
or ignores. Its NonEmpty repair has eleven focused passes and independent
approval. Fresh literals require actual members; splices require independent
NonEmpty evidence. Comprehensions retain that validation only from their
actual source through the canonical single, unfiltered mapping rule. Empty,
wrong-member and ordinary-list source mutations are rejected. Matching
verification products build: `xsh` is 36,307,072 bytes and `xsht` is 44,313,360
bytes. Seven existing runner/resource/constructor controls and all five new
comprehension selectors pass, with zero skips and unchanged product pins.
The comprehension file is integrated at `7fef1b5e6`; all five selectors also
pass against that clean integrated source.

Collector repair `50b9d543e` passes its 19 focused controls and independent
review, including both existing work-budget controls. It proves every writer
of the fresh lowered accumulator, preserves actual append types and runtime
checks, and reuses inherited lexical bindings only during the writer census.
The 16/32/64/128-writer cases, scope boundaries, early returns and forged
inputs pass. A separately reproduced scoped-callable baseline failure is also
fixed without dropping context initialization or effect/resource checks.
Fresh merged qualification and matching-product replay produce the results
above. The remaining XSH failures stay owned by the producer lane; this is
not a qualified full compiler baseline.

Isolated compiler checkpoint `4d67a68a5` passes 29 of 35 focused controls.
All 23 foundation controls pass. The source controls preserve immutable aliases,
returned callback identity and one requirement per source application.
Application-result origins retain their constraint owner and normalize through
the owning scheme's application evidence. Six Proc and recursive-return cases
still fail. Outer bound effect publication, latent application causality and
symbolic cold execution remain unfinished. This checkpoint stays outside the
integration branch and earns no F2 acceptance.

Outer-effects checkpoint `e142ea86f` compiles and passes 32 of
those 35 controls. Proc client order, Result-as-data and application-result
aliases now pass. All five default-ownership and scoped graph controls pass,
including rejected missing or wrong-owner substitution maps. The public
`CheckedEffects` projection distinguishes unknown, finite and symbolic effects.
Cross-edit comparisons retain canonical callable and body proof evidence.
The isolated XSHT consumer migration starts from this unqualified checkpoint.

After Capture admission through the existing body visitor, eleven of thirteen
source controls pass. Both captured and uncaptured error contributions pass.
Self and mutual recursion still fail at the explicit-return inference boundary.
The mutual source case has not reached its effect-map check. Cold execution and
effect-scheme acceptance remain unfinished.

Unqualified prerequisite `7a03e30fb` passes 26 of 28 selected controls.
Forwarded applications retain caller-owned obligations and actual returned
layouts. Ground calls still require selected argument evidence. Independent
source review finds no defect in that mapping. Cold verification must also
reject an in-range ordinal pointing to the wrong caller obligation.

Unqualified compiler checkpoint `096edb232` preserves inferred nominal return
schemas from actual constructors, separately from contextual expectations.
Both focused schema controls and the exact prepared-body comparison pass;
independent review approves this prerequisite only. Mutable reads cannot supply
constructor identity. The original private-return autofix again proposes both
expected fixes on this source. Generic Capture still fails during indexed
preparation, so this checkpoint earns no F2 or cold-execution acceptance.

The isolated seven-file XSHT migration at `9470db0ca` passes eleven focused
controls on that checkpoint, with zero failures or ignores and independent
review approval.
Finite, unknown and symbolic permissions remain distinct. Original effect-fact
coverage is required before any reload, and annotation removal requires a
successful canonical body proof. Three regressions reproduce and fix the
previous fail-open caller paths. The generic Capture body-contract fixture
remains known red because of the compiler prerequisite above; full tooling
acceptance remains open.

Isolated Result-pattern prerequisite `42af589d2` passes eight focused controls
and independent review. Built-in Ok/Err patterns constrain unresolved subjects
at their original solver level with transactional ownership. Enum shadowing
and incompatible callback results retain their checks. An ordinary typed-input
reference establishes the canonical inferred Error join; the new narrower
expectation was corrected without changing that policy. Cold execution and
recursive component inference remain unfinished.

The isolated producer repair now passes the actual frozen `cut` source and
its portable counter, self-slice and field-guard controls. Independently
anchored storage types retain all writer obligations. Six focused order and
guard controls pass, including rejected mutations of Binary, List, Fmt, Tag,
later conditions and completing branches. These results include two reproduced
evaluation-order defects repaired during review. Final classification review,
statement-level controls and work-budget qualification remain required.

Three further portable regressions reproduce the advanced corpus failures:
nullable conditional storage, immutable Boolean aliases of field guards, and
narrowed return values lifted into `Result`. The return-lifting control passes
after repair. The other two relationships remain under repair. The actual
AWK, developer module and system-report checks must pass before merged corpus
qualification. These isolated results earn no interface or removal credit.

The later isolated producer repair passes all four actual source preparations:
cut, AWK, the developer module and system-report. The 16/32/64/128-field guarded
workload also passes without optional hints, on both execution routes, under
the unchanged work limits. The repair removes expression visits from the writer
census only when complete statement coverage proves the old traversal's result,
and reuses positive predicates only for the same immutable snapshot and guard.
Final portable regression qualification, cleaned-source review and fresh merged
corpus replay remain required. No interface or annotation-removal credit follows.

The fresh eight-check corpus replay repeats all twenty missing producer
contracts and two slice-contract failures from `520757c30`. The first actual
Laputa failure is reproduced in `pm/graph.xsh::edges`: a lowered `collect`
block ends in its unannotated empty-list accumulator, whose element type
is not derived from its yield writers. This class is under repair; no full
baseline or corpus acceptance follows yet.

Source `4e0863e6c` combines the reviewed assignment and producer repairs for
fresh merged qualification. Assignment source `6591e9ab7` passes 21 focused
controls before integration. Its merged boundary has 609 passes and 24 failures:
the previous 22 producer failures and two test navigators that assume the old
assignment operand layout. The test-only repair restores those mutations
through the actual lazy operand blocks. It also confirms that a truthful
List[Int] RHS cannot change List[UInt] compound-assignment storage. All three
focused controls pass before rebase; production is unchanged by that repair.

Producer repair `4ce9e2c06` passes 45 focused controls before rebase and has
independent approval. It authenticates guarded storage, captured setters,
fresh checked producers and every comprehension target, including unused
record targets. Existing work limits remain unchanged. The combined merged
run on `4e0863e6c` selects 643 tests: 641 pass, two fail and none are ignored.
All 24 failures from the earlier assignment run are resolved. Two optional
typed-call producer controls regress during preparation because the receiver
lacks a non-null branch proof.

Repair `520757c30` passes the fresh merged boundary: 645 passed, zero failed
and zero ignored, including both retained budget-exhaustion controls. Its lane
boundary passes all 49 controls and independent review finds no P1/P2 issue.
Early callable validation and physical producer verification share the completed
owner census and its ordered guard proof. Every incomplete optional-payload
carrier requires that presence proof even when wrapper facts are absent.
A valid-first mutation that reverses the preceding guard is rejected. Prefix
scans borrow the existing child forest, and completed positive presence proofs
are reused only for their exact physical gate within that owner. Work limits
remain unchanged. Matching verification products build: `xsh` is 36,240,248
bytes and `xsht` is 44,245,808 bytes. All six runner/resource controls and the
qualified-alias generic-constructor selector pass with zero skips; source and
products remain unchanged. Documentation, corpus and full baseline acceptance
remain pending.

Argument source `2b37fee8c` passes the fresh merged boundary: 673 passed,
zero failed and zero ignored. Its 29 focused controls and independent review
cover canonical named spreads, final rest/splice layout, omitted interior
defaults in declaration scope, selected registered conversions, and actual
default/argument/effect/substitution proofs after frontend disposal. The same
expansion and argument-order policy serves concrete types and published
schemes. Standalone Result propagation uses the existing indexed Try operation.
These source-acceptance changes have matching SPEC and Architecture updates;
verification products still describe `520757c30`.

Eleven new native fixture probes against the `520757c30` pair have eight passes
and three compiler failures, with no skips. NonEmpty preservation fails first
on its fresh list input; generic mutable-row refinement after a disjoint write
and a generic deferred body also fail. The producer and core-inference lanes
own these classes. Initial qualifier-syntax and UInt assertion/display fixture
errors were corrected before this count. No capability or corpus exclusion
follows from these failures.

The matching `520757c30` diagnostic corpus replay fails all eight ordinary
checks: twenty missing checked-value producer contracts and two missing slice
collection contracts. Original and diagnostic/reduced views fail identically;
the frozen candidate is the 202-effect probe, not an accepted removal view.
All source and product pins remain unchanged. The producer lane owns these
failures. No interface, runtime-parity or annotation-removal credit follows.

Source `b0f594598` integrates reviewed generic List, Map and closed-key Set
comprehensions. All 20 focused controls pass before integration. They cover
sequential binding scope, actual producer domains, lazy qualifiers and
projections, Bool and Status filters, validation survival, and stream cleanup
on both indexed routes. Synthetic lazy blocks retain canonical resource
transfer evidence. The merged boundary selects 594 tests: 572 pass, 22 fail,
and zero are ignored. Its failed set exactly matches the previous iteration
boundary. Matching native products, documentation checks and corpus parity
remain pending. This is not a qualified compiler baseline.

Source `c0a282df8` integrates reviewed generic destructuring and iteration.
The lane's 25 focused controls pass before rebase. They cover source-first
binding publication, invariant writers, actual iterable item types, finite
and Result sources, lazy stream cleanup, and owner error boundaries on both
indexed routes. Ordinary and generic lowering share Result-stream adaptation.
A corrected coordinated callee mutation passes target/effect/header assertions,
then is accepted when the new stream-owner proof is omitted and rejected when
it is retained. The earlier wrong-ID mutation establishes no bypass.
The inherited merged boundary in `.work/wt/full-inference-iteration-qualified`
finishes with 549 passes and 22 failures, zero ignored. The iteration controls
and ordinary neighbors pass. All failures belong to the preceding producer
boundary; its unsigned-membership failure now passes. The producer lane's
guard, aggregate and work-limit repairs remain unmerged, so this source is
not a qualified compiler baseline.

Source `01b6b5a64` adds actual paired-comparator controls. All three selected
XSHT tests pass after integration in `.work/wt/full-inference-paired-qualified`.
One unchanged generic body preserves both concrete Int and Str instances.
Folded operator/domain changes beneath list and record parents are rejected;
malformed source is distinguished from a checked-body mismatch. These tests
do not qualify the frozen corpus or repair the producer regressions below.

Source `323a3e52a` is integrated after independent review of contextual
aggregate producer proofs. Fresh aggregate carriers require an authenticated
consumer; every physical record entry remains proved, including overwritten
values. Checked storage retains actual source-domain obligations. Native
argument facts retain exact owner/instruction identity, and the mandatory
census revisits only its collected pattern candidates. Eleven focused controls
pass, including the tracked generic-constructor preparation that previously
exhausted the unchanged work bound. The merged boundary in
`.work/wt/full-inference-context-qualified` finishes with 521 passes and 23
failures. Seventeen are new regressions: existing checked-value producers are
rejected, and two driver-projection controls exhaust the work bound. The six
inherited optional-field failures remain. The producer lane owns all repairs;
this source is not qualified and earns no corpus or annotation-removal credit.

Source `26d87d2bc` is integrated with reviewed generic operator contracts and
a shared comparison-chain representation containing each real operand once.
Ordinary depth-8/depth-12 preparation emits 60/88 rows, versus 3,574/57,334
before repair; generic preparation emits 190/282 and stays within unchanged
work limits. These are deterministic row counts, not timing measurements.
The focused operator and completion controls pass. The expanded merged boundary
has 527 passes and six failures, all in the existing typed-callable optional
field controls. Running those controls on `054fc7a3a` reproduces the same six
failures. Guard, loop-backedge, writer and block-order proof repairs remain
mandatory; this is not a fully qualified compiler baseline.

Source `054fc7a3a` passes the earlier combined 489-test targeted boundary after local
integration. Quantified bodies elaborate shorthand fields through resolved
binding identities and static record updates through the existing checked
paths. Both indexed routes preserve shadowing, immutable captures, base-before-
replacement evaluation and failure order. Typed Set checks remain executable;
truthful and coordinated wrong-domain producers are rejected. The 256- and
512-field update controls stay within the existing work bound. Independent
source review found no blocking issue. Matching products and corpus acceptance
remain at the earlier source below; this tranche earns no removal credit.

Source `8f80bc2f5` passes the earlier combined targeted boundary: 473 passed, zero
failed and zero ignored. Optional aggregate producers prove every actual field
and element. Immutable pattern reads retain narrowing under direct null guards
in active if and while bodies, or after a matching branch that cannot complete.
Wrong guards and writers fail; null exclusion cannot invent unsigned or
validated domains. Hint-erased execution passes on both routes, including a
32-node negation chain with charged iterative traversal. Ordinary unary-`!`
and short-circuit guard forms still fail preparation in confirmed minimal
sources; the producer lane retains them alongside the contextual carrier,
conditional-list and `cut` failures. No new corpus parity or annotation-removal
credit follows from these targeted tests.

Matching verification products build in `.work/wt/full-inference-producer-corpus`
under `.work/target/x86_64-unknown-linux-musl/verification`. The `xsh` is
35,929,080 bytes, SHA-256
`6a83777a7b1a26ca734350fd105d14d799c446833cdf26153ae99ad1e7c2bb3d`.
The `xsht` is 43,913,056 bytes, SHA-256
`cc49f85ffdc30f7cd2320b3d5e044e0e03e985b081487776355a15245729f094`.
The matching pair passes all six focused native Cargo-runner and hidden-resource
controls, with unchanged source and products before and after execution.
All eight normal corpus checks finish. Four Laputa checks pass. Frozen XSH
original and candidate still fail identically on `cut`, three conditional-list
splices and the `dev/main` list subject; the `system-report` record error is gone.
Practical XSH original and reduced still fail identically: `awk` now reaches a
missing loop-item contract instead of the prior nullable read failure, and
`sort` retains its map-subject failure. Interface and runtime parity remain
unrun, with no annotation-removal credit.
Faithful isolated reducers for frozen sites 3476, 7427, 2285 and 6338 pass both
annotated and erased checks with this pair. The first two previously failed
when erased. The mutable-record reducer includes its original import closure;
earlier missing-module fixture attempts supply no compiler evidence. These
admission results do not qualify the original files or earn removal credit.

Source `f53eb38d1` passes the earlier combined targeted boundary and construction
neighbors: 468 passed, zero failed and zero ignored. Generic bodies prepare
checked interpolation, list splices, ordered record spreads, computed maps and
ground-domain sets through the existing indexed operations. Both runtime routes
preserve values, overwritten-expression evaluation and first-failure order after
frontend disposal. Truthful malformed display, splice, map and spread producers
are rejected; formatting parts are charged before decoding. No new native
command path or registry policy is introduced. Matching products remain at
`cf1d47637`; corpus qualification and remaining body families stay open.

Source `a9be9aafa` passes the earlier combined targeted boundary and Optional inference
neighbors: 454 passed, zero failed and zero ignored. Null and concrete branches
infer one Optional envelope in either order. Required annotations and invariant
container domains retain their authority. Checked statement-tail joins survive
generalization and preparation; every actual arm remains independently proved.
Unresolved Optional returns preserve nested Result values as data on both
indexed routes after frontend disposal. Ordinary dynamic-return propagation
and the underdetermined dynamic-return diagnostic remain unchanged. SPEC and
Architecture describe this contract expansion. Matching products remain at
`cf1d47637`; corpus parity and broader native acceptance remain open.

Source `d052c08c0` passes the earlier combined targeted boundary: 432 passed, zero
failed and zero ignored. Generic Optional field, index, slice and selected
method access preserve null handling and lazy operands. Nullable return values
retain nested Result data. Actual Result errors fit the owner's boundary;
potentially failing unsigned requirements cannot escape a narrower error
family. Directional Optional widening preserves ordinary explicit matches.
Valid-first malformed-IR controls and both indexed runtime routes pass.
Matching products remain at `cf1d47637`; corpus and broader native qualification
do not transfer to this source. Nullable source joins and other F3 families
remain mandatory work.

Source `cf1d47637` passes the earlier combined targeted boundary: 414 passed, zero
failed and zero ignored. Driver projections prove their actual source,
envelope and every writer, including unused bindings and zero-output patterns.
Fresh command-word lists retain contextual typing only through authenticated
compiler-created bindings. Native process-option projection preserves every
other registered contract. Wide destructuring stays within the existing proof
work limits. All three exact native Cargo-runner bodies pass with matching
products: lock handoff, unmatched-filter rejection and lock release. The
source remains clean and product hashes match before and after the replay.
Broader native acceptance and corpus parity remain unqualified at this source.
All three hidden-resource lifetime controls also pass with matching products.
The qualified-alias generic-constructor selector fails preparation on a list
match subject at owner 11, instruction 261, subject 251. Its body does not run.
The producer lane owns this smaller regression alongside the corpus failures.

The matching-product corpus replay finishes all eight normal checks. All four
Laputa checks pass. Frozen XSH original and candidate fail identically on the
prior slice, record and list-match-subject proofs, plus three fresh-list-splice
contract mismatches in `dev/docker`, `dev/gates` and `dev/internal`. Practical
XSH original and reduced retain the prior `awk` producer disagreement and
`sort` map-match-subject failure. The producer lane owns the new splice
regressions together with the five existing classes. Interface comparison
and runtime parity remain unrun. These checks supply no annotation-removal credit.

Matching verification products build in `.work/wt/full-inference-driver-projection`
under `.work/target/x86_64-unknown-linux-musl/verification`. The `xsh` is
35,875,072 bytes, SHA-256
`50ae360b081ed9fcee035ff50ec42e3e96521e2edddfe598c95b81c984245f04`.
The `xsht` is 43,859,504 bytes, SHA-256
`6dc77944c9c3c2967bca4c43d25a5ff18abd58190beb710690e8139a3446d85e`.

Source `3c92adfb3` passes the earlier combined targeted boundary: 407 passed, zero
failed and zero ignored. Generic bodies prepare unguarded list indexing and
slicing, literal record-key indexing through compiler-created temporaries,
from-end literal indexing, and concrete arithmetic and list concatenation.
Actual operands authenticate unsigned list guarantees. Ordinary runtime checks
and invariant list typing remain intact. Validated list projections and
`NonEmpty` concatenation retain their contracts. Shared classifier and
512-block work-bound controls pass.

Guarded postfix access remains mandatory work. The next witness,
`pure keep(value, items: List[Int]?) { let _ = items?[0]; value }`, passes
checking but fails preparation at the Optional parameter representation.
Matching verification products remain at `934eab3b1`; no new corpus or native
acceptance follows from the targeted source checks.

Two faithful frozen-corpus reducers confirm the next nullable-join class:
site 3476 joins an `env.get` string with null, and site 7427 joins null with
a parsed integer. Each annotated version passes with the `934eab3b1` products;
each erased version fails despite its optional destination contract. A separate
site 2483 reducer infers its forwarded source schema in both versions. These
reducers identify relationships to repair; they supply no original-corpus
qualification or annotation-removal credit.

Source `934eab3b1` passes the earlier combined targeted boundary: 389 passed, zero
failed and zero ignored. Generic bodies now prepare lexical mutable and
immutable locals, match arms, while and value loops, assignments, explicit
discards and early returns. Declaration facts prove storage type, mutability,
owner and instruction identity. Every writer is checked. Compound results
cannot invent UInt validation, while lawful UInt-to-Int local storage passes.
The iterative lexical-frame proof passes the 256- and 512-block controls under
unchanged work limits. SPEC and Architecture describe this accepted tranche
and the remaining unsupported preparation forms. Broader F3 families and
matching-product corpus qualification remain open.

Matching verification products build in `.work/wt/full-inference-bodies` under
`.work/target/x86_64-unknown-linux-musl/verification`. The `xsh` is 35,780,280
bytes, SHA-256 `6fbad331c90aba0918a082a97b96bc833474f9ce8a9d693d23e4712b02806200`.
The `xsht` is 43,768,424 bytes, SHA-256
`8f5b96c53d5d46a93eaa97a1a03e507fa97b314778a68bd11844e5a73c7c4704`.
Eight normal corpus checks preserve both source cohorts and both pairs of
views. All four Laputa checks pass. Frozen XSH original and candidate fail
identically on slice proof in `cut`, record proof in `system-report`, and a
list match subject in `dev/main`. Practical XSH original and reduced fail
identically on a producer-contract mismatch in `awk` and a map match subject
in `sort`. The prior tail-less discard and Abort diagnostics are gone.
Interface comparison and runtime parity remain unrun. No annotation-removal
credit follows from these normal checks.

Source `cce02595c` passes the earlier combined targeted boundary: 362 passed, zero
failed and zero ignored. Normally completing tail-less value blocks prove Unit,
including compatibility with Any. Noncompleting blocks remain distinct from
Unit, and unresolved value tails retain their uncertainty. Abort rows prove
an Int status operand and require the process effect. The actual `lib.text_a2`
import and malformed status/effect controls pass. That checkpoint has no separate matching products or full native acceptance.

Source `ae9793745` passes the earlier combined targeted boundary: 357 passed, zero
failed and zero ignored. Scalar slots now use canonical lexical projections;
native overload shape determines a result only with unambiguous authority,
and actual operands remain independently checked. Fold accumulator proposals
must prove the actual seed, callback and every writer, including with optional
facts removed. Spawn, Wait and Command rows prove operands, canonical options,
result envelopes and their owner's return contract. Opaque Record environments
retain the existing API boundary. Two corruption tests now require the earlier
scalar predicate rejection instead of the later writer rejection. No invalid
program was admitted to make the replay pass. Matching verification products
build in `.work/wt/full-inference-process-contract`. The `xsh` is 35,587,984
bytes, SHA-256 `a4dcdc98ca6841493879f2316547865a8ef76e038320cb14bb245afb74983636`.
The `xsht` is 43,577,840 bytes, SHA-256
`1acbfb4fda8368b2f7940aeb8c952888a7c33d07546a54e4ee3f7cebe91a0666`.
Both are under `.work/target/x86_64-unknown-linux-musl/verification`.
Full corpus, docs, native and performance qualification remain open.

The matching-product replay checks the original and joint 202-effect-removal
views once each. Both Laputa checks pass. Both ten-root XSH checks fail with
identical diagnostics: implicit discard in `cut` and `tr`, Abort in `date`,
a missing record contract in `system-report`, and a missing match-subject
contract in `dev/main`. Scalar failures are gone. Interface comparison remains
unrun, and these checks supply no annotation-removal credit.

Guard `6bfb3ca22` rejects an unmatched focused Cargo test filter with exit 125.
The actual pinned Rust test binary passes one exact test, rejects a typo before
body execution, and lists its tests without running them. The inherited Cargo
lock is released before discovery and execution. The matching `ae9793745` pair
fails preparation on the first XSH runner selector, before its body runs.
The later `cf1d47637` driver repair proves the actual contextual command-word
producer, and all three native runner selectors pass with matching products.

Source `0b33e84bf` passes the earlier combined targeted boundary: 342 passed, zero
failed and zero ignored, including the previously failing dynamic-module-call
selector. Dynamic calls through required Module exports prove actual arguments,
defaults, results and effects. Runtime-loaded module calls execute on both
routes; erased unknown Proc calls still fail finite effect ceilings.
Matching verification products build in `.work/wt/full-inference-dynamic-contract`.
The imported constructor annotation selector now passes, as do all three
hidden-resource lifetime controls, with zero skips. No full native, docs,
corpus or performance acceptance follows from these focused results.

The matching `xsh` is 35,560,776 bytes, SHA-256
`3bd5fc41a9bc425b35ca5f2cf61f57a1b8f07fdd22a262199c7267a54b389ddb`;
`xsht` is 43,550,736 bytes, SHA-256
`4a9b47543bc2e3ef73fd8079b8af3271f8b83d682082b00b7de770b9ac8d3947`.
Both are under `.work/target/x86_64-unknown-linux-musl/verification`
in that frozen checkout. Earlier product evidence remains bound to its source.

Source `12f65d5ee` passes the earlier combined targeted boundary: 338 passed, zero
failed and zero ignored. Imported module reads use the actual verified Use
origin and registered export contracts, including reads with erased precision.
Five portable regressions cover execution after frontend disposal on both
routes, wrong-child exports, forged registrations and member-context forgery.
The broad-read regression fails under the old gate and passes with the fix.
The process-contract lane now owns the remaining Spawn and Wait proofs.

Corpus update `3f2535d9b` preserves the frozen denominator and validates exact
Map initializer shapes. All fourteen focused witness controls pass after
integration. The 758 historical Proc annotations remain an in-scope inference
frontier; conditional retention ceilings do not make them language limits.
No new annotation-removal or practical-runtime credit follows.

Source `1eeacf2e9` passes the earlier combined targeted boundary: 333 passed, zero
failed and zero ignored. It includes canonical environment assignment and
managed-resource release contracts, both indexed execution routes, wrong-value
and wrong-resource mutations, effect ceilings, and generated resource-scope
Bool and Result payload writer proofs. Internal descriptors reuse the existing
conversion policy and resource registry. Matching products and full acceptance
remain required.

Constructor correction `7b22689ef` replaces implicit recoverable validation
with the specified typed storage boundary. Negative UInt fields now stop with
`type-error`, run cleanup and escape `try`; explicit `.require(UInt)` still
returns validation errors as data. Three focused controls pass. A broader
346-test replay has 345 passes and one existing dynamic-module-call failure.
That exact failure also reproduces before the correction at `f3e4eb405`:
`ExprDynamicCall` has an unknown effect contract. It remains mandatory work;
it is not waived by the targeted boundary above.

Active implementation owns ordinary generic bodies and driver projection
proofs. Independent review, corpus qualification and verification speed follow
each accepted tranche. Lease change `b1c1b886d` permits at most two ordinary
Cargo builds with disjoint output directories. Existing exclusive builds and
performance windows still exclude them. Nineteen real-process controls pass,
including same-output exclusion, module-alias safety and interruption cleanup.
No lockfiles or job registry were added. The focused launcher rejects an exact
test filter that selects zero tests. Final gates retain their existing scope.

Read-only F2 analysis at `d052c08c0` separates two remaining mechanisms.
Concrete recursive effect propagation already exists. Joint clause comparison
still supports only one unclaused declaration, while generic callback effects
lack symbolic relationships in the canonical scheme constraints. The recursive
`item_html`/`blocks_html`/`block_html` group in `dev/tour_html.xsh` supplies a
source-supported joint witness, not newly resolved checker edges. The 60
historical non-findings do not establish missing propagation; authored effect
restrictions remain meaningful. All 202 private clauses remain pending.

## Current checkpoint

Source `cac5a2ef9` passes the combined immutable replay: 323 passed, zero
failed and zero ignored. Independently reviewed field projection resolves the
selected closed-record field through the existing type constraints, including
Result and Optional projections. Context-free Str, List and Stream iteration
now preserves a generic Proc's returned field relationships. Cross-binder
mismatches and non-iterable fields still reject. All 64 focused projection and
neighboring controls pass. Matching products remain frozen at `d2c820629`;
these source checks do not qualify new products.

Matching `d2c820629` products pass 13 selected native controls and fail the
imported record-constructor annotation-retention case. The preserved
`e0f8cbfac` pair reproduces that exact failure. It is a preexisting constructor
lowering gap, not evidence of a new regression. Source checking passes all
80 API examples; 12 of the 202 docs-owned files still fail preparation. The
actual docs command stops at the missing physical spawn proof. These checks
supply no full docs or corpus acceptance.

Source `d2c820629` passes the combined immutable replay: 297 passed, zero
failed and zero ignored. Independent review approves immediate error inference
for unbounded typed Proc calls. Such a call widens a Capture to Error before
its signature is published. Pure calls and saved Result and Stream data retain
their existing behavior. Effect ceilings remain unchanged. All 41 focused
source, neighbor and physical controls pass. Matching-product qualification is
next. Context-free iteration over a generic Proc's returned record field still
fails with `check.for-iterator`. Annotation-directed field controls pass, so
they do not close that separate consumer-resolution gap.

Source `f854ea6e2` passes the combined immutable replay: 295 passed, zero
failed and zero ignored. Independent review approves canonical Path receiver
contracts and physical Method and ModuleCall operand checks. Wrong receivers,
destinations and filesystem bounds reject. Pure module signatures now carry
empty effect metadata, while Path filesystem methods retain `fs`. The shared
integer-mode and Path executable operation selects its actual contract.
One generated dispatch table distinguishes public call support from executable
internal routes. Method-only opcode forgeries reject. Legitimate JSON bridges
pass, while a forged declaration contract or escaped instruction owner rejects.
The 253-test focused replay includes unchanged public-support assertions and
all signature controls. Matching products and actual docs and corpus replays
remain required.

Source `a320fbac9` passes the combined immutable replay: 287 passed, zero
failed and zero ignored. Independent review approves source Stream-origin
diagnostics and inferred Capture errors through returns, projections, bindings
and loop backedges. The existing file probes settle these relationships before
publishing signatures. The 69-case matrix and six driver cases cover mutable
captures, global writes, branch joins and reachable loop transfers. Safe saved
streams remain data. Existing module-cache work-count assertions pass unchanged.
The focused source and neighboring group passes all 43 controls. Matching
products, native replay and corpus qualification remain required. Generic Proc
record-return projection and unknown impure source inference remain open.

Source `15ba7642d` passes the combined immutable replay: 245 passed, zero
failed and zero ignored. Independent review approves Result-constructor mapping
through named-argument bindings. Bounded traversal retains argument evaluation
order and attaches the checked fact to the actual constructor. Both indexed
routes preserve the outer error and its cause. A valid-first in-place integer
cause mutation reaches the specific physical producer-contract rejection.
The unique argument-initializer control passes. Matching products and actual
docs and corpus replays remain required.

Source `d8e6dc342` passes the combined immutable replay: 243 passed, zero
failed and zero ignored. Independent review approves the physical abort-discard
repair. `exit` produces no discarded value, while its status operand retains
ordinary type and effect checks. The regression reproduces the former missing
value-contract rejection and preserves status 77 on both indexed routes.
Valid-first mutations retain rejection of a live expression without a value
contract and an exit-status call whose effect exceeds its owner's bound.
All 22 focused effect and abort controls pass. Matching products, snippet
replay and practical-corpus replay remain required. Existing products stay at
`e0f8cbfac`.

Source `80fea2f8d` fixes attribution of spanless compact-build errors to the
current entry. Explicit source spans, diagnostic codes and messages remain
unchanged. The two-source rendering regression fails before the fix and passes
afterward, including a blocker with an explicit span in the other source.
The coordinator's immutable merged replay passes that exact test.
Matching products below remain at `e0f8cbfac`; this reporting repair does not
repair the seventeen snippet preparation failures or qualify a new product pair.

Source `e0f8cbfac` passes the combined immutable replay: 238 passed, zero
failed and zero ignored. Independent review approves contextual verification
through an exact identity-match binding. The actual subject still proves the
expected contract through its canonical native operation. The minimal inline
fold and typed-seed controls produce the expected map on both indexed routes.
Changing MapEmpty to SetEmpty, removing the binding or returning another slot
rejects. The focused producer and Capture group passes all 61 controls.
Matching verification products build. All nine native annotation cases, three
hidden-resource cases and two docs fixture cases pass with zero failures or
skips. Actual repository docs replay passes the former parallel-map failure,
then stops at `tour/30-cancel.xsh`: ExprSpawnRun has no executable type proof.
Full docs qualification remains open.

Per-file checking establishes 16 unexpected preparation failures among the 202
docs-owned examples. Rejected examples match their diagnostic annotations.
A separate 80-file API pass finds one failure. The earlier directory invocation
selects 221 files and misattributes some diagnostic paths. It does not define
the docs-owned scope. The exact remaining failures under `docs/snippets` are:

| Failure class | Source files |
|---|---|
| Process producer proof | `tour/20-spawn-wait.xsh`, `tour/30-cancel.xsh`, `spec/35-spawn-wait.xsh` |
| Lambda body lowering | `tour/27-preflight.xsh` |
| Dynamic call effect | `spec/12-module-contract-failure.xsh`, `spec/47-exact-module-contract.xsh` |
| Capture identity | `spec/22-with-else.xsh` |
| Native module effect | `spec/43-env-strings.xsh`, `spec/65-path-kind.xsh`, `spec/69-argument-label.xsh`, `spec/71-with-resource.xsh` |
| Noncompletion discard | `spec/49-exit.xsh` |
| Result cause producer | `spec/61-fail-because.xsh`, `api/core-causes.xsh` |
| Deferred error effect | `spec/62-errdefer.xsh` |
| Match subject producer | `spec/64-try-run.xsh` |
| Membership container producer | `spec/69-sets.xsh` |

The repair removes optional native-subject hints in its regression. It retains
and independently checks the concrete match and accumulator claim. Three proof
completeness failures remain required work in
`strict_result_producers_ground_contextual_fold_seeds_inside_capture`:
removing all optional facts reaches a propagation with no derived Result
contract; adding `result?.get("k")` reaches a method with no registered
parameter contract; removing the identity match's concrete claim leaves the
mutable accumulator writer with no declared collection. These cases do not
receive qualification from the narrower repair.

Source `55da2de13` passes the combined immutable replay: 237 passed, zero
failed and zero ignored. Independent review approves the shared source checker
repair for actual Stream pulls in loops and comprehensions. Known fallible
pulls now contribute their error through the existing nearest propagation
boundary. Safe pulls and saved streams remain valid. Seven source cases check
the accepted forms and exact diagnostic code, message and failing-call span.
Source provenance through relays and Result wrappers remains open. A fifteen-case
source matrix accepts all nine controls but incorrectly accepts all six narrow
fallible cases: direct relay, Result wrapper, projected wrapper, selected
returned input, alias and typed alias. The repair must jointly settle stream
origins and effects in the existing file probe loop before final checking.
A nominal-seeded capture must infer Error for a fallible relay and retain the
nominal family for a safe relay, independently of declaration order.
Source `a320fbac9` above closes this source-origin repair, including stale
origins across mutable captures, loop backedges, match arms and global writes.
The earlier native results below remain bound to `18c920910`; they do not
qualify this newer source. The reduced docs regression is a Capture around
`fold(map.empty())`. A typed seed passes, as does the fold outside Capture.
Repair its generated identity-match contract before rebuilding products.

Source `18c920910` passes the combined immutable replay: 233 passed, zero
failed and zero ignored. Independent review approves physical ground Capture
verification and its two runtime repairs. Capture verifies actual success
values and propagated error families, including consumed streams through typed
and recursive relays. Saved streams and nested Result values remain data.
Canonical native contracts, nearest catch boundaries and existing verification
work caps remain authoritative. Captured stream tails retain their resource
owner. Failed Result iteration preserves its original payload and propagates
through cleanup to the nearest capture.

The compiler lane's focused group passes all 20 controls. Runtime controls
exercise normal and forced indexed routes. Matching verification products build.
All nine native annotation cases, three hidden-resource cases and two docs
fixture cases pass with zero failures or skips. The actual repository docs
check fails in `docs/snippets/tour/19-par-map.xsh`: a ground match subject
produced by `ExprModuleCall` has no independently derived contract. Repair and
replay remain required. A rejected two-filter invocation ran no docs tests;
both exact fixture selectors subsequently pass separately.
Source-origin diagnostics remain mandatory: consumed fallible streams can still
pass a narrow source Capture contract before physical verification rejects it.
The combined stream fixture also exceeds the unchanged proof-work budget when
its typed relay case joins the larger aggregate. Separate minimal controls pass;
that split does not qualify the aggregate. Repair both classes before closure.
Generic Try/Capture admission, ordinary generic-body frontiers, corpus parity,
performance, deployment and post-consolidation cleanup remain open.

Source `0a948eab4` passes the combined immutable replay: 213 passed, zero
failed and zero ignored. Independent review approves command-argument
verification. Command constructors authenticate targets, argument words and
every supplied option against native contracts, including without Result
constructors or optional type hints. Known word lists retain their invariant
storage contract; heterogeneous literals prove each element. Dynamic list
reads, incompatible operands and forged literal elements reject. Validation
wrappers and the existing verification work caps remain intact.
Ground and generic try-capture transport, ordinary generic-body frontiers,
matching products and complete capability/corpus qualification remain pending.

A diagnostic per-root replay with matching `d2c820629` products still fails ten
of the twelve original XSH roots; date-parse and docker pass. Failure frontiers
have changed: wc, date, gates and internal now stop at import producer contracts;
AWK stops at a generated constructor Try outside its inferred effect contract;
sort, cut and tr lack discard proofs; system-report lacks a scalar predicate
slot contract; main has an unknown dynamic-call effect contract. This isolated
original-view replay does not run reduced views, Laputa, interface/body
comparison or runtime selectors, and supplies no annotation-removal credit.

Practical replay with matching `e0f8cbfac` products passes exact source-view
validation, then fails the original twelve-root XSH check with six
`compact.indexed-build` diagnostics. Individual checks attribute failures to ten
of the twelve roots: wc, sort, AWK, cut and tr lack discard proofs; date,
internal and main lack native effect contracts; system-report lacks the Result
cause producer; gates lacks an import module contract. Date-parse and docker
pass source checking. The shared batch deduplicates some failures and does not
count failing roots. The reduced view, three Laputa roots, signature
and body comparison, and runtime selectors do not run. The four selected
annotation removals receive no new parity credit. The cohort remains XSH
`4233e34ee` and Laputa `a81760724`, separate from the historical 1,040 result.
Container-generated views preserve the pinned archive modes. Earlier setup
failures ran no cohort checks.

Practical corpus adapter `b84a128ef` now applies semantic interface and body
comparison to the same 12 XSH and three Laputa roots used by paired checking.
Its sixth runtime selector covers AWK include diagnostics and both changed
parser helpers. Shell syntax and all eight existing adapter controls pass;
those controls exercise frozen mode with stub products. Matching-product practical
replay is blocked at the original XSH check above. Historical wc diagnostic probes
and exact revealed-signature comparisons still need qualification against the
new inference behavior.

Source `475f8be3a` passes the combined immutable replay: 209 passed, zero
failed and zero ignored. Independent review approves its condition and
producer repair. Conditions authenticate their actual operands, pipeline
callback types come from bounded iterative prefix proofs, and mutable partial
writes use the independently proved root type. Coordinated slot-fact and
writer corruption rejects. Canonical builtin templates preserve contextual
empty-container types. Returned fallbacks preserve implicit scalar lifting
and nested Result data. The mixed-effect regression now corrupts an actual
operand instead of requiring optional type evidence.
Try-capture transport remains a required repair.
Matching products, native replay and complete capability/corpus qualification
remain pending.

Source `07ff1bbbd` integrates the reviewed resource-specialization repair.
Its immutable merged replay passes 64 focused body, callable and resource
controls with zero failures. One existing manual attribution test is ignored.
Body comparison specializes transfer operands through the same solved header
instance. Missing evidence, crossed binders and removed UInt guards reject.
Record and tag types may retain resources, including hidden payloads.
Canonical run-value sites preserve propagation selections.
Generic tail matches, break-valued loops and mutable proc bodies still fail
checker probes and remain required compiler work. Corpus comparison and fresh
native ownership replay remain pending.

Previous checkpoint `052b13165` passes 184 focused runtime, verifier, loader, semantic-body
and interface controls after local integration. The repaired public body
comparison rejects lost UInt checks, accepts proven Int annotation removal,
and preserves checker-only commands. Broader qualification remains open. The mutable typed-storage repair at
`152859aa2` has independent review and positive and corruption controls.
Physical operations now authenticate callable kinds and finite effect bounds.
Monomorphic shared reads authenticate the defining value and every writer,
including partial writes, nominal payloads and unsigned storage checks.
Typed mutable captures, Proc factories and optional erased callable fields execute on both
indexed routes. Corrupted kinds, signatures, payloads and writers remain rejected.
Matching verification products build at `3594b4969`, whose runtime source is
`77bd52859`. All nine annotation behavior tests and three hidden-resource
ownership tests pass with zero skips. Generated documentation and broader
qualification remain pending.

Source `786080e95` integrates the reviewed context and process producer repair.
Its four focused controls pass in the combined check. Context scopes authenticate
their body and inputs. Process payloads use the pipeline head's checked run kind.
The verifier also preserves driver exits followed by unreachable declarations.
All three matching native context, process and explicit-discard regressions pass
at `4f58546f2`. Its nine annotation cases fail during shared module preparation,
before their bodies run. The isolated Rust preparation regression identifies
an unsupported range producer. Repair `b7dc8f73c` passes that real fixture and
a minimal range test with an invalid-bound control. Matching native replay at `3594b4969` passes all nine annotation cases.
The full library check remains pending.

Documentation generation at `4f58546f2` stops before writing files because
the verifier cannot find the canonical effect contract for `FsRoot.host_path()`.
Repair `77bd52859` preserves that registered `fs` contract and authenticates
receiver operands independently of constructor demand. The original implicit
return passes, while wrong receivers and forged effect ceilings reject.
Independent review and the merged check pass. Matching documentation replay
gets past that failure, then rejects a ground match without a subject contract
before writing files. A focused Rust replay traces the generated match to
`take_list` in `tour_html`, reading a scoped local through `ExprParam`. The
origin repair at `4a12cc8a8` passes the real tour import and a mutable-list
writer control on both indexed routes. Its merged replay has 130 passes and
one failure: the embedded `template` module rejects a producer contract.
Repair `546a62469` leaves dynamic slots context-directed and restores the
entire catalog. Its merged replay passes all 131 selected controls. Expanded
docs preparation exposed binder and return-carrier cases. Reviewed repair
`561a011e6` passes eight new positive and corruption controls covering shared
iteration bindings, byte lines, nullable storage, optional return carriers,
opaque record returns, glob and last-status values. Both indexed routes run
after frontend disposal. The combined replay also passes all 38 grounded helper
transfer controls. Repair `052b13165` preserves exact optional payloads through
executed checks and authenticates typed calls even without Result constructors
or redundant wrapper facts. Three new controls execute both indexed routes and
reject incompatible callable producers. Removing only the typed-call proof root
makes the actual wrong-source regression fail. Existing With error-slot, driver
temporary and Result-context controls pass. The nullable-fallback test now
descends its checked scrutinee before applying the original unchecked-slot
corruption; its rejection requirement is unchanged.
Expanded docs preparation advances to a process-command argument-vector
producer; no matching documentation generation pass is claimed. Process
argument-vector and try-capture transport remain mandatory open repairs.
The condition-only producer repair passes the focused checkpoint above.
Combined corpus qualification remains pending.
Optional-record literals also fail to propagate a typed callable field context.
Using an already typed callable isolates the verifier tests but does not close
that checker case.

The paired interface command at `4f58546f2` preserves original logical source
identities while checking candidate bytes. Export, symlink and cancellation
controls pass before integration, alongside warning-denied Clippy for xsh and
xsht with all targets and features. The merged replay passes twelve process
tests and two unit controls. Actual corpus comparison remains pending.
The consumer at `d78615ba6` adds body selections, effects and actual emitted
validations to qualification. Its compilation and three focused controls pass,
including real contextual module keys. Independent review finds no P1/P2 issue.
All sixteen process controls still require matching rebuilt products.
Private semantic capture at `6aca16797` retains checked facts and canonical
source sites before checker disposal. The execution API at `8759502c1` combines
that pending snapshot with validations extracted after one normal lowering.
Repair `1abd4f41e` preserves proven integer driver bindings and checker-only
commands. Its six composite controls and the canonical-site lifetime control
pass in the integrated replay. Physical guards without an independent producer
proof remain exact. No corpus-removal credit follows from these focused checks.

Public API regression `9dbde9ebe` rejects a CLI parameter change from `Str` to
`Int` that previously passed body comparison. Owners without a callable
interface proof preserve exact header validations; normalization requires the
existing canonical equivalence or instance proof. The public regression and
all six existing composite controls pass. The library identity regression now
passes with the resource-specialization repair above. Public command replay
still needs matching rebuilt products.

Three native ownership controls at `f73caaa92` establish that record validation
retains extra fields, including hidden resources, and nominal variant payloads
retain resources until owner exit. Runtime replay passes with the matching
`3594b4969` products. These controls still need matching products for the
resource-classification repair above.

The separate origin fix `16d7c4f15` preserves explicit Result discards as data
and makes annotated storage violations fatal type errors. Explicit schema
conversion remains recoverable. Source `b77ab2c68` adds canonical concrete-to-scheme
interface comparison. Its eighteen focused tests and warning-denied library
Clippy pass. Unsupported comparisons report typed errors and earn no parity credit.

Source `1710bf2ff` passes all 31 focused replay cases, including the eighteen
earlier library failures. The replay covers private bridge contracts, native
and static constructor arguments, open typed-callable headers, Result payloads,
conditional and loop writers, branches that transfer control and Optional transport.
Forged argument, payload, slot and metadata controls remain rejected.
The entire embedded-module catalog passes. Matching verification tools built
successfully. Native replay passes all 26 private-effect cases, ten private-return
cases and the private/imported generic Result witness, with zero skips. The full
library boundary remains pending.
This checkpoint does not qualify the whole source or close inference G1.

The producer repair at `d06e66c34` removes a redundant writer census sweep.
Actual traversal counts remain within three sweeps at 64 and 128 local bindings;
the existing work cap is unchanged. CLI descriptor shapes at `9e63ec4dc` share
their prepared records and retain the initializing symbol owner. Both new
resource controls and the existing `parse` and `parse_full` execution control pass after
integration. These are deterministic resource checks, with no timing claim.

Procedure source `139868db6` integrates canonical callable kinds, inferred
finite-effect Proc interfaces and strict Result producer checking. Both grammar
generators, the parser/grammar omission matrix and thirteen lint consumer tests
pass. Matching native products pass immutable prefixes, value branches,
mutable-capture rejection and exported inference. Private/imported scalar and
nested Ok probes pass. Repair `e704557d6` passes nested Err disposal tests;
matching native replay now passes. Frontend-disposal tests reject forged Result payloads and owner
effect ceilings while preserving contextual collections and Any callable values.

The earlier `139868db6` full library boundary has 1,076 passes, eighteen failures
and three ignores. All eighteen selectors now pass in the focused replay above.
A fresh full library run remains required; its result is not implied by that replay.

Matching `1710bf2ff` tools reject documentation generation and all nine annotation
behavior tests before execution. Two minimal explicit-Result repros fail:
`Ok(ctx "rendering" { "page" })` lacks a physical producer proof, and
`Ok(result.stdout)` after `run.capture --text` lacks a propagation result
contract. Their implicit Result-lifting controls pass. The repair at `786080e95`
now passes matching native replay at `4f58546f2`. The annotation group still
cannot start because of the separate producer failure recorded above.
CLI cleanup `401950302` removes an unused internal accessor and fixes a new
test-only Clippy warning. Seven descriptor checks pass before integration,
and warning-denied Clippy passes for xsh and xsht with all targets and features.
The merged seven-test replay also passes.

The canonical `io.flush_stdout` coordinated callee/owner effect-forgery test
passes at `596a7f161`, rejecting the modified program. Its initial Print-based
premise was wrong: print and eprint require no language effect.
Separately, `22e4de86c` makes direct and transitive glob use require `fs`, fixing
the baseline's acceptance under an empty bound. The pure diagnostic stays
unchanged. All four matching native regressions pass; the production caller
census found no executable glob literals needing broader bounds.
The retry capability
fixture now includes its required time effect; its missing-time negative passes.

The constructor tranche at `12436b472`, with branch-fact repair
`a93a9b756`, supports independent Int/Str instantiation of list, record and
Result constructors and generic value branches. Focused checks execute both
indexed routes after frontend disposal and reject forged writer facts and
uninitialized sibling-branch reads. Matching verification products pass the
constructor, grounded lazy-branch and converted rank-1 collection witnesses.
The inline binary-condition branch remains a mandatory open case. These
results do not qualify the whole source or close inference G1.

Callable consumers now distinguish a selected invocation target from a
callable value. CLI annotation edits validate a fresh, import-aware candidate
before writing; lint consumers use the canonical contract. Thirteen focused
lint tests and the imported generic-alias annotation witness pass. Generic, concrete
and defaulted List results annotate correctly with matching `5fcc45c48` tools;
scalar locals remain unannotated. Deferred publication also passes the focused
later-refined empty-list and generalized-body controls. The wider annotation
group initially has eight passes and one public-return spelling failure:
candidate validation rejects generated `Result[Unit]` on an exported proc.
The renderer repair at `a92955568` spells `Result[Unit, Error]`. All nine
annotation behavior tests pass with matching rebuilt products, with no skips. Schema anchors preserve nominal
meaning across independent parses; semantically empty facade contexts compare
without erasing the checker's diagnostic context.

The independent finite-effect model passes eight tests but has no production
symbolic-effect differential yet. The strengthened type/row differential at
`3ac5eb1aa` passes all eleven focused controls after integration, including
128-seed generic quantification and post-trial continuation comparisons.
The current low-level publisher rejects unresolved environment roots; this
does not establish complete captured-environment inference.
Fixed scaling workloads cover eight families
at sizes 64, 128, 256 and 512. Their shape and generator checks are not measured
performance evidence. Three solver scaling checks pass at all four sizes. Seven host-obligation
checks pass, including actual candidate-trial counts and deduplicated dependency
requeues. These new counters are test-only; the older `worklist_wakeups` counter
measures solver pair work. All remaining capability, joint corpus, performance,
baseline, deployment and cleanup gates retain their requirements.

Isolated test commit `3264d5f5f` adds the production symbolic-effect comparison.
All three focused tests pass, and independent review finds no P1/P2 issue.
The model compares every assignment for 64 bounded problems with two variables
over four effects. Another 306 closed cases cover all seven effects and Unknown.
Trial, panic and clone controls compare the resulting continuation relations.
The commit depends on the unfinished compiler foundation at `ea24407c7`.
Integration and replay against the qualified compiler remain pending.

The first full-inference compiler slice, `dbfea6b51`, admits unannotated
immutable aliases to known generic module exports. Each call keeps fresh type
instantiation and the authenticated imported declaration. It preserves the
remaining typed-storage, conditional-callable, generic-body, proc and stream
frontiers. The focused checker and normal/forced frontend-disposal tests pass.
Four existing native controls and the new Int/Str/Bool alias witness each pass
with zero skips. Independent review finds no P1/P2 issue. The generated SPEC,
snippet diagnostics and all three tour-project tests pass. The integration
source also passes workspace Clippy with all targets/features and denied
warnings. This is focused progress, not full canonical or baseline acceptance.

F1 records 22 exact capability selectors: seven baseline controls pass and
15 expose missing capabilities or the wrong rejection reason. The imported
alias target now passes on the combined first-slice products. Other mandatory
cases remain open, including constant projections, typed storage, conditional
selection, ordinary generic bodies, proc/stream omission, recursion and callback
relationships. Safety and broader body witnesses continue in separate files.
F1's source-only frozen-corpus validator passes eight tests and reproduces all
1,843 retained-site records against both accepted view digests and the exact
1,040 edits. This adds no annotation removals. All 202 effects stay in one
conservative joint group until resolved dependencies are available.

Fixture source `0b2f9cd97` adds these F1 selectors. Prefix each name with
`tests/xsh/<file>::`. Reference products from `55032a7ee` give two passes,
seven failures and zero skips. Matching `1710bf2ff` products give the same
two passes and seven failures, with zero skips; the failures remain required
capabilities.

| File | Exact test name | Reference result |
|---|---|---|
| `full-inference-recursive-components.xsh` | `test_full_inference_self_recursive_scc_generalizes_after_checking` | Required return and monomorphic-call failures |
| `full-inference-recursive-components.xsh` | `test_full_inference_mutual_recursive_scc_is_declaration_order_independent` | Required return failures |
| `full-inference-captured-callables.xsh` | `test_full_inference_returned_generic_callable_uses_immutable_capture` | Generic-body and factory-return failures |
| `full-inference-captured-callables.xsh` | `test_full_inference_returned_callable_keeps_shared_capture_monomorphic` | Wrong rejection: proc omission parse failure |
| `full-inference-captured-callables.xsh` | `test_full_inference_returned_mutable_capture_ground_control` | Pass |
| `full-inference-stream-effects.xsh` | `test_full_inference_stream_effect_is_lazy_through_returned_alias` | Stream omission parse failure |
| `full-inference-stream-effects.xsh` | `test_full_inference_inferred_stream_effect_alias_rejects_empty_bound` | Wrong rejection: stream omission parse failure |
| `full-inference-stream-effects.xsh` | `test_full_inference_stream_effect_survives_inferred_returned_producer` | Wrong rejection: stream omission parse failure |
| `full-inference-stream-effects.xsh` | `test_full_inference_stream_effect_alias_typed_ground_control` | Pass |

Source `56a9532b4` retains two native host-result regressions. Both pass on
frozen reference products from `55032a7ee`; both fail on `1710bf2ff` products
with the context-producer and process-propagation proof errors above. They
check scope restoration and text, byte, capture and pipeline payload values.

The stream ground control uses a direct typed producer call. Its first alias
version tested another unsupported capability and failed name resolution.
The mandatory inferred alias cases remain unchanged.

Source `c1ae1db64` adds three selectors in
`tests/xsh/full-inference-callable-lists.xsh`. Reference products pass
`test_full_inference_callable_list_typed_ground_control` and
`test_full_inference_callable_list_rejects_an_incompatible_shared_contract`.
`test_full_inference_callable_list_generalizes_its_outer_relationship` fails
with `check.call-target` at both indexed calls. The target generalizes the
immutable list's outer type relationship; it requires no per-element forall.
Matching `1710bf2ff` products reproduce those two passes and one
`check.call-target` failure, with zero skips.

Source `b36ea9e40` adds default and callable-relay witnesses. Source `0c0eb1b6b`
splits imported Proc and Stream aliases into independent tests, giving eleven
witnesses. Protected
reference products pass five controls: conflicting default arguments, nested
row projection and its missing-field rejection, lazy stream defaults and their
creation-site IO obligation. Two inferred-rest cases remain red. Imported
Proc/Stream aliases and generic callable selection also remain red. The
incompatible-selection negative reports an unresolved call instead of the
required callable mismatch. These six failures remain mandatory work.
Matching candidate replay is pending.

Source `51b8cd899` extends the ground-capture witnesses to five tests.
Protected `55032a7ee` products pass four, including optional erased Pure calls
and rejection of direct calls without a checked signature. The checked-signature
mutable capture fails with `typed callee slot has no checked type fact`.
It remains a required capability. Matching candidate replay is pending.

F4 adapter source `d5f7b6267` adds the documented `check_views.sh --frozen`
invocation. Six focused controls pass before integration and on the merged source,
including source, product, view, retained-class, retained-reason and no-skip
rejection. These controlled-product tests earn no compiler or
annotation-removal credit. The next proposed candidate erases 758 Proc contracts
from the frozen retained table: 549 parameters and 209 returns. Together with the
accepted 1,040 edits, that would be 1,798 removals. Source reconstruction and AST
validation confirm that proposed count: 499 local and 1,299 internal annotations
across 250 Proc declarations. Compiler and runtime qualification remain pending.
All 202 effects and other retained classes stay outside this tranche.

Canonical interface comparison remains required before corpus qualification.
`CheckedCallableContract::equivalent_to` checks exact scheme equivalence.
It cannot establish that an old concrete private signature is an instance of
a newly inferred scheme. That comparison must use the existing constraint
solver with one shared instantiation, canonical schema identities and exact
callable layout. The corpus adapter must consume that result. Symbolic effect
substitution remains a separate compiler requirement.

The nested callable graph is integrated at `3c41f3b3a`. Eight focused tests
pass, covering shared binders, component resolution, rollback, rest parameters,
unresolved origins, schema guards, and independently verified prepared metadata.
The prepared test rejects forged binder, layout, effect and substitution data.
It executes after frontend disposal on both indexed routes. Independent source
review finds no remaining P1/P2 issue in this slice. Symbolic effects, callable
kind inference, stream materialization and broader generic bodies remain open.
This graph checkpoint does not complete inference G1 or qualify a baseline.

The independent type/row oracle now compares 128 fixed-seed problems with the
production solver. All ten focused tests pass, including rollback, generalized
schemes and deliberate comparison mutations. Bound IDs may be renamed. Free
environment identities remain anchored. The generator now satisfies the existing
row-tail lacks invariant. Its initial disagreement was malformed input, not a
confirmed production defect. Focused warning-denied Clippy passes after the
oracle fixes integrated at `52bbb4967`. Symbolic-effect correspondence remains
pending.

Thirteen annotated and erased corpus probes ran against the combined-55032
canonical products. Recursive parameter omission, null/concrete return joins
and empty-list accumulation pass. Recursive return omission, interpolation and
the nested SourceRead parameter expose remaining checker limits. The exact
results are in `PERFORMANCE-INFERENCE.md`. They add no annotation-removal credit.
The metamorphic fixtures are integrated at `916d31d7a`; their generic proc and
stream targets remain mandatory open capabilities.

Full-inference F0 integrates the warning repair at `2ed638691`. The pinned
offline xsh Clippy check passes with all targets and features under the ordinary
two-CPU, 8-GiB limit. The old failed supplemental receipt remains failed; no
full supplemental or baseline acceptance is inherited. The first current check
finds three residual warnings, then a bracket error during repair; both are
corrected before the successful focused run.

The existing canonical launcher already consumes terminal children and cleanup
before publishing successful receipts. Six real-child regressions now cover
success, failure and interruption when a child exits before observation. All
46 replay tests pass on the integration branch. No production polling service
or job registry is added. The compiler's imported-alias regression fails before
the change with `check.module-member`, then passes alongside a frontend-disposal
test on both indexed routes. Native verification and independent review of that
first behavior change pass. Broader callable and effect inference remains open.

The owner launched `full-inference-campaign.md` on 2026-10-19 with bounded
isolated edits, local commits/integration, and owned offline Docker operations.
Pushing remains forbidden. Launch source is `9134db7e7`; its only change since
the planning anchor is the committed completion plan. The inference checkpoints
do not replace this campaign's baseline, deployment, or cleanup obligations.

R12 source `55032a7ee2c4d340a6e5df9d4f362b0074d8a9e0` passes canonical
correctness: 14,077 native passes, zero failures and 150 skips, alongside the
Rust, structural and documentation boundaries. Its supplemental warning-denied
Clippy step fails with 53 errors. Baseline qualification remains open. Preserve
both the canonical receipt and failed supplemental attempt. Two launch probes
pass against the unchanged code using those protected products: identity/open
rows and the existing generic typed-storage rejection, one pass each with no
skips. These scoped probes do not qualify a new compiler source.

The full-inference integration owner is `.work/wt/full-inference-integration`
on `campaign/full-inference-integration`. F0 warning repair owns
`.work/wt/full-inference-warnings`; completion/loop checks own
`.work/wt/full-inference-loop`; capability fixtures own
`.work/wt/full-inference-fixtures`; corpus classification owns
`.work/wt/full-inference-corpus`; the single coupled compiler owner uses
`.work/wt/full-inference-compiler`. Their branches use the corresponding
`campaign/full-inference-` names. All start from the launch source. Compiler
work begins in files disjoint from the warning repair. No full suite or timing
gate has started for the new campaign.

R11 source `f3da00668ecb14baa9e7092585aa47330b47da0e` passes bootstrap,
then fails the deeper grammar generator. Root integration records 229 passes,
one failure and one filtered test. Cleanup resolves. Later canonical stages and
supplemental qualification do not run. Preserve the failed receipt in
`.work/wt/perf-i1-combined-freeze-r11`.

The reduced regression exposes command argument boundaries leaking into `spawn`
and `wait` index bounds. The R12 repair applies the existing nested-expression
reset to all three index and slice bound parses. The six-case regression fails
before the repair. All 39 grammar tests pass afterward, including both generator
depths and repository source recognition. Independent source review passes.
Fresh qualification uses `.work/wt/perf-i1-combined-freeze-r12`. No failed
source's gate credit transfers to this candidate. Grammar changes now require
both generator depths before full qualification.

R10 source `5da47cacc04f5bab6a69cceadf20f8aab52ee44e` passes bootstrap
and 229 root integration tests, then fails soundness: thirteen passes and one
failure in `mutants_get_ordinary_diagnostics`. Its cleanup resolves, and later
canonical stages and supplemental qualification do not run. Preserve the failed
receipt in `.work/wt/perf-i1-combined-freeze-r10`. The scanner test file crossed
the existing 12,000-byte corpus cutoff, changing fixed-seed selection and exposing
a latent parser/grammar disagreement. The isolated repair in
`.work/wt/perf-grammar-mutant-r11` retains the seed set, corpus and cutoff. Its
minimal unknown command-target and option-spacing regressions fail before the
repair. The thirteen-case regression, two adjacent boundary controls and shallow
grammar generator pass afterward in the pinned debug build. Independent source
review passes after correcting spaced known-option handling. Verification
soundness passes all 1,000 fixed-seed mutants. Matching verification tools regenerate
only the grammar reference, and documentation checks plus all three tour-project
tests pass. The reviewed repair is integrated. Its fresh R11 qualification
exposes the separate nested-expression parser defect described above.

The six M1 fixture roots are generated and validated under
`.work/wt/perf-i1-combined-freeze-r9/.work/tmp/m1-fixtures-r10`. Reuse requires
all existing snapshot comparisons against the qualified source's generator.
This preparation supplies no timings or acceptance credit.

R9 source `5454112e9f5fc10eae2faa75e5d6a5d09586bd4f` passes bootstrap,
229 root integration tests, 14 soundness tests, 1,049 library tests, 122 XSHT
integration tests, 82 Linux compatibility tests, ten privilege tests and the
full native suite: 14,073 passes, zero failures and 150 skips. Repository check,
lint and diff checks pass. Structural scanning then fails to resolve
`host_obligations` below an external module selected by `#[path]`. Canonical
qualification remains failed; documentation and supplemental stages do not run.
Cleanup resolves. Preserve the failed receipt in
`.work/wt/perf-i1-combined-freeze-r9`. The isolated scanner repair owns
`.work/wt/perf-scanner-module-r10` and is now integrated. Its four regressions,
all seventy focused native tests and the actual Rust positive/negative controls
pass. A fresh historical scan reproduces the frozen report and item inventory
byte for byte before the single active scanner pin changes. The complete current
source scan and existing structural policy pass. `SourceScan.modules` now carries
both default and path-owner directories; report and inventory schemas are unchanged.
Independent source review passes. Fresh R10 bootstrap and canonical qualification
use `.work/wt/perf-i1-combined-freeze-r10`; no R9 qualification is inherited.
Performance qualification and full source-aligned Laputa acceptance remain open.

R8 at `8ebea493e757eb8c52d76823cdc11f665ce2c2f5` fails canonical
qualification in the written tail corpus. Bootstrap, 229 root integration tests,
14 soundness tests and 1,049 library tests pass. XSHT integration finishes with
121 passes and one failure; subsequent stages do not run. Cleanup resolves.
Preserve the failed receipt in `.work/wt/perf-i1-combined-freeze-r8`.
No current-source G1 or full guest acceptance is claimed.

The tail repair in `.work/wt/perf-tail-initial-offset-r9` advances the follow
cursor by the entire buffered input, independently of the selected output.
The existing short-file regression fails before the fix and passes afterward,
including suffix and zero-output cases. Both existing tail files pass: 116
tests, zero failures and zero skips. All three new cases match GNU tail 9.4.
The initial reference attempt uses BusyBox tail, which rejects the fractional
poll interval and receives no parity credit. Independent source review passes.
Fresh combined qualification remains required.

On October 19, frozen R7 source `6468326009b62cfd8ed682b0dc7007b371d3cb4d`
passes bootstrap, 229 root integration tests, 14 soundness tests, 1,049
library tests, 122 XSHT integration tests, 82 Linux compatibility tests and ten
privilege tests. Its full native suite finishes with 14,070 passes, two failures
and 150 skips. Observer attestation and the UDP error queue fail.
The failed canonical receipt remains under
`.work/wt/perf-i1-combined-freeze-r7/.work/tmp/i1-combined-r7-646832600`.
Supplemental correctness is not qualified. Canonical cleanup resolves.
Preserve the failed R7 receipt. The isolated observer repair in
`perf-observer-attestation-r8` reproduces the frozen report and item inventory
byte for byte before updating the scanner pin. Its 40 Python owner tests and
66 focused native tests pass, with no native failures or skips.
Canonical preflight now rejects stale active pins before builds. Changed-path
gates select both current metric modules for changes to their shared contract.
The first focused launch lacks its required `mold` mount. The corrected launch
passes with unchanged tests and products. Both attempts remain recorded.
Use the clean `laputa-firecracker-verification` checkout for the supplemental
corpus. The temporary clock diagnostic has been removed from the active world
checkout.

The byte-entry fix and filesystem effects for six file-backed byte operations
are integrated. The latter has a failing-before/passing-after regression and
four passing effect checks. The 60-row R6 effect audit replays exactly. Locale
quoting explains `wc.count_input`'s `env` requirement; its `io` clause covers it.
Keep the frozen inference denominator and removal credit unchanged.

The UDP fixture released its destination before connecting the probe. An exact
host-socket diagnostic reproduces a self-connection after 17,482 iterations:
source and destination are both `127.0.0.1:52275`, and the ordinary receive
returns the sent packet. The repaired fixture holds the destination until the
probe has a distinct local port. It passes 100,000 host-socket iterations,
ten exact native runs and its 22-test file: 21 passes and one raw-socket
permission skip. The original deadline and ICMP assertions remain unchanged.

The R13 guest clock diagnostic observes XSH time at 2,514 ms since epoch and
AWK `systime()` at two seconds. The AWK timestamp assertion fails as expected.
Cleanup resolves. Logs remain under
`laputa-world-acceptance/.work/tmp/asterinas-guest-test-20261019T085523Z-4331-ab0b6398`.
The original failed console and Firecracker logs remain preserved. The separate
`asterinas-clock-adapter/run-clock` permits only the exact AWK shard and the
reviewed six-line boot-proof patch. Nine build/package and eight metadata
argument controls pass. The diagnostic supplies no guest acceptance.

The reviewed clock repair is committed locally in the pinned Asterinas fork at
`995610cdfe462a38f617abb7e0a7d8f56f29d097`. It adds a BSP-only KVM clock reader
after CMOS and retains fractional epoch offsets in vDSO realtime data. Five pure
clock tests pass. The actual vDSO encoding regression fails before the fix and
passes afterward in an extracted host check. The mixed syscall/libc C regression
compiles and passes on Linux. The actual kernel compiles without the one new
warning found and fixed during its first build. Existing toolchain warnings
remain. No dependencies or toolchain pins change.

Isolated Laputa source `88be9729efb6738e9bffd040c0d1f3033c2eee43` pins that
kernel and its gitlink together. Its real offline kernel build and KVM smoke
pass; smoke reaches `ASTERINAS_PVH_BOOT_OK`. Kernel SHA-256 is
`10638524e469d69fc1c6dcd5e187b94edfb9f8d7c3a1a2b1c3c0ae746e62cdf5`.
The exact AWK shard passes in the real guest: one pass, zero failures and zero
skips. Cleanup resolves and the qualified R13 seed remains unchanged. Evidence
is under `laputa-world-acceptance/.work/tmp/asterinas-guest-test-20261019T093052Z-7314-d5856f7d`.
Both clock arithmetic tests also pass in actual KVM kernel guests on unchanged
kernel source `995610cd`. Run `osdk test` with each exact function name:
`wall_clock_adds_scaled_elapsed_time_and_carries_nanoseconds` and
`wall_clock_rejects_invalid_fields_and_arithmetic_overflow`, plus
`--target-arch=x86_64 --boot-method=vmm-direct --features=pvh_boot`.
Each guest reports one pass, zero failures and 224 filtered tests. Scratch-only
manifest and QEMU overlays select `ostd`, two CPUs and 3 GiB guest memory;
they omit unrelated filesystem fixtures. Kernel source and qualified products
remain unchanged. The initial `wall_clock_` selector matches zero tests and
receives no credit: the runner matches complete suffix path elements.
Evidence remains under `laputa-world-acceptance/.work/tmp/asterinas-wall-clock-ktest/`;
the owned container is removed and shared leases are released. Full
source-aligned guest acceptance remains pending. These focused platform
results do not qualify the combined compiler.

## Charter

The coordinator owns integration, shared contracts, acceptance, and the final review.
Use the campaign and lane contracts in the installed `orchestrate` skill. Assign
component-sized work and keep disjoint ready work running within the limits below.
The coordinator writes no feature code while implementation lanes run.

Resolve technical choices inside this document's bounds. Record only decisions a
future reader needs. Stop an item when it needs forbidden authority, unavailable
inputs, or a verification loop that cannot establish a result. Continue independent
items. An incomplete mandatory item prevents a successful campaign close.

Run correctness and parity before optimization. Measurement preparation can overlap
inference work, but representation-changing optimization waits for its scheme/IR
contract. Do not keep reopening the retired consolidation campaign. Its finite
remaining acceptance work is the first capability of this campaign.

After two repetitions of a focused check over 30 seconds, assign loop repair before
more implementation width. Compiler rebuilds are recorded separately from test
execution. A focused command group has a 120-second execution ceiling. Preserve
existing intentional stress deadlines. A slow full gate is never the repair loop.

After three refuted hypotheses, recheck source, products, inputs, and ownership.
Bisect the cause before another repair. Never retry a flaky test to green. Keep its
smallest failing case and preserve failed full receipts.
Bound each failing item's active investigation to three repair attempts or two
hours, whichever comes first. A revised causal hypothesis counts as an attempt.
Park an unresolved item with its reproducer and continue independent work.
A fresh bounded review may reopen it once with a materially different hypothesis;
otherwise retain the hard stop and report the campaign incomplete.

Put all worktrees and scratch under the owning repository's ignored `.work/`.
Check `.git/info/exclude` before the first write. Set `TMPDIR` there. Use
`.work/wt/perf-<lane>` for XSH lanes. Create isolated Laputa worktrees under
the XSH campaign's `.work/wt/laputa-<lane>` and keep each checkout's derived
state under that checkout's `.work/` or its existing ignored build paths.
Never write the sibling's normal checkout or shared artifact store.

Only commit after a separate execution grant permits local commits. Use Conventional
Commits and put capability IDs in bodies. Never push, publish, run formatters,
change CI, or update the Rust nightly. Preserve existing branches and historical
evidence. Use the cleanup skill for completed worktrees and retain ambiguous work.

## Decisions

### Owner decisions

| Decision | Scope |
|---|---|
| Pursue performance and deployment after consolidation and inference integration. | Owner direction, 2026-10-17. Improve the existing language; no syntax-feature campaign. |
| Execute this campaign with local commits and integration. | Owner launch grant, 2026-10-17. Includes the bounded isolated edits, pinned fetches, and owned Docker/KVM operations in this document. No push. |
| Allow isolated Laputa edits and pinned input fetches. | Owner answer during planning, 2026-10-17. A separate execution grant is still required. No live roots, host installation, publishing, or pushes. |
| Allow `doas` for KVM access. | Owner direction during planning, 2026-10-17. Limit elevation to required device/container/VM operations using owned paths. Do not run arbitrary recipe code as host root. |
| Allow one build of the existing Laputa host-tools image. | Owner answer, 2026-10-18. Its pinned Alpine base installs seven unversioned APK packages. Build only this exact image, save it in the isolated checkout, and use that saved image offline afterwards. This does not authorize other moving inputs. |
| Retain the thirteen exact `lsipc` reference-crash exceptions. | Existing owner decision in the util-linux campaign. Independent XSH assertions remain behavior evidence and receive no exact-parity credit. |
| Start Laputa world and Asterinas acceptance in parallel with consolidation. | Latest owner direction, 2026-10-18, supersedes waiting for cleanup before starting G6. Preserve exclusive G1 timing windows. Laputa must remain functional. Use an isolated checkout of the sibling's named source, the frozen XSH candidate, the existing world recipe set and existing Asterinas backend. Final G6 acceptance requires that same XSH source to qualify under G1. |
| Start inference integration and performance work alongside G1 and G6. | Owner direction, 2026-10-18. Stagger compiler integration, corpus work, measurement preparation and deployment. Preserve exclusive timing windows, real close prerequisites and matching source/product qualification. |

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Start at local master `3ed58895c0b1423c8f7ac4852bb9ec9a23922da2`. | It includes the merged util-linux work and preserves the accepted Lean tree. The supplied review's `7e8881d9` predates that merge. Reseal once if master advances before launch. |
| Keep x86-64 Linux musl and the existing pinned test image for XSH qualification. | Reuse established product settings and isolation. G6 additionally qualifies Laputa's existing pinned Asterinas backend under the owner's explicit direction. No replacement of the Linux baseline or new backend. |
| Audit inference by capability and patch, then merge only reviewed changes. | The accepted checkpoint is hundreds of commits divergent. A whole-branch merge can restore obsolete compiler or toolchain state. |
| Keep original accounting and practical acceptance separate. | Preserve 9,194 sites, 693 eligible locals, 2,190 eligible internal annotations, and the historical 95%/85% targets. Do not claim those targets pass unless they do. |
| Classify every retained eligible annotation. | Distinguish semantic meaning, unavailable information, and missing relationships. `{}` remains a Record; Map selection remains purposeful. |
| Preserve explicit capability restrictions. | Eliding a clause is allowed only when the inferred contract is equivalent under the existing effect rules. A smaller current body does not authorize weakening a declared restriction. |
| Evaluate immutable release-shipped modules first. | Their source, registry, compiler, and feature set are controlled. Arbitrary-script caches and persistent user formats are excluded. |
| Keep prepared artifacts private and exact-version matched. | No stable serialized ABI or migration promise is needed. Reject malformed or mismatched data before execution; do not use silent source fallback to pass acceptance. |
| Treat prepared modules as an experiment with a keep/reject gate. | Ship only when correctness and measured tradeoffs pass. A rejected experiment leaves no runtime feature flag or alternate executor. |
| Reuse current measurement and VM tooling. | Extend narrow selectors and phase counters where missing. Do not build another benchmark platform, VM supervisor, proof inventory, or provenance system. |
| Test immutable package/root generations. | Laputa has no mutable install/uninstall contract. Model upgrade as a replacement generation and removal as recomposition plus reachability-based collection. |
| Use the existing `qemu-dwl-foot` profile. | It is Laputa's supported image and guest proof. Add bounded shutdown assertions there; do not invent a second distribution or a handwritten package list. |
| Keep all test/build execution offline after explicit input acquisition. | Fetch only recipe/image-pinned inputs for the selected closure. G6 includes the existing full x86-64 world plan and Asterinas inputs. No broad `make fetch`, new package catalogue entries, or moving dependency versions. |
| Preserve source-facing APIs and the single indexed executor. | New private types and scheme metadata may change. A new public contract, native backend, daemon, or compile-first workflow is outside this plan. |
| Start with no new production dependency. | Use existing codecs and tools. A concrete missing operation can justify a dependency only after license, maintenance, transitive-weight, build, size, and startup review. |
| Keep compiler qualification separate from a receipt-reader repair. | The existing adapter configuration binds compiler source, products and receipts independently of its tool checkout. Freeze and name both revisions. A reader-only correction gives no compiler credit to the newer tool revision or master. |

The proposed optimization thresholds below are prescribed engineering targets,
not measured wins. The owner can overrule them before execution.

### Delegated decisions

Choose fixtures and algorithms inside the fixed families below. Freeze workload
selection before candidate measurement. Prefer an existing fixture and the smaller
reversible design when benefits overlap. Limit each mechanism to three distinct,
profile-supported experiments before recording the result and moving on.

The scheme/IR owner chooses the private encoding, linker relocation, and bounds
after inspecting the integrated inference implementation. It must preserve the
existing semantic verifier's authority. Never serialize process-local pointers,
interned integer identities without an owner, or live resources.

## Baseline

### Source and acceptance state

The local planning source is `3ed58895c`. The normal Laputa checkout is clean at
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`. Neither is qualified by a fresh full
gate during planning. `DECISIONS.md` is absent. Reuse the decision tables in the
existing campaign documents instead of creating a parallel decision register.

The retired campaigns leave XSH-1, XSH-2, and XSH-3 in `TODO.md`: util-linux
resource/close acceptance, an intermittent timeout forwarding failure, and the
remaining consolidation matrix. Historical accepted source `24cc0d704` has
13,916 canonical passes, zero failures, 78 skips, and accepted resource evidence.
It is not current-master acceptance.

The retained `7e8881d96` normal-profile receipt records 13,857 passes, zero failures,
and 138 skips. The later observer attempt records 13,916 passes, one failure, and
78 skips. The timeout selector subsequently passed five focused runs. Its cause
remains unestablished; those reruns do not repair the failed full receipt.

The accepted inference corpus checkpoint is
`15243e97f6fd751016ab28690ada2cadb6fc973a`, on
`campaign/typing-next-corpus-1038-20261017`. It records 499/693 local removals
(72.0%) and 541/2,190 internal removals (24.7%), totaling 1,040. All 149 Map
annotations on `{}` and 202 eligible private effect clauses remain. These are
checkpoint counts, not current-master counts. Planning found master and this
checkpoint 483 and 281 commits beyond their common ancestor `b235a1a43`.
The checkpoint commit records evidence; implementation lives in its ancestry.

The original 95%/85% targets remain unmet. Preserve their exact denominators.
Publish a class-by-class attainable ceiling when semantic disambiguation makes a
target impossible without a contract change. Do not shrink the denominator or
change literal semantics. This campaign accepts useful inference by the practical
criteria below and reports the older numeric targets separately and honestly.

### Fresh planning probes

Source-qualified release products from the complete `7e8881d96` profile receipt
were available. Their recorded digests matched the actual files. `xsh` is
30,158,976 bytes; `xsht` is 40,737,768 bytes. These sizes are for those products,
not newly built master binaries.

Probes ran in the pinned Linux image with one CPU, 512 MiB memory, no extra swap,
no network, `LC_ALL=C`, and `TZ=UTC`. Each timing used seven fresh processes.
Filesystem caches were warm. Container startup was excluded. Parent wall time
includes the `/usr/bin/time` wrapper and output collection; child CPU includes
that wrapper. Exact output, empty stderr, and status zero were asserted.
`perf-gate-ready` and its immediate `-1` check passed for both batches.

| Workload | Median wall ms [range] | Median child CPU ms [range] | Median process peak bytes |
|---|---:|---:|---:|
| Existing `cold_trivial.xsh`, `hello` | 1.283 [1.073, 1.696] | 1.207 [1.035, 1.595] | 19,099,648 |
| Existing `cold_quote.xsh`, `'two words'` | 2.879 [2.713, 2.903] | 2.737 [2.643, 2.827] | 27,119,616 |
| Existing `cold_cli_parse.xsh`, `3` | 2.926 [2.734, 3.162] | 2.867 [2.692, 3.082] | 28,098,560 |
| `print template.render("hello {{.name}}", {name: "xsh"})?` | 16.242 [15.832, 19.579] | 15.841 [15.605, 19.405] | 32,595,968 |

Separate `xsht runtime-stats` probes reported these allocation counts and bytes:

| Workload | Construction allocations | Construction allocated bytes | Execution allocations | Execution allocated bytes |
|---|---:|---:|---:|---:|
| Trivial | 1,675 | 280,860 | 46 | 4,376 |
| Embedded template | 123,388 | 25,157,105 | 1,014 | 236,768 |

These are diagnostic observations. They identify construction as a candidate for
profiling, not a measured speedup or a bound on time saved. Allocation traffic is
not RSS. Phase-separated CPU attribution and substantial-workload throughput are
unmeasured. Establish them at the accepted combined source before optimization.

The stream probe failed before execution: `bench/stream-stage-cost.xsh:24` calls
removed `abort(2)` and receives `check.removed-abort`. The same stale form appears
in the terminal and producer memory probes. This is a known-red benchmark input,
not a streaming measurement. Repair its exit spelling in P0, preserving behavior.

The resource-controller loop passed 53 tests in 0.14 s wall, 0.12 s user, and
0.02 s system, with 25,404 KiB peak RSS. Readiness passed on both sides. Two
existing exact native selectors passed in the pinned container: template field
behavior (5 ms test-reported) and rooted traversal rejection (2 ms test-reported).
Those reported test durations are loop evidence, not performance acceptance.

Planning host: 32 logical CPUs, 64,899,153,920 bytes RAM, about 59.5 GB available
at survey time. Swap had about 1.17 GB occupied; do not infer ongoing pressure
from occupancy alone. Measure swap activity and cgroup events before setting width.
The sandbox hid `/dev/kvm`; authorized `doas` inspection found the character
device and successfully opened it, reporting KVM API version 12. No guest ran.
No ready Laputa guest bundle was established.

### Verified execution foundation

The pinned image exists locally:
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Planning proved that the available source-qualified products can execute the
embedded-module path and the existing rooted-confinement test in this image.
It also proved that actual KVM access works through the granted elevation.

Artifact independence from build-machine state, inference merge correctness,
substantial-workload scaling, and real guest boot remain unproven. Their tests
are prerequisites below. No full build, suite, proof recheck, fetch, or VM launch
was performed during planning.

## Sequence

### Resource and ownership limits

Ordinary iteration allows at most two compiler builds, each at two jobs and
8 GiB. The isolated Laputa Deno link may use 12 GiB with two CPUs and no extra
swap: its 8-GiB container OOM was reproduced, and the 12-GiB node build passed. Inherited release-profile builds keep their 12 jobs, 12 CPUs, and 24 GiB
and run alone. A full native observer keeps its existing 12-CPU, 24-GiB allocation.
Each inherited scheduler pair keeps 32 CPUs and 40 GiB and runs alone. Do not
reduce those allocations to obtain incomparable timing. Park the affected gate
when its allocation cannot fit. Never run two full native suites together.
Lightweight independent test lanes get two CPUs and 4 GiB. A VM lane gets four
CPUs and 8 GiB host allowance, including guest memory.

Reserve 12 GiB for the host and coordinator. Admit another lane only when the
sum of enforced memory limits fits current available memory minus that reserve.
Reduce width on observed swap activity, cgroup memory events, or disk pressure.
Measure lane peaks after the first tranche and revise the admission calculation.
Timing windows exclude other builds, suites, VMs, and profiling.

| Phase | Maximum lanes, excluding coordinator | Floor while independent work is ready |
|---|---:|---|
| P0 foundation | 1 until G0 passes, then 4 | One per admitted ownership set |
| P1 completeness | 6 | One per admitted ownership set |
| P2 hardening | 5 | One per admitted ownership set |
| P3 optimization | 5 | One per admitted ownership set; pause measured competitors during timing |
| P4 close | 2 review/preparation lanes, then one measurement owner | All feature lanes stopped before final freeze |

Replace a stopped lane in the same turn when it has ready work. Overlap integration
and disjoint next slices. Run verification from a frozen scratch checkout, not the
moving integration tree. Split independent focused inputs; preserve the one-full-
suite lock. Width limits are ceilings, never a requirement to saturate timing runs.

Use `collaboration.followup_task` for every message to a campaign worker,
including when a lane appears to be running. That call starts an idle lane
automatically. Do not use `collaboration.send_message` in this campaign.
Before waiting, reporting status, or starting another long operation, reconcile
`collaboration.list_agents` with every outstanding process session and its actual
result. Handle completed work in that turn: review and integrate it, launch its
next ready step, or record the concrete blocker. A stale "running" note is never
evidence that a lane is active.

While any lane or process is outstanding, use bounded tool waits and reconcile
again within 15 minutes. Completion notifications trigger immediate reconciliation;
the interval is a backstop. Do not end the coordinating turn with background work
that requires a later user prompt to notice its completion. A resumed session
reconstructs outstanding work from agent status, process state and gate receipts
before launching anything. Every handoff names outstanding session IDs, receipt
paths and the next action for each result.

### P0: Prove the loop and finish the baseline

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| G0: Qualify existing inputs and exact checks | S; execution grant | Coordinator; `bench/stream-*.xsh` only for demonstrated stale inputs, focused controller selectors | Reproduce the stale `abort` rejection, repair it, assert exact count output, and run the two native foundation selectors. No other implementation starts until these checks work. |
| G1: Close the inherited acceptance queue | L; G0; final qualification requires G2's functional validator | Acceptance lane; timeout fixture/owner and existing consolidation gate controllers; no inference code | Reproduce and fix XSH-2 at its origin. Run the finite current-source queue below, including util-linux resources/close. Zero unaccounted failures or skips; finish XSH-1 to XSH-3 only after their own proofs pass. |
| G2: Add narrow measurement adapters | M; G0 | Tooling lane; `dev/performance/`, existing benchmark selectors, `src/frontend_stats.rs`, `src/runner.rs` phase-observation seam | Extend existing reports with parse/load, check/solve, publish, lower, verify, execute, and first-output boundaries. Separate inclusive from exclusive time. No observer in production hot paths when disabled. Validate exact output and reject wrong products, unsupported workloads, and failed readiness. |
| G3: Audit isolated inference and freeze the practical cohort | M; G0 | Read-only implementation audit plus `dev/performance/` corpus subdirectory and inference evidence docs; tooling owner assigns disjoint files first | Produce a minimal reviewed capability dependency list, source diff, original/reduced cohort, and retained-site classes. No whole-history merge. Pin the actual implementation source behind the accepted evidence. |
| G4: Prove disposable Laputa inputs | M; G0 | Laputa foundation lane; isolated checkout's local test fixture adapter and owned output roots | Run synthetic PM controls, verify exact candidate-XSH injection, prove network-disabled execution and output confinement, inspect existing profile input closure and KVM path. No guest or recipe is credited by a host-only fixture. |
| G5: Retire consolidation infrastructure | M; G1 | Tooling owner; `dev/consolidation/`, `dev/performance/`, `dev/util-linux/`, testing and baseline docs | Complete XSH-19. Trace current callers, retain reusable checks under ordinary developer commands, and remove obsolete campaign adapters, duplicate entry points and historical-pin dependencies. Preserve required acceptance behavior and prove replacement commands reject invalid inputs before deleting their predecessors. |
| G6: Qualify Laputa world and Asterinas | L; G4 to start, G1 for final source qualification | Laputa acceptance lane; isolated sibling-source checkout, existing world and Asterinas owners, pinned fork for demonstrated kernel defects | Start in parallel with consolidation. Run the full existing x86-64 world build with the frozen XSH candidate, prove root functionality and unchanged rebuild reuse, then require the actual Asterinas smoke and full guest acceptance. Resolve failures at their owning boundary. No mock, filtered package subset, stale compiler pin or host-only proof supplies acceptance. |

At G1 completion, publish `docs/PERFORMANCE-BASELINE.md` with the accepted source,
product locations, platform, limitations, and gate commands. Update that same page
after inference integration and final close. Keep the accepted code source explicit
when the documentation commit differs. Do not wait until campaign end to provide
the everyday baseline reference.

Run G5 immediately after G1. Start G6, I1 and M1 in parallel with consolidation.
Stagger builds and diagnostics within the resource limits. Accepted M1 results
still require their real close prerequisites and the post-inference source.
During exclusive timing windows, other lanes may prepare source and review
results without running builds, suites, VMs or profiling. Schedule G6
builds and VM execution outside those exclusive windows.
This is infrastructure cleanup, separate from deleting disposable worktrees and
build products. Keep checks required by the remaining campaign until ordinary
commands own them. Archive historical evidence outside the everyday command path.
Use focused owner tests and rejection controls for the cleanup. The next combined
regression boundary remains I1's responsibility. R3 removes machinery introduced
later in this campaign.

At G6 launch, name a clean Laputa source from `../laputa` and test it in an
isolated checkout. Keep the normal checkout and its shared artifact store intact.
Bind the frozen XSH source and products into the host tools, seed and package
build. Asterinas currently uses its own XSH commit pin. Update that isolated pin
and its existing source-cache selection to the same frozen local revision.
An older locked XSH build cannot qualify the current candidate.
If G1 requires a different source, refresh the affected G6 products and results.

Use `make plan` and `make build ARCH=x86_64` without `PKGS` or `STOP` to cover the
existing world. Fetch only the frozen plan's pinned missing inputs, then execute
offline. Run the existing Laputa checks and suites, root inspection and execution
proof, and an unchanged rebuild. Keep publication local to an owned loopback
mirror when the existing root workflow requires it. No external publication.

Run `make asterinas-setup` for its real KVM smoke marker, then
`make asterinas-test` for the PM-built root, xinit, XSH, filesystem and process
proofs. Require the existing guest markers, successful result and complete owned
VM cleanup. Smoke alone does not establish full Laputa acceptance. Keep the
existing pinned fork and input policy. Required syscall defects belong in that
fork with kernel regressions, never userspace workarounds or skips. Bound build
attempts by the concrete stage and resume that stage after a diagnosed failure.
Rerun applicable G6 checks after I1 and later shared-path changes. A broken world
or Asterinas acceptance blocks progression and final campaign closure.

G1, G2, G3, and G4 can overlap after G0. G1 owns broad acceptance; other lanes
stop expensive work for its timed windows. The coordinator resolves any shared
controller or source-identity seam before assigning writers.

G2 creates `dev/performance/check.py` as a thin dispatcher over current tools.
Its commands listed below are **planned**, not existing interfaces. Add only
missing selection, attribution, and result checks. Reuse existing fixture makers,
paired sampling, stats, trace, locks, and process cleanup. Do not duplicate their
implementations. Adapter failures remain nonzero and retain their primary cause.

Validate the proof foundation before opening dependent work: a wrong-output
control, a mismatched product/source control, a real cleanup failure control, and
a before/after-readiness rejection must each fail the exact relevant command.
The new `self-test` owns those controls. Unit policy cases must avoid real clocks
and processes; bounded real adapters prove the OS boundary.

#### Run the finite inherited queue

Freeze a clean integrated source and use the existing gate tools. Adapt paths,
source pins, and product qualification in their existing owners only where the
merged source proves a mismatch. Never manufacture `functional_complete: true`
or copy source credit from an earlier receipt.

1. Revalidate current functional/structural obligations with the existing canonical
   owner and supplemental correctness launcher. Run the deep grammar proof once.
   G2's `functional --check` validates the results and issues the existing
   functional-close record for this exact source before any profile consumes it.
2. Run a fresh normal profile with `profile-cost.py`. With its qualified release
   products, run the existing `dev/perf/btop-profile.xsh` scaling gate in an
   isolated readiness-bracketed window; the ordinary native suite retains its
   clock-free join correctness checks.
3. Run one timed canonical/resource observer with `final-observers.py --execute`.
4. Run the forced profile, final comparative profile, and three scheduler pairs.
5. Run util-linux `resources --check` and `close --check` through its release scope.
6. Check final metrics and document only required known limitations.

The source-qualified inherited receipt must satisfy its existing budgets,
including the isolated 15-second repository lint gate. Readiness is mandatory
for time credit; binary size and memory still run when timing is deferred.
Keep the existing exact `lsipc` and rooted-memory exceptions unchanged.

The inherited tools use absolute workspace paths and local `.work` prerequisites.
G1 must validate their `--plan` output and required files before an expensive run.
Fix relocation only when needed by a demonstrated failure. Do not replace the
controllers or add a second source/product receipt protocol.

`functional --check` is a missing qualification step that G2 must supply. It
consumes the actual canonical receipt, supplemental context/skip review,
structural report, and required semantic review rows for the clean source.
Use the current `metrics.xsh check-current` owner for those rows. Reject a bare
boolean, absent result, stale product, or unreviewed mandatory skip. Then write
the existing functional-close shape with its evidence bindings. Supplementary
`launch.py` success alone does not issue a close. Run the canonical owner's
bootstrap stage before `execute`:

```sh
python3 -B dev/consolidation/final_canonical_replay.py bootstrap --repo SOURCE --head COMMIT --tree TREE --output .work/tmp/bootstrap-RUN --run-name bootstrap-RUN
```

`execute --bootstrap-receipt` consumes
`.work/tmp/bootstrap-RUN/bootstrap-receipt.json`. The stage records the exact
singleton Cargo build and writes the existing `validate_bootstrap_receipt`
schema.

### P1: Integrate inference and complete workflow behavior

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| I1: Integrate the accepted inference capabilities | L; G3 to start; matching G1/G6 qualification before close | One compiler lane; `src/sema/`, corresponding syntax metadata, compact facts, lowerer, indexed schema/verifier, compiler-facing tests | Review and port/cherry-pick the minimal dependency set. Preserve master Lean, cleanup, and util-linux changes. Run ground-type, schemes, rows, host anchors, recursion, rollback, importer-order, diagnostics, frontend-drop, and forged-IR tests. Then run the combined regression boundary once. |
| I2: Remove missing inference relationships by class | L; I1; corpus preparation from G3 can precede it | Same compiler lane for implementation; a separate corpus lane owns transformed fixtures and accounting only | Classify 100% of retained eligible sites. Investigate all 202 effect clauses by cause. Fix shared propagation/constraint causes; keep intentional restrictions and all semantic Map annotations. Prove joint original/reduced behavior and useful inferred signatures. |
| D1: Complete immutable package lifecycle tests | L; G4; use accepted baseline compiler first, then I1 | Laputa PM lane; `tests/pm/`, narrow PM implementation owners, local recipe fixtures; never XSH compiler files | Local source validation, unpack/build/proof, root generation, version replacement, recomposition/removal, GC, and injected failure at each publication boundary. Check exact files, modes, links, ownership, receipts, and unchanged old generation on failure. |
| D2: Complete process and rooted-filesystem workflows | M; G4 | Boundary lane; `tests/runtime/process.rs`, `tests/xsh/within.xsh`, `tests/xsh/stdlib/fs*.xsh`, narrow host owners; no scheme/IR files | Bound process trees, partial output, signal/timeout outcomes, complete reaping, primary plus cleanup diagnostics, raw filenames, symlink swaps, hardlinks, permission errors, and interrupted publication. No escape from the declared root. |
| M1: Freeze representative measurements | M; G2 to start; real close prerequisites and post-I1 products for accepted baseline | Measurement lane; existing benchmark files and `dev/performance/` selection; no runtime optimization | Qualify all four families, exact outputs, common/demanding shapes, n/2n/4n scaling, completed-work and allocation/syscall counts. Establish the post-inference source/product baseline. |
| F1: Bind one transformation to applicable formal assumptions | M; G2; final subject after profiles | Formal lane; `formal/` maintenance/evidence and one optimization-specific finite validation fixture; compiler edits belong to I1/P3 owner | Select one measured transformation. Map actual checked input and emitted IR assumptions to the relevant formal result; mutation controls reject a lost assumption. Keep theorem statements separate from finite Rust evidence. |

All scheme, effect, compact publication, and IR changes stay with one compiler
owner. Do not run competing solver and effect-representation writers. Corpus,
Laputa, host boundaries, measurements, and formal work provide the remaining
disjoint lanes. If D2 needs an IR fix, return it to I1 instead of widening ownership.

I1's combined boundary includes the inference relationship suite, unchanged
annotated corpus, full util-linux parity, existing process/rooted tests, and one
full native suite. Run the existing changed-path Rust checks for shared compiler
changes. Do not infer correctness from corpus checking without execution.

For I2, use at least `core/wc.xsh`, `core/sort.xsh`, `core/awk.xsh`, and
`pm/generation.xsh` from the isolated Laputa checkout. Retain the original frozen
48-source corpus and its eligibility categories. New util-linux files are an
additional cohort, never an excuse to change the old denominator. A declaration's
inferred interface must be independent of importer order and unrelated callers.

The practical inference gate requires every selected program to check and run
with eligible implementation annotations removed jointly wherever supported.
Compare outputs, failures, cleanup, evaluation order, inferred signatures, and
negative diagnostics. Record each retained site with its class and a minimal
witness. Every missing relationship found in the selected practical cohort must
be repaired or be a visible incomplete item. Purposeful restrictions and
ambiguous literals are accepted retained sites. Do not rewrite algorithms to
improve the score. Reproduce at least the pinned 1,040-removal result on its
original corpus before assigning new credit at the merged source.

### P2: Harden the contracts before performance changes

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| H1: Qualify prepared-module behavior and format boundaries | M; I1 scheme/IR contract settled, G2 | Preparation lane; artifact tests, private loader/verification contract design; coordinate compiler seams with I1 | Source path is the oracle. Freeze module/linker corpus and failure controls before the experiment. Run all baseline cases. Artifact-specific rejection cases become the experiment's mandatory tests. |
| H2: Harden deployment and input acquisition | M; D1 and D2 | Same Laputa and boundary owners, disjoint slices | Corrupt/truncated inputs, duplicate concurrent publication, interruption between writes, partial output, cleanup failure, and idempotent rerun leave only valid generations. Fetch pinned inputs separately; test/build runs have network disabled. |
| H3: Maintain formal trust and documentation | M; F1, I1 | Formal lane plus coordinator-owned generated docs | Run existing Lake build, axiom self-tests, guarded paranoid check, inventory checks, and applicable finite controls. Update SPEC sources for inference behavior, registry docs for actual API changes, and deployment reproduction docs. |

H1 must cover module identity, imports, private bindings, nominal types, schemes,
labels/defaults/rest arguments, exact diagnostic spans, malformed table bounds,
unknown schema/version/features, resource exhaustion, and function evidence.
Run each behavior through source preparation and candidate loading on the same
inputs. Corrupt an instruction, substitution, and identity on purpose; rejection
must occur before a script side effect.

Prepare on one cwd/environment/filesystem and execute on another. Assert that
module initializers run once at ordinary runtime order, never during artifact
generation. Keep source locations and source-map ownership after parser, arena,
checker, solver, and compact frontend facts are actually dropped in a Rust test.
A subprocess-only test is insufficient. Raw external input never constructs
`FullProgram` or bypasses `FullVerifier` directly.

Freeze decoder limits before O1: at most 64 MiB encoded data, one million entries
in any table, and nesting no deeper than the current semantic limit of 256.
Check lengths, aggregate sizes, ranges, and arithmetic before allocation.
The tighter product-size and memory gates still apply. Exact-release data has
one canonical writer, an explicit format version, and no legacy readers.

### P3: Optimize the measured shared work

No P3 item starts until the affected workload is complete and passes its P1/P2
behavior corpus. Performance snapshots may precede complete inference, but artifact
schema work waits for I1 and H1. Other runtime mechanisms depend only on their
own stable checked interface and the accepted measurement source.

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| O1: Experiment with prepared shipped modules | L; H1, H3, M1 | Preparation lane; `src/stdlib.rs`, narrow loader/runner seams, private artifact implementation, indexed verification changes; no competing compiler writer | Build exact-release immutable data, link and verify it, then use the existing indexed executor. Evaluate one catalog module first, then broaden only after it passes. Ship or reject using the thresholds below. |
| O2: Remove stream materialization and copying | L; D2, M1, affected parity | Streaming lane; stream/stage implementation and byte/text library owners | Preserve pull timing, short-circuiting, errors, chunk boundaries, backpressure, cancellation, and cleanup. Prove bounded memory for unbounded input and compare useful bytes/records per CPU second. |
| O3: Improve measured script algorithms | L; I2 practical corpus, M1 | Algorithm lane; selected `core/lib/` algorithm and applet sources only, one coherent algorithm family at a time | Improve measured comparisons/copies/spill behavior. Preserve exact GNU/uutils/BusyBox behavior only on the tested overlapping corpus and keep unsupported differences explicit. No applet-named Rust acceleration. |
| O4: Remove redundant host and frame work | L; I1, D2, M1 | Host lane; selected syscall adapters and runtime value/frame owners; excludes O1 verifier/loader and O2 stages | Remove repeated metadata or argument/value work identified by profiles. Preserve host errors, stat/symlink policy, evaluation order, frame ownership, and cleanup. A new primitive must be reusable and bounded. |
| F2: Prove the selected transformation's actual boundary | M; F1 and selected O-item | Formal lane, with runtime probes supplied by its owning lane | Validate actual checked/IR assumptions, run the transformation, compare observable outcomes, and run a mutation negative. State exactly which property is theorem-backed and which is finite differential evidence. |

Profile before each experiment and after each retained win. Rank candidates by
the removable fraction of end-to-end work. Keep up to three supported experiments
per mechanism. Reject unmeasurable experiments and delete their unused paths.
Do not chase dispatch while avoidable I/O, materialization, or algorithms dominate.

#### Set numeric performance gates

M1 freezes primary workloads and common/demanding inputs before O1 to O4.
Use at least five alternating baseline/candidate pairs, with AB/BA reversal.
Retain existing larger sample counts where the reused runner already owns them.
Report median, range, minimum CPU, and paired spread per workload, never one
aggregate XSH speed ratio. Count failures, bytes/records, subprocesses, syscalls,
allocation traffic, process RSS, and cgroup peaks separately.

Treat a change as a regression only when it exceeds the permitted budget and
observed paired spread. A benefit must exceed spread as well as its numeric target.
Do not treat overlapping ranges as a reliable win without more independent pairs.

| Target | Pass condition |
|---|---|
| Prepared-module experiment | At least 20% lower median end-to-end startup on two shipped-module workloads, including template, with all H1 cases passing. Otherwise reject and remove the experiment. |
| Useful shared runtime improvements | At least 15% lower median CPU on two preselected substantial workloads from different non-startup families, each completing at least 50 ms of baseline CPU work. Mechanism counts explain the change. |
| Untouched startup controls | Retain the existing per-case increase limit `max(5% of baseline, 1 ms)` and require no new failure or helper process. |
| Substantial workload regressions | Candidate median CPU no more than baseline plus `max(5% of baseline, paired spread)` on every frozen unaffected workload. Preserve any stricter existing absolute budget. |
| Production binary sizes | Each release product at most 5% larger than the post-inference baseline. Count embedded artifact bytes in the product. |
| RSS and finalized retained memory | Each measured independently; at most baseline plus `max(10% of baseline, 2 MiB)` on unchanged source. Retain any stricter inherited gate. |
| Build cost | At most 10% additional median CPU for matched incremental and release builds, outside measured spread; identical flags, concurrency, cache state, and absolute staging path. |
| Solver scaling | Existing inference bounds and negative cases pass at n/2n/4n. Explain genuine output growth separately; no elevated limits, silent recovery, or new `Any` to meet time budgets. |
| Observation overhead | Disabled observers allocate nothing per operation. Measured production controls stay within their existing budgets. Profiling runs never supply timing samples. |

Define cold as a fresh process without XSH session/cache reuse. State filesystem
page-cache warmth separately. Do not flush host caches. Measure first useful
output separately from complete exit; both include load, link, verification,
and required initialization. Keep output comparison outside timing but consume
the actual output inside each measured process. Report helpers as part of the
workflow users pay for.

Run `perf-gate-ready` immediately before every timed measurement and
`perf-gate-ready -1` immediately after, including failed measurements. Discard
overlapped runs. When the host stays busy, report `perf gate deferred: host busy`.
Memory and size still run. Deferred timing never passes, and blocks full release.

### P4: Qualify deployment and close

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| R1: Boot and shut down the candidate userspace | L; D1/D2/H2, selected profile input closure, exact candidate compiler | Laputa VM lane; existing `system/qemu.xsh`, guest proof, profile sources, bounded fixtures | Run `make profile-test` in the isolated checkout with the candidate XSH substrate bound into its immutable package/root identity. Require actual guest marker, screenshot, intended userspace, bounded clean shutdown, zero surviving VM descendants, and valid unchanged old bundle on failure. |
| R2: Freeze and review the release candidate | M; all feature lanes stopped, R1 | Coordinator and independent reviewer | Run combined changed-area gates, one full native suite, applicable inherited gates, workload parity, the final serial performance window, and formal maintenance. Resolve P1/P2 review findings, or leave the campaign incomplete. |
| R3: Retire campaign machinery and publish local acceptance record | S; R2 | Coordinator | Remove dead experiment paths, duplicate temporary adapters, and integrated clean worktrees. Update only completed TODO tickets. Write one accepted-baseline record and deployment reproduction commands. No release upload, tag, or push. |

R1 reuses Laputa's existing 180-second guest proof deadline and shutdown
supervisor. Set a 15-minute bound on one supplied-source fixture package workflow.
Run a selected profile preparation/build stage at most 45 minutes per attempt,
then inspect the concrete limiting package and resume only its bounded stage.
Do not repeat a whole closure after a known package failure. A missing pinned
input, unusable KVM, or unsupported host parks R1 and prevents release credit.
No mock, screenshot-only host fixture, or software-acceleration substitution
can pass the real boot requirement.

## Acceptance commands

G2 supplies these **new** commands. Until its tested implementation exists,
their rows are requirements, not evidence. The dispatcher reads the explicit
source/product and Laputa checkout paths from arguments; no hidden normal-checkout
fallback is allowed. Keep one small configuration for those paths and selected
workloads, with validation before effects. Pass that configuration as `--config`.

| Deliverable | Command after G2 | Threshold |
|---|---|---|
| Proof foundation | `python3 -B dev/performance/check.py --config PATH self-test` | All real negative controls reject; exact positive fixtures pass; zero leaked processes. |
| Functional qualification | `python3 -B dev/performance/check.py --config PATH functional --check` | Actual required correctness/review results qualify the exact source before emitting the existing close record. |
| Accepted baseline | `python3 -B dev/performance/check.py --config PATH baseline --check` | Existing finite inherited gates pass for the named source and products, including util-linux resources and close. |
| Integrated inference | `python3 -B dev/performance/check.py --config PATH inference --check` | Full selected relationship and joint corpus matrix passes; all retained eligible sites classified; old denominators preserved. |
| Four measured families | `python3 -B dev/performance/check.py --config PATH measure --check` | Every selected input has exact output and completion checks, raw samples, CPU, RSS, and attribution; readiness accepted. |
| Prepared experiment and shared wins | `python3 -B dev/performance/check.py --config PATH optimize --check` | O1 ships only if its keep gate passes; shared workload wins and all non-regression budgets pass. |
| Deployment | `python3 -B dev/performance/check.py --config PATH deployment --check` | Four complete workflow families pass, including real guest boot and shutdown; no mandatory skip or leaked resource. |
| Release candidate | `python3 -B dev/performance/check.py --config PATH close --check` | Same-source applicable proofs pass. Reports distinguish experiment rejection, target misses, deferred time, and blocked work from success. |

Do not embed the inherited full suite in `measure`, `optimize`, or each focused
iteration. Run it only for G1's acceptance source, I1's combined integration, and
R2's final source. An existing mandatory close command can own that suite at the
same milestone. Deduplicate invocation, not its acceptance obligations. Widen
after a material uncovered integration risk and state the reason first.

Existing narrow commands remain the development loop:

```sh
python3 -B -m unittest discover -s dev/util-linux -p test_resources.py
cargo dev gate src/sema/types.rs
cargo dev gate PATH...
xsht test --exact tests/xsh/stdlib/template.xsh::test_template_fields_and_dot
xsht test --exact tests/xsh/stdlib/fs.xsh::test_fs_root_operations_reject_traversal
xsht runtime-stats --json .work/tmp/runtime.json SCRIPT
xsht trace --syscalls --trace-top-syscalls 10 SCRIPT
```

Use absolute source-qualified verification/release `xsh`, `xsht`, and helper
paths inside the pinned container. The spellings above abbreviate those paths.
Rust builds use kache; Linux musl links with mold through existing target settings.
`cargo dev gate` does not replace private root-lib scheme/verifier tests. Select
those tests by exact name in a reusable debug harness, then run them directly.

The inherited profile adapter invokes these existing interfaces with real values:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-correctness-gates/launch.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --execute --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --profile-receipt PROFILE_JSON --release-opt-level 3 --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/forced-frames.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --profile-receipt PROFILE_JSON --output OUTPUT
python3 -B dev/util-linux/check.py resources --check --laputa-corpus LAPUTA
python3 -B dev/util-linux/check.py close --check --laputa-corpus LAPUTA
```

Reuse `forced-frames.py`, `final-comparative.py`, and
`native-scheduler-comparison/compare.py` for their rows. The adapter validates
their current argument parsers, receipts, mandatory selector counts, accounted
skips, and unchanged source before invoking them. Do not write replacement suites.
The old ARM64-only `bench/stdlib-port/run.py --linux` route must not be used as
the x86-64 gate. Reuse its workloads through the current pinned x86-64 runner.

The scheduler comparator keeps networking disabled and supplies a local
`xsh-test` hostname mapping. Its read-only mounts expose the source at `/work`
and its original absolute path, the shared Git directory, and
`/usr/local/bin/mold`. The existing `runner_artifacts` map binds mold's bytes;
each stage checks that executable before and after execution. The launch owner
must match it to the passed profile's mold binding before starting comparison.

Within the isolated Laputa tree, bind `XSHT` and its sibling `xsh` to candidate
products and use existing exact selectors. Start with these current controls:

```sh
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_builds_dependency_levels_in_isolated_roots_and_reuses
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_proof_failure_and_corrupt_final_never_publish_replacement
"$XSHT" test --exact tests/pm/pm_root.xsh::test_root_failed_composition_leaves_completed_output_untouched
"$XSHT" test --exact tests/pm/pm_store.xsh::test_store_gc_keeps_named_artifacts_and_removes_the_rest
"$XSHT" test --exact tests/system/qemu.xsh::test_qemu_supervisor_retries_qmp_injects_once_and_escalates_shutdown
make profile-test
```

Those controls do not supply the complete new lifecycle by themselves. D1, D2,
and R1 extend existing tests with the missing observable transitions. Prove the
tested and booted compiler is the candidate, not an old seed or `PATH` binary.

For R1, pass the clean candidate checkout as `XSH_ROOT`, and pass absolute
candidate host-tool paths as `XSH_BIN_DIR`, `XSH`, `XSHT`, and `XSH_HOST` to
the isolated Laputa Makefile. Run `make seed ARCH=x86_64 CARGO_JOBS=2` with
those bindings after the pinned input acquisition stage. This uses Laputa's
existing offline seed build and its own recorded release profile. Qualify that
profile separately if its flags differ from the benchmark products.
Check `.out/seed/x86_64/manifest.json`: `xsh_commit` must equal the candidate,
`xsh_dirty` must be false, features/target must match, and every listed product
and `core.tar.xz` must match its actual bytes. Feed that seed through the existing
`packages/xsh/PKGBUILD.xsh` repository input into the root generation.
Check the completed generation's `/usr/bin/xsh`, `xshi`, `xsht`, and installed
core content against the seed before boot. Bind that generation to the existing
image/guest proof. Candidate host tests alone cannot satisfy this condition.
Do not add executor version or timing data to immutable package keys; seed
payload identity already belongs to the package's repository input.

Formal maintenance follows `formal/README.md`: use `.work` for Lake workspace,
build and cache outputs. From `formal/`, run `scripts/audit-axioms.sh`,
`scripts/audit-axioms.sh --self-test`, and `python3 scripts/check-fresh.py`.
Keep their existing 8-GiB, 180-second build and 1,080-second check bounds.
Maintain inventory structure checks separately. No fragment count closes a
universal obligation or Rust-correspondence claim.

## Execution boundary

The owner explicitly launched this campaign on 2026-10-17. That grant covers
isolated XSH and Laputa edits, local commits and integration, pinned fetches,
owned Docker/VM operations, and bounded `doas` use within this document.
The deployment-scope and KVM decisions above further bound the grant.
Keep test and package writes inside disposable owned roots.
Do not edit the sibling normal checkout, host packages, live services, devices
other than required virtual-machine access, boot configuration, or real mounts.

Public API and language semantic changes beyond the already accepted inference
scope are forbidden. Preserve diagnostics, effects, module visibility, path bytes,
process exit semantics, and publication boundaries. Report every actual contract
change. Internal representation changes require their own negative validation
tests. A dependency cannot bypass the measured build, size, memory, or startup
gates. Do not update pinned Lean or Rust tools as part of this campaign.

## Non-goals

- New syntax batches, general ownership syntax, exact-record syntax, or expanding the utility catalogue.
- A native backend, JIT, daemon, mandatory warmup, or user compilation ritual.
- Command-specific Rust implementations behind XSH command names.
- Persistent arbitrary-script caches or a public artifact format.
- Another kernel, OS port, package-catalog campaign, production deployment, release upload, or CI changes.
- Closing all 72 universal formal obligations or calling the Rust compiler verified from fragment theorems.

## Reference map

| Source | Disposition | Role |
|---|---|---|
| [Architecture](ARCHITECTURE.md#the-libxsh-facade) | Reuse | One checked/indexed execution path and XSH policy/Rust mechanism boundary. |
| `src/stdlib.rs:51`, `src/stdlib.rs:183` | Change after profiling | Six embedded module sources and required-module selection. |
| `src/runner.rs:205`, `src/runner.rs:377` | Extend narrowly | Construction allocations and ordinary preparation boundary. |
| `src/frontend_stats.rs:37` | Extend narrowly | Existing stage memory/count report, not phase CPU timers. |
| `src/runtime/eval/indexed/full.rs:854`, `src/runtime/eval/indexed/full.rs:12874` | Reuse/change with one owner | Owned executable representation and semantic verifier. |
| `dev/consolidation/designs/inference.md:173`, `dev/consolidation/designs/inference.md:251` | Reuse accepted contracts | Immutable scheme authority, call substitutions, frontend disposal, verifier obligations. |
| `dev/consolidation/HANDOFF.md:71`, `dev/consolidation/CAMPAIGN.md:1494` | Reproduce | Pinned corpus result and unchanged historical accounting targets. |
| `dev/consolidation/winddown/gate-tools/` | Reuse | Finite inherited correctness, resource, forced, comparative, and scheduler gates. |
| `dev/util-linux/resources.py:1560`, `docs/CAMPAIGN-UTIL-LINUX.md:79` | Reuse | Same-source resource measurement and exact approved parity exceptions. |
| `bench/stdlib-port/run.py:25`, `bench/consolidation/README.md:1` | Reuse fixtures/sampling | Startup and complete glue workloads; ARM-only routes remain separate. |
| [Benchmarking](TESTING.md#benchmarking), `crates/xsht/src/stats.rs:29` | Reuse | Benchmark policy, allocation reports, and syscall diagnostics. |
| `tests/xsh/stdlib/fs.xsh:309`, `tests/runtime/process.rs:303` | Extend | Actual confinement and process cleanup boundaries. |
| `../laputa/docs/PM.md:81`, `../laputa/tests/pm/pm_execute.xsh:294` | Reuse/extend in isolated tree | Immutable build, proof, and generation operations. |
| `../laputa/docs/QEMU.md:24`, `../laputa/tests/system/qemu.xsh:199` | Reuse/extend | Hardware-backed guest proof and supervised shutdown. |
| [Formal scope](../formal/README.md), [trust checks](../formal/README.md#build-and-check) | Maintain | Trust-check bounds and explicit absence of a general source/IR replay corpus. |
| `dev/performance/check.py` | New in G2 | Thin selection/qualification adapter for the existing mechanisms. |
| `docs/PERFORMANCE-BASELINE.md`, isolated Laputa deployment reproduction doc | New at G1, updated at close | Concise accepted source/products/platform/gates/limitations for everyday use. |

The old campaigns mixed long historical receipts with active requirements.
Another source-specific full failure then sent the executor back through many
checkpoints. Keep their archives, but make the new baseline page answer whether
the current revision is usable without requiring that history. Never preserve
duplicate per-commit copies of controllers solely to record provenance.

## Decision log

- 2026-10-17: Plan against local merged master; keep historical acceptance and current verification separate.
- 2026-10-17: Owner selected isolated Laputa edits plus pinned fetches, and permitted `doas` for KVM access.
- 2026-10-17: KVM API probe succeeded through `doas`; sandbox visibility is not a platform limitation.
- 2026-10-17: Keep 95%/85% historical targets visible without changing `{}` semantics or hiding retained restrictions.
- 2026-10-17: Prepared modules receive a keep/reject experiment; useful shared runtime gains remain a separate deliverable.
- 2026-10-17: Launch source is `c6d36683872e4d0dc9173a9c2c8ff09e8e6b9d23`, the owner's documentation-only plan commit over `3ed58895c`. No product-source change requires a new planning baseline.
- 2026-10-17: G2 may extend explicit `check-current --review-evidence` to call the existing review verifier. Preserve default checks and strict `final-close` behavior. G1 owns the canonical bootstrap producer seam in existing controllers.

## Campaign log

- Planning: source survey, seven-sample startup diagnostics, allocation probes, 53 resource-controller tests, two native selectors, and KVM API probe complete. No execution phase started.
- G0: Reproduced removed-abort failures in all three stream probes. Replaced the stale exit spelling and diagnosed redundant pipeline parentheses. Five exact-output cases, three invalid-mode cases, and both native foundation selectors pass in the pinned container with qualified `7e8881d96` tools. No current-source build or performance credit is claimed.
- P0 lanes from `b1ebd7fd8`: `.work/wt/perf-acceptance` on `campaign/performance-acceptance-20261017`; `.work/wt/perf-measurement` on `campaign/performance-measurement-20261017`; `.work/wt/perf-inference-audit` on `campaign/performance-inference-audit-20261017`. Root master remains the integration checkout.
- G4 checkout: `.work/wt/laputa-foundation` on Laputa branch `campaign/xsh-deployment-foundation-20261017`, at `bbe05e4`, the owner's documentation update after the planning survey. Its source remains isolated from the normal sibling checkout.
- G2 foundation integrated at `4c13a8b7f`: twelve adapter policy controls and the exact native functional-versus-final review control pass. Functional qualification omits only the timing review row; final qualification still requires every row. No functional-close record has been issued.
- G1 timeout repair integrated at `85e39ab0b`, with the retained deterministic proof at `b51357241`. The old shell fixture can fork after SIGINT forwarding and defer its trap beyond cancellation grace; the repaired fixture blocks in a builtin read. Both deterministic controls and the exact INT/TERM native selector pass with the matched candidate tools. Runtime cancellation behavior is unchanged; full acceptance remains pending. The review checkout is `.work/wt/perf-timeout-review` on `campaign/performance-timeout-review-20261017`.
- G4 tool preparation uses read-only detached XSH checkout `.work/wt/perf-products` at `429f6ff080efac12e5706ab45ec05539a24e57e6`. The bounded offline build produced matched `xsh`, `xshi`, and `xsht` under the isolated Laputa checkout's `.out/xsh-target/x86_64-unknown-linux-musl/release`. Both package execution/publication selectors and explicit executor-digest provenance pass. These seed-feature products are not the canonical singleton bootstrap receipt or a boot qualification.
- 2026-10-18: Phase diagnostics integrated at `d12c360a4`; four stats tests, the runner's three-scenario observation test, and actual success/type-error/parse-error JSON probes pass. The coordinator reran the five Rust tests with the source-identical emitted harness and frozen integration checkout. First useful output and performance qualification remain separate work.
- 2026-10-18: Inference audit and corpus adapter integrated through `d53440e8a`; all five retained adapter controls pass. The audit reconstructs the historical 1,040 view without crediting the later 877-view recipe or broader behavior witnesses. Compiler capability integration has not started.
- 2026-10-18: `.work/wt/perf-baseline` freezes clean `d53440e8a`. Its first canonical bootstrap failed before Cargo because Docker could not create `/work/target` under the read-only source bind. The failed attempt remains under that checkout's `.work/tmp/canonical-bootstrap-d53440e8-20261018`; cleanup resolved. Repair the existing controller's private target mount before a fresh freeze and attempt; no build or acceptance credit attaches to this failure.
- 2026-10-18: Isolated Laputa integration is `.work/wt/laputa-integration`, branch `campaign/performance-deployment-20261018`, at `639184e`. Its exact executor-provenance selector passes with the matched tools. Profile planning stops at the absent `.out/seed/x86_64`; the transitive input closure and real boot remain unqualified. The normal sibling checkout is untouched.
- G1 follow-up: `.work/wt/perf-acceptance-mount` on `campaign/performance-acceptance-bootstrap-mount-20261018` owns the mount and host-boundary repairs. Clean freeze `.work/wt/perf-baseline-r2` at `69091f57c` passed the actual singleton bootstrap. Its canonical run then stopped at nine host-boundary tests writing under read-only source. Repair `760877c45` uses the configured temp directory; the retained red and all nine focused greens pass. Neither failed full attempt supplies acceptance credit.
- G2 completion: the original Rust allocation result and JSON are preserved at `c28e0170d`; a separate tooling entrypoint supplies phase JSON. Five focused stats tests and three actual CLI probes pass. The integrated hostpipe sampler passes eighteen tests, and all fifteen frozen workload cases pass with immutable seed products and null timing fields. An earlier tranche using shared products remains unqualified because those files changed during the run.
- Inference preparation: `.work/wt/perf-inference-audit` now uses `campaign/performance-inference-i2-prep-20261018`; `.work/wt/perf-inference-port` uses `campaign/performance-inference-port-20261018`. The reviewed source delta applies textually without conflict and includes the accepted host, Set, Optional, unsigned, and constant-authentication capabilities. Implementation was waiting for G1 acceptance at this checkpoint. The source audit classifies 911 retained sites and leaves 932 pending; protected contracts make the historical percentage targets unattainable.
- Formal preparation: `.work/wt/perf-formal` on `campaign/performance-formal-20261018` corrected the provisional inventory at `c1adbbd36`. Inventory, 51 inventory tests, three isolation controls, the Lake build, axiom audit and negatives, and all six kernel checks pass. Lean sources are unchanged; the 91-owner inventory retains 72 planned obligations, null source identity, and no conformance cases. These are maintenance results, not Rust verification or F1 completion.
- G4 inputs: the existing publisher produced a validated seed from frozen `429f6ff0`. Offline planning resolves 47 packages; all 37 pinned archives are fetched and SHA-verified, totaling 368,424,255 bytes. Two PM selectors pass against the read-only product snapshot `.work/tmp/perf-products-429-seed-r1`. The required Laputa host-tools image and saved tar are absent; its Dockerfile installs unpinned APK packages, outside the pinned-fetch boundary. Package builds and real boot remain blocked on that input, and no guest qualification is claimed.
- G1 follow-up: clean freeze `.work/wt/perf-baseline-r3` at `b31bebc73` passed its fresh singleton bootstrap. Canonical execution stopped at the documentation code scanner: 751 library tests passed, one failed, three were ignored; the preceding integration group passed 58 with three ignored. The scanner mistook the documented basename of `dev/performance/corpus/check.sh` for a diagnostic code. Repair `42771860e` resolves actual referenced source paths while retaining unknown-code checks. The minimal red and all six diagnostic-catalog greens pass; the failed full receipt remains preserved and supplies no acceptance credit.
- G2 baseline adapter integrated at `922b2591c`; nineteen policy tests pass. Optional explicit receipt paths compose the existing functional, profile, observer, forced-frame, comparative, and scheduler owners. Util-linux close still requires its actual one-use independent review and its distinct private-namespace suite; standalone review delivery remains incomplete. No completed receipt chain or current-source baseline is claimed.
- D2 focused workflows integrated at `a2c375153` and `3725c5bff`. The default process-stream path buffered despite its live-stream contract; the existing producer now serves default text and bytes while retaining zero-only completion and explicit acceptance policies. The coordinator replayed six native stream/filesystem selectors as UID 1000 and the exact Rust process-tree cancellation/cleanup test with byte-checked products; all pass. Two existing direct-pipeline selectors also pass. Rooted checks cover raw names, hardlinks, symlink confinement, failed replacement, and permission denial; deterministic mid-write interruption remains separate H2 work.
- G2 CPU diagnostics integrated at `eca7a41c6`; all eight focused controls pass in the coordinator's replay, and three CLI probes preserve output and error behavior. The additive tooling report measures process CPU across its threads, excluding descendants and waiting. The old allocation report remains unchanged; these observations carry no performance credit. The retained-effect review at `a8586ffa4` leaves all 202 clauses in place and identifies the missing facts for 60 non-findings.
- G1 invocation repairs integrated at `c2138ba25` and `4b8a1ec22`. The existing owners accept exact owned frozen checkouts while retaining common archive pins and locks; profile and forced-frame commands now require an explicit clean Laputa checkout. The adapter at `1942b831b` validates those bindings, and twenty policy tests pass. Focused owner tests and an actual main/frozen checkout, isolated corpus, scratch, pinned-helper, and target-path preflight pass. No full gate has run since the stream repair. Isolated Laputa integration `263a650` passes the composed replacement/publication/removal/GC fixture with historical seed tools; candidate qualification remains pending.
- G4 input update: the exact digest-pinned Alpine base is now fetched. The absent host-tools image still requires seven unversioned APK inputs; none were resolved or fetched, and no package build or VM run has started.
- G1 follow-up: frozen `.work/wt/perf-baseline-r5` at `a857527a7` passed its fresh singleton bootstrap after an earlier bootstrap was interrupted for unrelated host memory pressure. The canonical attempt has recorded a corpus layout-invariance failure; preserve its receipt and isolate the offending input before another full run. No current-source acceptance is claimed.
- Independent close review found selector reconstruction, callback working-directory, private-runner deadline, candidate diagnostic, and process-stream deadline defects. Controller repairs integrated at `71c34e1ab` and `8d885aa72`; the coordinator replayed 57 resource tests, 21 adapter tests, and two cancellation/cleanup controls successfully. The rooted-memory selector fails with the old matcher and passes with the tightened matcher using snapshotted products rebuilt from the frozen source. The thirteen `lsipc` crash exceptions remain unchanged. The process-stream deadline repair and the separate layout failure remain open.
- The existing structural scanner completed bounded small- and large-file diagnostic probes with matching source/product hashes and resolved container cleanup. A full-source measurement exceeded its initial 120-second bound without a report; this supplies no acceptance or performance credit. The functional adapter now selects the historical 900-second full-scan budget explicitly, while its focused self-test retains 60 seconds. The required full structural report remains pending.
- Isolated Laputa integration advanced to `cf61f4b`, adding concurrent publication retries with both workers acknowledging readiness before release. The coordinator replayed the composed replacement/publication/removal/GC selector successfully with the immutable historical seed. Candidate qualification and real boot remain pending. The normal sibling checkout is untouched.
- Follow-up checkouts are `.work/wt/perf-util-close` on `campaign/performance-util-close-20261018` for the integrated controller repairs and `.work/wt/perf-layout` on `campaign/performance-layout-20261018` for the failed layout investigation. The boundary and measurement owners retain their existing isolated checkouts. Keep these checkouts and failed-run evidence until integration and acceptance resolve.
- The `a857527a7` canonical run finished unsuccessfully with cleanup resolved. Its layout harness rejected existing lint errors when perturbation added redundant-parentheses errors. Harness repair `630e064ba` preserves all diagnostics and rejects unrelated checker and parse errors. The minimal fixture failed before the repair; the coordinator replayed all five focused controls successfully with unchanged harness and tool hashes. A fresh full run remains required.
- Supplemental and util-linux gates now require an explicit clean Laputa corpus. Supplemental receipts retain the executed Docker arguments, and the reader checks the selected read-only mount and environment. Local policy checks pass; no supplemental acceptance receipt has been issued.
- The authorized host-tools image is saved under `.work/wt/laputa-foundation/.cache/images/laputa-host-tools-x86_64-faf126e1d76d709e.tar`. Reload preserved image ID `sha256:0f1d14f60e28ec442566326db92872916b2ba61fc437ac07fa0e32c9a4fde371`; the coordinator checked the tar digest and loaded image independently. Reuse the saved image offline. Package-closure execution and real boot remain pending.
- Offline preparation subsequently built the package-tools image and all 47 planned artifacts for the eight profile roots plus Linux. The coordinator checked every selected artifact key, package ID, and built origin against `.work/wt/laputa-foundation/.out/world/x86_64/plan.json`. The build used the historical `429f6ff` seed and the 37 validated cached URL inputs. Its container had networking disabled, two CPUs, and 8 GiB memory; cleanup resolved and both source checkouts remained clean. Keep the isolated artifact cache. Current-candidate, profile, boot, and timing qualification remain pending.
- Deadline repair `d674b26a1` adds an owned timeout watcher and shared child-status authority. Paused-consumer and cancellation regressions fail on pre-watcher products and pass on the candidate; the worker-failure regression also has an executable red and green. Independent source review found no remaining P1/P2 findings. Two focused Rust controls and fifteen native selectors pass; the coordinator retained a replay of all fifteen after integration under `.work/tmp/runtime-integration-r2`, with unchanged product hashes and resolved cleanup. Timed streams now report `HostIo` before spawning when child status is discarded; restoring status collection does not revive an invalidated identity. SPEC source and generated output record this contract. Full acceptance remains pending.
- Deployment readiness found that the historical seed's `xshi` is disabled and the old headless profile proof does not detect that. Isolated Laputa repair `0ce56a3` adds an exact-output shell command to the graphical proof and routes capture failures through the existing guest failure protocol. The disabled historical shell fails the probe as expected; source checking and the existing overlay control pass. Enabled-candidate and guest execution remain pending. Keep it on the foundation branch while G1 binds the unchanged `cf61f4b` integration corpus. The existing `make seed-asterinas-shell` producer enables `xshi/laputa-asterinas` for the Linux shell payload; this selects no replacement kernel.
- G1 freezes clean `c77bb05b3` in `.work/wt/perf-baseline-r6`. Its fresh singleton bootstrap passed. The canonical run then failed at XSHT integration: 119 passed, two failed, five filtered; the earlier library group passed 757 with three ignored. Layout perturbation lost default-parameter and redundant-close diagnostics in three file/code groups. The written FIFO polling test missed a hangup while its desugared version passed. The failed receipt and logs remain under that freeze's `.work/tmp/canonical-replay-c77bb05b-r6`; cleanup resolved and shared locks are free. No later gate ran and no baseline is accepted. Keep the frozen source and Laputa corpus unchanged for diagnosis.
- Independent inference review identified nested-schema ownership ranges and unsigned literal validation without optional facts as cases to reproduce before I1 acceptance. The pending controls and constructor-origin coverage limit are recorded in `docs/PERFORMANCE-INFERENCE.md`. No inference implementation or executable qualification is claimed.
- Follow-up writers are `.work/wt/perf-layout-r6` on `campaign/performance-layout-r6-20261018` and `.work/wt/perf-unix-poll` on `campaign/performance-unix-poll-20261018`. Each must establish a minimal causal failure before repair; no unchanged full-run retry is authorized by a focused pass. Genuine r6 products are byte-checked copies under `.work/tmp/perf-products-c77-r6`; the older integration target is not that product set.
- M1 wiring is staged in `.work/wt/perf-measurement-m1` on `campaign/performance-measurement-m1-20261018`. Independent source review passed after fixing diagnostic-file collisions. The additive tooling contract accepts explicit fixture/output paths, returns and validates the existing in-process close report, and distinguishes owned-leader CPU/RSS, batch memory, first-line latency, and instrumented-process syscall diagnostics. Focused execution remains required; no measurement or performance acceptance is claimed.
- Actual mid-write interruption fixtures are staged separately in `.work/wt/perf-boundaries-publication` on `campaign/performance-boundaries-publication-20261018` and isolated Laputa `.work/wt/laputa-publication` on `campaign/performance-publication-interruption-20261018`. They use child-only file-size limits and SIGXFSZ to check old-state preservation, confined partial writes, and Laputa retry cleanup. Neither fixture has executed; the G1-bound Laputa `cf61f4b` remains unchanged.
- M1 wiring integrated at `247bbaa4b` after independent source review and 29 selected passing controls. Six integration controls also pass across `.work/tmp/m1-integration-r1` and `r2`; the first attempt's missing writable sampler-fixture mount is retained. This establishes adapter behavior, not an executed workload baseline. The typed close report remains a trusted in-process owner result, not a serialized acceptance token.
- Rooted interruption coverage integrated at `ac49f64eb`; FIFO polling isolation and an inherited-writer barrier control integrated at `c23fecba3`. The original FIFO test sampled a shared process descriptor table without waiting; isolation removes interference from parallel test forks. The barrier proves the inherited-writer topology, not a particular historical fork timing. The coordinator replayed the new filesystem case and both FIFO cases successfully with unchanged r6 products; logs are under `.work/tmp/native-integration-c23-r1`.
- Lint repair `6f1906b1b` permits only redundant-parentheses diagnostics through the existing semantic proof gates. All diagnostics remain visible and other checker errors still veto fixes. Independent source review and three focused controls pass, including autofix/recheck; the coordinator replayed both proof-safety controls. On the three actual affected c23 files, all 86 original diagnostics remain under grouped operands, with only four redundant-parentheses errors added. An explicit formatter probe was rejected by automatic approval review under the repository's no-formatter rule; the completed file comparison used lint only.
- Bootstrap retention repairs `4a03de9d7` and `1a73cac8d` address an actual r6 reread failure: later Cargo stages replaced the executable named by its bootstrap receipt. The existing schema now records exact retained bytes beside the receipt; preflight and prelaunch still require the mutable target to match. Independent review and all 36 preflight controls pass in the pinned container. Copy failures retain both the primary error and independent close failures. The failed r6 receipt remains untouched; this repair supplies no build or gate credit.
- Isolated Laputa `e516312` adds actual mid-copy SIGXFSZ interruption and retry to the lifecycle fixture. Final-source checking and the exact lifecycle selector pass with matched c77 r6 products, preserving old index/generation, checking the 4096-byte partial file, and verifying retry content and cleanup. This is focused fixture evidence, not an accepted compiler or guest result. Keep this branch and graphical-proof `0ce56a3` separate from the unchanged G1-bound `cf61f4b` until its receipt chain closes.
- Clean freeze `.work/wt/perf-baseline-r7` at `598af3a52` passed fresh bootstrap and the full layout-invariance corpus check. Retained bootstrap validation also passed after Cargo replaced the mutable executable. The desugared-corpus check then stalled in `dev/tests/test-system-report-check.xsh`; a bounded futex trace and executable symbol mapping identify the global symbol interner as the shared wait address. JSON record-key sorting nests two reads of that lock, contrary to the callback's contract. The coordinator cancelled the run through its owner after preserving diagnostics. Its receipt is failed-or-incomplete with SIGINT, not an assertion failure or owner timeout; cleanup resolved and shared locks are free. No G1 acceptance credit follows. `.work/wt/perf-json-symbol-lock` on `campaign/performance-json-symbol-lock-20261018` owns a focused reproducer and repair; no unchanged full-run retry is planned.
- JSON repair `cc03ec1ea` replaces that nested callback with the existing lexical `Name` comparison. A bounded subprocess reproducer stalled on the old comparator and was killed and reaped; all four JSON tests pass after the one-line repair. Independent review passed, and the coordinator replayed all four against the source-identical integrated checkout with unchanged harness bytes and resolved cleanup. Logs are under `.work/tmp/json-integration-cc-r1`. This establishes the exercised concurrency defect and exact output preservation; the schedule-sensitive reproducer does not identify the canonical process's precise blocked callsite. A fresh canonical run remains required.
- Freeze `.work/wt/perf-baseline-r8` at `100e2551e` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, 14 soundness tests and ten Linux privilege tests. The full native suite selected 14,152 tests and emitted twenty failures before coordinator cancellation. The receipt is failed-or-incomplete with SIGINT, not a timeout; cleanup resolved and shared locks are free. Focused probes found a runnable fixture discovered as a test, missing private-context admission, privileged device/owner operations under an unprivileged UID, direct kernel reads in `lsipc`, and stale active metric-observer pins. The btop scaling failure's assertion detail was lost; one subsequent isolated readiness-qualified diagnostic passed, leaving the original cause inconclusive. No current baseline or performance credit follows.
- Repairs integrated at `7a57b181f`, `607dc1360`, `b7232833e` and `3c717fe96` retain the timing threshold in the existing release profile, move the observer fixture into the existing excluded data hierarchy, distinguish ordinary unsupported contexts from strict advertised private contexts, and route IPC observations through a typed XSH domain. Raw limit text and command policy remain unchanged. The current metric observer reproduced the exact frozen start-source report and item inventory before only its two active pins changed; historical evidence is unchanged. Independent review passed after correcting a missing Linux guard and an unwanted limit-validation narrowing. The coordinator replayed nineteen integrated tests, including seven genuine private positives with zero skips, under `.work/tmp/native-integration-3c-r1`; source bytes matched the reviewed lanes, product hashes remained unchanged, and cleanup resolved. A fresh canonical run and the readiness-qualified release btop gate remain mandatory. The four repair branches and their `.work/wt/perf-{btop-scaling,native-context,ipc-domain,observer-provenance}` checkouts retain the failed and focused evidence.

- Freeze `.work/wt/perf-baseline-r9` at `2a1cd7176` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, both deep grammar checks, 14 soundness tests and ten Linux privilege tests. Its native suite emitted one failure in `test_process_stream_timeout_preserves_unobservable_child_identity` before controlled cancellation. The retained receipt is failed-or-incomplete with SIGINT, not a timeout; final cleanup resolved. No assertion body was emitted, so the historical failure's exact cause remains unconfirmed. Repair `f0687f324` adds a child-ready barrier after its successful initial write, preserving identity invalidation, survival assertions and the five-second timeout. A forced late-write schedule reproduces a 5.3-second failure, and a PIPE-trap control confirms Broken pipe before the survival marker. Independent source review passed. The coordinator replayed both negative controls and the repaired selector plus three neighbors against the frozen integrated checkout; all expected results held, product hashes were unchanged and cleanup resolved. Separate logs are under `.work/tmp/stream-integration-f068-r1`. Keep the repair checkout `.work/wt/perf-stream-identity-r9`, frozen integration checkout `.work/wt/perf-stream-integration`, and failed r9 evidence. A fresh canonical run remains required; no supplemental gate or current-source acceptance follows from these focused results.

- Freeze `.work/wt/perf-baseline-r10` at `dfee5f276` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, 14 soundness tests and ten Linux privilege tests. The repaired identity selector passed in the full native run. That run later failed `test_api_surface_matches_the_recorded_reference`; controlled cancellation retained a failed-or-incomplete SIGINT receipt with resolved cleanup and unchanged products. The snapshot omitted the three Linux IPC observation functions and record already introduced by `c772562b9`. Fixture-only repair `c48f7db47` adds exactly those entries and their counts; all other summary content is unchanged. The coordinator replayed the old-fixture negative control, all 67 API-tool tests, and the byte-exact live summary against `.work/wt/perf-api-integration`. The existing documentation check passed, including three tour-project tests, after a retained setup failure targeting a nonexistent read-only mountpoint. Repository check, lint, diff and the full current structural ratchet also passed with unchanged products and resolved cleanup. Evidence is under `.work/tmp/api-integration-c48-r1`, `.work/tmp/api-docs-tail-c48-r2` and `.work/tmp/canonical-tail-c48-r1`. The structural report retains zero lowering resolvers and 46 generated stages; source-line growth remains advisory under the existing policy, not a source-reduction result. Keep the failed r10 freeze and `.work/wt/perf-api-surface-r10` repair branch. The next canonical run must finish its already-running native suite after any failure marker so the existing runner can emit complete assertions and counts; its deadline and no-credit-on-failure rules remain unchanged. No full G1 acceptance follows from these diagnostics.

- Freeze `.work/wt/perf-baseline-r11` at `89408f371` passed the canonical gate: 758 library tests, 122 XSHT integration tests, both deep grammar checks, 14 soundness tests, ten Linux privilege tests, and 14,008 native tests. Native failures were zero, with 150 skips reviewed separately. Repository, documentation and structural checks passed. The retained bootstrap remained valid after Cargo replaced its mutable executable. Cleanup resolved, products stayed unchanged, and the structural report matched the preceding focused run. This untimed run supplies no performance credit. The supplemental run built its products and three harnesses, then stopped at seven warning-denied Clippy diagnostics. Its five Linux contexts did not run. Keep both receipts and products unchanged. The Unix-socket selector that skipped for excessive scratch-path length subsequently passed with scratch mounted at `/t`, using the same read-only source and products. Its exact result was one pass and zero skips, with resolved cleanup, under `.work/tmp/socket-r11-r1`. G1 remains open. Repair the Clippy diagnostics, then run supplemental correctness before another fresh canonical qualification.

- Warning repair `44de8d71c` preserves portable IPC widening and simplifies six equivalent expressions. Independent source review found no P1 or P2 issue. The scoped IPC Clippy exemption records why the conversion remains necessary. Freeze `.work/wt/perf-baseline-r12` passed the full supplemental gate, including warning-denied Clippy and all five Linux contexts. Those contexts passed 23, 58, two, two, and 18 tests, respectively. Three ignored date-clock tests require a disposable guest. Two traceroute skips require raw-network capabilities absent from their unprivileged context. Filtered tests receive no coverage credit. The short-path Unix-socket selector also passed with zero skips using this source and its supplemental products, under `.work/tmp/socket-r12-r1`. Source and Laputa stayed clean, products stayed unchanged, and cleanup resolved. The fresh canonical qualification is pending. No functional close, performance acceptance, inference integration, or guest boot follows from supplemental success.

- The r12 canonical gate passed at `44de8d71c`: 14,008 native passes, zero failures and 150 reviewed skips. Both earlier failing selectors passed. Library, XSHT, deep grammar, soundness, privilege, repository, structural and documentation checks passed. Cleanup resolved and retained-bootstrap validation passed after the mutable target changed. The structural values retain zero lowering resolvers and 46 stages. Its production-source count is one line lower; shifted evidence positions make the report different from r11. This is not a source-reduction result. Final-product socket coverage passed with zero skips. The existing shared-import trace recorded eight requests, one preparation, one check and seven reuses for one shared module owner. Evidence is under `.work/tmp/socket-r12-canonical-r2`, `.work/tmp/cache-r12-r1` and the frozen r12 checkout. Functional validation stopped before effects because its reader rejected the producer's extended environment metadata and copy-only harness staging step. The isolated reader repair is under review in `.work/wt/perf-functional-reader-r12`. Preserve the passed receipts and qualified compiler source. Run the corrected reader from a separately named frozen tool checkout; do not transfer canonical credit to that checkout. Functional publication and the remaining G1 queue are still pending.

- The corrected reader is integrated through `2444bef65` and frozen separately in `.work/wt/perf-validation-r2`. It accepts the producer's extended environment and exact copy-only stage, rejects staging roots overlapping the Laputa corpus, and runs the structural container as the caller. Fourteen policy tests and the caller-identity regression pass. The first full scan failed Git's ownership check as root; a bounded read-only Docker contrast reproduced status 128 as root and exact-source success as the caller. The corrected full functional check passed and published `.work/final-review-evidence/functional-close-44de8d71c04f155034a7c1e8cb8e7edddd9da15f.json` under the unchanged r12 freeze. Its canonical and supplemental receipts, structural report, semantic review and skip dispositions are bound by the existing validator. Both failed tool attempts remain preserved. The normal opt3 profile is next; G1, inference integration, timing acceptance and guest boot remain incomplete.

- The r12 normal opt3 build passed, but its completed native suite failed: 13,946 passed, one failed and 211 skipped. `tests/xsh/stdlib/env.xsh::test_env_functions_and_path_list` reported `spawn: Text file busy` while executing its freshly written helper. The failed receipt remains under the freeze's `.work/consolidation/final-performance-gates/profile-cost-44de8d71c04f155034a7c1e8cb8e7edddd9da15f`; both readiness checks passed and container cleanup resolved. The 61-skip increase is accounted: the caller-UID context adds 64 namespace/capability skips, while the short scratch path and group setup allow three canonical skips to run. Both runs enumerate 14,158 tests. No failed profile acceptance or speed claim follows. `.work/wt/perf-env-exec-r12` owns bounded reproduction; preserve the qualified source, products and failed receipt unchanged.

- Fixture repair `0dfebb001` creates the executable in a completed child and retains the original scoped-parent environment and PATH execution assertions. The inherited-writer control demonstrates `ETXTBSY`; it does not establish the historical fork schedule. Independent review found no P1/P2 issue. The coordinator's integrated env-file check passed all 20 tests with unchanged r12 diagnostic products and resolved cleanup, under `.work/wt/perf-validation-r2/.work/tmp/env-integration-r13`. Freeze `.work/wt/perf-baseline-r13` at this commit requires fresh qualification; r12 receipts do not qualify it. Its supplemental correctness run precedes fresh bootstrap and canonical execution. Inference remains unintegrated.

- R13 passed fresh supplemental correctness, singleton bootstrap and canonical execution. Canonical native results are 14,008 passed, zero failed and 150 skipped; the repaired env selector passed. Compiler/runtime, corpus-invariance, soundness, privilege, repository, structural and documentation checks passed. All 150 native skip selectors/reasons and 11 Rust ignore instances match the reviewed r12 dispositions, apart from one run-specific capability fixture path. Cleanup resolved, shared locks are free, source stayed clean and retained-bootstrap validation passed after mutable-target replacement. These are current-source correctness results, with no timing credit. On the owner's acceleration request, the remaining baseline queue has four workstream owners: serialized performance producers, functional-close assembly and controls, skip/context review, and final private/resource close. Functional publication, the normal profile, observers, forced/comparative/scheduler runs and actual util-linux close remain required before G1 closes.

- R13 functional qualification passed and published `.work/final-review-evidence/functional-close-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7.json` under the frozen checkout. The validator checked the current canonical and supplemental results, structural report, nine semantic review rows and six skip contexts. The reviewer rechecked all 39 evidence bindings and 11 source identities. Current-product socket and shared-import controls passed. The first attempt completed validation but could not publish because the output directory was absent. That setup failure remains recorded. The unchanged-input rerun passed after directory creation, with resolved cleanup, free locks, unchanged products and clean source. The serialized performance lane now owns the remaining profile, btop, observer, forced-frame, comparative and scheduler runs without intermediate approval pauses. Actual private/resource close follows those runs. G1 remains open.

- R13's normal opt3 profile passed with 13,947 native passes, zero failures and 211 reviewed skips. Both build and suite readiness brackets passed. Cleanup resolved and products stayed unchanged. The skip set matches the preceding profile context. Compared with canonical, 64 namespace/capability selectors skip, while two group-dependent selectors and the short-path socket selector pass. The separate btop profile passed with both readiness checks successful. The observer then failed during isolated repository lint while traversing a generated `.work/` long-path fixture. Its failed receipt and stderr remain under `.work/final-review-evidence/final-performance-gates/observers-0dfebb001-r13` in the freeze. Cleanup resolved, but the observer batch did not run. The repair lane is investigating the controller's relocated source mount and inaccessible linked-worktree Git metadata. Forced, comparative, scheduler and private/resource acceptance remain pending. Preserve the qualified compiler source and its passed results.

- Observer repair `82b3ca7b4` preserves the source's absolute mount path and exposes the shared Git metadata read-only. The old mount hides linked-worktree exclusions. A minimal actual Docker control reproduces `ENAMETOOLONG` with the old command and lints exactly one visible file with the fix. Existing observer and temporary-directory tests pass, and independent review found no P1/P2 issue. Validation tool `.work/wt/perf-lint-scratch-r13` is frozen separately at `9b65865e542f1b00b161d8b93a0aa7a5991253bd`. Compiler source, products and qualification remain R13 at `0dfebb001`. The retained historical artifact snapshot and its pins stay unchanged. A fresh timed observer attempt uses `observers-0dfebb001-r13-r2`; the failed first receipt remains a failure. Focused controls supply no full observer acceptance.

- The corrected R13 observer attempt `observers-0dfebb001-r13-r2` passed with clean source and unchanged selected release products. Isolated repository lint and the canonical resource batch completed successfully, with accepted readiness brackets and resolved cleanup. Native results are 14,008 passed, zero failed and 150 skipped. The native skip selectors and reasons exactly match canonical. All Rust ignores and filters retain their reviewed context restrictions. The independent review binds the final receipt and logs. The executing validation tool remains `9b65865e542f1b00b161d8b93a0aa7a5991253bd`; the qualified compiler remains R13. The first failed observer attempt supplies no acceptance credit. Forced frames, comparative profiling, scheduler pairs and private/resource close remain required.

- The first R13 forced-frame gate failed with 13,942 passes, five failures and 211 skips. Its receipt and logs remain under `forced-frames-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7`. All skip selectors and reasons match the normal profile. Three hostname tests failed DNS resolution. Two Docker-tooling tests failed with `StageError.MissingTool`. The owned offline container had no hosts entry for its own hostname and lacked `/usr/local/bin/mold`. Cleanup resolved and inputs stayed unchanged. A four-way causal control reproduced all five failures with the qualified products. Adding only a local host entry fixed only the three hostname tests. Adding only read-only mold fixed only the two tooling tests. Both changes passed all five. Source and products remained unchanged, networking stayed disabled, and every control cleaned up. Evidence is under `.work/wt/perf-forced-context-r13/.work/tmp/context-controls`. The repair belongs to the forced-frame controller and its strict receipt reader. A fresh full forced-frame pass remains required before comparative and scheduler execution.

- Forced-context repair `1e0f2cf77` retains offline networking, supplies a local self-host mapping, and mounts the profile-qualified mold executable read-only. Producer and reader now require matching `mold` snapshots alongside `source` and `binaries` in schema-1 inputs. Incomplete older forced receipts are rejected. Reader controls reject conflicting Docker option aliases before the pinned image. The repaired command passed all five affected selectors. Existing controller tests, eleven baseline-reader tests and independent review passed. Clean validation tool `.work/wt/perf-forced-context-r13` is frozen at `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`, separate from qualified compiler R13. Its fresh full run uses `forced-frames-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7-attempt-context-r2`. The prior failed receipt stays unchanged. Final baseline validation uses this corrected tool with tool-local scratch and the original R13 source, product and functional bindings.

- The corrected full R13 forced-frame attempt passed with 13,947 passes, zero failures and 211 reviewed skips. All five former failures passed. The skip selectors and reasons exactly match the normal profile. Source, Laputa corpus, selected binaries and the qualified mold snapshot stayed unchanged. Process/container cleanup resolved and all three shared locks were free after the run. The independent reviewer bound the completed receipt and native log. This qualifies forced execution behavior, without a speedup or private-parity claim. Comparative profiling, scheduler pairs and private/resource close remain pending.

- Comparative attempt `comparative-0dfebb001-r13-r2` passed on clean R13 source with the qualified opt3 product. This was the first actual comparative measurement; the unused original output name remains absent. All 35 parity cases matched across five workloads and seven tools, and all 3,780 samples completed. Readiness passed before and after, timing was accepted, and cleanup resolved. The independent reviewer bound the final report and receipt. These finite benchmark cases do not extend util-linux exact-parity credit or establish the later four-family performance objective. Three scheduler pairs and private/resource close remain required before G1.

- Scheduler preflight stopped before any reference build because the bridge selected a source-local target outside the launcher's shared namespace and omitted explicit Laputa/capture bindings. The genuine source-local candidate receipt and pinned two-file reference overlay remain unchanged. Repair `dd87fb9f9` derives the shared namespace from the common Git directory, uses the unchanged launcher from its reviewed tool checkout, binds the passed profile's clean Laputa corpus, and validates captured inner Docker argv. Namespace, environment, capture and corpus rejection tests pass, including launcher preflight from main and linked checkouts. Independent review passed. Clean bridge tool `.work/wt/perf-scheduler-bridge-r13` is frozen at `e59008ec1a5ed0425290a860c55bb1112a9e3d98`. It owns the reference build for run `r13-0dfebb001`. The comparison receipt uses `.work/consolidation/native-scheduler-comparison/r13-0dfebb001/receipt.json` under R13. Final baseline validation still uses tool `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`; compiler qualification remains R13. No scheduler execution credit follows from preflight tests.

- Reference build run `r13-0dfebb001` exited 101 before compilation because its existing Cargo argument encoded LTO's `thin` value without TOML quotes. The process record, logs and exact Docker capture remain preserved. No reference artifact or receipt was produced. Container cleanup resolved and locks were free. Repair `a81ba825c` quotes that value. A regression parses all three emitted profile settings and fails on the old argument. Existing lightweight bridge tests pass. Clean tool `.work/wt/perf-scheduler-cargo-r13` is frozen at `5fbca8d82d394c3c39edc1251b00c12cbcde08b3`. Fresh run `r13-0dfebb001-r2` uses the existing qualify, candidate, reference-build and comparison owners. Its final receipt selection is `.work/consolidation/native-scheduler-comparison/r13-0dfebb001-r2/receipt.json` under R13. The earlier failed run and tools remain unchanged. No scheduler pair or reference-build acceptance follows from the syntax regression.

- Scheduler run `r13-0dfebb001-r2` produced valid candidate and reference build receipts. Its comparison stopped after reference listing: the cgroup writer appended command status to the resource snapshot, which the unchanged parser rejected. No scheduler pair ran. Repair `d45ef379d` separates those outputs and passes real-writer success and failure controls. A subsequent environment review independently reproduced missing mold and linked-worktree Git access. Read-only mounts, a fixed local hostname and disabled networking passed all five affected selectors on both arms. The reviewed environment repair is integrated at `f68b1f312`; clean executing tool `.work/wt/perf-scheduler-env-r13` is frozen at `b53ef3d0fa0c7af8439bb92a1e3bcd972366b253`. The r3 launch stopped before Docker because its PATH omitted `perf-gate-ready`; its failed receipt remains unchanged and supplies no acceptance credit. The corrected launcher resolves its external commands before execution. Reuse the genuine r2 builds with fresh comparison output `.work/consolidation/native-scheduler-comparison/r13-0dfebb001-r2-attempt-cgroup-r4/receipt.json` under R13. Failed attempts remain unchanged. Three scheduler pairs and actual private/resource close are still required. Compiler qualification remains R13; final baseline validation still uses tool `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`.

- Final-consumer review found that the scheduler reader expected all three binaries in the reference build receipt, although the reference producer rebuilds and receipts only `xsht`. Repair `641a77c02` matches that existing producer contract. Full binary comparison and shared-child integrity checks remain mandatory. Eleven focused tests pass, including coordinated receipt tampering and child-binary mutations on both arms. Final baseline validation now uses clean tool `.work/wt/perf-scheduler-reader-r13` at `549466bd8c99f051c797fc85078da0b3bb6d3490`. This supersedes the previous consumer tool selection, without changing R13 or rerunning scheduler measurements. Parallel Laputa acceptance owns `.work/wt/laputa-world-acceptance`, based on clean sibling source `bbe05e466e95f7ba8b26b17dea0bfcd5365510e9`. Its expensive work remains outside exclusive G1 timing windows.

- Scheduler R4 completed all three pairs, but pair validation rejected the results. Each reference suite recorded 13,944 passes, one failure and 213 skips. Candidate suites recorded 23, 19 and 24 failures. Every suite failed the same `uu-ls` fixture when scratch creation reached read-only `/work/.work/tmp`. Candidate logs also retain output mismatches, descriptor/network failures and timeouts. All readiness and cgroup checks passed, inputs stayed unchanged and cleanup resolved. These runs supply diagnostics, not scheduler acceptance. Repair work owns `.work/wt/perf-scheduler-diagnosis-r13`; preserve R4 unchanged and keep final private/resource close blocked. Laputa acceptance now owns the heavy build and VM window. Its source preparation found and reproduced a separate snapshot bug: ordinary Git staging omitted an ignored but tracked test fixture. Force-adding the trusted archive preserves the source tree and passes the focused regression. Full world and guest acceptance remain pending.

- Focused scheduler-failure repairs are integrated locally. `a758be15e` isolates the TTY test's stale-descriptor assertions in a child, preventing legal descriptor reuse in another test from turning a second close into a foreign close. `2b6fcfc66` moves the `ls` socket fixture into writable temporary storage and preserves its short relative address. `2515907c8` replaces three tail fixtures' startup sleeps with output and diagnostic barriers, and starts tee's byte deadline after application readiness. Controlled old fixtures fail and repaired fixtures pass. The parent replay passed seven exact selectors on the integrated source using unchanged R13 products; source and product qualification remain distinct. Long scratch-path and withheld-output controls preserve the socket limit and original tee deadline. One mistyped short-read filter selected no test; its corrected invocation passed. UDP passes alone and with its 22-test file, while bounded system-report groups also pass. Their full-corpus failures remain unexplained. Preserve R13 and failed R4; no new freeze, scheduler acceptance or private/resource close follows from these focused results.

- Owner accelerated parallel I1/M1 launch on 2026-10-18 after the documentation refresh. Compiler integration owns `.work/wt/perf-inference-integration`; corpus work owns `.work/wt/perf-inference-corpus`; measurement work owns `.work/wt/perf-measurement-launch`. Independent compiler review and G1 failure diagnosis run alongside the existing isolated Laputa lane. The Deno diagnostic retries only its failed node under the same 2-CPU, 8-GiB, offline limits. This subset supplies no full-world acceptance. Timing remains exclusive and no synthetic close report supplies measurement credit.

- Inference compiler candidate `4233e34ee` was reviewed and merged locally at `ed7077a9c`. Its focused boundary passed 126 root controls, 14 tooling controls and 226 native tests with zero failures or skips; one unchanged autofix test was excluded. Both prior audit defects and a ground `with` binding producer-proof defect have red/green controls. Generated SPEC rendering, snippet diagnostics and all three tour-project tests passed after integration. Formal inventory maintenance and the combined regression boundary remain required.
- The R13 performance cohort completed all 1,530 timed samples and 30 parity checks with both readiness checks passing and cleanup resolved. These are provisional pre-inference observations, not an accepted baseline or speedup claim. Candidate corpus checking reproduces the frozen 1,040-removal result; removing all additional 730 checker-restored rows fails. Four practical pure-return removals pass paired behavior and signature checks. All 202 effect clauses remain retained.
- The isolated Deno node built successfully offline under the 12-GiB exception. Its prerequisite artifacts and the full-world plan are retained. Full-world completion, rooted functionality, unchanged rebuild reuse and actual Asterinas acceptance remain required; the deployment lane resumes that full remainder.

- Combined source `7851fd22e` passed singleton bootstrap, then failed canonical root integration: 221 passes and four failures before the full native stage. The failures are map/record per-read allocation budgets, signal-hook registration snapshot semantics and pure-parameter grammar correspondence. Source stayed clean and cleanup resolved. Preserve that failed receipt under `.work/wt/perf-i1-combined-freeze`; focused repairs use `.work/wt/perf-i1-combined-fixes`, with the original freeze and prior qualified products unchanged. No canonical, G1 or I1 combined acceptance follows from this attempt.

- Laputa's isolated world build, store verification, rooted functionality and unchanged rebuild had passed before an accidental `make verify` deleted its derived outputs. Its driver implicitly called `make clean`. Ordinary Laputa and persistent input caches were untouched. Fix `294e848` removes that implicit clean and is integrated into the acceptance checkout. A real-driver regression first reproduced deletion, then passed with exact sentinel bytes preserved and no Docker cleanup. All 18 seed tests passed, followed by the exact regression on the integrated checkout. Recovery uses the supported package publisher and remote importer for matching cached artifacts. Current world, root, reuse and Asterinas acceptance remain pending.

- The measurement selection now appends six demanding JSON and directory cases at package ranges 16,384, 32,768 and 65,536. The original fifteen cases and their provisional observations remain unchanged. Three focused generator, selection and measurement-failure controls pass on the integrated source. The new cases have no timing or acceptance credit yet.

- Combined source `63130a3d5` passed bootstrap, 228 root integration tests and the previously failing allocation, hook and grammar controls. Canonical execution then stopped in the debug library: 1,040 passed, one failed and three were ignored. The documentation scanner mistook the member expression `source.observation.state` for a diagnostic code. The corpus consistency check passed. A focused scanner regression reproduces the false positive; the token-boundary fix passes all six diagnostic checks while retaining actual unknown-code controls. Preserve the failed receipt in `.work/wt/perf-i1-combined-freeze-r2`. Native and supplemental stages did not run.

- Combined source `257c2d52a` passed the formerly failing documentation check and all 1,041 debug-library tests, with three existing ignores. Its tooling target then failed the contextual nominal-import test. The exact selector reproduced immediately: local slot facts used the resolved error-family owner while emitted annotations retained the import alias. The producer fix applies the existing scope-aware canonicalization to three local annotation paths. Four accepted declaration cases, two unchanged const rejections, all three contextual-root selectors and two verifier-authority controls pass with matching rebuilt products. Preserve the failed r3 receipt; native and supplemental stages remain unrun.

- Laputa acceptance now contains the reviewed lifecycle and XSHI-proof port at `636113a`. The exact lifecycle and runner tests pass with the current R13 seed; the guest proof has only passed typechecking. Deno recovery verified all eight closure artifacts. The subsequent full-world build stopped on remote metapackage import: publication correctly omits its archive, but planning substitutes a legacy archive URL. The container was removed and partial artifacts remain reusable. The isolated fix owns `.work/wt/laputa-remote-meta-import`; no full-world, rooted-profile or guest acceptance follows from this failed stage.

- H3 maintenance passed on the unchanged Lean 4.35 formal project: Lake build, axiom audit and self-test, guarded fresh paranoid check, sandbox self-tests, inventory check and all 51 inventory tests. The proof inventory still has 72 planned implementation obligations, no frozen Rust source identity and no conformance cases. These checks do not establish Rust compiler verification.

- Combined source `6957e88f4` passed 228 root integration tests, 1,042 debug-library tests with three existing ignores, and all 122 XSHT integration tests with five existing exclusions. Soundness then failed its unchanged minimum probe-count guard: five scalar receiver families were omitted after their registry types became explicit. The adapter repair exposed a separate checker panic on an unknown entry-parameter type. Reviewed repairs `03569f728` and `e91c777f8` restore scalar probe coverage and retain ordinary diagnostics for invalid entry signatures. The nine-source, three-mode diagnostic matrix and all 14 unchanged soundness tests pass; both fixes are integrated locally. Preserve the failed r4 receipt; native and supplemental stages did not run. Commit `4105a3823` moves soundness ahead of the debug and XSHT corpus targets without changing commands or exclusions. The ordering regression failed before the change and both focused workflow tests pass afterward.

- Laputa fix `5b016b43` is reviewed and integrated into the isolated acceptance checkout. Remote retrieval now distinguishes archives from metadata-only packages; existing index and plan wire versions remain unchanged. Import validates metadata before creating the canonical metapackage marker. All 229 package-manager tests pass with unchanged seed products. The run also corrected an incomplete response mock and a scratch login shell that discarded the configured runner path. Full-world recovery, rooted functionality, rebuild reuse and guest acceptance remain pending.

- Laputa acceptance is clean at `17211df05ee51a8e9698d5179b4c044f2908ea3b`. Firecracker installation checks the pinned binary hash before executing it. Rejected-hash and accepted-installation host controls pass after a reproduced premature-execution failure. Offline full-world reuse verifies all 85 artifacts without starting a package build. The profile composes eight runtime roots, and XSH executes inside that root. Validation checks 1,482 files, 58 ELF files and 20 dynamic ELF files with zero loader failures. Actual KVM smoke passes with `ASTERINAS_PVH_BOOT_OK` on the pinned Asterinas source. Kernel SHA-256 is `3f316100ad13c8bf399748ae362d0dc7b96c302ff07c292ae4b3f7e41073ec89`. The owned build adapter disables Kache Rust path normalization because component registration depends on `file!()` paths. Its initial normalized build panicked before guest initialization. A subsequent partial bundle cleanup exposed a stale OSDK manifest, which was removed before successful packaging. Owned cleanup resolves and seed products remain unchanged. Full guest XSH acceptance remains pending. These results use the R13 seed, not the combined inference candidate.

- Combined source `c24ce1759` passed bootstrap, 228 root integration tests, all 14 soundness tests, 1,043 debug-library tests and all 122 XSHT integration tests. Existing ignores and exclusions remained unchanged. Full native execution finished with 14,017 passes, 32 failures and 150 skips. Preserve its failed receipt in `.work/wt/perf-i1-combined-freeze-r5`. Reviewed repairs restore root signature isolation, driver temporary metadata, optional producer derivation and default inference from solved returns. Fixture repairs preserve unique wire ordinals, update the 153-tag inventory, relocate support programs into existing fixture data and accept generalized required pure parameters. Candidate `621513b7a` builds. Its 38 focused invocations have 35 passes and three residual IR failures, covering guard record sources and ProcessError metadata. All 24 btop source tests pass. Repair work continues in `.work/wt/perf-control-flow-r5`. Both new scalar mutation controls and eleven existing corruption controls pass. The optional-producer mutation fixture still stops at the guard failure before reaching its checks. No combined acceptance follows from these partial results.
- Candidate `9c20697b7` passes 39 of 40 focused native invocations and all 18 selected verifier controls. Previously failing R5 selectors now pass. The added compound guard regression exposes a prior record payload decoded under the current call's owner. Reviewed fix `83b88901c` uses the source step's decoder, retains strict ownership checks and adds a minimal record-to-generic-call regression. Its pinned library typecheck passes; rebuilt behavior checks and full acceptance remain pending. The unexecuted R6 checkout is `.work/wt/perf-i1-combined-freeze-r6` on `campaign/performance-i1-freeze-r6`. Laputa's offline guest build publishes bundle `1b8af701198629cabf74c3c9b48c1e225ff5a501131d51d9dae0fb61118ad7c3` with resolved cleanup and unchanged qualified seed. Full guest XSH execution remains pending and still uses the R13 seed.


- Combined R6 source `0f2cbe61b` passes singleton bootstrap but fails the root
  integration boundary: 226 pass and two byte-entry argument-domain failures.
  Cleanup resolves. Later canonical stages did not run. Preserve the failed R6
  receipt and checkout. Repair `f7978b07f` selects the entry domain from checked
  parameters independently of pending return inference. Its strengthened checker
  regression fails before the fix. All three selected checker controls and all
  four byte-entry process tests pass afterward. Full combined qualification
  remains required on a fresh freeze.
- The R13-seed Asterinas guest boots through XSH initialization and starts the
  full suite. Execution stops at 159 passes, one failure and four skips. The
  failing AWK timestamp case requires `systime() > 1000000000`. The harness
  reports a stdout mismatch without the observed timestamp. RTC device failures
  and the pinned kernel's epoch fallback motivate a focused clock diagnostic.
  They do not yet prove the observed timestamp. Cleanup resolves and the
  qualified seed remains unchanged. No full guest acceptance is claimed.

## For the owner afterwards

- Read the concise accepted baseline page and deployment reproduction commands.
- Review source-level behavior/API changes, inference classes and retained sites, individual workload results, and the artifact experiment decision.
- Review the exact finite/formal distinction for the selected transformation.
- Inspect local XSH and Laputa integration branches. Merge only reviewed accepted commits under the launch grant. Nothing is pushed by this campaign.
