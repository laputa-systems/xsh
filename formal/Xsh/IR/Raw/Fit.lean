import Xsh.IR.Raw.Document
import Xsh.IR.Raw.Shape

namespace Xsh.IR.Raw

open Xsh.Foundation

def irNone : Nat := 4294967295

structure Env where
  schema : Schema
  doc : Document
  instructions : List TagSchema
  patterns : List TagSchema
  stages : List TagSchema
  values : List TagSchema
  drivers : List TagSchema

structure Scope where
  segStart : Nat
  segEnd : Nat
  user : Option Nat
  slots : Nat
  patternBound : Nat
  stageBound : Nat
  valueBound : Nat
  owner : Nat
  importer : Option Nat

def wordAt (doc : Document) (position : Nat) : Option Nat := (doc.extra[position]?).map (·.val)

def rowWord (row : Row) (column : Nat) : Nat := ((row[column]?).map (·.val)).getD 0

def poolCount (doc : Document) (pool : Nat) : Nat := ((doc.poolCounts[pool]?).map (·.val)).getD 0

def OneBased (word count : Nat) : Prop := 1 ≤ word ∧ word ≤ count ∧ word ≠ irNone

def oneBased (word count : Nat) : Bool := decide (1 ≤ word) && decide (word ≤ count) && decide (word ≠ irNone)

def programEnd (doc : Document) (program : Nat) : Nat :=
  ((doc.driverPrograms[program]?).map fun row => rowWord row 0 + rowWord row 1).getD 0

def Imports (doc : Document) (scope : Scope) (word : Nat) : Prop :=
  match scope.importer with
  | none => False
  | some step => OneBased word doc.driverPrograms.length ∧ programEnd doc (word - 1) ≤ step

def imports (doc : Document) (scope : Scope) (word : Nat) : Bool :=
  match scope.importer with
  | none => false
  | some step => oneBased word doc.driverPrograms.length && decide (programEnd doc (word - 1) ≤ step)

def Precedes (scope : Scope) (word : Nat) : Prop :=
  match scope.user with
  | none => True
  | some user => word < user

def OperandOk (env : Env) (scope : Scope) (kind : RowKind) (word : Nat) : Prop :=
  scope.segStart ≤ word ∧ word < scope.segEnd ∧ Precedes scope word ∧
    (env.instructions[word]?).map (·.kind) = some kind

def RoleOk (env : Env) (scope : Scope) : Role → Nat → Prop
  | .raw, _ => True
  | .symbol, word => word < env.doc.symbolBase.val + env.doc.symbols.length
  | .slot, word => word < scope.slots
  | .stringId, word => OneBased word env.doc.strings.length
  | .bytesId, word => OneBased word env.doc.bytes.length
  | .location, word => OneBased word env.doc.locations.length
  | .typeId, word => OneBased word (poolCount env.doc 0)
  | .loweredType, word => word = irNone ∨ OneBased word (poolCount env.doc 0)
  | .expr, word => OperandOk env scope .expr word
  | .stmt, word => OperandOk env scope .stmt word
  | .int, word => OperandOk env scope .int word
  | .bool, word => OperandOk env scope .bool word
  | .pattern, _ => False
  | .stage, _ => False
  | .value, word => word < scope.valueBound
  | .function, word => OneBased word env.doc.functions.length
  | .regex, word => word < poolCount env.doc 3
  | .constant, word => word < poolCount env.doc 4
  | .wireEnum, word => word < poolCount env.doc 5
  | .schema, word => word < poolCount env.doc 6
  | .modulePlan, word => word < poolCount env.doc 7
  | .materialization, word => word < poolCount env.doc 8
  | .schemaExpectation, word => word ≠ irNone ∧ word < poolCount env.doc 9
  | .runtimeOp, word => word < env.doc.runtimeOps.length
  | .assignOp, word => word < env.doc.assignOps.length
  | .binaryOp, word => word < env.doc.binaryOps.length
  | .runKind, word => word < env.doc.runKinds.length
  | .redirectionKind, word => word < env.doc.redirectionKinds.length
  | .bindingIdentity, word => word < env.doc.bindingIdentities.length
  | .childProgram, word => Imports env.doc scope word
  | .deferKind, word => word < env.schema.deferKinds
  | .endDistance, word => word ≠ 0
  | .textHole, word => word < env.schema.textHoleKinds

def precedes (scope : Scope) (word : Nat) : Bool :=
  match scope.user with
  | none => true
  | some user => decide (word < user)

def operandOk (env : Env) (scope : Scope) (kind : RowKind) (word : Nat) : Bool :=
  decide (scope.segStart ≤ word) && decide (word < scope.segEnd) && precedes scope word &&
    decide ((env.instructions[word]?).map (·.kind) = some kind)

def roleOk (env : Env) (scope : Scope) : Role → Nat → Bool
  | .raw, _ => true
  | .symbol, word => decide (word < env.doc.symbolBase.val + env.doc.symbols.length)
  | .slot, word => decide (word < scope.slots)
  | .stringId, word => oneBased word env.doc.strings.length
  | .bytesId, word => oneBased word env.doc.bytes.length
  | .location, word => oneBased word env.doc.locations.length
  | .typeId, word => oneBased word (poolCount env.doc 0)
  | .loweredType, word => decide (word = irNone) || oneBased word (poolCount env.doc 0)
  | .expr, word => operandOk env scope .expr word
  | .stmt, word => operandOk env scope .stmt word
  | .int, word => operandOk env scope .int word
  | .bool, word => operandOk env scope .bool word
  | .pattern, _ => false
  | .stage, _ => false
  | .value, word => decide (word < scope.valueBound)
  | .function, word => oneBased word env.doc.functions.length
  | .regex, word => decide (word < poolCount env.doc 3)
  | .constant, word => decide (word < poolCount env.doc 4)
  | .wireEnum, word => decide (word < poolCount env.doc 5)
  | .schema, word => decide (word < poolCount env.doc 6)
  | .modulePlan, word => decide (word < poolCount env.doc 7)
  | .materialization, word => decide (word < poolCount env.doc 8)
  | .schemaExpectation, word => decide (word ≠ irNone) && decide (word < poolCount env.doc 9)
  | .runtimeOp, word => decide (word < env.doc.runtimeOps.length)
  | .assignOp, word => decide (word < env.doc.assignOps.length)
  | .binaryOp, word => decide (word < env.doc.binaryOps.length)
  | .runKind, word => decide (word < env.doc.runKinds.length)
  | .redirectionKind, word => decide (word < env.doc.redirectionKinds.length)
  | .bindingIdentity, word => decide (word < env.doc.bindingIdentities.length)
  | .childProgram, word => imports env.doc scope word
  | .deferKind, word => decide (word < env.schema.deferKinds)
  | .endDistance, word => decide (word ≠ 0)
  | .textHole, word => decide (word < env.schema.textHoleKinds)

def wordClaims : Role → Nat → List Claim
  | .expr, word => [.instruction word]
  | .stmt, word => [.instruction word]
  | .int, word => [.instruction word]
  | .bool, word => [.instruction word]
  | .childProgram, word => [.program (word - 1)]
  | _, _ => []

def blockRange (doc : Document) (owner word : Nat) (statements : Bool) : Option (Nat × Nat) :=
  if 1 ≤ word then
    match doc.blocks[word - 1]? with
    | some row =>
        if rowWord row 4 % 2 = (if statements then 1 else 0) ∧ (rowWord row 3 = irNone ∨ rowWord row 3 = owner) ∧
            rowWord row 0 + rowWord row 1 ≤ doc.extra.length then
          some (rowWord row 0, rowWord row 1)
        else
          none
    | none => none
  else
    none

def payloadRange (table : List Row) (index : Nat) (extra : Nat) : Option (Nat × Nat) :=
  match table[index]? with
  | some row => if rowWord row 0 + rowWord row 1 ≤ extra then some (rowWord row 0, rowWord row 1) else none
  | none => none

def lookupRecursive (schema : Schema) (name : String) : Option RecursiveShape :=
  schema.recursive.find? (·.name == name)

abbrev Fit := Option (Nat × List Claim)

abbrev Enter := Scope → Shape → Nat → Nat → Fit

def check (condition : Prop) [Decidable condition] : Option Unit := if condition then some () else none

def readWord (doc : Document) (position limit : Nat) : Option Nat :=
  if position < limit then wordAt doc position else none

def joinFit (first : Fit) (rest : Nat → Fit) : Fit :=
  first.bind fun (middle, claims) => (rest middle).map fun (stop, more) => (stop, claims ++ more)

def iterate (step : Nat → Fit) : Nat → Nat → Fit
  | 0, position => some (position, [])
  | count + 1, position => joinFit (step position) (iterate step count)

def wordFit (env : Env) (scope : Scope) (role : Role) (position limit : Nat) : Fit :=
  (readWord env.doc position limit).bind fun word =>
    (check (roleOk env scope role word = true)).map fun _ => (position + 1, wordClaims role word)

def entryFit (enter : Enter) (scope : Scope) (fields : List Shape) (start length : Nat) : Option (List Claim) :=
  (enter scope (.seq fields) start (start + length)).bind fun (stop, claims) =>
    (check (stop = start + length)).map fun _ => claims

def patternFit (env : Env) (enter : Enter) (scope : Scope) (position limit : Nat) : Fit :=
  (readWord env.doc position limit).bind fun word =>
    (check (word < scope.patternBound)).bind fun _ =>
      (env.patterns[word]?).bind fun tag =>
        (payloadRange env.doc.patternData word env.doc.extra.length).bind fun (start, length) =>
          (entryFit enter { scope with patternBound := word } tag.fields start length).map fun claims =>
            (position + 1, .pattern word :: claims)

def stageFit (env : Env) (enter : Enter) (scope : Scope) (position limit : Nat) : Fit :=
  (readWord env.doc position limit).bind fun word =>
    (check (word < scope.stageBound)).bind fun _ =>
      (env.stages[word]?).bind fun tag =>
        (payloadRange env.doc.stageData word env.doc.extra.length).bind fun (start, length) =>
          (entryFit enter { scope with stageBound := word } tag.fields start length).map fun claims =>
            (position + 1, .stage word :: claims)

def countedFit (doc : Document) (step : Nat → Fit) (position limit : Nat) : Fit :=
  (readWord doc position limit).bind fun count => iterate step count (position + 1)

def blockFit (doc : Document) (step : Nat → Nat → Fit) (owner word : Nat) (statements : Bool) : Option (List Claim) :=
  (blockRange doc owner word statements).bind fun (start, length) =>
    (countedFit doc (step (start + length)) start (start + length)).bind fun (stop, claims) =>
      (check (stop = start + length)).map fun _ => claims

mutual
def fitAt (env : Env) (enter : Enter) (scope : Scope) : Shape → Nat → Nat → Fit
  | .word .pattern, position, limit => patternFit env enter scope position limit
  | .word .stage, position, limit => stageFit env enter scope position limit
  | .word role, position, limit => wordFit env scope role position limit
  | .seq items, position, limit => fitList env enter scope items position limit
  | .choice tags variants, position, limit =>
      (readWord env.doc position limit).bind fun tag => fitChoice env enter scope tag tags variants (position + 1) limit
  | .many item, position, limit =>
      countedFit env.doc (fun cursor => fitAt env enter scope item cursor limit) position limit
  | .block statements item, position, limit =>
      (readWord env.doc position limit).bind fun word =>
        (blockRange env.doc scope.owner word statements).bind fun (start, length) =>
          (fitAt env enter scope item start (start + length)).bind fun (stop, claims) =>
            (check (stop = start + length)).map fun _ => (position + 1, .block (word - 1) :: claims)
  | .recur name, position, limit =>
      (lookupRecursive env.schema name).bind fun shape => enter scope (.choice shape.tags shape.variants) position limit
  | .textPattern, position, limit =>
      (readWord env.doc position limit).bind fun word =>
        (blockFit env.doc (fun bound cursor => patternFit env enter scope cursor bound) scope.owner word false).bind fun holes =>
          (blockRange env.doc scope.owner word false).bind fun (start, _) =>
            (wordAt env.doc start).bind fun count =>
              (iterate (fun cursor => wordFit env scope .textHole cursor limit) count (position + 1)).bind
                fun (middle, tags) =>
                  (iterate (fun cursor => wordFit env scope .stringId cursor limit) (count + 1) middle).map
                    fun (stop, texts) => (stop, .block (word - 1) :: holes ++ (tags ++ texts))
def fitList (env : Env) (enter : Enter) (scope : Scope) : List Shape → Nat → Nat → Fit
  | [], position, _ => some (position, [])
  | item :: rest, position, limit =>
      joinFit (fitAt env enter scope item position limit) fun middle => fitList env enter scope rest middle limit
def fitChoice (env : Env) (enter : Enter) (scope : Scope) (tag : Nat) : List Nat → List Shape → Nat → Nat → Fit
  | candidate :: tags, variant :: variants, position, limit =>
      if candidate = tag then fitAt env enter scope variant position limit
      else fitChoice env enter scope tag tags variants position limit
  | _, _, _, _ => none
end

def fitFuel (env : Env) : Nat → Enter
  | 0 => fun _ _ _ _ => none
  | fuel + 1 => fun scope shape position limit => fitAt env (fitFuel env fuel) scope shape position limit

abbrev EntrySpec := Scope → Shape → Nat → Nat → Nat → List Claim → Prop

inductive WordsFit (env : Env) (scope : Scope) (role : Role) (limit : Nat) : Nat → Nat → Nat → List Claim → Prop
  | zero {position} : WordsFit env scope role limit 0 position position []
  | succ {count position word stop claims} :
      readWord env.doc position limit = some word → RoleOk env scope role word →
      WordsFit env scope role limit count (position + 1) stop claims →
      WordsFit env scope role limit (count + 1) position stop (wordClaims role word ++ claims)

def EntryFits (entry : EntrySpec) (scope : Scope) (fields : List Shape) (start length : Nat) (claims : List Claim) : Prop :=
  entry scope (.seq fields) start (start + length) (start + length) claims

def PatternRef (env : Env) (entry : EntrySpec) (scope : Scope) (position limit : Nat) (claims : List Claim) : Prop :=
  ∃ word tag start length inner, readWord env.doc position limit = some word ∧
    word < scope.patternBound ∧ env.patterns[word]? = some tag ∧
    payloadRange env.doc.patternData word env.doc.extra.length = some (start, length) ∧
    EntryFits entry { scope with patternBound := word } tag.fields start length inner ∧
    claims = .pattern word :: inner

def StageRef (env : Env) (entry : EntrySpec) (scope : Scope) (position limit : Nat) (claims : List Claim) : Prop :=
  ∃ word tag start length inner, readWord env.doc position limit = some word ∧
    word < scope.stageBound ∧ env.stages[word]? = some tag ∧
    payloadRange env.doc.stageData word env.doc.extra.length = some (start, length) ∧
    EntryFits entry { scope with stageBound := word } tag.fields start length inner ∧
    claims = .stage word :: inner

inductive PatternRefs (env : Env) (entry : EntrySpec) (scope : Scope) (limit : Nat) : Nat → Nat → Nat → List Claim → Prop
  | zero {position} : PatternRefs env entry scope limit 0 position position []
  | succ {count position stop first rest} : PatternRef env entry scope position limit first →
      PatternRefs env entry scope limit count (position + 1) stop rest →
      PatternRefs env entry scope limit (count + 1) position stop (first ++ rest)

mutual
inductive Fits (env : Env) (entry : EntrySpec) : Scope → Shape → Nat → Nat → Nat → List Claim → Prop
  | word {scope role position limit word} : role ≠ .pattern → role ≠ .stage →
      readWord env.doc position limit = some word → RoleOk env scope role word →
      Fits env entry scope (.word role) position limit (position + 1) (wordClaims role word)
  | pattern {scope position limit claims} : PatternRef env entry scope position limit claims →
      Fits env entry scope (.word .pattern) position limit (position + 1) claims
  | stage {scope position limit claims} : StageRef env entry scope position limit claims →
      Fits env entry scope (.word .stage) position limit (position + 1) claims
  | seq {scope items position limit stop claims} : FitsList env entry scope items position limit stop claims →
      Fits env entry scope (.seq items) position limit stop claims
  | choice {scope tags variants position limit tag stop claims} :
      readWord env.doc position limit = some tag →
      FitsChoice env entry scope tag tags variants (position + 1) limit stop claims →
      Fits env entry scope (.choice tags variants) position limit stop claims
  | many {scope item position limit count stop claims} :
      readWord env.doc position limit = some count →
      FitsMany env entry scope item limit count (position + 1) stop claims →
      Fits env entry scope (.many item) position limit stop claims
  | block {scope statements item position limit word start length claims} :
      readWord env.doc position limit = some word → blockRange env.doc scope.owner word statements = some (start, length) →
      Fits env entry scope item start (start + length) (start + length) claims →
      Fits env entry scope (.block statements item) position limit (position + 1) (.block (word - 1) :: claims)
  | recur {scope name position limit stop claims shape} : lookupRecursive env.schema name = some shape →
      entry scope (.choice shape.tags shape.variants) position limit stop claims →
      Fits env entry scope (.recur name) position limit stop claims
  | textPattern {scope position limit word start length count holes middle tags stop texts} :
      readWord env.doc position limit = some word → blockRange env.doc scope.owner word false = some (start, length) →
      readWord env.doc start (start + length) = some count →
      PatternRefs env entry scope (start + length) count (start + 1) (start + length) holes →
      WordsFit env scope .textHole limit count (position + 1) middle tags →
      WordsFit env scope .stringId limit (count + 1) middle stop texts →
      Fits env entry scope .textPattern position limit stop (.block (word - 1) :: holes ++ (tags ++ texts))
inductive FitsList (env : Env) (entry : EntrySpec) : Scope → List Shape → Nat → Nat → Nat → List Claim → Prop
  | nil {scope position limit} : FitsList env entry scope [] position limit position []
  | cons {scope item rest position limit middle stop first more} :
      Fits env entry scope item position limit middle first →
      FitsList env entry scope rest middle limit stop more →
      FitsList env entry scope (item :: rest) position limit stop (first ++ more)
inductive FitsChoice (env : Env) (entry : EntrySpec) : Scope → Nat → List Nat → List Shape → Nat → Nat → Nat → List Claim → Prop
  | here {scope tag tags variant variants position limit stop claims} :
      Fits env entry scope variant position limit stop claims →
      FitsChoice env entry scope tag (tag :: tags) (variant :: variants) position limit stop claims
  | there {scope tag candidate tags variant variants position limit stop claims} : candidate ≠ tag →
      FitsChoice env entry scope tag tags variants position limit stop claims →
      FitsChoice env entry scope tag (candidate :: tags) (variant :: variants) position limit stop claims
inductive FitsMany (env : Env) (entry : EntrySpec) : Scope → Shape → Nat → Nat → Nat → Nat → List Claim → Prop
  | zero {scope item limit position} : FitsMany env entry scope item limit 0 position position []
  | succ {scope item limit count position middle stop first more} :
      Fits env entry scope item position limit middle first →
      FitsMany env entry scope item limit count middle stop more →
      FitsMany env entry scope item limit (count + 1) position stop (first ++ more)
end

def EntryAt (env : Env) : Nat → EntrySpec
  | 0 => fun _ _ _ _ _ _ => False
  | fuel + 1 => Fits env (EntryAt env fuel)

end Xsh.IR.Raw
