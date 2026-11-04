import Xsh.Conformance.Connected.RawExpression

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

structure RawLocalClaim where
  site : SourceSite
  row : Nat
  deriving DecidableEq

structure RawLocalFact where
  instruction : Nat
  mutable : Bool
  deriving DecidableEq

structure RawLocalDeclarations where
  function : Nat
  locals : List RawLocalClaim
  facts : List RawLocalFact
  deriving DecidableEq

def decodeRawLocalClaim (value : WireValue) : Option RawLocalClaim := do
  let fields ← value.closedObject? ["site", "row"]
  pure ⟨← siteField fields "site", ← wordField fields "row"⟩

def decodeRawLocalFact (value : WireValue) : Option RawLocalFact := do
  let fields ← value.closedObject? ["instruction", "kind"]
  let mutable ← match textField fields "kind" with
    | some "mutable" => some true
    | some "immutable" => some false
    | _ => none
  pure ⟨← wordField fields "instruction", mutable⟩

def decodeRawLocalDeclarations (value : WireValue) : Option RawLocalDeclarations := do
  let fields ← value.closedObject? ["schema", "function", "locals", "facts"]
  if textField fields "schema" = some "xsh-connected-raw-locals-v1" then pure () else none
  let locals ← listField fields "locals" decodeRawLocalClaim
  let facts ← listField fields "facts" decodeRawLocalFact
  if (locals.map RawLocalClaim.site).Nodup && (locals.map RawLocalClaim.row).Nodup &&
      (facts.map RawLocalFact.instruction).Nodup then
    pure ⟨← wordField fields "function", locals, facts⟩
  else none

structure SourceLocal where
  binder : BinderId
  mutable : Bool
  site : SourceSite
  initializer : Node

def sourceLocals (occurrences : List ScopedOccurrence) : List SourceLocal :=
  occurrences.filterMap fun occurrence => occurrence.declared.map fun (binder, kind, site) =>
    ⟨binder, decide (kind = .mutable), site, occurrence.candidate⟩

def rawFunctionStatements (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat) : List Nat :=
  (List.range functions.blocks.length).flatMap fun index =>
    match functions.blocks[index]? with
    | some block => if block.owner = owner then (readRawStatementBlock rows functions owner (index + 1)).getD [] else []
    | none => []

def rawLetDeclaration (rows : RawInstructionRows) (statement : Nat) : Option (Nat × Nat) := do
  let instruction ← readRawInstruction rows statement
  if instruction.tag = ⟨"StmtLet", 115⟩ then pure () else none
  let [slot, value, 0] := instruction.payload | none
  if value < statement then some (slot, value) else none

mutual
  def RawValueTree.mentions (slot : Nat) : RawValueTree → Bool
    | .slot read => read == slot
    | .binary _ left right => RawValueTree.mentions slot left || RawValueTree.mentions slot right
    | .field base _ => RawValueTree.mentions slot base
    | .record fields => RawValueFields.mentions slot fields
    | .conditional branches otherwise => RawValueBranches.mentions slot branches || RawValueTree.mentions slot otherwise
    | _ => false
  termination_by structural tree => tree

  def RawValueFields.mentions (slot : Nat) : RawValueFields → Bool
    | .nil => false
    | .cons _ tree rest => RawValueTree.mentions slot tree || RawValueFields.mentions slot rest
  termination_by structural fields => fields

  def RawValueBranches.mentions (slot : Nat) : RawValueBranches → Bool
    | .nil => false
    | .cons condition tree rest =>
        RawValueTree.mentions slot condition || RawValueTree.mentions slot tree || RawValueBranches.mentions slot rest
  termination_by structural branches => branches
end

def substituteRawTemporaries (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat)
    (reserved : List Nat) (fuel : Nat) (statements : List Nat) (tree : RawValueTree) : Option RawValueTree :=
  statements.foldlM (fun tree statement =>
    match rawLetDeclaration rows statement with
    | some (slot, value) =>
        if reserved.contains slot || !RawValueTree.mentions slot tree then some tree
        else (readRawValueTree rows functions owner reserved fuel value).map fun definition =>
          RawValueTree.bind slot definition tree
    | none => some tree) tree

def resolveRawTemporaries (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat)
    (reserved : List Nat) (fuel : Nat) (statements : List Nat) : Nat → RawValueTree → Option RawValueTree
  | 0, tree => some tree
  | passes + 1, tree => do
      resolveRawTemporaries rows functions owner reserved fuel statements passes
        (← substituteRawTemporaries rows functions owner reserved fuel statements tree)

def claimedLocalRow (declarations : RawLocalDeclarations) (declaration : SourceLocal) : Option Nat :=
  (declarations.locals.find? fun claim => decide (claim.site = declaration.site)).map RawLocalClaim.row

def claimedLocalSlot (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat)
    (declarations : RawLocalDeclarations) (declaration : SourceLocal) : Option Nat := do
  let row ← claimedLocalRow declarations declaration
  let (slot, _) ← rawLetDeclaration rows row
  let fact ← declarations.facts.find? fun fact => decide (fact.instruction = row)
  if (rawFunctionStatements rows functions owner).contains row && fact.mutable == declaration.mutable then
    some slot
  else none

structure LocalEvidence where
  row : Nat
  slot : Nat
  value : Nat
  tree : RawValueTree

def localEvidence (fuel : Nat) (names : RawNameBytes) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (owner : Nat) (declarations : RawLocalDeclarations) (slots : List (BinderId × Nat))
    (declaration : SourceLocal) : Option LocalEvidence := do
  let row ← claimedLocalRow declarations declaration
  let slot ← claimedLocalSlot rows functions owner declarations declaration
  let (_, value) ← rawLetDeclaration rows row
  let reserved := slots.map Prod.snd
  let raw ← readRawValueTree rows functions owner reserved fuel value
  let statements := (rawFunctionStatements rows functions owner).filter fun statement => decide (statement < row)
  let tree ← resolveRawTemporaries rows functions owner reserved fuel statements (statements.length + 1) raw
  let expected ← nodeRawValueTree names slots fuel declaration.initializer
  if tree = expected then some ⟨row, slot, value, tree⟩ else none

def indexedSlots : Nat → List BinderId → List (BinderId × Nat)
  | _, [] => []
  | index, binder :: rest => (binder, index) :: indexedSlots (index + 1) rest

def claimedRawSlots (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat)
    (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) : List (BinderId × Nat) :=
  indexedSlots 0 (parameters.map Binding.binder) ++
    (sourceLocals occurrences).filterMap fun declaration =>
      (claimedLocalSlot rows functions owner declarations declaration).map (declaration.binder, ·)

def deriveRawSlots (fuel : Nat) (names : RawNameBytes) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (owner : Nat) (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) : Option (List (BinderId × Nat)) :=
  match functions.functions[owner - 1]? with
  | none => none
  | some function =>
    let slots := claimedRawSlots rows functions owner parameters declarations occurrences
    if decide (0 < owner) && decide (declarations.function = owner) &&
        decide (function.parameters.length = parameters.length) &&
        decide (declarations.locals.length = (sourceLocals occurrences).length) &&
        decide ((slots.map Prod.fst).Nodup) && decide ((slots.map Prod.snd).Nodup) &&
        slots.all (fun entry => decide (entry.2 < function.slotCount)) &&
        (sourceLocals occurrences).all (fun declaration =>
          (localEvidence fuel names rows functions owner declarations slots declaration).isSome)
    then some slots else none

theorem claimed_local_row_sound (declarations : RawLocalDeclarations) (declaration : SourceLocal) (row : Nat)
    (claimed : claimedLocalRow declarations declaration = some row) :
    ∃ claim ∈ declarations.locals, claim.site = declaration.site ∧ claim.row = row := by
  unfold claimedLocalRow at claimed
  cases found : declarations.locals.find? (fun claim => decide (claim.site = declaration.site)) with
  | none => simp [found] at claimed
  | some claim =>
      simp [found] at claimed
      have member := List.mem_of_find?_eq_some found
      have same := List.find?_some found
      exact ⟨claim, member, by simpa using same, claimed⟩

theorem claimed_local_slot_sound (rows : RawInstructionRows) (functions : RawFunctionRows) (owner : Nat)
    (declarations : RawLocalDeclarations) (declaration : SourceLocal) (slot : Nat)
    (claimed : claimedLocalSlot rows functions owner declarations declaration = some slot) :
    ∃ row value, claimedLocalRow declarations declaration = some row ∧
      rawLetDeclaration rows row = some (slot, value) ∧
      row ∈ rawFunctionStatements rows functions owner ∧
      ∃ fact ∈ declarations.facts, fact.instruction = row ∧ fact.mutable = declaration.mutable := by
  unfold claimedLocalSlot at claimed
  cases rowEq : claimedLocalRow declarations declaration with
  | none => simp [rowEq] at claimed
  | some row =>
    cases letEq : rawLetDeclaration rows row with
    | none => simp [rowEq, letEq] at claimed
    | some pair =>
      obtain ⟨found, value⟩ := pair
      cases factEq : declarations.facts.find? (fun fact => decide (fact.instruction = row)) with
      | none => simp [rowEq, letEq, factEq] at claimed
      | some fact =>
        by_cases accepted : (rawFunctionStatements rows functions owner).contains row = true ∧
            (fact.mutable == declaration.mutable) = true
        · simp [rowEq, letEq, factEq] at claimed
          obtain ⟨⟨member, same⟩, equal⟩ := claimed
          subst equal
          exact ⟨row, value, rfl, letEq, member, fact, List.mem_of_find?_eq_some factEq,
            by simpa using List.find?_some factEq, same⟩
        · simp [rowEq, letEq, factEq, Bool.and_eq_true] at claimed
          simp_all

theorem local_evidence_sound (fuel : Nat) (names : RawNameBytes) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (owner : Nat) (declarations : RawLocalDeclarations)
    (slots : List (BinderId × Nat)) (declaration : SourceLocal) (evidence : LocalEvidence)
    (accepted : localEvidence fuel names rows functions owner declarations slots declaration = some evidence) :
    claimedLocalRow declarations declaration = some evidence.row ∧
    claimedLocalSlot rows functions owner declarations declaration = some evidence.slot ∧
    rawLetDeclaration rows evidence.row = some (evidence.slot, evidence.value) ∧
    nodeRawValueTree names slots fuel declaration.initializer = some evidence.tree := by
  unfold localEvidence at accepted
  cases rowEq : claimedLocalRow declarations declaration with
  | none => simp [rowEq] at accepted
  | some row =>
    cases slotEq : claimedLocalSlot rows functions owner declarations declaration with
    | none => simp [rowEq, slotEq] at accepted
    | some slot =>
      cases letEq : rawLetDeclaration rows row with
      | none => simp [rowEq, slotEq, letEq] at accepted
      | some pair =>
        obtain ⟨found, value⟩ := pair
        cases rawEq : readRawValueTree rows functions owner (slots.map Prod.snd) fuel value with
        | none => simp [rowEq, slotEq, letEq, rawEq] at accepted
        | some raw =>
          cases treeEq : resolveRawTemporaries rows functions owner (slots.map Prod.snd) fuel
              ((rawFunctionStatements rows functions owner).filter fun statement => decide (statement < row))
              (((rawFunctionStatements rows functions owner).filter fun statement => decide (statement < row)).length + 1) raw with
          | none => simp [rowEq, slotEq, letEq, rawEq, treeEq] at accepted
          | some tree =>
            cases expectedEq : nodeRawValueTree names slots fuel declaration.initializer with
            | none => simp [rowEq, slotEq, letEq, rawEq, treeEq, expectedEq] at accepted
            | some expected =>
              by_cases same : tree = expected
              · simp [rowEq, slotEq, letEq, rawEq, treeEq, expectedEq, same] at accepted
                subst evidence
                subst same
                obtain ⟨sameRow, sameValue, rowAgain, letAgain, _⟩ :=
                  claimed_local_slot_sound rows functions owner declarations declaration slot slotEq
                rw [rowEq] at rowAgain
                cases rowAgain
                rw [letEq] at letAgain
                cases letAgain
                simp [expectedEq, rowEq, slotEq, letEq]
              · simp [rowEq, slotEq, letEq, rawEq, treeEq, expectedEq, same] at accepted

theorem indexed_slots_hold_each_position (start : Nat) (binders : List BinderId) (index : Nat) (binder : BinderId)
    (at_index : binders[index]? = some binder) : (binder, start + index) ∈ indexedSlots start binders := by
  induction binders generalizing start index with
  | nil => simp at at_index
  | cons head rest ih =>
      cases index with
      | zero =>
          simp at at_index
          subst at_index
          simp [indexedSlots]
      | succ index =>
          simp at at_index
          have := ih (start + 1) index at_index
          have equal : start + 1 + index = start + (index + 1) := by omega
          rw [equal] at this
          simp [indexedSlots, this]

theorem derive_raw_slots_sound (fuel : Nat) (names : RawNameBytes) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (owner : Nat) (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) (slots : List (BinderId × Nat))
    (derived : deriveRawSlots fuel names rows functions owner parameters declarations occurrences = some slots) :
    slots = claimedRawSlots rows functions owner parameters declarations occurrences ∧
    (slots.map Prod.fst).Nodup ∧ (slots.map Prod.snd).Nodup ∧
    (∀ index binder, (parameters.map Binding.binder)[index]? = some binder → (binder, index) ∈ slots) ∧
    ∀ declaration ∈ sourceLocals occurrences, ∃ evidence,
      localEvidence fuel names rows functions owner declarations slots declaration = some evidence ∧
      (declaration.binder, evidence.slot) ∈ slots := by
  unfold deriveRawSlots at derived
  cases functionEq : functions.functions[owner - 1]? with
  | none => simp [functionEq] at derived
  | some function =>
    simp only [functionEq] at derived
    split at derived
    · rename_i accepted
      cases derived
      simp only [Bool.and_eq_true, decide_eq_true_eq, List.all_eq_true] at accepted
      obtain ⟨⟨⟨⟨⟨⟨⟨_, _⟩, _⟩, _⟩, uniqueBinders⟩, uniqueSlots⟩, _⟩, evidenced⟩ := accepted
      refine ⟨rfl, uniqueBinders, uniqueSlots, ?_, ?_⟩
      · intro index binder at_index
        have := indexed_slots_hold_each_position 0 (parameters.map Binding.binder) index binder at_index
        simpa [claimedRawSlots] using Or.inl this
      · intro declaration member
        have present := evidenced declaration member
        cases evidence : localEvidence fuel names rows functions owner declarations
            (claimedRawSlots rows functions owner parameters declarations occurrences) declaration with
        | none => simp [evidence] at present
        | some found =>
          have sound := local_evidence_sound fuel names rows functions owner declarations
            (claimedRawSlots rows functions owner parameters declarations occurrences) declaration found evidence
          refine ⟨found, rfl, ?_⟩
          simp only [claimedRawSlots, List.mem_append, List.mem_filterMap]
          exact Or.inr ⟨declaration, member, by simp [sound.2.1]⟩
    · simp at derived

end Xsh.Conformance.Connected
