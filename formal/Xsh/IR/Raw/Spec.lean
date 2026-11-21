import Xsh.IR.Raw.Document
import Xsh.IR.Raw.Shape

/-!
Structural well-formedness of an `xsh-raw-ir-v1` document, stated from the raw-IR rules
without the checker's jobs, scopes, segments or fuel: exact tags, payload grammars over
the word table, one-based identities and the `u32::MAX` sentinel, ranges, pools,
patterns and stages at descending indexes, driver programs, locations and the single
ownership of every claimed entry.
-/

namespace Xsh.IR.Raw.Spec

open Xsh.Foundation

/-- `u32::MAX`, which names no entry. -/
def absent : Nat := 4294967295

/-- The bit that marks a block owner word as a driver step. -/
def driverBit : Nat := 2147483648

/-- The capture word's top bit marks mutable storage; the other bits are the slot. -/
def mutableBit : Nat := 2147483648

/-- Column `column` of a row; a column past the row reads as zero. -/
def cell (row : Row) (column : Nat) : Nat := ((row[column]?).map (·.val)).getD 0

/-- A one-based identity into a pool of `count` entries. -/
def IdIn (count word : Nat) : Prop := 0 < word ∧ word ≤ count ∧ word ≠ absent

/-- The range of `count` entries from `start` lies inside a table of `total` entries. -/
def Within (start count total : Nat) : Prop := start + count ≤ total

/-- `words` are the `count` words of the word table from `start`, which lie inside it. -/
def Slice (doc : Document) (start count : Nat) (words : List Nat) : Prop :=
  Within start count doc.extra.length ∧ words = ((doc.extra.drop start).take count).map (·.val)

/-- The table entry a tag word names: the same ordinal and the same name. -/
def Names (table : List TagSchema) (tag : TagWord) (entry : TagSchema) : Prop :=
  entry ∈ table ∧ entry.ordinal = tag.2.val ∧ entry.name = tag.1

/-- The size of semantic pool `pool` as the document records it. -/
def poolSize (doc : Document) (pool : Nat) : Nat := ((doc.poolCounts[pool]?).map (·.val)).getD 0

/-- An executable owner of instructions and blocks. -/
inductive Owner where
  | function (index : Nat)
  | step (index : Nat)
  deriving DecidableEq

namespace Owner

/-- The owner word a block row records. -/
def word : Owner → Nat
  | .function index => index + 1
  | .step index => driverBit + index + 1

/-- The owner's first instruction. -/
def first (doc : Document) : Owner → Nat
  | .function index => ((doc.functionInstructionStarts[index]?).map (·.val)).getD 0
  | .step index => ((doc.driverSteps[index]?).map (cell · 6)).getD 0

/-- One past the owner's last instruction: the next function's start, then the first driver
step's start, then the next step's start, and finally the end of the instruction table. -/
def bound (doc : Document) : Owner → Nat
  | .function index =>
      match doc.functionInstructionStarts[index + 1]? with
      | some start => start.val
      | none => ((doc.driverSteps[0]?).map (cell · 6)).getD doc.tags.length
  | .step index => ((doc.driverSteps[index + 1]?).map (cell · 6)).getD doc.tags.length

/-- The number of local slots the owner provides. -/
def slots (doc : Document) : Owner → Nat
  | .function index => ((doc.functions[index]?).map (cell · 8)).getD 0
  | .step index => ((doc.driverSteps[index]?).map (cell · 7)).getD 0

/-- The owner's instructions, in table order. -/
def instructions (doc : Document) (owner : Owner) : List Nat :=
  (List.range (owner.bound doc - owner.first doc)).map (owner.first doc + ·)

end Owner

/-- Every function, then every driver step. -/
def owners (doc : Document) : List Owner :=
  (List.range doc.functions.length).map .function ++ (List.range doc.driverSteps.length).map .step

/-- What a payload may name: instructions of its owner before its user, the owner's slots, and
patterns, stages and values below the current ceilings. A driver step may import programs. -/
structure Frame where
  owner : Option Owner
  user : Option Nat
  slots : Nat
  patterns : Nat
  stages : Nat
  values : Nat
  importer : Option Nat

/-- Instruction `index` has a tag naming an entry of the given kind. -/
def KindIs (schema : Schema) (doc : Document) (index : Nat) (kind : RowKind) : Prop :=
  ∃ tag entry, doc.tags[index]? = some tag ∧ Names schema.instructions tag entry ∧ entry.kind = kind

/-- An operand names an instruction of the requested kind, owned by the payload's owner and
earlier than the instruction that uses it. -/
def Operand (schema : Schema) (doc : Document) (frame : Frame) (kind : RowKind) (word : Nat) : Prop :=
  (∃ owner, frame.owner = some owner ∧ owner.first doc ≤ word ∧ word < owner.bound doc) ∧
    (∀ user, frame.user = some user → word < user) ∧ KindIs schema doc word kind

/-- A `Use` step imports a program whose steps all precede it. -/
def Imports (doc : Document) (frame : Frame) (word : Nat) : Prop :=
  ∃ step, frame.importer = some step ∧ IdIn doc.driverPrograms.length word ∧
    ∃ row, doc.driverPrograms[word - 1]? = some row ∧ cell row 0 + cell row 1 ≤ step

/-- The meaning of one payload word of each role. -/
def WordRule (schema : Schema) (doc : Document) (frame : Frame) : Role → Nat → Prop
  | .raw, _ => True
  | .symbol, word => word < doc.symbolBase.val + doc.symbols.length
  | .slot, word => word < frame.slots
  | .stringId, word => IdIn doc.strings.length word
  | .bytesId, word => IdIn doc.bytes.length word
  | .location, word => IdIn doc.locations.length word
  | .typeId, word => IdIn (poolSize doc 0) word
  | .loweredType, word => word = absent ∨ IdIn (poolSize doc 0) word
  | .expr, word => Operand schema doc frame .expr word
  | .stmt, word => Operand schema doc frame .stmt word
  | .int, word => Operand schema doc frame .int word
  | .bool, word => Operand schema doc frame .bool word
  | .pattern, _ => False
  | .stage, _ => False
  | .value, word => word < frame.values
  | .function, word => IdIn doc.functions.length word
  | .regex, word => word < poolSize doc 3
  | .constant, word => word < poolSize doc 4
  | .wireEnum, word => word < poolSize doc 5
  | .schema, word => word < poolSize doc 6
  | .modulePlan, word => word < poolSize doc 7
  | .materialization, word => word < poolSize doc 8
  | .schemaExpectation, word => word ≠ absent ∧ word < poolSize doc 9
  | .runtimeOp, word => word < doc.runtimeOps.length
  | .assignOp, word => word < doc.assignOps.length
  | .binaryOp, word => word < doc.binaryOps.length
  | .runKind, word => word < doc.runKinds.length
  | .redirectionKind, word => word < doc.redirectionKinds.length
  | .bindingIdentity, word => word < doc.bindingIdentities.length
  | .childProgram, word => Imports doc frame word
  | .deferKind, word => word < schema.deferKinds
  | .endDistance, word => word ≠ 0
  | .textHole, word => word < schema.textHoleKinds

/-- The entries a payload word takes: operands take their instruction and imports their program. -/
def taken : Role → Nat → List Claim
  | .expr, word => [.instruction word]
  | .stmt, word => [.instruction word]
  | .int, word => [.instruction word]
  | .bool, word => [.instruction word]
  | .childProgram, word => [.program (word - 1)]
  | _, _ => []

/-- Block `word` (one-based) is a statement or list sequence as requested, unowned or owned by
the payload's owner, and `words` are its words. -/
def BlockOf (doc : Document) (frame : Frame) (word : Nat) (statements : Bool) (words : List Nat) : Prop :=
  ∃ row, 0 < word ∧ doc.blocks[word - 1]? = some row ∧ cell row 4 % 2 = (if statements then 1 else 0) ∧
    (cell row 3 = absent ∨ frame.owner.map Owner.word = some (cell row 3)) ∧ Slice doc (cell row 0) (cell row 1) words

/-- Row `index` of `table` has a payload range whose words are `words`. -/
def PayloadOf (doc : Document) (table : List Row) (index : Nat) (words : List Nat) : Prop :=
  ∃ row, table[index]? = some row ∧ Slice doc (cell row 0) (cell row 1) words

mutual
/-- `shape` reads a prefix of `words`, leaves `rest`, and takes `claims`. A choice reads its tag
and takes the first variant with that tag. A pattern or stage word enters that row's payload
with the ceiling lowered to its own index. -/
inductive Parses (schema : Schema) (doc : Document) : Frame → Shape → List Nat → List Nat → List Claim → Prop
  | word {frame role word rest} : role ≠ .pattern → role ≠ .stage → WordRule schema doc frame role word →
      Parses schema doc frame (.word role) (word :: rest) rest (taken role word)
  | pattern {frame word rest tag entry words claims} : word < frame.patterns → doc.patterns[word]? = some tag →
      Names schema.patterns tag entry → PayloadOf doc doc.patternData word words →
      ParsesAll schema doc { frame with patterns := word } entry.fields words [] claims →
      Parses schema doc frame (.word .pattern) (word :: rest) rest (.pattern word :: claims)
  | stage {frame word rest tag entry words claims} : word < frame.stages → doc.stages[word]? = some tag →
      Names schema.stages tag entry → PayloadOf doc doc.stageData word words →
      ParsesAll schema doc { frame with stages := word } entry.fields words [] claims →
      Parses schema doc frame (.word .stage) (word :: rest) rest (.stage word :: claims)
  | seq {frame items words rest claims} : ParsesAll schema doc frame items words rest claims →
      Parses schema doc frame (.seq items) words rest claims
  | choice {frame tags variants tag variant words rest claims} {position : Nat} : tags[position]? = some tag →
      (∀ earlier, earlier < position → tags[earlier]? ≠ some tag) →
      variants[position]? = some variant → Parses schema doc frame variant words rest claims →
      Parses schema doc frame (.choice tags variants) (tag :: words) rest claims
  | many {frame item count words rest claims} : ParsesTimes schema doc frame item count words rest claims →
      Parses schema doc frame (.many item) (count :: words) rest claims
  | block {frame statements item word rest inner claims} : BlockOf doc frame word statements inner →
      Parses schema doc frame item inner [] claims →
      Parses schema doc frame (.block statements item) (word :: rest) rest (.block (word - 1) :: claims)
  | recur {frame name shape words rest claims} : shape ∈ schema.recursive → shape.name = name →
      Parses schema doc frame (.choice shape.tags shape.variants) words rest claims →
      Parses schema doc frame (.recur name) words rest claims
  | text {frame word count holes rest claims} {kinds texts : List Nat} : BlockOf doc frame word false (count :: holes) →
      ParsesTimes schema doc frame (.word .pattern) count holes [] claims → kinds.length = count →
      (∀ kind ∈ kinds, kind < schema.textHoleKinds) → texts.length = count + 1 →
      (∀ text ∈ texts, IdIn doc.strings.length text) →
      Parses schema doc frame .textPattern (word :: (kinds ++ (texts ++ rest))) rest (.block (word - 1) :: claims)
/-- The shapes read one after another. -/
inductive ParsesAll (schema : Schema) (doc : Document) : Frame → List Shape → List Nat → List Nat → List Claim → Prop
  | nil {frame words} : ParsesAll schema doc frame [] words words []
  | cons {frame item items words middle rest first more} : Parses schema doc frame item words middle first →
      ParsesAll schema doc frame items middle rest more →
      ParsesAll schema doc frame (item :: items) words rest (first ++ more)
/-- The shape read `count` times. -/
inductive ParsesTimes (schema : Schema) (doc : Document) : Frame → Shape → Nat → List Nat → List Nat → List Claim → Prop
  | zero {frame item words} : ParsesTimes schema doc frame item 0 words words []
  | succ {frame item count words middle rest first more} : Parses schema doc frame item words middle first →
      ParsesTimes schema doc frame item count middle rest more →
      ParsesTimes schema doc frame item (count + 1) words rest (first ++ more)
end

/-- A UTF-8 continuation byte. -/
def Continues (byte : Nat) : Prop := 128 ≤ byte ∧ byte ≤ 191

/-- Well-formed UTF-8, one row of the Unicode well-formed byte sequence table per constructor. -/
inductive Utf8 : List Nat → Prop
  | nil : Utf8 []
  | ascii {first rest} : first < 128 → Utf8 rest → Utf8 (first :: rest)
  | two {first second rest} : 194 ≤ first → first ≤ 223 → Continues second → Utf8 rest →
      Utf8 (first :: second :: rest)
  | threeE0 {second third rest} : 160 ≤ second → second ≤ 191 → Continues third → Utf8 rest →
      Utf8 (224 :: second :: third :: rest)
  | three {first second third rest} : 225 ≤ first → first ≤ 239 → first ≠ 237 → Continues second →
      Continues third → Utf8 rest → Utf8 (first :: second :: third :: rest)
  | threeED {second third rest} : 128 ≤ second → second ≤ 159 → Continues third → Utf8 rest →
      Utf8 (237 :: second :: third :: rest)
  | fourF0 {second third fourth rest} : 144 ≤ second → second ≤ 191 → Continues third → Continues fourth →
      Utf8 rest → Utf8 (240 :: second :: third :: fourth :: rest)
  | four {first second third fourth rest} : 241 ≤ first → first ≤ 243 → Continues second → Continues third →
      Continues fourth → Utf8 rest → Utf8 (first :: second :: third :: fourth :: rest)
  | fourF4 {second third fourth rest} : 128 ≤ second → second ≤ 143 → Continues third → Continues fourth →
      Utf8 rest → Utf8 (244 :: second :: third :: fourth :: rest)

/-- Parallel columns have the same length, and there are ten semantic pool counts. -/
def Columns (doc : Document) : Prop :=
  doc.data.length = doc.tags.length ∧ doc.resourceTransfers.length = doc.tags.length ∧
    doc.checkedCallTargetRequired.length = doc.tags.length ∧ doc.patternData.length = doc.patterns.length ∧
    doc.stageData.length = doc.stages.length ∧ doc.valueData.length = doc.values.length ∧
    doc.driverSteps.length = doc.driverStepTags.length ∧
    doc.functionInstructionStarts.length = doc.functions.length ∧
    doc.functionMetadata.length = doc.functions.length ∧ doc.locationSources.length = doc.locations.length ∧
    doc.poolCounts.length = 10

/-- Every tag word of every table names an entry of its schema table. -/
def Tags (schema : Schema) (doc : Document) : Prop :=
  (∀ tag ∈ doc.tags, ∃ entry, Names schema.instructions tag entry) ∧
    (∀ tag ∈ doc.patterns, ∃ entry, Names schema.patterns tag entry) ∧
    (∀ tag ∈ doc.stages, ∃ entry, Names schema.stages tag entry) ∧
    (∀ tag ∈ doc.values, ∃ entry, Names schema.values tag entry) ∧
    (∀ tag ∈ doc.driverStepTags, ∃ entry, Names schema.drivers tag entry)

/-- Document-wide words: distinct symbols, the source, one-bit flags, resource transfers only on
transferring instructions, the driver root and location sources. -/
def Scalars (schema : Schema) (doc : Document) : Prop :=
  doc.symbols.Nodup ∧ doc.sourceId.val < doc.sourceCount.val ∧
    (∀ word ∈ doc.resourceTransfers, word.val ≤ 1) ∧ (∀ word ∈ doc.checkedCallTargetRequired, word.val ≤ 1) ∧
    (∀ (index : Nat) (flag : Word32) (tag : TagWord), doc.resourceTransfers[index]? = some flag →
      doc.tags[index]? = some tag → flag.val = 1 → tag.2.val ∈ schema.resourceTransfers) ∧
    (doc.driverRoot.val = absent ∨ IdIn doc.driverPrograms.length doc.driverRoot.val) ∧
    (∀ source ∈ doc.locationSources, source.val < doc.sourceCount.val)

/-- Owner ranges tile the instruction table from instruction zero. -/
def Layout (doc : Document) : Prop :=
  (owners doc = [] → doc.tags = []) ∧ (∀ owner, (owners doc).head? = some owner → owner.first doc = 0) ∧
    ∀ owner ∈ owners doc, owner.first doc ≤ owner.bound doc

/-- The range in columns `column` and `column + 1` lies inside a table of `total` entries. -/
def RangeIn (row : Row) (column total : Nat) : Prop := Within (cell row column) (cell row (column + 1)) total

/-- Every row of the range `[start, start + count)` of `table` exists and satisfies `rule`. -/
def EachRow (table : List Row) (start count : Nat) (rule : Row → Prop) : Prop :=
  ∀ offset, offset < count → ∃ row, table[start + offset]? = some row ∧ rule row

def BlockRule (doc : Document) (row : Row) : Prop :=
  RangeIn row 0 doc.extra.length ∧ cell row 2 = absent ∧ cell row 4 < 4 ∧
    cell row 5 = 0 ∧ cell row 6 = 0 ∧ cell row 7 = 0 ∧
    (cell row 3 = absent ∨ (cell row 3 < driverBit ∧ IdIn doc.functions.length (cell row 3)) ∨
      (driverBit ≤ cell row 3 ∧ IdIn doc.driverSteps.length (cell row 3 - driverBit)))

def FunctionRule (doc : Document) (row : Row) : Prop :=
  IdIn doc.strings.length (cell row 0) ∧
    ((cell row 1 = 0 ∧ IdIn (poolSize doc 1) (cell row 2)) ∨ (cell row 1 = 1 ∧ cell row 2 < poolSize doc 2)) ∧
    RangeIn row 3 doc.params.length ∧ RangeIn row 5 doc.captures.length ∧ RangeIn row 9 doc.slotTypeFacts.length ∧
    (cell row 11 = absent ∨ IdIn (poolSize doc 0) (cell row 11)) ∧
    0 < cell row 7 ∧ (doc.blocks[cell row 7 - 1]?).map (cell · 4) = some 3

def MetadataRule (schema : Schema) (doc : Document) (row : Row) : Prop :=
  (cell row 0 = absent ∨ IdIn doc.strings.length (cell row 0)) ∧ cell row 1 < 8 ∧ cell row 2 < schema.callableKinds ∧
    cell row 3 = 0 ∧ cell row 4 = 0

def ParamRule (doc : Document) (row : Row) : Prop :=
  IdIn doc.strings.length (cell row 0) ∧ cell row 1 < 8 ∧ cell row 2 = 0 ∧ cell row 3 = 0 ∧ cell row 4 = 0

def CaptureRule (doc : Document) (slots : Nat) (row : Row) : Prop :=
  IdIn doc.strings.length (cell row 0) ∧ cell row 2 % mutableBit < slots ∧ cell row 3 ≠ absent ∧
    cell row 3 < doc.bindingIdentities.length

def StepRule (doc : Document) (row : Row) : Prop :=
  RangeIn row 2 doc.driverSlots.length ∧ RangeIn row 4 doc.driverWriters.length ∧
    IdIn doc.locations.length (cell row 8) ∧ cell row 11 = 0 ∧ cell row 12 = 0 ∧ cell row 13 = 0

/-- Block, function, metadata, parameter, capture, driver step, program and region rows. -/
def Rows (schema : Schema) (doc : Document) : Prop :=
  (∀ row ∈ doc.blocks, BlockRule doc row) ∧ (∀ row ∈ doc.functions, FunctionRule doc row) ∧
    (∀ row ∈ doc.functionMetadata, MetadataRule schema doc row) ∧
    (∀ row ∈ doc.functions, EachRow doc.params (cell row 3) (cell row 4) (ParamRule doc) ∧
      EachRow doc.captures (cell row 5) (cell row 6) (CaptureRule doc (cell row 8))) ∧
    (∀ row ∈ doc.driverSteps, StepRule doc row) ∧
    (∀ row ∈ doc.driverPrograms, RangeIn row 0 doc.driverSteps.length ∧ RangeIn row 2 doc.driverRegions.length) ∧
    (∀ row ∈ doc.driverRegions, RangeIn row 0 doc.driverSteps.length ∧ RangeIn row 2 doc.driverSync.length)

/-- UTF-8 strings, byte ranges, and operator pools inside their closed domains. -/
def Pools (schema : Schema) (doc : Document) : Prop :=
  (∀ row ∈ doc.strings, RangeIn row 0 doc.stringBytes.length ∧
      Utf8 (((doc.stringBytes.drop (cell row 0)).take (cell row 1)).map (·.val))) ∧
    (∀ row ∈ doc.bytes, RangeIn row 0 doc.byteData.length) ∧
    (∀ word ∈ doc.assignOps, word.val < schema.assignOps) ∧ (∀ word ∈ doc.binaryOps, word.val < schema.binaryOps) ∧
    (∀ word ∈ doc.runKinds, word.val < schema.runKinds) ∧
    (∀ word ∈ doc.redirectionKinds, word.val < schema.redirectionKinds)

def instructionFrame (doc : Document) (owner : Owner) (index : Nat) : Frame :=
  { owner := some owner, user := some index, slots := owner.slots doc, patterns := doc.patterns.length,
    stages := doc.stages.length, values := doc.values.length, importer := none }

def stepFrame (doc : Document) (index : Nat) : Frame :=
  { owner := some (.step index), user := none, slots := (Owner.step index).slots doc,
    patterns := doc.patterns.length, stages := doc.stages.length, values := doc.values.length,
    importer := some index }

def bodyFrame (doc : Document) (index : Nat) : Frame :=
  { owner := some (.function index), user := none, slots := (Owner.function index).slots doc,
    patterns := doc.patterns.length, stages := doc.stages.length, values := doc.values.length, importer := none }

/-- A prepared value may name only earlier values and unowned blocks. -/
def valueFrame (index : Nat) : Frame :=
  { owner := none, user := none, slots := 0, patterns := 0, stages := 0, values := index, importer := none }

/-- The entries each instruction (as its owner reads it), driver step, function body and value takes. -/
structure Assignment where
  instruction : Owner → Nat → List Claim
  step : Nat → List Claim
  body : Nat → List Claim
  value : Nat → List Claim

/-- Each driver program takes the steps in its range. -/
def programSteps (doc : Document) : List Claim :=
  (List.range doc.driverPrograms.length).flatMap fun program =>
    match doc.driverPrograms[program]? with
    | some row => (List.range (cell row 1)).map fun offset => Claim.step (cell row 0 + offset)
    | none => []

/-- Every claim, owner by owner, then steps, bodies, values and program steps. -/
def Assignment.all (doc : Document) (assignment : Assignment) : List Claim :=
  (owners doc).flatMap (fun owner => (owner.instructions doc).flatMap (assignment.instruction owner)) ++
    (List.range doc.driverSteps.length).flatMap assignment.step ++
    (List.range doc.functions.length).flatMap assignment.body ++
    (List.range doc.values.length).flatMap assignment.value ++ programSteps doc

/-- Every payload reads exactly its words under its tag's shape: instructions in their owner's
frame, driver steps in their own, each function body as a statement block, and each value. -/
def Payloads (schema : Schema) (doc : Document) (assignment : Assignment) : Prop :=
  (∀ owner ∈ owners doc, ∀ index ∈ owner.instructions doc, ∃ tag entry words,
      doc.tags[index]? = some tag ∧ Names schema.instructions tag entry ∧ PayloadOf doc doc.data index words ∧
      ParsesAll schema doc (instructionFrame doc owner index) entry.fields words []
        (assignment.instruction owner index)) ∧
    (∀ step, step < doc.driverSteps.length → ∃ tag entry words,
      doc.driverStepTags[step]? = some tag ∧ Names schema.drivers tag entry ∧ PayloadOf doc doc.driverSteps step words ∧
      ParsesAll schema doc (stepFrame doc step) entry.fields words [] (assignment.step step)) ∧
    (∀ function, function < doc.functions.length → ∃ row, doc.functions[function]? = some row ∧
      Parses schema doc (bodyFrame doc function) (.block true (.many (.word .stmt))) [cell row 7] []
        (assignment.body function)) ∧
    (∀ value, value < doc.values.length → ∃ tag entry words,
      doc.values[value]? = some tag ∧ Names schema.values tag entry ∧ PayloadOf doc doc.valueData value words ∧
      ParsesAll schema doc (valueFrame value) entry.fields words [] (assignment.value value))

/-- No entry is taken twice. Every instruction, owned block, pattern, stage and driver step is
taken, and every driver program except the root is imported. -/
def Claimed (doc : Document) (claims : List Claim) : Prop :=
  claims.Nodup ∧ (∀ index, index < doc.tags.length → Claim.instruction index ∈ claims) ∧
    (∀ index row, doc.blocks[index]? = some row → cell row 3 ≠ absent → Claim.block index ∈ claims) ∧
    (∀ index, index < doc.patterns.length → Claim.pattern index ∈ claims) ∧
    (∀ index, index < doc.stages.length → Claim.stage index ∈ claims) ∧
    (∀ index, index < doc.driverPrograms.length → (Claim.program index ∈ claims ↔ index + 1 ≠ doc.driverRoot.val)) ∧
    (∀ index, index < doc.driverSteps.length → Claim.step index ∈ claims)

/-- Structural well-formedness of a raw-IR document against a schema. -/
structure WellFormed (schema : Schema) (doc : Document) : Prop where
  shaped : doc.WellShaped
  columns : Columns doc
  tags : Tags schema doc
  scalars : Scalars schema doc
  layout : Layout doc
  rows : Rows schema doc
  pools : Pools schema doc
  payloads : ∃ assignment, Payloads schema doc assignment ∧ Claimed doc (assignment.all doc)

end Xsh.IR.Raw.Spec
