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

structure RawParameter where
  name : Nat
  flags : Nat
  reserved : Nat × Nat × Nat
  deriving DecidableEq

def decodeRawParameter (value : WireValue) : Option RawParameter := do
  let fields ← value.closedObject? ["name", "flags", "reserved0", "reserved1", "reserved2"]
  pure ⟨← wordField fields "name", ← rawByteField fields "flags",
    (← rawByteField fields "reserved0", ← rawByteField fields "reserved1", ← rawByteField fields "reserved2")⟩

def decodeRawParameters (value : WireValue) : Option (List RawParameter) := do
  let fields ← value.closedObject? ["schema", "params"]
  if textField fields "schema" = some "xsh-connected-raw-parameters-v1" then pure () else none
  listField fields "params" decodeRawParameter

def rawFunctionParameters (table : List RawParameter) (function : RawFunction) : Option (List RawParameter) :=
  if function.parameters.start + function.parameters.length ≤ table.length then
    some ((table.drop function.parameters.start).take function.parameters.length)
  else none

def rawParameterRow (function : RawFunction) (slot : Nat) : Option Nat :=
  if slot < function.parameters.length then some (function.parameters.start + slot) else none

def rawParameterNamed (names : RawNameBytes) (rows : RawInstructionRows) (parameter : RawParameter)
    (binding : Binding) : Bool :=
  match names binding.name with
  | some spelling => decide (parameter.flags = 0) && decide (parameter.reserved = (0, 0, 0)) &&
      decide (readRawStringBytes rows parameter.name = some spelling)
  | none => false

def rawParametersNamed (names : RawNameBytes) (rows : RawInstructionRows) :
    List RawParameter → List Binding → Bool
  | [], [] => true
  | parameter :: parameters, binding :: bindings =>
      rawParameterNamed names rows parameter binding && rawParametersNamed names rows parameters bindings
  | _, _ => false

def rawParametersAuthenticated (names : RawNameBytes) (rows : RawInstructionRows) (table : List RawParameter)
    (function : RawFunction) (parameters : List Binding) : Bool :=
  match rawFunctionParameters table function with
  | some found => rawParametersNamed names rows found parameters
  | none => false

def RawParameterNamesBinding (names : RawNameBytes) (rows : RawInstructionRows) (table : List RawParameter)
    (function : RawFunction) (slot : Nat) (binding : Binding) : Prop :=
  ∃ row parameter spelling, rawParameterRow function slot = some row ∧ table[row]? = some parameter ∧
    parameter.flags = 0 ∧ parameter.reserved = (0, 0, 0) ∧
    names binding.name = some spelling ∧ readRawStringBytes rows parameter.name = some spelling

theorem raw_parameter_named_sound (names : RawNameBytes) (rows : RawInstructionRows) (parameter : RawParameter)
    (binding : Binding) (named : rawParameterNamed names rows parameter binding = true) :
    parameter.flags = 0 ∧ parameter.reserved = (0, 0, 0) ∧ ∃ spelling,
      names binding.name = some spelling ∧ readRawStringBytes rows parameter.name = some spelling := by
  unfold rawParameterNamed at named
  cases spelled : names binding.name with
  | none => simp [spelled] at named
  | some spelling =>
      simp only [spelled, Bool.and_eq_true, decide_eq_true_eq] at named
      exact ⟨named.1.1, named.1.2, spelling, rfl, named.2⟩

theorem raw_parameters_named_at (names : RawNameBytes) (rows : RawInstructionRows) :
    ∀ (found : List RawParameter) (bindings : List Binding), rawParametersNamed names rows found bindings = true →
      ∀ (index : Nat) (binding : Binding), bindings[index]? = some binding →
        ∃ parameter, found[index]? = some parameter ∧ rawParameterNamed names rows parameter binding = true
  | [], [], _, _, _, at_index => by simp at at_index
  | parameter :: found, head :: bindings, named, index, binding, at_index => by
      simp only [rawParametersNamed, Bool.and_eq_true] at named
      cases index with
      | zero =>
          simp at at_index
          subst at_index
          exact ⟨parameter, rfl, named.1⟩
      | succ index =>
          simp at at_index
          exact raw_parameters_named_at names rows found bindings named.2 index binding at_index
  | [], _ :: _, named, _, _, _ => by simp [rawParametersNamed] at named
  | _ :: _, [], named, _, _, _ => by simp [rawParametersNamed] at named

theorem raw_function_parameters_at (table : List RawParameter) (function : RawFunction)
    (found : List RawParameter) (read : rawFunctionParameters table function = some found)
    (index : Nat) (parameter : RawParameter) (at_index : found[index]? = some parameter) :
    rawParameterRow function index = some (function.parameters.start + index) ∧
      table[function.parameters.start + index]? = some parameter := by
  unfold rawFunctionParameters at read
  split at read
  · cases read
    simp only [List.getElem?_take, List.getElem?_drop] at at_index
    split at at_index
    · rename_i bound
      exact ⟨by simp [rawParameterRow, bound], at_index⟩
    · simp at at_index
  · simp at read

theorem raw_parameters_authenticated_sound (names : RawNameBytes) (rows : RawInstructionRows)
    (table : List RawParameter) (function : RawFunction) (parameters : List Binding)
    (authenticated : rawParametersAuthenticated names rows table function parameters = true) :
    ∀ index binding, parameters[index]? = some binding →
      RawParameterNamesBinding names rows table function index binding := by
  intro index binding at_index
  unfold rawParametersAuthenticated at authenticated
  cases read : rawFunctionParameters table function with
  | none => simp [read] at authenticated
  | some found =>
      simp only [read] at authenticated
      obtain ⟨parameter, found_at, named⟩ :=
        raw_parameters_named_at names rows found parameters authenticated index binding at_index
      obtain ⟨row_eq, table_at⟩ := raw_function_parameters_at table function found read index parameter found_at
      obtain ⟨flags, reserved, spelling, spelled, bytes⟩ :=
        raw_parameter_named_sound names rows parameter binding named
      exact ⟨_, parameter, spelling, row_eq, table_at, flags, reserved, spelled, bytes⟩

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
    | .resultOk inner | .resultError inner | .propagate inner => RawValueTree.mentions slot inner
    | .call _ arguments => RawValueArguments.mentions slot arguments
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

  def RawValueArguments.mentions (slot : Nat) : RawValueArguments → Bool
    | .nil => false
    | .cons tree rest => RawValueTree.mentions slot tree || RawValueArguments.mentions slot rest
  termination_by structural arguments => arguments
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
    (table : List RawParameter) (owner : Nat) (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) : Option (List (BinderId × Nat)) :=
  match functions.functions[owner - 1]? with
  | none => none
  | some function =>
    let slots := claimedRawSlots rows functions owner parameters declarations occurrences
    if decide (0 < owner) && decide (declarations.function = owner) &&
        decide (function.parameters.length = parameters.length) &&
        rawParametersAuthenticated names rows table function parameters &&
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
    (functions : RawFunctionRows) (table : List RawParameter) (owner : Nat) (parameters : List Binding)
    (declarations : RawLocalDeclarations) (occurrences : List ScopedOccurrence) (slots : List (BinderId × Nat))
    (derived : deriveRawSlots fuel names rows functions table owner parameters declarations occurrences = some slots) :
    slots = claimedRawSlots rows functions owner parameters declarations occurrences ∧
    (slots.map Prod.fst).Nodup ∧ (slots.map Prod.snd).Nodup ∧
    (∃ function, functions.functions[owner - 1]? = some function ∧
      ∀ index binding, parameters[index]? = some binding →
        (binding.binder, index) ∈ slots ∧ RawParameterNamesBinding names rows table function index binding) ∧
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
      obtain ⟨⟨⟨⟨⟨⟨⟨⟨_, _⟩, _⟩, named⟩, _⟩, uniqueBinders⟩, uniqueSlots⟩, _⟩, evidenced⟩ := accepted
      refine ⟨rfl, uniqueBinders, uniqueSlots, ⟨function, rfl, ?_⟩, ?_⟩
      · intro index binding at_index
        have binder_at : (parameters.map Binding.binder)[index]? = some binding.binder := by simp [at_index]
        have held := indexed_slots_hold_each_position 0 (parameters.map Binding.binder) index binding.binder binder_at
        exact ⟨by simpa [claimedRawSlots] using Or.inl held,
          raw_parameters_authenticated_sound names rows table function parameters named index binding at_index⟩
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
