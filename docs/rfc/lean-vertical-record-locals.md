# Admit record locals into the certified Lean source/IR vertical

Status: approved by the owner on 2026-11-19 with every recommendation in
section 8; implementation follows section 7. No claim in `formal/coverage.json`
or `formal/README.md` changes until a slice lands its own evidence.

Decisions (section 8): the base machine gains the record forms of section 3.2;
every raw-IR document moves to one new projection scope; record labels cross as
canonical label-table positions; `IntFieldSlot` reads and record facts wait for
later slices; callers of record-local functions are the following widening;
the connected reader's label mismatch is XSH-116.

## Summary

The open item in `xsh-lean.md` (lines 40-43) names the next vertical widening
as an error producer or a record local, each needing a new row form. This
document designs the record local. It admits one immutable `let` of a record
literal with integer fields, read only by field reads in the function's
integer result:

```xsh
pure spans(value: Int) -> Int {
  let box = {low: value - 3, high: 60 / value, wide: value + 9223372036854775800}
  return box.high - box.low
}
```

The design rests on four decisions:

1. The record's meaning lives in the independent base machine
   (`Xsh.Semantics.BaseMachine`), which gains an integer-field record type
   tag, a record literal and a field read of a named local.
2. The raw-IR document gains a record row form: the `ExprRecord`, `ExprField`
   and `StmtLet`-of-a-record rows, a record-entries block, the store's string
   pool, and a canonical table of record labels.
3. Record labels cross the exchange as spellings, never as interner symbol
   words, because a `Name` word in `extra` is a process-interner ID.
4. Calls of a record-local function, field reads in integer-family positions
   and checked facts for records wait for later slices.

The widening closes none of the 77 universal obligations.

## 1. Admitted source shape

### The admitted production

A function is admitted with a record local when all of the following hold:

- It is `pure`, has exactly one `Int` parameter and declares `-> Int`.
- Its body is one line `let NAME = {LABEL: VALUE, ...}`, then one line
  `return RESULT`.
- The literal has at least one field. Each `LABEL` is a non-keyword word, and
  the labels are distinct.
- Each `VALUE` is an existing `SourceValue`: an atom, or two atoms under one
  arithmetic operator. Its atoms are integer literals or the parameter.
- `RESULT` is an operand or two operands under one arithmetic operator. An
  operand is an atom (literal or parameter) or a field read `NAME.LABEL` whose
  `LABEL` is one of the literal's labels.
- `NAME` differs from the parameter and appears nowhere except as the base of
  a field read.
- The function has no literal `let`, no computed `let`, no propagation and no
  assignment.

Within that shape the meaning matches the language contract exactly:

- Every entry evaluates once, in source order, and a failure stops
  construction (SPEC 6.4, `docs/SPEC.md:2491-2499`).
- An entry the result never reads still evaluates, so its failure ends the
  call. In the example, `spans(9)` fails with `integer-overflow` from `wide`.
- Records have value semantics (SPEC 4.5, `docs/SPEC.md:1074`). An immutable
  local is read, never copied or written, so a field read is the field's value
  from the one construction.
- Integer arithmetic keeps its specified failures, `division-by-zero` and
  `integer-overflow` (`formal/README.md`, "Checked Int arithmetic").

### What stays out, and why

Each exclusion is a domain restriction of the Lean grammar. None changes the
language contract.

| Excluded form | Reason |
|---|---|
| Record update `{...r, a.b: v}` | Rust lowers it to `ExprRecordUpdate` (`schema.rs:965-969`) with path lists, and the contract adds disjoint paths (`check.record-update-overlap`), one base evaluation and no partial publication (SPEC 6.4, `docs/SPEC.md:2529-2535`). That is a separate production with its own failure semantics. |
| Spreads `{...r, a: v}` | A spread operand's type is a lower bound, so the contract needs the `check.spread-hides-field` rule and later-replaces-earlier semantics (`docs/SPEC.md:2496-2508`). The bridge decoder rejects the `Spread` entry, tag 1 of `RecordEntryView` (`decode.rs:1167-1170`). |
| Nested records `{a: {b: 1}}`, `r.a.b` | The new type tag is deliberately non-recursive. A nested field needs a recursive record type, chained projection and a decision on how Rust lowers `r.a.b`. |
| Nominal schemas and constructors | Nominal identity belongs to `VAL-NOMINAL`. `formal/README.md` (Pilot 1A) records that Rust has no runtime representation of nominal record identity and relies on the checker. Constructors add defaults (SPEC 4.7). |
| Optional fields, `?.` | The base machine has no `null` type or Optional. |
| A record escaping into a call, a result or a comparison | The base machine's call binds integers only (`BaseFrame.callArgument` holds `List IntValue`, `BaseMachine.lean:261-262`). An escaping record also needs record-valued observations in the exchange (`ObservedOutcome`, `Artifact.lean:200-205`) and the width rule at call boundaries (SPEC 4.5). |
| String, Bool or Result fields | The tokenizer has no string literal (`Frontend.lean:47-81`). Kernel evaluation of strings is what pushes certificates toward the memory cap (`xsh-lean.md:48-51`). Integer fields alone exercise eager construction and projection. |
| A call of a record-local function | A callee form would have to bind the record inside the caller's call, which is the escaping-record machine change above. The Lean lowerer refuses the call because the function is not marked callable (`Lower.pureCall`, `Lowering.lean:319`). |
| A field read inside a computed `let` value or an assignment | In those positions Rust emits `IntFieldSlot` (`schema.rs:839-843`) only when checker facts mark the read (`int_record_slot_field_reads`, `lower.rs:9155-9161`). Reproducing that choice makes the Lean lowerer depend on a published type fact, which belongs with the facts slice (section 3.8). |
| Empty `{}`, punned `{a}`, quoted `{"a": 1}`, keyword labels `{if: 1}` | `{}` is a record or a map depending on context (`is_empty_record_in_map_context`, `lower.rs:7116`). Punning reads a binding through shorthand. Quoted labels need string tokens. The contract allows keyword labels (`docs/SPEC.md:172-175`), so excluding them only narrows the domain. |
| `var` records, annotated `let box: T = ...` | Mutation and declared storage types are separate productions (assignment paths, `StmtAssignPath`). |
| Other parameter counts, other statements in the same body | The record's slot is the exporter's choice. Slot 2 is observed only for one-parameter functions (`SourceFunction.scope?`, `Lowering.lean:36-58`). |

One limit of the evidence must be stated plainly. For a pure body with one
parameter and atom operands, no input makes two entries fail with different
failures, so the order of entry evaluation is not observable in this domain.
The machine fixes the order, but no observation tests it.

## 2. The new row forms

### Source admission

`SourceExpression` (`Frontend.lean:94-104`) gains one form, following the
computed-let precedent that keeps a body's bindings inside its result form:

```lean
inductive SourceOperand where
  | atom (value : SourceAtom)
  | field (record label : String)

inductive SourceRecordValue where
  | operand (value : SourceOperand)
  | binary (op : FragmentOp) (left right : SourceOperand)

-- new constructor of SourceExpression
  | recordLocal (target : String) (fields : List (String × SourceValue))
      (result : SourceRecordValue)
```

`SourceAtom` stays unchanged. Every existing proof that matches on atoms, in
`Facts.lean`, `FactsIRConsistency.lean`, `SourceAdmissionSound.lean` and
`LoweringSound.lean`, therefore keeps its cases. The record name is kept in
`SourceOperand.field` so that a read of another name, such as `other.low`, is
a checked refusal rather than an unparseable token.

The slot projection gains one entry form: the parameter at slot 0 and the
record local at slot 2, as `ScopeProjection.computed` places a computed let
(`SourceAdmissionSound.lean:934-937`).

### IR export

The raw-IR document (`decodeRawDocument`, `Artifact.lean:312-322`) gains these
rows and tables:

| Element | Wire form | Lean reading |
|---|---|---|
| `ExprRecord`, ordinal 44 | payload `[entries]`, one block ID (`schema.rs:962`) | only as a `StmtLet` value |
| record-entries block | block flags `0` (`BLOCK_LIST`, `full.rs:22406`), words `[count, (0, label, valueRow) × count]` (`impl_vec_codec!`, `full.rs:20962-20984`) | each entry tag is `0` (`Field`), `label` is a one-based index into `names`, and `valueRow` precedes the record row and reads as an integer expression |
| `ExprField`, ordinal 56 | payload `[base, label, span]` (`schema.rs:1002-1008`) | `base` is an `ExprParam` row, and `label` is a one-based index into `strings` |
| `StmtLet`, ordinal 115, over `ExprRecord` | payload `[slot, recordRow, 0]` (`schema.rs:1275-1279`) | the record local's binding, with annotation word `0` |
| `strings` | the store's string pool, exact spellings in pool order | `ExprField` labels resolve here, and spellings are distinct |
| `names` | canonical record labels, in first-occurrence row order | record-entry labels resolve here, and spellings are distinct |

The two name tables exist because the two label words have different owners:

- An `ExprField` label is `NameText`, encoded as a store string-pool ID
  (`FullCodec for NameText`, `full.rs:20779-20797`). The pool is deterministic
  for a program, so the document can carry it exactly and keep the store's
  word. `FullStore::string` reads it one-based (`full.rs:1016`).
- A record entry's label is `Name`, encoded as `self.symbol().raw()`
  (`FullCodec for Name`, `full.rs:20634-20649`). That word is an index into
  the process-global interner. Preloaded words are fixed by the build
  (`build.rs:40-74`, `symbol_is_preloaded`, `symbol.rs:690-692`). Dynamic words
  depend on interning order across all threads of the process, and IDs are
  reused after a session's last `SymbolOwner` drops (`symbol.rs:138-169`). The
  program image needs probe-and-fill restoration to reproduce them
  (`restore_symbols`, `program_image.rs:1049`).

A raw symbol word is therefore not an artifact identity. The exporter rewrites
each record-entry label word in its own copy of `extra` to the label's
one-based index in `names`. It never touches the program's store. That rewrite
is a renaming of an internal identity, which the prescribed observation rule
permits (`xsh-lean.md:122`). The Lean lowerer recomputes `names` from the
source in the same first-occurrence order, so the table is reproduced rather
than trusted. The Lean lowerer takes `strings` from the document, as it takes
span words (`spanWords`, `Lowering.lean:470-481`). Its order is a
`FullBuilder::intern_string` artifact (`full.rs:5811-5824`) unrelated to
meaning, and the lowerer emits each label's index by spelling. A forged pool
either misses the spelling, so lowering fails, or moves it, so the emitted word
differs from the document's word.

### Checked facts document

The facts slice (section 3.8) extends the facts document with a record fact
type, a record construction node, a field read node and a local row form. The
first slices do not need it.

### Relation to the existing row forms

The record row form follows two precedents:

- The propagation binding (`propagationOf?`, `Meaning.lean:282-296`), which
  reads a `StmtLet` over a specific value row and stages a separate machine
  run (`Propagation.run?`, `Meaning.lean:481-496`).
- The computed let, whose value rows read the parameter and literals.

It differs from both in one way: it is the vertical's first row form whose
payload names something other than a slot, row, block, operation or function.
That is why it brings name tables into the exchange.

### Why a certificate-only check is insufficient

A certificate-only widening would add one fixture and a check over its bytes
without extending the decoder, the meaning or the universal theorems. That
cannot work here, for four reasons:

1. **The bytes do not decode.** `FragmentTag` is closed (`Artifact.lean:27-45`)
   and has no `ExprRecord` or `ExprField`. `decodeRawDocument` rejects unknown
   tags and unknown fields by design.
2. **The meaning has nothing to denote.** `BaseType` has no record tag
   (`BaseMachine.lean:15-24`), `BaseExpr` has no record or projection
   (`BaseMachine.lean:91-102`), and `scalarTypedValue?` rejects
   `CompositeValue.record` (`BaseMachine.lean:63`). A check that interpreted
   the record rows itself would put the record's meaning inside the
   certificate, which is the smuggling the audit rejects (`xsh-lean.md:107`).
3. **Labels need a binding rule.** Meaning depends on label spellings. A table
   from words to spellings, with its distinctness and lookup rules, is part of
   the exchange contract. A single certificate cannot state it for other
   documents.
4. **The claims are the universal theorems.** The vertical's value is
   `lowering_means_the_source` (`LoweringSound.lean:2145`) and
   `admitted_lowering_means_source` (`SourceAdmissionSound.lean:1073`), which
   hold for every admitted tree. A certificate shows agreement for one byte
   string only.

## 3. Lean changes, layer by layer

### 3.1 Criteria for where the record's meaning lives

These criteria were fixed before comparing designs:

- **Independent meaning.** Record construction and projection must be steps of
  an independent machine, not definitions inside the vertical's readers. If
  both readers scalarize the record, a consistent misreading on both sides
  passes every theorem.
- **Shared invariant.** The plan requires one shared machine invariant and
  forbids unrelated toy evaluators (`xsh-lean.md:120`).
- **Contract fit.** Eager construction in source order, failure stops
  construction, projection yields the field.
- **Blast radius.** Proof churn in existing roots, and ownership of shared
  interfaces (G1 owns `formal/Xsh/{Syntax,Values,Semantics,Proofs}`).
- **Kernel cost.** Documents stay small enough for the 8 GiB address-space cap
  (`ADDRESS_SPACE_LIMIT`, `formal/scripts/check-fresh.py:22`).

Three whole designs were compared:

| Design | Independent meaning | Shared invariant | Blast radius | Verdict |
|---|---|---|---|---|
| A. Extend `BaseMachine` with an integer-field record tag, a record literal and a field read of a named local. The vertical stages the record run as it stages propagation | yes: construction and projection are machine steps | yes: one machine | moderate: new cases in the base machine's proofs (about 4,500 lines import it: `Proofs/BaseMachine.lean`, `Proofs/BaseInvariant.lean`, `Proofs/Base/*`) | **selected** |
| B. Scalarize in both readers: a record local denotes one computed binding per field, and `r.f` reads that binding | no: the record exists in neither machine, and the readers define projection | yes | small | rejected: fails the first criterion, and an IR reader that maps `high` to `low` consistently on both sides is caught only by finite observations |
| C. Move the vertical's meaning to the connected machine (`Xsh.Semantics.Connected`), which already has `Value.record` and field steps (`Connected/Machine.lean:323-376`) | yes | yes | large: rewrites `Meaning.lean`, `Reading.lean` and `LoweringSound.lean` (about 4,000 lines) against a relational machine with a different binder model | rejected for this widening: a migration decision for the owner, not a record design (open question 1) |

Design A also beats a variant that generalizes `BaseFrame.callArgument` to
typed values. That variant would make record-local functions callable, but
under the machine's substitution semantics (`callBody`,
`BaseMachine.lean:384-386`) projection would happen inside `substitute`, not
as a machine step. It also changes the type of `substitute`, which the
pure-integer evaluation lemmas use (`Meaning.lean:1055-1065`). It is the
natural next widening, once callers are wanted.

### 3.2 Base machine and value domain

The vertical's meaning is `Xsh.Semantics.BaseMachine`, not the connected
machine. Its value carrier, `CompositeValue`, already has
`record (fields : List (String × CompositeValue))` (`Values/Composite.lean:34`).
No new value constructor is needed. The machine lacks a type tag, expression
forms and a frame:

```lean
-- BaseType (BaseMachine.lean:15-24)
  | integerRecord (labels : List String)

-- hasBaseType (BaseMachine.lean:34-43)
  | .integerRecord labels, .record fields =>
      fields.map (·.1) = labels ∧ ∀ field ∈ fields, ∃ value, field.2 = .integer value

-- BaseExpr (BaseMachine.lean:91-102)
  | record (fields : List (String × BaseExpr))
  | fieldRead (record : Xsh.Syntax.RawName) (label : String)

-- BaseFrame (BaseMachine.lean:254-264)
  | recordField (done : List (String × IntValue)) (label : String)
      (rest : List (String × BaseExpr))

-- BaseFault (BaseMachine.lean:105-113)
  | expectedRecord
  | missingField (label : String)
```

The step rules are:

- `record []` returns the empty integer record.
- `record ((label, value) :: rest)` evaluates `value` under a `recordField`
  frame.
- A `recordField` frame takes an integer and either evaluates the next entry
  or returns the finished record. Any other value fails with
  `expectedInteger`.
- `fieldRead r label` resolves `r` through `lookupBaseValue`, then returns the
  field's integer. A value that is not an integer record fails with
  `expectedRecord`, and an absent label fails with `missingField`.

`fieldRead` takes a name rather than an expression, so it needs no frame.
`BaseFrame.consumes` and `produces` (`BaseMachine.lean:493-502`) stay
single-typed: a `recordField` frame consumes `integer` and produces the
`integerRecord` of its labels. `inferBaseType?` types `record` as the
`integerRecord` of its labels when every entry is an integer, and types
`fieldRead` from the store. `staticType` refuses both, because callee bodies
never touch records. The new faults are internal faults for ill-typed
configurations, and the extended progress theorem must show that a
well-formed configuration never reaches them.

The tag is an exact runtime shape: the labels of the value in order, every
field an integer. It is not a static record type and has no width rule. The
machine needs no width rule because no record is written into a declared
cell.

This design deliberately leaves two neighbouring value domains alone:

- **The connected machine.** Its record (`Value.record (List (NameId × Value))`,
  `Connected/Values.lean:36`) and its width-compatible `typeFits` are a
  separate domain. `formal/README.md` already records that its value and
  membership domains do not correspond to shared membership.
- **`Xsh.Core.Values` and `Core.Membership`.** Core's record
  (`Core/Values.lean:115`) is judged by `Membership.row`
  (`Core/Membership.lean:190-197`), which looks fields up by name and accepts
  extra fields. The new tag is strictly finer.

A future migration of the base machine to Core should prove that
`hasBaseType (.integerRecord labels) (.record fields)` implies Core row
membership of the embedded value for every row that names a subset of
`labels` at `Int`. This widening proves nothing about Core and makes no
`TYPE-MEMBER` claim. It uses `String` labels, as Core and `CompositeValue` do,
so that the later migration is a renaming.

### 3.3 Grammar and reading

All changes are in `Frontend.lean`:

- `Token` (`Frontend.lean:9-25`) gains `dot`, and `tokenize` maps code 46 to
  it. Text that contained `.` failed tokenization before and now fails
  parsing, so every existing refusal theorem keeps its result.
- `parseRecordLet` reads `let NAME = {` then `LABEL : VALUE` entries separated
  by `,`, then `}` and a line feed. `parseRecordOperand` reads
  `word NAME . word LABEL` or an atom.
- `parseFunctionBody` (`Frontend.lean:348-383`) accepts a record local only
  with no literal lets, no computed lets, no propagation, no assignment, one
  parameter and the `integer` shape. After `return`, a record-local body reads
  its result with `parseRecordValue` instead of `parseExpression`, so a call,
  conditional, comparison or constructor result is refused, as
  `buildComputedResult` refuses them for computed lets
  (`Frontend.lean:254-260`).
- `recordLocalAdmitted`, the analogue of `computedBindingsAdmitted`
  (`Frontend.lean:330-335`), requires distinct labels, at least one entry,
  entry values that read only the parameter and literals, a target that is
  neither the parameter nor a keyword, and result reads that name the target
  with a listed label. It also refuses a bare atom naming the target, such as
  `return box`.
- `SourceFunction.callableShape` and `SourceFunction.callable`
  (`Frontend.lean:420-424`, `Lowering.lean:126-127`) return false for a
  record-local function.

### 3.4 Meaning of a record-local program

All changes are in `Meaning.lean`:

```lean
structure RecordLocal where
  slot : Word32
  fields : List (String × BaseExpr)

-- FunctionProgram (Meaning.lean:351-356) gains
  record : Option RecordLocal
```

`FunctionProgram.outcome?` (`Meaning.lean:549-565`) gains a record stage, in
the style of `Propagation.run?`:

1. Substitute the argument literals for the parameter names in every entry
   value, as `applyLetBindings` substitutes (`Meaning.lean:141-146`). The
   result is a closed record literal.
2. Run `machineOf [] (.record closedEntries)`. An arithmetic fault ends the
   call with that failure. Any other non-value end is not an outcome.
3. Store the resulting typed record in the activation cell of the local's
   slot. This generalizes `machineOf`'s cell builder from `literalInt?`
   (`Meaning.lean:427-439`) to stored typed values.
4. Run the returned expression, with the parameters substituted, against that
   store. Each `fieldRead` is a machine step.

Composing the two runs is the vertical's definition, exactly as the
propagation stage composes its call run and its continuation. The machine has
no `let` form, so no theorem relates the composition to a single machine run.
`FunctionProgram.callee?` (`Meaning.lean:386-398`) returns `none` when
`record` is present.

### 3.5 IR reading

The IR reading changes in `Meaning.lean` and `Reading.lean`:

- `MeaningContext` (`Meaning.lean:156-160`) gains `strings` and `names`.
- `expression?` (`Meaning.lean:180-230`) gains `.exprField, [base, label, _]`.
  The base row must precede the field row and be `exprParam [slot]`, and the
  reading is `.fieldRead (NameId.mk slot) strings[label - 1]`.
- `recordLocalOf?`, beside `propagationOf?`, reads
  `StmtLet [slot, recordRow, 0]` over `ExprRecord [block]`. The block must be
  owned by the function and have flags `0`. Its entries must have tag `0`, a
  label index in range and a value row before the record row. The labels
  resolve through `names` and must be distinct.
- `bodyStep` (`Meaning.lean:299-324`) tries `recordLocalOf?` when
  `propagationOf?` fails. Its result tuple (`Meaning.lean:300-301`) gains the
  record local. A small structure would read better than a five-tuple.
- `rowWellFormed`, `blockReferences` and `validateArtifact`
  (`Meaning.lean:588-669`) cover the new tags and the entries block, and
  `validateArtifact` requires both tables to have distinct spellings.
- `Reading.lean` gains `reads_field` and a record-local body lemma, the
  analogues of `reads_param` and `propagation_of`.

### 3.6 Lowering and `LoweringSound`

`Lowering.lean` changes as follows:

- `LoweringState` (`Lowering.lean:192-199`) gains `strings`, taken from the
  document like `spans`, and `names`, appended in first-occurrence order.
  `toArtifact` and `lowersTo` compare both.
- `SourceFunction.scope?` and `SourceFunction.program?`
  (`Lowering.lean:36-58`, `162-181`) cover `recordLocal`: the parameter at
  slot 0, the record at slot 2, entries read in the parameter's scope, and
  field reads read as `.fieldRead (NameId.mk 2) label`.
- `Lower.recordLocal` emits each entry's value rows with `Lower.value`, the
  entries block with `Lower.pushBlock owner 0`, then `ExprRecord` and
  `StmtLet [2, recordRow, 0]`. `Lower.recordValue` emits `ExprParam [2]` and
  `ExprField [base, label, span]` for each field read, where `label` is the
  label's string-pool index, and `ExprBinary` over the operands.
- `spanWords` (`Lowering.lean:470-481`) adds `.exprField => word 2`.
- `Lower.functionTail` counts the local in `slotCount`
  (`Lowering.lean:365-366`) as one extra slot.

`LoweringSound.lean` adds `field_operand_lowers`, `record_entries_lower`,
`record_local_lowers`, `body_record_return` and `function_lowers_record`, the
last modelled on `function_lowers_computed` (`LoweringSound.lean:1738`). The
dispatcher `function_lowers` (`LoweringSound.lean:1967`) gains the case. The
`Aligned` invariant and `lower_all` (`LoweringSound.lean:2051`) need no new
idea, because a record-local function is not callable and its callee form is
`none` on both sides. `lowering_means_the_source` keeps its statement and
covers the new tree form.

### 3.7 Source admission soundness

`SourceAdmissionSound.lean` changes as follows:

- `Lexeme` and `tokenizer_syntax` (`SourceAdmissionSound.lean:623-671`) cover
  the dot.
- New derivations `RecordFieldsSyntax`, `RecordOperandSyntax` and
  `RecordValueSyntax`, a `BodySyntax.record` constructor and
  `ScopeProjection.record`, which maps the parameter to slot 0 and the target
  to slot 2.
- `check`, `check_sound` and `admitted_lowering_means_source`
  (`SourceAdmissionSound.lean:1028-1080`) keep their statements.
- New witnesses: `record_bytes_admitted`, and refusals for each excluded form
  in section 1.

These changes must land with the parser change, because
`parse_source_admitted` is proved about the whole parser.

### 3.8 Facts and `FactsIRConsistency` (CERT-FACTS)

CERT-FACTS admits only literal-let, call and conditional functions today:
`expressionFacts` returns `none` for computed lets, assignments, comparisons
and constructors (`Facts.lean:141-161`). Record-local facts need these
additions:

- `FactType` (`Facts.lean:6-9`) gains `integerRecord (labels : List String)`.
  The checker's `Type::Record` order is its own, so the validator compares
  labels as a set. The slice must confirm the order Rust publishes.
- `NodeFact` (`Facts.lean:20-26`) gains `record (labels : List String)`,
  emitted after the entry value nodes in source order, and
  `field (label : String) (result : FactType)`, emitted after the
  `read (.letBinding 0)` of the local.
- `FunctionFacts.localTypes` lists the record's type, and `functionFacts`
  admits the record-local form.
- The facts document (`FactsDocument.lean:44-71`) decodes the new kinds as
  closed objects. A record type is an object with a `fields` list of label and
  type pairs, because `FactType.ofName?` decodes only bare names.
- `FactsIRConsistency` gains `Rows.field` for the `ExprParam` and `ExprField`
  pair with its string-pool label, and `LocalRows.record` for `StmtLet`,
  `ExprRecord` and the entries block. `FunctionRows`
  (`FactsIRConsistency.lean:230-241`) then accepts a record local.
  `accepted_facts_match_ir` (`FactsIRConsistency.lean:574`) keeps its
  statement.

This slice is independent of computed-let facts, but it is the first to give
CERT-FACTS a record type. It is also the prerequisite for admitting
`IntFieldSlot` reads, whose emission depends on the published checker type of
the base.

## 4. Rust changes

Every change is in `#[cfg(test)]` bridge code (`full.rs:58421-58422`) and one
new fixture. No production lowering, schema or runtime path changes.

### Exporter and decoder (`src/runtime/eval/indexed/full/formal_bridge/arithmetic.rs`)

- `ProjectedTag` (`arithmetic.rs:8-47`) gains `ExprRecord` and `ExprField`,
  with payload widths 1 and 3 (`arithmetic.rs:90-101`). `ExprField` is an
  expression row. `ExprRecord` is valid only as a `StmtLet` value.
- `validate_arithmetic_document` accepts a `StmtLet` whose value is
  `ExprTry` or `ExprRecord` (`arithmetic.rs:440-447`). It gains
  `BlockRole::RecordEntries` (`arithmetic.rs:317-322`), which checks flags
  `0`, `entries.len() == 3 * count`, entry tag `0`, a label index in
  `1..=names.len()` and a value row that precedes the record row and is an
  expression. It requires every `ExprField` base to be `ExprParam`, every
  `ExprField` label to lie in `1..=strings.len()`, and both tables to have
  distinct spellings.
- `ArithmeticDocument` (`arithmetic.rs:287-295`) gains `strings: Vec<String>`
  and `names: Vec<String>`, encoded and decoded as closed fields. The scope
  string `ARITHMETIC_SCOPE` (`arithmetic.rs:5`) and Lean's `arithmeticScope`
  (`Artifact.lean:23`) move together (open question 2).
- `export_arithmetic_document` (`arithmetic.rs:859-955`) copies
  `store.strings` as spellings. It then walks `ExprRecord` blocks in row
  order, resolves each entry's symbol word to its spelling under the
  program's `symbol_owner()`, assigns first-occurrence indices, and rewrites
  those words in the exported copy of `extra` (today `store.extra.clone()`,
  `arithmetic.rs:947`). An entry whose symbol has no spelling fails the
  export.
- `check_artifact_against_program` keeps re-exporting and comparing, which now
  also covers the tables.

### Fixture and schedule

`tests/fixtures/runtime/formal-bridge-integer-record-local.xsh` holds the
`spans` function from the summary. Its schedule and expected outcomes are:

| Call | Entries `low`, `high`, `wide` | Outcome |
|---|---|---|
| `spans(5)` | 2, 12, 9223372036854775805 | value 10 |
| `spans(-4)` | -7, -15, 9223372036854775796 | value -8 |
| `spans(0)` | -3, then `60 / 0` fails | `division-by-zero` |
| `spans(9)` | 6, 6, then `9 + 9223372036854775800` fails | `integer-overflow`, although `wide` is never read |

Every entry reads the parameter, so the record cannot be folded to a prepared
constant (`lower_prepared_constant`, `lower.rs:8020`). An observed all-literal
`{low: 1, high: 2}` record in a local emitted `ExprRecord`; it did not fold
to a prepared constant in that probe.

### Observed rows

A static reading of the lowerer gives the shape below. The `let` lowers to
`BuildStmtRow::Let` because the binding is neither `Int` nor `Bool`
(`lower.rs:7129-7165`). The literal lowers to `BuildExprRow::Record` with
`LoweredRecordEntry::Field` entries (`lower/values.rs:451-478`). The field
reads lower to `BuildExprRow::Field` (`lower.rs:9154`). The observed export
confirms the row order and record slot below. The entries block is block 1 at
extra offset 21; the body is block 2. The string pool is
`[spans, value, args, high, low]`, and the canonical names table is
`[low, high, wide]`.
The Lean lowerer will follow these rows when it gains record support.

| Row | Tag | Payload |
|---|---|---|
| 0-2 | `ExprParam`, `ExprInt`, `ExprBinary` | `value - 3` |
| 3-5 | `ExprInt`, `ExprParam`, `ExprBinary` | `60 / value` |
| 6-8 | `ExprParam`, `ExprInt`, `ExprBinary` | `value + 9223372036854775800`, literal words `fffffff8`, `7fffffff` |
| 9 | `ExprRecord` | `[block]`, block words `[3, 0, 1, 2, 0, 2, 5, 0, 3, 8]` with `names = [low, high, wide]` |
| 10 | `StmtLet` | `[2, 9, 0]` |
| 11-12 | `ExprParam`, `ExprField` | `box.high` |
| 13-14 | `ExprParam`, `ExprField` | `box.low` |
| 15-16 | `ExprBinary`, `StmtReturn` | `high - low`, return |

### Rust tests

- Extend `exported_documents_are_the_lean_certified_bytes`
  (`arithmetic.rs:1142-1223`) with `recordLocalSourceDocumentCodes` and
  `recordLocalRawIrDocumentCodes`.
- `record_local_labels_are_canonical_across_interner_states`: export the
  fixture, intern a batch of unrelated dynamic names, build and export again,
  and require identical bytes. This test fails for an exporter that writes raw
  symbol words.
- `exported_record_local_rows_are_the_record_family`: the tags in order and
  the entries block shape.
- `record_local_runs_evaluate_every_entry`: `spans(9)` fails with
  `integer-overflow`.
- `mutated_record_local_programs_are_rejected_by_the_verifier_or_the_recorded_run`:
  swapped entry symbols and an `ExprField` pointing at the other label in the
  `FullProgram`.
- `structurally_malformed_record_local_rows_are_rejected`: a `Spread` entry, a
  count mismatch, label index 0 or past the table, an `ExprField` over a
  non-`ExprParam` base, an `ExprRecord` outside a `StmtLet`, and a
  repeated-spelling table.

### Memory

The independent kernels hold the whole export in memory under the 8 GiB
address-space limit (`formal/README.md`, "Build and check";
`check-fresh.py:22`). The fixture keeps one function, three entries and four
observations. The estimate is 17 rows and about 3.1 KB of raw-IR JSON. The
largest accepted document today is `propagationRawIrDocumentCodes` at
4,643 bytes, and `severalLetRawIrDocumentCodes` is 3,043 bytes. The
certificate follows `ComputedLetCertificate.lean:41-60`: it keeps the source
text as code points (`recordLocalTextCodes`) so the kernel never decodes the
string, and it decodes each document once. Labels stay `String`, like
`FunctionRow.name`. If the fresh check approaches the cap, switch the tables
to byte lists, as `Connected.readRawStringBytes` does
(`Connected/RawRows.lean:98-101`). Record the peak of each kernel when the
certificate lands.

## 5. Evidence plan

### Certificate

`recordLocal_artifact_certified` is
`certifyFromSource recordLocalSourceDocumentCodes recordLocalRawIrDocumentCodes = some recordLocalExpectedObservations`.
Its supporting theorems mirror `ComputedLetCertificate.lean`: decoding,
parsing, `lowersTo`, `validateArtifact`, the meaning equality from
`lowers_to_means_the_source`, and `namedResults`. The new `ARTIFACT_CERTIFIED`
record states the domain and boundary in the style of
`artifact.integer-several-let-calls`.

### Rejection theorems over the artifact

Each mutant goes through `certifiesExactly` (`Certificate.lean:80-81`):

1. Swapped `names` spellings (`low` and `high`). `lowersTo` fails because the
   lowerer recomputes the table. Read alone, the IR computes
   `spans(5) = 2 - 12 = -10`, so the recorded 10 is refused.
2. The `high` field read pointing at the `low` string. The mutant is well
   formed: it validates once its own outcomes are recorded, and it computes
   `spans(5) = 0`. This is the well-typed target that computes a different
   result (`xsh-lean.md:375-376`).
3. The unused `wide` entry dropped. The mutant is well formed and computes
   `spans(9) = 0` where `integer-overflow` is recorded. This rejects
   dead-field elimination.
4. A field read whose base is the parameter slot: `expectedRecord`, no
   outcome.
5. `StmtLet` binding slot 3 while the reads use slot 2: an unbound read, no
   outcome.
6. Entry tag 1 (`Spread`), a wrong count, label index 0, label index past the
   table, and a repeated spelling: each malformed and refused.
7. Wrong recorded outcomes, including the two failure kinds exchanged.
8. A substituted source (a label renamed, a constant changed) and wrong
   lowering trees (labels swapped in the tree, a field read changed), refused
   by `lowersTo` and `sourceAgrees`.

Reordering the entries, with their labels, is refused by `lowersTo` only,
because it is not the lowerer's output. Its meaning is the same, since entry
order is unobservable in this domain (section 1). The rejection theorem says
so rather than claiming a semantic detection.

### Source admission witnesses

- `record_bytes_admitted`: `check (witnessBytes ...) [] [tree] [[("value", 0), ("box", 2)]] = true`.
- Refusals, one per excluded form: update, spread, nested, `{}`, punned,
  quoted, keyword label, repeated label, unknown label, `return box`,
  `g(box)`, `value.low`, `box.low.x`, `var`, annotated, two parameters, a
  record local beside a computed let, and a caller of `spans`.

### Base machine witnesses

- Construction and projection runs.
- `missingField` and `expectedRecord` on ill-typed configurations, with
  progress showing well-formed configurations avoid them.
- A fault in a later entry after earlier entries succeed.

### What each piece of evidence is

| Evidence | Class | Subject |
|---|---|---|
| Extended `lowering_means_the_source`, `check_sound`, `admitted_lowering_means_source`, and the base machine fragment theorems | `SPEC_PROVED` fragment roots | the Lean lowerer, the Lean grammar and the base machine, nothing about Rust |
| `recordLocal_artifact_certified` and its rejection theorems | `ARTIFACT_CERTIFIED` | the two identified documents only |
| Byte equality of Rust's export with the Lean documents, Rust executions on four inputs, Rust mutation tests | finite, private bridge evidence | one fixture, not `RUST_CONFORMANCE` until run at a matching source freeze with the ledger's commands (`xsh-lean.md:136-157`) |
| The exporter's label renaming | Rust test only | `record_local_labels_are_canonical_across_interner_states` checks it, and nothing proves it |

Grammar refusals are domain refusals, not typing rejections. Rust's checker
rejects `return box` and unknown labels, and the Lean grammar refuses them, but
that pair of outcomes is not `TYPE-DIAG` agreement (`xsh-lean.md:477-478`).

### Checks that would prove the slice

Run from the repository root with the pinned toolchain (`formal/README.md`,
"Build and check"):

```sh
lake --dir="$PWD/.work" --file="$PWD/formal/lakefile.toml" build
(cd formal && /bin/bash scripts/audit-axioms.sh && /bin/bash scripts/audit-axioms.sh --self-test)
(cd formal && python3 scripts/check-fresh.py && python3 -B tests/test_check_fresh.py)
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
cargo dev gate src/runtime/eval/indexed/full/formal_bridge/arithmetic.rs tests/fixtures/runtime/formal-bridge-integer-record-local.xsh
```

Then run the named Rust tests with `--exact` in the pinned Linux container
(`docs/TESTING.md`).

## 6. Obligations

The widening closes none of the 77 universal obligations. Every row in
`formal/coverage.json` stays `planned`.

It advances these obligations as evidence:

- **BASE-VERTICAL**: the admitted shape gains record construction and
  projection.
- **SRC-ADMIT**: the witness checker over original bytes gains one production
  set.
- **BASE-STEP, BASE-PROGRESS, BASE-RUNNER**: the base machine's fragment
  theorems are extended over record forms.
- **XSH-CERTIFIED-ARTIFACT**: one more certificate.
- **CERT-FACTS**: only if the facts slice lands.

These obligations do not move:

- **TYPE-MEMBER**: no correspondence with `Core.Membership`.
- **VAL-NOMINAL**: nominal schemas are excluded.
- **CERT-SOUND**: no CERT-SOUND validator exists (`xsh-lean.md:39`).
  `lowersTo` reproduces and compares. It is not a translation validator for
  foreign IR.
- **LOWER-***, **MODE-EQUIV**, **IMPL-REFINE**: the Lean lowerer is not
  Rust's lowerer, and Rust execution is observed, not modelled.

## 7. Sequencing and size

Each slice lands on its own with its checks green. Sizes are rough added
lines.

| Slice | Content | Lands as | Size |
|---|---|---|---|
| 1. Exchange | Rust exporter and decoder rows, tables and canonical labels. Lean `FragmentTag`, `RawArtifact` and decoder. The record fixture is exported and recorded, and the rows of section 4 are confirmed. The new document decodes in Lean. `validateArtifact` refuses it, which is the honest intermediate. The hand-written artifacts in `Fixture.lean` gain the two tables, and the nine existing certificates still pass. | decode theorems, Rust byte-identity and canonical-label tests | Rust 350, Lean 150, about 300 lines of new document codes, and, if open question 2 goes as recommended, regenerated raw-IR codes for most of the 1,632 lines of `Documents.lean` |
| 2. Base machine records | Section 3.2 and its proofs in `Proofs/BaseMachine.lean`, `Proofs/BaseInvariant.lean` and `Proofs/Base/*`. Independent of the vertical. | fragment theorems and witnesses | Lean 500 to 900 |
| 3. IR meaning | Sections 3.4 and 3.5, then an IR-only certificate through `certify` (`Meaning.lean:686-691`) with mutants 1 to 7 in their IR-only form. | `ARTIFACT_CERTIFIED` for the raw-IR document | Lean 400 |
| 4. Source vertical | Sections 3.3, 3.6 and 3.7 together, then the source/IR pair certificate, the remaining mutants and the admission witnesses. | `ARTIFACT_CERTIFIED` for the pair, extended universal theorems | Lean 1,200 to 1,600 |
| 5. Facts (deferred) | Section 3.8, plus the Rust facts exporter (`facts.rs`) publishing the record binding type and field-read types. | extended `accepted_facts_match_ir` | Lean 500, Rust 300 |
| 6. Inventory | README paragraph, `coverage.json` roots and record, `check-inventory.py` run. Integrator-owned. | inventory checks | small |

Slice 4 is the largest because `function_lowers_record` must thread the
entries block through `StateLE`, as `function_lowers_computedLets` does
(`LoweringSound.lean:1868`, about 100 lines for the computed case alone).

## 8. Open questions for the owner

1. **May the base machine, a G1-owned shared interface, gain record forms?**
   The integrator owns shared semantic interfaces (`xsh-lean.md:92`), and
   `xsh-lean.md:52-55` lists the base machine's domain.
   Recommendation: yes, as section 3.2 specifies. It is the only design that
   keeps projection inside an independent machine without a migration. The
   owner would update the domain sentence in `xsh-lean.md`. Migrating the
   vertical to the connected machine remains a separate later decision.

2. **One raw-IR scope or two?** The tables can be added to every raw-IR
   document under a new projection scope, which regenerates the nine existing
   documents and re-checks their certificates. Alternatively, a separate
   record scope can carry them while the v3 documents stay byte-identical.
   Recommendation: one new scope for all documents, with `names` empty where
   no record occurs. A decoder that accepts two closed shapes is a
   compatibility path, which the non-goals rule out (`xsh-lean.md:596`). The
   cost is a rebuild of the existing certificates and edits to nine
   `boundary` strings that name the v3 scope.

3. **Canonical label indices or exact symbol words?** Recommendation:
   canonical indices that Lean recomputes. Exact words with a word-to-spelling
   table would make the recorded bytes depend on interner state. Reproducing
   them in a test needs the probe-and-fill restoration of
   `program_image.rs:1049`.

4. **Should integer-family field reads (`IntFieldSlot`) be in this widening?**
   Recommendation: no. Rust emits them only on checker type facts
   (`lower.rs:9155-9161`), so they belong after record facts (slice 5).

5. **Must record facts land before the widening counts as done?**
   Recommendation: no. Slices 1 to 4 are a complete source/IR widening. Record
   facts are the first CERT-FACTS record type and deserve their own review.

6. **Should callers of record-local functions be admitted next?**
   Recommendation: yes, as the following widening. Generalize
   `BaseFrame.callArgument` to typed values, and substitute a record value
   form rather than its fields, so that projection of a call-bound record
   stays a machine step. The alternative, projecting inside `substitute`, is
   smaller but moves projection out of the machine.

7. **Who fixes the connected reader's record labels?** Section 9 describes a
   label mismatch in the connected path. Recommendation: file it against the
   connected path's owner. Reuse this design's canonical label table when that
   path first carries an `ExprRecord` row. The vertical does not depend on it.

## 9. Observation outside this design

`Xsh.Conformance.Connected.readRawValueFields`
(`formal/Xsh/Conformance/Connected/RawExpression.lean:181-192`) reads an
`ExprRecord` entry's label word as a string-pool ID through
`readRawStringBytes`. Rust writes that word as an interner symbol
(`FullCodec for Name`, `full.rs:20634-20649`). The connected path's own
assignment-path decoder keeps field labels as raw symbols
(`decodeRawAssignSteps`, `Connected/RawRows.lean:133-137`), so the two
readers disagree with each other. No connected document carries an
`ExprRecord` row today. The raw instruction documents in
`RawLocalDocuments.lean` and `RawResultDocuments.lean` decode to tag sets
without it, so the reading is unexercised rather than demonstrated wrong. The
first record row exchanged through that reader would resolve labels through
the wrong table.
