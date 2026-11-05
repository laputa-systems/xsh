import Xsh.Conformance.Connected.RawStoreFixture
import Xsh.Conformance.Connected.RawExpression
import Xsh.Conformance.Connected.RawResultDocuments

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultControlSource : String :=
  "pure keep(a: Int) -> Result[Int, Str] {\n  return Ok(a)\n}\npure refuse() -> Result[Int, Str] {\n  return Err(\"no\")\n}\npure through(r: Result[Int, Str]) -> Result[Int, Str] {\n  return Ok(r?)\n}\npure advance(r: Result[Int, Str]) -> Result[Int, Str] {\n  return Ok(r? + 1)\n}\n"

structure ResultSignature where
  name : String
  body : SourceSite
  parameters : List (String × MonoType)
  result : MonoType
  deriving DecidableEq

def decodeResultType : Nat → WireValue → Option MonoType
  | 0, _ => none
  | fuel + 1, value => do
      let .object fields := value | none
      match textField fields "kind" with
      | some "int" => do
          let _ ← value.closedObject? ["kind"]
          some (.primitive .integer)
      | some "str" => do
          let _ ← value.closedObject? ["kind"]
          some (.primitive .string)
      | some "result" => do
          let fields ← value.closedObject? ["kind", "value", "error"]
          let left ← (WireValue.field? fields "value").bind (decodeResultType fuel)
          let right ← (WireValue.field? fields "error").bind (decodeResultType fuel)
          some (.result left right)
      | _ => none

def decodeResultParameter (value : WireValue) : Option (String × MonoType) := do
  let fields ← value.closedObject? ["name", "type"]
  pure (← textField fields "name", ← (WireValue.field? fields "type").bind (decodeResultType 8))

def decodeResultSignature (value : WireValue) : Option ResultSignature := do
  let fields ← value.closedObject? ["name", "body", "parameters", "result"]
  pure ⟨← textField fields "name", ← siteField fields "body", ← listField fields "parameters" decodeResultParameter,
    ← (WireValue.field? fields "result").bind (decodeResultType 8)⟩

def decodeResultSignatures (codes : ExchangeCodes) : Option (List ResultSignature) := do
  let value ← parseCodes codes
  let fields ← value.closedObject? ["schema", "functions"]
  if textField fields "schema" = some "xsh-connected-result-signatures-v1" then pure () else none
  listField fields "functions" decodeResultSignature

def resultOkBinder : BinderId := ⟨⟨10, by decide⟩⟩
def resultErrorBinder : BinderId := ⟨⟨11, by decide⟩⟩

def resultBinders : ResultBinders := fun binder =>
  if binder = resultOkBinder then some .ok else if binder = resultErrorBinder then some .error else none

def resultTypeNames (unit : SourceUnit) : Option TypeNames := do
  pure ⟨← sourceName unit.names "Int", ← sourceName unit.names "Bool", ← sourceName unit.names "Str",
    ← sourceName unit.names "Result"⟩

def resultParameterBindingsFor (names : List String) (parameters : List (String × MonoType)) :
    Option (List Binding) :=
  (parameters.zipIdx).mapM fun ((text, ty), index) => do
    pure ⟨← sourceName names text, ← BinderId.decode? index, .immutable, ⟨[], ty, []⟩⟩

def resultParameterBindings (unit : SourceUnit) (parameters : List (String × MonoType)) : Option (List Binding) :=
  resultParameterBindingsFor unit.names parameters

def resultBuiltinBindings (unit : SourceUnit) : Option (List Binding) := do
  pure [⟨← sourceName unit.names "Ok", resultOkBinder, .immutable, okScheme⟩,
    ⟨← sourceName unit.names "Err", resultErrorBinder, .immutable, errorScheme⟩]

def validateResultFunction (unit : SourceUnit) (signature : ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep) : Except ValidationError Unit := do
  let some (.function source) := unit.declarations[index]? | throw .mismatch
  let some names := resultTypeNames unit | throw .unsupported
  let some parameters := resultParameterBindings unit signature.parameters | throw .mismatch
  let some builtins := resultBuiltinBindings unit | throw .unsupported
  requireCheck (decide (sourceName unit.names signature.name = some source.name) &&
    decide (signature.body = source.bodySite) && decide (source.kind = .pure) &&
    source.declaredEffects.isNone && source.regions.isEmpty)
  validateSourceParameters names identity source.parameters parameters
  validateSourceAnnotation names identity source.result signature.result
  validateDerivedBlock 48 names (parameters ++ builtins) signature.result source.body candidate (fun _ => none) steps

def rawFunctionReturn (rows : RawInstructionRows) (functions : RawFunctionRows) (index : Nat) : Option Nat := do
  let function ← functions.functions[index]?
  let statements ← readRawStatementBlock rows functions (index + 1) function.body
  let [statement] := statements | none
  let instruction ← readRawInstruction rows statement
  if instruction.tag = ⟨"StmtReturn", 150⟩ then pure () else none
  let [value] := instruction.payload | none
  if value < statement then some value else none

def returnedNode : List StatementNode → Option Node
  | [.returned value] => some value
  | _ => none

def namesRawBytes (names : List String) : RawNameBytes := fun name => (names[name.value]?).map rawBytes

def rawResultReturnCheckFor (names : List String) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (signature : ResultSignature) (index : Nat) (candidate : List StatementNode) : Bool :=
  match resultParameterBindingsFor names signature.parameters, returnedNode candidate,
      rawFunctionReturn rows functions index with
  | some parameters, some node, some row =>
      decide ((functions.functions[index]?).map (·.parameters.length) = some parameters.length) &&
      rawValueCorrespondenceWith 16 (namesRawBytes names)
        (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot)) resultBinders rows functions
        (index + 1) row node
  | _, _, _ => false

def rawResultReturnCheck (unit : SourceUnit) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (signature : ResultSignature) (index : Nat) (candidate : List StatementNode) : Bool :=
  rawResultReturnCheckFor unit.names rows functions signature index candidate

def certifyResultFunction (unit : SourceUnit) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (signature : ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  (match validateResultFunction unit signature index candidate steps with | .ok _ => true | .error _ => false) &&
    rawResultReturnCheck unit rows functions signature index candidate

theorem certified_result_function_components (unit : SourceUnit) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (signature : ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep)
    (certified : certifyResultFunction unit rows functions signature index candidate steps = true) :
    validateResultFunction unit signature index candidate steps = .ok () ∧
      rawResultReturnCheck unit rows functions signature index candidate = true := by
  unfold certifyResultFunction at certified
  cases validated : validateResultFunction unit signature index candidate steps with
  | error error => simp [validated] at certified
  | ok unitValue =>
    cases unitValue
    simp [validated] at certified
    exact ⟨rfl, certified⟩

theorem validated_result_function_is_typed (unit : SourceUnit) (signature : ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep)
    (validated : validateResultFunction unit signature index candidate steps = .ok ()) :
    ∃ source names parameters builtins,
      unit.declarations[index]? = some (.function source) ∧
      resultTypeNames unit = some names ∧
      resultParameterBindings unit signature.parameters = some parameters ∧
      resultBuiltinBindings unit = some builtins ∧
      ∃ generated state solution,
        (block 48 names (parameters ++ builtins) signature.result source.body).run
          (initialBlockState (parameters ++ builtins) signature.result) = .ok (generated, state) ∧
        checkDerivation steps state.constraints = .ok solution ∧
        BlockHasType names solution (parameters ++ builtins) (substitute solution signature.result) source.body := by
  unfold validateResultFunction at validated
  cases declaration : unit.declarations[index]? with
  | none => simp [declaration] at validated
  | some entry =>
  cases entry with
  | statement _ _ _ => simp [declaration] at validated
  | function source =>
  cases namesFound : resultTypeNames unit with
  | none => simp [declaration, namesFound] at validated
  | some names =>
  cases parametersFound : resultParameterBindings unit signature.parameters with
  | none => simp [declaration, namesFound, parametersFound] at validated
  | some parameters =>
  cases builtinsFound : resultBuiltinBindings unit with
  | none => simp [declaration, namesFound, parametersFound, builtinsFound] at validated
  | some builtins =>
  simp only [declaration, namesFound, parametersFound, builtinsFound, except_bind_ok, exists_require_ok] at validated
  obtain ⟨_, _, _, _, _, _, derived⟩ := validated
  obtain ⟨generated, state, solution, run, derivation, _, _, _, typed⟩ :=
    derived_block_certificate_sound 48 names (parameters ++ builtins) signature.result source.body candidate
      (fun _ => none) steps derived
  exact ⟨source, names, parameters, builtins, rfl, rfl, rfl, rfl, generated, state, solution, run, derivation, typed⟩

theorem raw_result_return_check_matches_the_checked_node (names : List String) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (signature : ResultSignature) (index : Nat) (candidate : List StatementNode)
    (checked : rawResultReturnCheckFor names rows functions signature index candidate = true) :
    ∃ parameters node row tree,
      resultParameterBindingsFor names signature.parameters = some parameters ∧
      returnedNode candidate = some node ∧
      rawFunctionReturn rows functions index = some row ∧
      readRawValueTree rows functions (index + 1) (parameters.zipIdx.map fun (_, slot) => slot) 16 row = some tree ∧
      nodeRawValueTreeWith (namesRawBytes names)
        (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot)) resultBinders 16 node = some tree := by
  unfold rawResultReturnCheckFor at checked
  cases parametersFound : resultParameterBindingsFor names signature.parameters with
  | none => simp [parametersFound] at checked
  | some parameters =>
  cases nodeFound : returnedNode candidate with
  | none => simp [parametersFound, nodeFound] at checked
  | some node =>
  cases rowFound : rawFunctionReturn rows functions index with
  | none => simp [parametersFound, nodeFound, rowFound] at checked
  | some row =>
  simp only [parametersFound, nodeFound, rowFound, Bool.and_eq_true] at checked
  obtain ⟨_, checked⟩ := checked
  obtain ⟨tree, actual, expected⟩ := raw_value_correspondence_with_sound 16 (namesRawBytes names)
    (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot)) resultBinders rows functions (index + 1)
    row node checked
  refine ⟨parameters, node, row, tree, rfl, rfl, rfl, ?_, expected⟩
  simpa [List.map_map, Function.comp_def] using actual

end Xsh.Conformance.Connected
