# Joint annotation cohort

This cohort retains the roots, source pins, source graph and 9,194 AST sites in
[the cohort design](../designs/typing-cohort.md). `eligibility.tsv` fixes each
site's category before the first removal run. Site numbers index the retained
inventory's data rows from zero. No compiler result determines eligibility.

All private local and signature sites are eligible, including private schema
references and explicit `Any` locals. Exceptions retain entry argument decoding,
implicit `require()` validation targets, and explicit entry effect bounds.
Schema definitions and exported interfaces remain unchanged. The `schema`
category includes application options, domain models and wire definitions. It
does not claim that every definition is a wire format.

`cohort.py` reconstructs the joint candidate from exact pinned source archives.
It removes only AST inventory edit spans. Sites restored after a failed joint
check stay in their frozen eligible denominator. `--retain` accepts a TSV whose
first column identifies those restored sites.

The source pins are XSH `ddc17d3d53c819c343f9ecaa738ac67d59e9033a` and Laputa
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`. The checking compiler is the qualified
typing successor, not either program-source pin. Compiler or runtime inference,
caller-dependent training and algorithm changes are outside this migration.

## Reproduce the candidate

Run from the isolated corpus worktree. Preserve any earlier scratch attempt
before preparing fresh source views.

```sh
bash dev/consolidation/typing-corpus/prepare.sh
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/original" "$PWD/.work/tmp/corpus/check-original"
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/reduced-final" "$PWD/.work/tmp/corpus/check-joint"
```

Preparation adds the accepted `fff7fed23` flock helper and its one lifecycle
witness call to both copied test trees. This fixture correction is outside the
48 program sources and changes no assertion. The pinned program-source views
and annotation counts remain unchanged.

Run `run-witnesses.sh` only in the assigned native-suite slot. It fails immediately
if the shared native lock is busy. Each invocation creates a fresh attempt
folder. Each exact selector must actually select and pass one test with no
failures or skips. Container cleanup and source and product checks run on failure
and interruption too.

The twelve XSH witnesses assert byte and character counts, fixed count order,
translation, deletion, squeezing, parser errors, file and stdin operand order,
Unicode delimiters, date precision, offline structured replay and redaction,
malformed replay rejection, gate selection, target propagation and failed
container cleanup. The Laputa witness asserts stable build-plan JSON, round-trip
identity, format rejection and digest rejection. Those assertions provide the
observable comparison for both views. Harness durations are not compared.

## Read the result

The current joint checker candidate removes 348 of 693 eligible locals (50.22%)
and 529 of 2,190 eligible internal annotations (24.16%). It restores 345 eligible
locals and 1,661 eligible internal sites. The internal removals are 165 parameter
and 364 return annotations. All 202 private effect clauses are retained.
Neither campaign target is met. No restored site leaves the frozen denominator.

Restoration uses parameters first, then required return contexts and directly
implicated local declarations. Some failing declaration groups remain
conservative. The result does not prove that every restored annotation is
individually necessary or that 877 is the maximum possible removal count.
Individual effect-clause lint proofs do not establish joint equivalence through
calls to other declared clauses. No unmeasured joint effect change contributes
to the score.

Required Proc parameters and explicit non-Unit Proc returns retain current
language semantics. Ambiguous method receivers need a distinguishing annotation:
both Str and Bytes provide `byte_at(Int) -> Int?`. The return type cannot select
between them. A caller must not train that declaration. `slice` and `record`
witnesses pass when their existing return context resolves the receiver or field.
The `issue` witness rejects the generic body under the existing direct-composition
restriction. These are distinct from the `replace` and `split` witnesses, whose
known Str helper anchors the parameter but whose earlier method calls fail
verified-indexed lowering. The direct-anchor gaps are fixed by the qualified host-anchor repair. The separate
`indirect-anchor` witness has a genuinely ambiguous receiver: Str and Regex
`replace` both accept the positional Str and labeled `with: Str` arguments and
return Str. Constraining its local intermediate to Str does not determine the
input receiver. Its rejection is preserved; a caller must not select Str for it.

The [prior raw evidence](../evidence/typing-corpus-b235/) preserves the original
613-removal checkpoint. The [host-anchor continuation](../evidence/typing-corpus-host-anchor-b235/)
preserves the 615-removal candidate. The
[finite host-obligation continuation](../evidence/typing-corpus-host-obligations/)
records the current 877-removal candidate and its qualified product identities.
Existing generic and adversarial acceptance relies on the retained typing and
host-obligation qualification. This migration changes no compiler source or
embedded standard library. Native behavior qualification remains pending its
named slot.
