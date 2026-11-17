import Xsh.Conformance.Connected.RawStoreFixture
import Xsh.Conformance.Connected.RawDeclarations
import Xsh.Conformance.Connected.RawResultDocuments

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

def resultControlSource : String :=
  "pure through(r: Result[Int, Str]) -> Result[Int, Str] { return Ok(r?) }\npure refuse() -> Result[Int, Str] { return Err(\"no\") }\n"

structure ResultSignature where
  name : String
  body : SourceSite
  parameters : List (String × MonoType)
  result : MonoType
  expressions : List (SourceSite × MonoType)
  calls : List (SourceSite × String)
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

def decodeResultExpression (value : WireValue) : Option (SourceSite × MonoType) := do
  let fields ← value.closedObject? ["site", "type"]
  pure (← siteField fields "site", ← (WireValue.field? fields "type").bind (decodeResultType 8))

def decodeResultCall (value : WireValue) : Option (SourceSite × String) := do
  let fields ← value.closedObject? ["site", "function"]
  pure (← siteField fields "site", ← textField fields "function")

def decodeResultSignature (value : WireValue) : Option ResultSignature := do
  let fields ← value.closedObject? ["name", "body", "parameters", "result", "expressions", "calls"]
  pure ⟨← textField fields "name", ← siteField fields "body", ← listField fields "parameters" decodeResultParameter,
    ← (WireValue.field? fields "result").bind (decodeResultType 8),
    ← listField fields "expressions" decodeResultExpression, ← listField fields "calls" decodeResultCall⟩

def decodeResultSignatures (codes : ExchangeCodes) : Option (List ResultSignature) := do
  let value ← parseCodes codes
  let fields ← value.closedObject? ["schema", "functions"]
  if textField fields "schema" = some "xsh-connected-result-signatures-v3" then pure () else none
  listField fields "functions" decodeResultSignature

def resultOkBinder : BinderId := ⟨⟨10, by decide⟩⟩
def resultErrorBinder : BinderId := ⟨⟨11, by decide⟩⟩

def resultParameterLimit : Nat := 10
def resultFunctionBinderBase : Nat := 20

def resultCalleeBinders (count : Nat) : CalleeBinders := fun binder =>
  if binder = resultOkBinder then some .ok else if binder = resultErrorBinder then some .error
  else if resultFunctionBinderBase ≤ binder.value ∧ binder.value < resultFunctionBinderBase + count then
    some (.function (binder.value - resultFunctionBinderBase))
  else none

def resultTypeNames (unit : SourceUnit) : Option TypeNames := do
  pure ⟨← sourceName unit.names "Int", ← sourceName unit.names "Bool", ← sourceName unit.names "Str",
    ← sourceName unit.names "Result"⟩

def resultParameterBindingsFor (names : List String) (parameters : List (String × MonoType)) :
    Option (List Binding) :=
  (parameters.zipIdx).mapM fun ((text, ty), index) => do
    pure ⟨← sourceName names text, ← BinderId.decode? index, .immutable, ⟨[], ty, []⟩⟩

def resultParameterBindings (unit : SourceUnit) (parameters : List (String × MonoType)) : Option (List Binding) :=
  resultParameterBindingsFor unit.names parameters

def resultBuiltinBindings (unit : SourceUnit) : List Binding :=
  [("Ok", resultOkBinder, okScheme), ("Err", resultErrorBinder, errorScheme)].filterMap fun (text, binder, scheme) =>
    (sourceName unit.names text).map fun name => ⟨name, binder, .immutable, scheme⟩

def resultFunctionType (signature : ResultSignature) : MonoType :=
  .function (signature.parameters.foldr (fun (_, ty) rest => .argument ty rest) .argumentsEmpty) signature.result

def resultFunctionBindingsFor (names : List String) (signatures : List ResultSignature) : Option (List Binding) :=
  signatures.zipIdx.mapM fun (signature, index) => do
    pure ⟨← sourceName names signature.name, ← BinderId.decode? (resultFunctionBinderBase + index), .immutable,
      ⟨[], resultFunctionType signature, []⟩⟩

def resultScope (unit : SourceUnit) (signatures : List ResultSignature) (signature : ResultSignature) :
    Option Scope := do
  pure ((← resultParameterBindings unit signature.parameters) ++ resultBuiltinBindings unit ++
    (← resultFunctionBindingsFor unit.names signatures))

def validateResultFunction (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep) : Except ValidationError Unit := do
  let some signature := signatures[index]? | throw .mismatch
  let some (.function source) := unit.declarations[index]? | throw .mismatch
  let some names := resultTypeNames unit | throw .unsupported
  let some parameters := resultParameterBindings unit signature.parameters | throw .mismatch
  let some scope := resultScope unit signatures signature | throw .mismatch
  requireCheck (decide (sourceName unit.names signature.name = some source.name) &&
    decide (signature.body = source.bodySite) && decide (source.kind = .pure) &&
    source.declaredEffects.isNone && source.regions.isEmpty &&
    decide (signature.parameters.length ≤ resultParameterLimit))
  validateSourceParameters names identity source.parameters parameters
  validateSourceAnnotation names identity source.result signature.result
  validateDerivedBlock 48 names scope signature.result source.body candidate (fun _ => none) steps

def returnedNode : List StatementNode → Option Node
  | [.returned value] => some value
  | _ => none

mutual
  def alignedNodes : Nat → Expr → Node → Option (List (Expr × Node))
    | 0, _, _ => none
    | fuel + 1, source@(.call (.ident _) arguments), node@(.call (.read _ _) nodes) => do
        pure ((source, node) :: (← alignedArguments fuel arguments nodes))
    | fuel + 1, source@(.tryExpr inner), node@(.propagate innerNode _) => do
        pure ((source, node) :: (← alignedNodes fuel inner innerNode))
    | fuel + 1, source@(.binary _ left right), node@(.binary _ leftNode rightNode) => do
        pure ((source, node) :: ((← alignedNodes fuel left leftNode) ++ (← alignedNodes fuel right rightNode)))
    | _ + 1, source@(.literal _), node => some [(source, node)]
    | _ + 1, source@(.ident _), node => some [(source, node)]
    | _ + 1, _, _ => none

  def alignedArguments : Nat → List CallArgument → List Node → Option (List (Expr × Node))
    | 0, _, _ => none
    | _ + 1, [], [] => some []
    | fuel + 1, .positional source :: sources, node :: nodes => do
        pure ((← alignedNodes fuel source node) ++ (← alignedArguments fuel sources nodes))
    | _ + 1, _, _ => none
end

def withoutCallees : List (SourceSite × Expr) → List SourceSite
  | [] => []
  | (site, .call (.ident _) _) :: _ :: rest => site :: withoutCallees rest
  | (site, _) :: rest => site :: withoutCallees rest

def calleeSites : List (SourceSite × Expr) → List (Option SourceSite)
  | [] => []
  | (_, .call (.ident _) _) :: (callee, _) :: rest => some callee :: calleeSites rest
  | _ :: rest => none :: calleeSites rest

def returnedSource : List Statement → Option Expr
  | [.returnValue (some value)] => some value
  | _ => none

def resultExpressionTypes (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep) :
    Option (List SourceSite × List MonoType) := do
  let signature ← signatures[index]?
  let some (.function source) := unit.declarations[index]? | none
  let names ← resultTypeNames unit
  let scope ← resultScope unit signatures signature
  let (_, state) ← ((block 48 names scope signature.result source.body).run
    (initialBlockState scope signature.result)).toOption
  let solution ← (checkDerivation steps state.constraints).toOption
  let expression ← returnedSource source.body
  let node ← returnedNode candidate
  let pairs ← alignedNodes 32 expression node
  let types ← (pairs.mapM fun (expression, node) => validateExpression 48 scope solution expression node).toOption
  pure (withoutCallees source.expressions, types)

def checkResultExpressionTypes (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep) : Bool :=
  match signatures[index]?, resultExpressionTypes unit signatures index candidate steps with
  | some signature, some (sites, types) => decide (sites = signature.expressions.map Prod.fst) &&
      decide (types = signature.expressions.map Prod.snd)
  | _, _ => false

theorem checked_expression_types_are_validated (unit : SourceUnit) (signatures : List ResultSignature)
    (index : Nat) (candidate : List StatementNode) (steps : List DerivationStep)
    (checked : checkResultExpressionTypes unit signatures index candidate steps = true) :
    ∃ signature sites types, signatures[index]? = some signature ∧
      resultExpressionTypes unit signatures index candidate steps = some (sites, types) ∧
      sites = signature.expressions.map Prod.fst ∧ types = signature.expressions.map Prod.snd := by
  unfold checkResultExpressionTypes at checked
  cases signatureFound : signatures[index]? with
  | none => simp [signatureFound] at checked
  | some signature =>
  cases found : resultExpressionTypes unit signatures index candidate steps with
  | none => simp [signatureFound, found] at checked
  | some pair =>
    obtain ⟨sites, types⟩ := pair
    simp only [signatureFound, found, Bool.and_eq_true, decide_eq_true_eq] at checked
    exact ⟨signature, sites, types, rfl, rfl, checked.1, checked.2⟩

def calleeTarget (callees : CalleeBinders) (signatures : List ResultSignature) :
    Option SourceSite → Node → Option (Option (SourceSite × String))
  | some site, .call (.read binder _) _ =>
      match callees binder with
      | some (.function index) => (signatures[index]?).map fun signature => some (site, signature.name)
      | some .ok | some .error => some none
      | none => none
  | some _, _ => none
  | none, .call _ _ => none
  | none, _ => some none

def resultCallTargets (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) : Option (List (SourceSite × String)) := do
  let some (.function source) := unit.declarations[index]? | none
  let expression ← returnedSource source.body
  let node ← returnedNode candidate
  let pairs ← alignedNodes 32 expression node
  let callees := calleeSites source.expressions
  if callees.length = pairs.length then pure () else none
  let targets ← (callees.zip (pairs.map Prod.snd)).mapM fun (site, node) =>
    calleeTarget (resultCalleeBinders signatures.length) signatures site node
  pure targets.reduceOption

def checkResultCallTargets (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) : Bool :=
  match signatures[index]?, resultCallTargets unit signatures index candidate with
  | some signature, some targets => decide (targets = signature.calls)
  | _, _ => false

theorem mapM_validation_types_each (fuel : Nat) (scope : Scope) (solution : Substitution)
    (pairs : List (Expr × Node)) (types : List MonoType)
    (accepted : pairs.mapM (fun (expression, node) => validateExpression fuel scope solution expression node) =
      .ok types) :
    pairs.length = types.length ∧
      ∀ entry ∈ pairs.zip types, ExpressionHasType scope solution entry.1.1 entry.2 := by
  induction pairs generalizing types with
  | nil =>
    simp [List.mapM_nil] at accepted
    cases accepted
    simp
  | cons pair rest ih =>
    obtain ⟨expression, node⟩ := pair
    simp only [List.mapM_cons] at accepted
    cases first : validateExpression fuel scope solution expression node with
    | error error => simp [first] at accepted; cases accepted
    | ok ty =>
      cases others : rest.mapM (fun (expression, node) => validateExpression fuel scope solution expression node) with
      | error error => simp [first, others] at accepted; cases accepted
      | ok tys =>
        simp [first, others] at accepted
        cases accepted
        obtain ⟨lengths, typed⟩ := ih tys others
        refine ⟨by simp [lengths], ?_⟩
        intro entry member
        simp only [List.zip_cons_cons, List.mem_cons] at member
        rcases member with same | later
        · subst same
          exact accepted_expression_has_type fuel scope solution expression node ty first
        · exact typed entry later

def rawFunctionReturn (rows : RawInstructionRows) (functions : RawFunctionRows) (index : Nat) : Option Nat := do
  let function ← functions.functions[index]?
  let statements ← readRawStatementBlock rows functions (index + 1) function.body
  let [statement] := statements | none
  let instruction ← readRawInstruction rows statement
  if instruction.tag = ⟨"StmtReturn", 150⟩ then pure () else none
  let [value] := instruction.payload | none
  if value < statement then some value else none

def namesRawBytes (names : List String) : RawNameBytes := fun name => (names[name.value]?).map rawBytes

def rawResultReturnCheckFor (names : List String) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (table : List RawParameter) (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode) :
    Bool :=
  match signatures[index]?, functions.functions[index]? with
  | some signature, some function =>
      match resultParameterBindingsFor names signature.parameters, returnedNode candidate,
          rawFunctionReturn rows functions index with
      | some parameters, some node, some row =>
          decide (function.parameters.length = parameters.length) &&
          rawParametersAuthenticated (namesRawBytes names) rows table function parameters &&
          decide (readRawStringBytes rows function.name = some (rawBytes signature.name)) &&
          rawValueCorrespondenceWith 16 (namesRawBytes names)
            (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot))
            (resultCalleeBinders signatures.length) rows functions (index + 1) row node
      | _, _, _ => false
  | _, _ => false

def rawResultReturnCheck (unit : SourceUnit) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (table : List RawParameter) (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode) :
    Bool :=
  rawResultReturnCheckFor unit.names rows functions table signatures index candidate

def certifyResultFunction (unit : SourceUnit) (rows : RawInstructionRows) (functions : RawFunctionRows)
    (table : List RawParameter) (signatures : List ResultSignature) (index : Nat) (candidate : List StatementNode)
    (steps : List DerivationStep) : Bool :=
  (match validateResultFunction unit signatures index candidate steps with | .ok _ => true | .error _ => false) &&
    rawResultReturnCheck unit rows functions table signatures index candidate &&
    checkResultExpressionTypes unit signatures index candidate steps &&
    checkResultCallTargets unit signatures index candidate

theorem certified_result_function_components (unit : SourceUnit) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (table : List RawParameter) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep)
    (certified : certifyResultFunction unit rows functions table signatures index candidate steps = true) :
    validateResultFunction unit signatures index candidate steps = .ok () ∧
      rawResultReturnCheck unit rows functions table signatures index candidate = true ∧
      checkResultExpressionTypes unit signatures index candidate steps = true ∧
      checkResultCallTargets unit signatures index candidate = true := by
  unfold certifyResultFunction at certified
  cases validated : validateResultFunction unit signatures index candidate steps with
  | error error => simp [validated] at certified
  | ok unitValue =>
    cases unitValue
    simp [validated] at certified
    exact ⟨rfl, certified.1.1, certified.1.2, certified.2⟩

theorem validated_result_function_is_typed (unit : SourceUnit) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode) (steps : List DerivationStep)
    (validated : validateResultFunction unit signatures index candidate steps = .ok ()) :
    ∃ signature source names scope,
      signatures[index]? = some signature ∧
      unit.declarations[index]? = some (.function source) ∧
      resultTypeNames unit = some names ∧
      resultScope unit signatures signature = some scope ∧
      ∃ generated state solution,
        (block 48 names scope signature.result source.body).run
          (initialBlockState scope signature.result) = .ok (generated, state) ∧
        checkDerivation steps state.constraints = .ok solution ∧
        BlockHasType names solution scope (substitute solution signature.result) source.body := by
  unfold validateResultFunction at validated
  cases signatureFound : signatures[index]? with
  | none => simp [signatureFound] at validated
  | some signature =>
  cases declaration : unit.declarations[index]? with
  | none => simp [signatureFound, declaration] at validated
  | some entry =>
  cases entry with
  | statement _ _ _ => simp [signatureFound, declaration] at validated
  | function source =>
  cases namesFound : resultTypeNames unit with
  | none => simp [signatureFound, declaration, namesFound] at validated
  | some names =>
  cases parametersFound : resultParameterBindings unit signature.parameters with
  | none => simp [signatureFound, declaration, namesFound, parametersFound] at validated
  | some parameters =>
  cases scopeFound : resultScope unit signatures signature with
  | none => simp [signatureFound, declaration, namesFound, parametersFound, scopeFound] at validated
  | some scope =>
  simp only [signatureFound, declaration, namesFound, parametersFound, scopeFound, except_bind_ok] at validated
  obtain ⟨_, _, _, _, _, _, derived⟩ := validated
  obtain ⟨generated, state, solution, run, derivation, _, _, _, typed⟩ :=
    derived_block_certificate_sound 48 names scope signature.result source.body candidate
      (fun _ => none) steps derived
  exact ⟨signature, source, names, scope, rfl, rfl, rfl, scopeFound, generated, state, solution, run, derivation, typed⟩

theorem raw_result_return_check_matches_the_checked_node (names : List String) (rows : RawInstructionRows)
    (functions : RawFunctionRows) (table : List RawParameter) (signatures : List ResultSignature) (index : Nat)
    (candidate : List StatementNode)
    (checked : rawResultReturnCheckFor names rows functions table signatures index candidate = true) :
    ∃ signature function parameters node row tree,
      signatures[index]? = some signature ∧
      functions.functions[index]? = some function ∧
      readRawStringBytes rows function.name = some (rawBytes signature.name) ∧
      resultParameterBindingsFor names signature.parameters = some parameters ∧
      (∀ slot binding, parameters[slot]? = some binding →
        RawParameterNamesBinding (namesRawBytes names) rows table function slot binding) ∧
      returnedNode candidate = some node ∧
      rawFunctionReturn rows functions index = some row ∧
      readRawValueTree rows functions (index + 1) (parameters.zipIdx.map fun (_, slot) => slot) 16 row = some tree ∧
      nodeRawValueTreeWith (namesRawBytes names)
        (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot))
        (resultCalleeBinders signatures.length) 16 node = some tree := by
  unfold rawResultReturnCheckFor at checked
  cases signatureFound : signatures[index]? with
  | none => simp [signatureFound] at checked
  | some signature =>
  cases functionFound : functions.functions[index]? with
  | none => simp [signatureFound, functionFound] at checked
  | some function =>
  cases parametersFound : resultParameterBindingsFor names signature.parameters with
  | none => simp [signatureFound, functionFound, parametersFound] at checked
  | some parameters =>
  cases nodeFound : returnedNode candidate with
  | none => simp [signatureFound, functionFound, parametersFound, nodeFound] at checked
  | some node =>
  cases rowFound : rawFunctionReturn rows functions index with
  | none => simp [signatureFound, functionFound, parametersFound, nodeFound, rowFound] at checked
  | some row =>
  simp only [signatureFound, functionFound, parametersFound, nodeFound, rowFound, Bool.and_eq_true,
    decide_eq_true_eq] at checked
  obtain ⟨⟨⟨_, authenticated⟩, named⟩, checked⟩ := checked
  obtain ⟨tree, actual, expected⟩ := raw_value_correspondence_with_sound 16 (namesRawBytes names)
    (parameters.zipIdx.map fun (binding, slot) => (binding.binder, slot)) (resultCalleeBinders signatures.length)
    rows functions (index + 1) row node checked
  refine ⟨signature, function, parameters, node, row, tree, rfl, rfl, named, parametersFound,
    raw_parameters_authenticated_sound (namesRawBytes names) rows table function parameters authenticated,
    rfl, rfl, ?_, expected⟩
  simpa [List.map_map, Function.comp_def] using actual

end Xsh.Conformance.Connected
