import Xsh.Conformance.Connected.RawStore

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected

mutual
  inductive RawValueTree where
    | integer (value : Int)
    | boolean (value : Bool)
    | string (bytes : List Nat)
    | slot (slot : Nat)
    | binary (operator : BinaryOp) (left right : RawValueTree)
    | field (base : RawValueTree) (name : List Nat)
    | record (fields : RawValueFields)
    | conditional (branches : RawValueBranches) (otherwise : RawValueTree)
    | resultOk (value : RawValueTree)
    | resultError (value : RawValueTree)
    | propagate (value : RawValueTree)
    | call (function : Nat) (arguments : RawValueArguments)
    deriving DecidableEq

  inductive RawValueArguments where
    | nil
    | cons (value : RawValueTree) (rest : RawValueArguments)
    deriving DecidableEq

  inductive RawValueFields where
    | nil
    | cons (name : List Nat) (value : RawValueTree) (rest : RawValueFields)
    deriving DecidableEq

  inductive RawValueBranches where
    | nil
    | cons (condition value : RawValueTree) (rest : RawValueBranches)
    deriving DecidableEq
end

def RawValueFields.names : RawValueFields → List (List Nat)
  | .nil => []
  | .cons name _ rest => name :: rest.names

def RawValueBranches.length : RawValueBranches → Nat
  | .nil => 0
  | .cons _ _ rest => 1 + rest.length

def RawValueArguments.length : RawValueArguments → Nat
  | .nil => 0
  | .cons _ rest => 1 + rest.length

abbrev RawNameBytes := NameId → Option (List Nat)

inductive CalleeForm where
  | ok
  | error
  | function (index : Nat)
  deriving DecidableEq

abbrev CalleeBinders := BinderId → Option CalleeForm

def rawBytes (text : String) : List Nat := text.toUTF8.data.toList.map UInt8.toNat

def sourceRawNameBytes (unit : SourceUnit) : RawNameBytes := fun name =>
  (unit.names[name.value]?).map rawBytes

theorem source_raw_name_bytes_match_admitted_name (unit : SourceUnit) (text : String) (name : NameId)
    (resolved : sourceName unit.names text = some name) : sourceRawNameBytes unit name = some (rawBytes text) := by
  have facts : text ∈ unit.names ∧ NameId.decode? (unit.names.idxOf text) = some name := by
    simpa [sourceName] using resolved
  have index : unit.names.idxOf text = name.value := by
    simp [NameId.decode?, Word32.decode?] at facts
    obtain ⟨_, _, equal⟩ := facts
    cases equal
    rfl
  have bound := List.idxOf_lt_length_of_mem facts.1
  simp [sourceRawNameBytes, ← index, List.getElem?_eq_getElem bound, List.getElem_idxOf bound]

def rawSignedWords (low high : Nat) : Option Int :=
  if low < 4294967296 && high < 4294967296 then
    let bits := low + high * 4294967296
    some (if bits < 9223372036854775808 then Int.ofNat bits else Int.ofNat bits - 18446744073709551616)
  else none

mutual
  def RawValueTree.bind (slot : Nat) (value : RawValueTree) : RawValueTree → RawValueTree
    | .slot read => if read = slot then value else .slot read
    | .binary operator left right => .binary operator (RawValueTree.bind slot value left) (RawValueTree.bind slot value right)
    | .field base name => .field (RawValueTree.bind slot value base) name
    | .record fields => .record (RawValueFields.bind slot value fields)
    | .conditional branches otherwise => .conditional (RawValueBranches.bind slot value branches) (RawValueTree.bind slot value otherwise)
    | .resultOk inner => .resultOk (RawValueTree.bind slot value inner)
    | .resultError inner => .resultError (RawValueTree.bind slot value inner)
    | .propagate inner => .propagate (RawValueTree.bind slot value inner)
    | .call function arguments => .call function (RawValueArguments.bind slot value arguments)
    | tree => tree
  termination_by structural tree => tree

  def RawValueFields.bind (slot : Nat) (value : RawValueTree) : RawValueFields → RawValueFields
    | .nil => .nil
    | .cons name tree rest => .cons name (RawValueTree.bind slot value tree) (RawValueFields.bind slot value rest)
  termination_by structural fields => fields

  def RawValueBranches.bind (slot : Nat) (value : RawValueTree) : RawValueBranches → RawValueBranches
    | .nil => .nil
    | .cons condition tree rest => .cons (RawValueTree.bind slot value condition) (RawValueTree.bind slot value tree) (RawValueBranches.bind slot value rest)
  termination_by structural branches => branches

  def RawValueArguments.bind (slot : Nat) (value : RawValueTree) : RawValueArguments → RawValueArguments
    | .nil => .nil
    | .cons tree rest => .cons (RawValueTree.bind slot value tree) (RawValueArguments.bind slot value rest)
  termination_by structural arguments => arguments
end

mutual
  def readRawValueTree (rows : RawInstructionRows) (functions : RawFunctionRows)
      (owner : Nat) (reserved : List Nat) : Nat → Nat → Option RawValueTree
    | 0, _ => none
    | fuel + 1, row => do
      let instruction ← readRawInstruction rows row
      if connectedTagOrdinal instruction.tag.name = some instruction.tag.ordinal then pure () else none
      match ← rawExpressionShape rows instruction with
      | .integer low high => some (.integer (← rawSignedWords low high))
      | .boolean value => some (.boolean value)
      | .string bytes => if bytes.all (fun byte => byte < 256) then some (.string bytes) else none
      | .slot slot => some (.slot slot)
      | .binary operator left right _ => do
          if left < row && right < row then pure () else none
          some (.binary operator (← readRawValueTree rows functions owner reserved fuel left)
            (← readRawValueTree rows functions owner reserved fuel right))
      | .field base name _ => do
          if base < row && name.all (fun byte => byte < 256) then pure () else none
          some (.field (← readRawValueTree rows functions owner reserved fuel base) name)
      | .record block => do
          let count :: words ← readOwnedRawBlock rows functions owner block 0 | none
          let fields ← readRawValueFields rows functions owner reserved fuel row words
          if fields.names.length = count && fields.names.Nodup then some (.record fields) else none
      | .conditional block otherwise _ => do
          if otherwise < row then pure () else none
          let count :: words ← readOwnedRawBlock rows functions owner block 0 | none
          let branches ← readRawValueBranches rows functions owner reserved fuel row words
          if branches.length = count then
            some (.conditional branches (← readRawValueTree rows functions owner reserved fuel otherwise))
          else none
      | .resultOk value => do
          if value < row then pure () else none
          some (.resultOk (← readRawValueTree rows functions owner reserved fuel value))
      | .resultError value none => do
          if value < row then pure () else none
          some (.resultError (← readRawValueTree rows functions owner reserved fuel value))
      | .propagate value _ => do
          if value < row then pure () else none
          some (.propagate (← readRawValueTree rows functions owner reserved fuel value))
      | .call true function arguments _ => do
          if function = 0 then none else pure ()
          let count :: words ← readOwnedRawBlock rows functions owner arguments 0 | none
          let values ← readRawValueArguments rows functions owner reserved fuel row words
          if values.length = count then some (.call (function - 1) values) else none
      | .valueBlock block _ => do
          let statements ← readRawStatementBlock rows functions owner block
          let tail :: bindings := statements.reverse | none
          if statements.all (· < row) then pure () else none
          let valueInstruction ← readRawInstruction rows tail
          if valueInstruction.tag = ⟨"StmtValue", 126⟩ then pure () else none
          let [value] := valueInstruction.payload | none
          if value < tail then pure () else none
          let body ← readRawValueTree rows functions owner reserved fuel value
          readRawValueBindings rows functions owner reserved fuel body bindings
      | _ => none

  def readRawValueBindings (rows : RawInstructionRows) (functions : RawFunctionRows)
      (owner : Nat) (reserved : List Nat) : Nat → RawValueTree → List Nat → Option RawValueTree
    | 0, _, _ => none
    | _ + 1, body, [] => some body
    | fuel + 1, body, statement :: earlier => do
        let binding ← readRawInstruction rows statement
        if binding.tag = ⟨"StmtLet", 115⟩ then pure () else none
        let [slot, value, 0] := binding.payload | none
        if value < statement && !reserved.contains slot then pure () else none
        let tree ← readRawValueTree rows functions owner reserved fuel value
        readRawValueBindings rows functions owner reserved fuel (RawValueTree.bind slot tree body) earlier

  def readRawValueFields (rows : RawInstructionRows) (functions : RawFunctionRows)
      (owner : Nat) (reserved : List Nat) : Nat → Nat → List Nat → Option RawValueFields
    | 0, _, _ => none
    | _ + 1, _, [] => some .nil
    | fuel + 1, parent, 0 :: name :: value :: rest => do
        if value < parent then pure () else none
        let key ← readRawLabelBytes rows name
        if key.all (fun byte => byte < 256) then pure () else none
        let tree ← readRawValueTree rows functions owner reserved fuel value
        let fields ← readRawValueFields rows functions owner reserved fuel parent rest
        some (.cons key tree fields)
    | _ + 1, _, _ => none

  def readRawValueBranches (rows : RawInstructionRows) (functions : RawFunctionRows)
      (owner : Nat) (reserved : List Nat) : Nat → Nat → List Nat → Option RawValueBranches
    | 0, _, _ => none
    | _ + 1, _, [] => some .nil
    | fuel + 1, parent, condition :: value :: rest => do
        if condition < parent && value < parent then pure () else none
        let guard ← readRawValueTree rows functions owner reserved fuel condition
        let tree ← readRawValueTree rows functions owner reserved fuel value
        let branches ← readRawValueBranches rows functions owner reserved fuel parent rest
        some (.cons guard tree branches)
    | _ + 1, _, _ => none

  def readRawValueArguments (rows : RawInstructionRows) (functions : RawFunctionRows)
      (owner : Nat) (reserved : List Nat) : Nat → Nat → List Nat → Option RawValueArguments
    | 0, _, _ => none
    | _ + 1, _, [] => some .nil
    | fuel + 1, parent, 0 :: value :: rest => do
        if value < parent then pure () else none
        let tree ← readRawValueTree rows functions owner reserved fuel value
        let values ← readRawValueArguments rows functions owner reserved fuel parent rest
        some (.cons tree values)
    | _ + 1, _, _ => none
end

mutual
  def nodeRawValueTreeWith (names : RawNameBytes) (slots : List (BinderId × Nat)) (results : CalleeBinders) :
      Nat → Node → Option RawValueTree
    | 0, _ => none
    | _ + 1, .integer text => do
        let value ← parseInteger text
        some (.integer value.val)
    | _ + 1, .boolean value => some (.boolean value)
    | _ + 1, .string value => some (.string (rawBytes value))
    | _ + 1, .read binder [] => do
        let (_, slot) ← slots.find? (fun entry => decide (entry.1 = binder))
        some (.slot slot)
    | fuel + 1, .binary operator left right => do
        some (.binary operator (← nodeRawValueTreeWith names slots results fuel left)
          (← nodeRawValueTreeWith names slots results fuel right))
    | fuel + 1, .field base name => do
        some (.field (← nodeRawValueTreeWith names slots results fuel base) (← names name))
    | fuel + 1, .record fields => do
        let values ← nodeRawValueFieldsWith names slots results fuel fields
        if values.names.Nodup then some (.record values) else none
    | fuel + 1, .conditional branches otherwise => do
        some (.conditional (← nodeRawValueBranchesWith names slots results fuel branches)
          (← nodeRawValueTreeWith names slots results fuel otherwise))
    | fuel + 1, .call (.read binder _) arguments => do
        match ← results binder, arguments with
        | .ok, [argument] => some (.resultOk (← nodeRawValueTreeWith names slots results fuel argument))
        | .error, [argument] => some (.resultError (← nodeRawValueTreeWith names slots results fuel argument))
        | .function index, _ => some (.call index (← nodeRawValueArgumentsWith names slots results fuel arguments))
        | _, _ => none
    | fuel + 1, .propagate value _ => do
        some (.propagate (← nodeRawValueTreeWith names slots results fuel value))
    | _ + 1, _ => none

  def nodeRawValueFieldsWith (names : RawNameBytes) (slots : List (BinderId × Nat)) (results : CalleeBinders) :
      Nat → List (NameId × Node) → Option RawValueFields
    | 0, _ => none
    | _ + 1, [] => some .nil
    | fuel + 1, (name, value) :: rest => do
        let key ← names name
        let tree ← nodeRawValueTreeWith names slots results fuel value
        let fields ← nodeRawValueFieldsWith names slots results fuel rest
        some (.cons key tree fields)

  def nodeRawValueBranchesWith (names : RawNameBytes) (slots : List (BinderId × Nat)) (results : CalleeBinders) :
      Nat → List (Node × Node) → Option RawValueBranches
    | 0, _ => none
    | _ + 1, [] => some .nil
    | fuel + 1, (condition, value) :: rest => do
        let guard ← nodeRawValueTreeWith names slots results fuel condition
        let tree ← nodeRawValueTreeWith names slots results fuel value
        let branches ← nodeRawValueBranchesWith names slots results fuel rest
        some (.cons guard tree branches)

  def nodeRawValueArgumentsWith (names : RawNameBytes) (slots : List (BinderId × Nat)) (results : CalleeBinders) :
      Nat → List Node → Option RawValueArguments
    | 0, _ => none
    | _ + 1, [] => some .nil
    | fuel + 1, value :: rest => do
        let tree ← nodeRawValueTreeWith names slots results fuel value
        let values ← nodeRawValueArgumentsWith names slots results fuel rest
        some (.cons tree values)
end

def noCalleeBinders : CalleeBinders := fun _ => none

def nodeRawValueTree (names : RawNameBytes) (slots : List (BinderId × Nat)) : Nat → Node → Option RawValueTree :=
  nodeRawValueTreeWith names slots noCalleeBinders

def rawValueCorrespondenceWith (fuel : Nat) (names : RawNameBytes) (slots : List (BinderId × Nat))
    (results : CalleeBinders) (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (candidate : Node) : Bool :=
  match readRawValueTree rows functions owner (slots.map Prod.snd) fuel row,
      nodeRawValueTreeWith names slots results fuel candidate with
  | some actual, some expected => decide (actual = expected)
  | _, _ => false

theorem raw_value_correspondence_with_sound (fuel : Nat) (names : RawNameBytes) (slots : List (BinderId × Nat))
    (results : CalleeBinders) (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (candidate : Node)
    (accepted : rawValueCorrespondenceWith fuel names slots results rows functions owner row candidate = true) :
    ∃ value, readRawValueTree rows functions owner (slots.map Prod.snd) fuel row = some value ∧
      nodeRawValueTreeWith names slots results fuel candidate = some value := by
  cases actual : readRawValueTree rows functions owner (slots.map Prod.snd) fuel row with
  | none => simp [rawValueCorrespondenceWith, actual] at accepted
  | some value =>
      cases expected : nodeRawValueTreeWith names slots results fuel candidate with
      | none => simp [rawValueCorrespondenceWith, actual, expected] at accepted
      | some node =>
          simp [rawValueCorrespondenceWith, actual, expected] at accepted
          subst node
          exact ⟨value, rfl, rfl⟩

def rawValueCorrespondence (fuel : Nat) (names : RawNameBytes) (slots : List (BinderId × Nat))
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat) (candidate : Node) : Bool :=
  rawValueCorrespondenceWith fuel names slots noCalleeBinders rows functions owner row candidate

theorem raw_value_correspondence_sound (fuel : Nat) (names : RawNameBytes) (slots : List (BinderId × Nat))
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat) (candidate : Node)
    (accepted : rawValueCorrespondence fuel names slots rows functions owner row candidate = true) :
    ∃ value, readRawValueTree rows functions owner (slots.map Prod.snd) fuel row = some value ∧
      nodeRawValueTree names slots fuel candidate = some value :=
  raw_value_correspondence_with_sound fuel names slots noCalleeBinders rows functions owner row candidate accepted

end Xsh.Conformance.Connected
