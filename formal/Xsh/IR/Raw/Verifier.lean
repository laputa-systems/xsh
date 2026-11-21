import Xsh.IR.Raw.Fit
import Xsh.IR.Raw.Schema

namespace Xsh.IR.Raw

open Xsh.Foundation
open Xsh.Conformance.Transport

def rawSchema : Schema :=
  { instructions := instructionSchema, patterns := patternSchema, stages := stageSchema, values := valueSchema,
    drivers := driverSchema, recursive := recursiveShapes, assignOps := assignOpCount, binaryOps := binaryOpCount,
    runKinds := runKindCount, redirectionKinds := redirectionKindCount, deferKinds := deferKindCount,
    textHoleKinds := textHoleKindCount }

def lookupTag (table : List TagSchema) (tag : TagWord) : Option TagSchema :=
  match table.find? (fun entry => entry.ordinal == tag.2.val) with
  | some entry => if entry.name = tag.1 then some entry else none
  | none => none

def resolveTags (table : List TagSchema) (tags : List TagWord) : Option (List TagSchema) :=
  tags.mapM (lookupTag table)

def Env.resolve? (schema : Schema) (doc : Document) : Option Env := do
  let instructions ← resolveTags schema.instructions doc.tags
  let patterns ← resolveTags schema.patterns doc.patterns
  let stages ← resolveTags schema.stages doc.stages
  let values ← resolveTags schema.values doc.values
  let drivers ← resolveTags schema.drivers doc.driverStepTags
  pure { schema, doc, instructions, patterns, stages, values, drivers }

def ownerStarts (doc : Document) : List Nat :=
  doc.functionInstructionStarts.map (·.val) ++ doc.driverSteps.map (rowWord · 6)

def ownerEnds (doc : Document) : List Nat := (ownerStarts doc).tail ++ [doc.tags.length]

def ownerSlots (doc : Document) : List Nat :=
  doc.functions.map (rowWord · 8) ++ doc.driverSteps.map (rowWord · 7)

def ownerSegment (doc : Document) (owner : Nat) : Nat × Nat × Nat :=
  (((ownerStarts doc)[owner]?).getD 0, ((ownerEnds doc)[owner]?).getD 0, ((ownerSlots doc)[owner]?).getD 0)

def scopeFor (doc : Document) (owner : Nat) (user : Option Nat) : Scope :=
  let (start, stop, slots) := ownerSegment doc owner
  { segStart := start, segEnd := stop, user, slots, patternBound := doc.patterns.length,
    stageBound := doc.stages.length, valueBound := doc.values.length }

structure Job where
  scope : Scope
  fields : List Shape
  start : Nat
  length : Nat

def instructionJobs (env : Env) (owner : Nat) : Option (List Job) :=
  let (start, stop, _) := ownerSegment env.doc owner
  ((List.range (stop - start)).map (start + ·)).mapM fun row => do
    let tag ← env.instructions[row]?
    let (payload, length) ← payloadRange env.doc.data row env.doc.extra.length
    pure { scope := scopeFor env.doc owner (some row), fields := tag.fields, start := payload, length := length }

def driverJob (env : Env) (step : Nat) : Option Job := do
  let tag ← env.drivers[step]?
  let (payload, length) ← payloadRange env.doc.driverSteps step env.doc.extra.length
  pure { scope := scopeFor env.doc (env.doc.functions.length + step) none, fields := tag.fields, start := payload, length := length }

def bodyBlock (doc : Document) (function : Nat) : Nat := ((doc.functions[function]?).map (rowWord · 7)).getD 0

def bodyJob (env : Env) (function : Nat) : Option Job := do
  let (start, length) ← blockRange env.doc (bodyBlock env.doc function) true
  pure { scope := scopeFor env.doc function none, fields := [.many (.word .stmt)], start := start, length := length }

def valueScope (value : Nat) : Scope :=
  ⟨0, 0, none, 0, 0, 0, value⟩

def valueJob (env : Env) (value : Nat) : Option Job := do
  let tag ← env.values[value]?
  let (payload, length) ← payloadRange env.doc.valueData value env.doc.extra.length
  pure { scope := valueScope value, fields := tag.fields, start := payload, length := length }

def documentJobs (env : Env) : Option (List Job) := do
  let instructions ← (List.range (ownerStarts env.doc).length).mapM (instructionJobs env)
  let drivers ← (List.range env.doc.driverSteps.length).mapM (driverJob env)
  let bodies ← (List.range env.doc.functions.length).mapM (bodyJob env)
  let values ← (List.range env.doc.values.length).mapM (valueJob env)
  pure (instructions.flatten ++ drivers ++ bodies ++ values)

def budget (doc : Document) : Nat := (doc.patterns.length + doc.stages.length + 1) * (doc.extra.length + 1)

def JobFits (env : Env) (job : Job) (claims : List Claim) : Prop :=
  EntryFits (EntryAt env (budget env.doc + 1)) job.scope job.fields job.start job.length claims

def runJob (env : Env) (job : Job) : Option (List Claim) :=
  entryFit (fitFuel env (budget env.doc + 1)) job.scope job.fields job.start job.length

def bodyClaims (doc : Document) : List Claim := (List.range doc.functions.length).map fun function =>
  .block (bodyBlock doc function - 1)

def ClaimsWF (doc : Document) (claims : List Claim) : Prop :=
  claims.Nodup ∧ (∀ pattern, pattern < doc.patterns.length → Claim.pattern pattern ∈ claims) ∧
    (∀ stage, stage < doc.stages.length → Claim.stage stage ∈ claims)

def continuation (byte : Nat) : Bool := 128 ≤ byte && byte ≤ 191

def utf8Valid : List Nat → Bool
  | [] => true
  | byte :: rest =>
      if byte < 128 then utf8Valid rest
      else if 194 ≤ byte && byte ≤ 223 then
        match rest with
        | second :: rest => continuation second && utf8Valid rest
        | [] => false
      else if 224 ≤ byte && byte ≤ 239 then
        match rest with
        | second :: third :: rest =>
            (if byte = 224 then 160 ≤ second && second ≤ 191
              else if byte = 237 then 128 ≤ second && second ≤ 159 else continuation second) &&
              continuation third && utf8Valid rest
        | _ => false
      else if 240 ≤ byte && byte ≤ 244 then
        match rest with
        | second :: third :: fourth :: rest =>
            (if byte = 240 then 144 ≤ second && second ≤ 191
              else if byte = 244 then 128 ≤ second && second ≤ 143 else continuation second) &&
              continuation third && continuation fourth && utf8Valid rest
        | _ => false
      else false

def slice (bytes : List WireByte) (start length : Nat) : List Nat := ((bytes.drop start).take length).map (·.val)

def RangeWithin (row : Row) (column limit : Nat) : Prop := rowWord row column + rowWord row (column + 1) ≤ limit

def driverOwnerBit : Nat := 2147483648

def BlockRowWF (doc : Document) (row : Row) : Prop :=
  RangeWithin row 0 doc.extra.length ∧ rowWord row 2 = irNone ∧ rowWord row 4 < 4 ∧
    rowWord row 5 = 0 ∧ rowWord row 6 = 0 ∧ rowWord row 7 = 0 ∧
    (rowWord row 3 = irNone ∨ (rowWord row 3 < driverOwnerBit ∧ OneBased (rowWord row 3) doc.functions.length) ∨
      (driverOwnerBit ≤ rowWord row 3 ∧ OneBased (rowWord row 3 - driverOwnerBit) doc.driverSteps.length))

def FunctionRowWF (doc : Document) (row : Row) : Prop :=
  OneBased (rowWord row 0) doc.strings.length ∧
    ((rowWord row 1 = 0 ∧ OneBased (rowWord row 2) (poolCount doc 1)) ∨
      (rowWord row 1 = 1 ∧ rowWord row 2 < poolCount doc 2)) ∧
    RangeWithin row 3 doc.params.length ∧ RangeWithin row 5 doc.captures.length ∧
    RangeWithin row 9 doc.slotTypeFacts.length ∧
    (rowWord row 11 = irNone ∨ OneBased (rowWord row 11) (poolCount doc 0)) ∧
    1 ≤ rowWord row 7 ∧ ((doc.blocks[rowWord row 7 - 1]?).map (rowWord · 4)) = some 3

def DriverStepRowWF (doc : Document) (row : Row) : Prop :=
  RangeWithin row 2 doc.driverSlots.length ∧ RangeWithin row 4 doc.driverWriters.length ∧
    (rowWord row 8 = irNone ∨ OneBased (rowWord row 8) doc.locations.length) ∧
    rowWord row 11 = 0 ∧ rowWord row 12 = 0 ∧ rowWord row 13 = 0

def SegmentsWF (doc : Document) : Prop :=
  (ownerStarts doc = [] → doc.tags.length = 0) ∧ (ownerStarts doc ≠ [] → (ownerStarts doc).head? = some 0) ∧
    ∀ owner, owner < (ownerStarts doc).length → (ownerSegment doc owner).1 ≤ (ownerSegment doc owner).2.1

def LengthsWF (doc : Document) : Prop :=
  doc.data.length = doc.tags.length ∧ doc.resourceTransfers.length = doc.tags.length ∧
    doc.checkedCallTargetRequired.length = doc.tags.length ∧ doc.patternData.length = doc.patterns.length ∧
    doc.stageData.length = doc.stages.length ∧ doc.valueData.length = doc.values.length ∧
    doc.driverSteps.length = doc.driverStepTags.length ∧
    doc.functionInstructionStarts.length = doc.functions.length ∧
    doc.functionMetadata.length = doc.functions.length ∧ doc.locationSources.length = doc.locations.length ∧
    doc.poolCounts.length = 10

def ScalarsWF (doc : Document) : Prop :=
  (doc.symbols.map String.toList).Nodup ∧ doc.sourceId.val < doc.sourceCount.val ∧
    (∀ word ∈ doc.resourceTransfers, word.val ≤ 1) ∧ (∀ word ∈ doc.checkedCallTargetRequired, word.val ≤ 1) ∧
    (doc.driverRoot.val = irNone ∨ OneBased doc.driverRoot.val doc.driverPrograms.length) ∧
    (∀ source ∈ doc.locationSources, source.val < doc.sourceCount.val)

def RowsWF (doc : Document) : Prop :=
  (∀ row ∈ doc.blocks, BlockRowWF doc row) ∧ (∀ row ∈ doc.functions, FunctionRowWF doc row) ∧
    (∀ row ∈ doc.params, OneBased (rowWord row 0) doc.strings.length ∧ rowWord row 2 = 0 ∧ rowWord row 3 = 0 ∧
      rowWord row 4 = 0) ∧
    (∀ row ∈ doc.driverSteps, DriverStepRowWF doc row) ∧
    (∀ row ∈ doc.driverPrograms, RangeWithin row 0 doc.driverSteps.length ∧ RangeWithin row 2 doc.driverRegions.length) ∧
    (∀ row ∈ doc.driverRegions, RangeWithin row 0 doc.driverSteps.length ∧ RangeWithin row 2 doc.driverSync.length)

def PoolsWF (schema : Schema) (doc : Document) : Prop :=
  (∀ row ∈ doc.strings, RangeWithin row 0 doc.stringBytes.length ∧
      utf8Valid (slice doc.stringBytes (rowWord row 0) (rowWord row 1)) = true) ∧
    (∀ row ∈ doc.bytes, RangeWithin row 0 doc.byteData.length) ∧
    (∀ word ∈ doc.assignOps, word.val < schema.assignOps) ∧ (∀ word ∈ doc.binaryOps, word.val < schema.binaryOps) ∧
    (∀ word ∈ doc.runKinds, word.val < schema.runKinds) ∧
    (∀ word ∈ doc.redirectionKinds, word.val < schema.redirectionKinds)

def TablesWFOf (schema : Schema) (doc : Document) : Prop :=
  LengthsWF doc ∧ ScalarsWF doc ∧ SegmentsWF doc ∧ RowsWF doc ∧ PoolsWF schema doc

def TablesWF (env : Env) : Prop := TablesWFOf env.schema env.doc

def distinct {α : Type} [DecidableEq α] : List α → Bool
  | [] => true
  | item :: rest => !(rest.any fun other => decide (item = other)) && distinct rest

def rangeWithin (row : Row) (column limit : Nat) : Bool := decide (rowWord row column + rowWord row (column + 1) ≤ limit)

def blockRowOk (doc : Document) (row : Row) : Bool :=
  rangeWithin row 0 doc.extra.length && decide (rowWord row 2 = irNone) && decide (rowWord row 4 < 4) &&
    decide (rowWord row 5 = 0) && decide (rowWord row 6 = 0) && decide (rowWord row 7 = 0) &&
    (decide (rowWord row 3 = irNone) ||
      (decide (rowWord row 3 < driverOwnerBit) && oneBased (rowWord row 3) doc.functions.length) ||
      (decide (driverOwnerBit ≤ rowWord row 3) && oneBased (rowWord row 3 - driverOwnerBit) doc.driverSteps.length))

def functionRowOk (doc : Document) (row : Row) : Bool :=
  oneBased (rowWord row 0) doc.strings.length &&
    ((decide (rowWord row 1 = 0) && oneBased (rowWord row 2) (poolCount doc 1)) ||
      (decide (rowWord row 1 = 1) && decide (rowWord row 2 < poolCount doc 2))) &&
    rangeWithin row 3 doc.params.length && rangeWithin row 5 doc.captures.length &&
    rangeWithin row 9 doc.slotTypeFacts.length &&
    (decide (rowWord row 11 = irNone) || oneBased (rowWord row 11) (poolCount doc 0)) &&
    decide (1 ≤ rowWord row 7) && decide (((doc.blocks[rowWord row 7 - 1]?).map (rowWord · 4)) = some 3)

def driverStepRowOk (doc : Document) (row : Row) : Bool :=
  rangeWithin row 2 doc.driverSlots.length && rangeWithin row 4 doc.driverWriters.length &&
    (decide (rowWord row 8 = irNone) || oneBased (rowWord row 8) doc.locations.length) &&
    decide (rowWord row 11 = 0) && decide (rowWord row 12 = 0) && decide (rowWord row 13 = 0)

def segmentsOk (doc : Document) : Bool :=
  (match ownerStarts doc with
    | [] => decide (doc.tags.length = 0)
    | first :: _ => decide (first = 0)) &&
    (List.range (ownerStarts doc).length).all fun owner =>
      decide ((ownerSegment doc owner).1 ≤ (ownerSegment doc owner).2.1)

def lengthsOk (doc : Document) : Bool :=
  decide (doc.data.length = doc.tags.length) && decide (doc.resourceTransfers.length = doc.tags.length) &&
    decide (doc.checkedCallTargetRequired.length = doc.tags.length) &&
    decide (doc.patternData.length = doc.patterns.length) && decide (doc.stageData.length = doc.stages.length) &&
    decide (doc.valueData.length = doc.values.length) && decide (doc.driverSteps.length = doc.driverStepTags.length) &&
    decide (doc.functionInstructionStarts.length = doc.functions.length) &&
    decide (doc.functionMetadata.length = doc.functions.length) &&
    decide (doc.locationSources.length = doc.locations.length) && decide (doc.poolCounts.length = 10)

def scalarsOk (doc : Document) : Bool :=
  distinct (doc.symbols.map String.toList) && decide (doc.sourceId.val < doc.sourceCount.val) &&
    doc.resourceTransfers.all (fun word => decide (word.val ≤ 1)) &&
    doc.checkedCallTargetRequired.all (fun word => decide (word.val ≤ 1)) &&
    (decide (doc.driverRoot.val = irNone) || oneBased doc.driverRoot.val doc.driverPrograms.length) &&
    doc.locationSources.all (fun source => decide (source.val < doc.sourceCount.val))

def rowsOk (doc : Document) : Bool :=
  doc.blocks.all (blockRowOk doc) && doc.functions.all (functionRowOk doc) &&
    doc.params.all (fun row => oneBased (rowWord row 0) doc.strings.length && decide (rowWord row 2 = 0) &&
      decide (rowWord row 3 = 0) && decide (rowWord row 4 = 0)) &&
    doc.driverSteps.all (driverStepRowOk doc) &&
    doc.driverPrograms.all (fun row => rangeWithin row 0 doc.driverSteps.length && rangeWithin row 2 doc.driverRegions.length) &&
    doc.driverRegions.all (fun row => rangeWithin row 0 doc.driverSteps.length && rangeWithin row 2 doc.driverSync.length)

def poolsOk (schema : Schema) (doc : Document) : Bool :=
  doc.strings.all (fun row => rangeWithin row 0 doc.stringBytes.length &&
      utf8Valid (slice doc.stringBytes (rowWord row 0) (rowWord row 1))) &&
    doc.bytes.all (fun row => rangeWithin row 0 doc.byteData.length) &&
    doc.assignOps.all (fun word => decide (word.val < schema.assignOps)) &&
    doc.binaryOps.all (fun word => decide (word.val < schema.binaryOps)) &&
    doc.runKinds.all (fun word => decide (word.val < schema.runKinds)) &&
    doc.redirectionKinds.all (fun word => decide (word.val < schema.redirectionKinds))

def tablesOk (env : Env) : Bool :=
  lengthsOk env.doc && scalarsOk env.doc && segmentsOk env.doc && rowsOk env.doc && poolsOk env.schema env.doc

def shapedOk (doc : Document) : Bool :=
  doc.data.all (fun row => decide (row.length = 2)) &&
    doc.patternData.all (fun row => decide (row.length = 2)) &&
    doc.stageData.all (fun row => decide (row.length = 2)) &&
    doc.valueData.all (fun row => decide (row.length = 2)) &&
    doc.blocks.all (fun row => decide (row.length = 8)) &&
    doc.driverSteps.all (fun row => decide (row.length = 14)) &&
    doc.driverSlots.all (fun row => decide (row.length = 8)) &&
    doc.driverWriters.all (fun row => decide (row.length = 2)) &&
    doc.driverSync.all (fun row => decide (row.length = 6)) &&
    doc.driverRegions.all (fun row => decide (row.length = 5)) &&
    doc.driverPrograms.all (fun row => decide (row.length = 6)) &&
    doc.bindingIdentities.all (fun row => decide (row.length = 7)) &&
    doc.functions.all (fun row => decide (row.length = 12)) &&
    doc.functionMetadata.all (fun row => decide (row.length = 5)) &&
    doc.params.all (fun row => decide (row.length = 5)) &&
    doc.paramCold.all (fun row => decide (row.length = 3)) &&
    doc.captures.all (fun row => decide (row.length = 6)) &&
    doc.validations.all (fun row => decide (row.length = 2)) &&
    doc.strings.all (fun row => decide (row.length = 2)) &&
    doc.bytes.all (fun row => decide (row.length = 2)) &&
    doc.locations.all (fun row => decide (row.length = 2))

def claimsOk (doc : Document) (claims : List Claim) : Bool :=
  distinct claims && (List.range doc.patterns.length).all (fun pattern => claims.contains (.pattern pattern)) &&
    (List.range doc.stages.length).all (fun stage => claims.contains (.stage stage))

inductive AllFit (env : Env) : List Job → List (List Claim) → Prop
  | nil : AllFit env [] []
  | cons {job jobs claims rest} : JobFits env job claims → AllFit env jobs rest → AllFit env (job :: jobs) (claims :: rest)

def StructuralWF (schema : Schema) (doc : Document) : Prop :=
  doc.WellShaped ∧ ∃ env, Env.resolve? schema doc = some env ∧ TablesWF env ∧
    ∃ jobs, documentJobs env = some jobs ∧ ∃ claims, AllFit env jobs claims ∧
      ClaimsWF doc (claims.flatten ++ bodyClaims doc)

def verify (schema : Schema) (doc : Document) : Bool :=
  shapedOk doc &&
    match Env.resolve? schema doc with
    | none => false
    | some env =>
        tablesOk env &&
          match documentJobs env with
          | none => false
          | some jobs =>
              match jobs.mapM (runJob env) with
              | none => false
              | some claims => claimsOk doc (claims.flatten ++ bodyClaims doc)

def verifyCodes (codes : ExchangeCodes) : Bool :=
  match decodeDocumentCodes codes with
  | some doc => verify rawSchema doc
  | none => false

def AcceptedDocument (codes : ExchangeCodes) : Prop :=
  ∃ doc, decodeDocumentCodes codes = some doc ∧ StructuralWF rawSchema doc

end Xsh.IR.Raw
