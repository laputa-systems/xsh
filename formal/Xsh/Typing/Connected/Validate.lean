import Xsh.Typing.Connected.Judgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

inductive ValidationError where
  | exhausted
  | invalidLiteral
  | invalidContext
  | unsupported
  | mismatch
  | unresolvedName
  deriving DecidableEq, Repr

def requireCheck (condition : Bool) : Except ValidationError Unit :=
  if condition then .ok () else .error .mismatch

def witnessSubstitution (arguments : List (Variable × MonoType)) : Substitution := fun id =>
  (arguments.lookup id).getD (.variable id)

def instantiationKeysValid (scheme : Scheme) (arguments : List (Variable × MonoType)) : Bool :=
  let keys := arguments.map Prod.fst
  decide keys.Nodup && decide scheme.quantified.Nodup &&
    decide (keys.length = scheme.quantified.length) &&
    scheme.quantified.all (fun id => decide (id ∈ keys))

def instantiationSortsValid (solution : Substitution) (arguments : List (Variable × MonoType)) : Bool :=
  arguments.all fun (id, ty) =>
    checksWellFormed (substitute solution ty) && decide (typeSort (substitute solution ty) = id.kind.sort)
abbrev BinderTypes := BinderId → Option MonoType

def checkAnnotation (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType) : Bool :=
  match declared with
  | none => true
  | some source => match annotation names source with
    | none => false
    | some annotated => decide (substitute solution annotated = ty)

theorem check_annotation_sound (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType)
    (accepted : checkAnnotation names solution declared ty = true) :
    AnnotationAgrees names solution declared ty := by
  cases declared with
  | none => trivial
  | some source =>
    cases found : annotation names source with
    | none => simp [checkAnnotation, found] at accepted
    | some annotated =>
      simp [checkAnnotation, found] at accepted
      exact ⟨annotated, found, accepted⟩

def checkCompletion (expected : Option MonoType) (actual : MonoType) : Bool :=
  match expected with
  | none => true
  | some ty => decide (ty = actual)

theorem check_completion_sound (expected : Option MonoType) (actual : MonoType)
    (accepted : checkCompletion expected actual = true) : CompletionAccepts expected actual := by
  cases expected with
  | none => exact Or.inl rfl
  | some ty =>
    simp [checkCompletion] at accepted
    exact Or.inr (by cases accepted; rfl)

abbrev ValidationContext := Option (TypeNames × MonoType × BinderTypes)

def sourceTypeContext : ValidationContext → Option (TypeNames × MonoType)
  | none => none
  | some (names, result, _) => some (names, result)

def nodeTypeContext : ValidationContext → Option MonoType
  | none => none
  | some (_, result, _) => some result

mutual
  def validateExpressionIn : Nat → ValidationContext → Scope → Substitution → Expr → Node → Except ValidationError MonoType
    | 0, _, _, _, _, _ => .error .exhausted
    | fuel + 1, context, scope, solution, source, candidate => do
      match source, candidate with
      | .literal (.integer text), .integer other => do
          if decimalLexeme text then pure () else throw .unsupported
          let some _ := parseInteger text | throw .invalidLiteral
          requireCheck (decide (text = other))
          pure (.primitive .integer)
      | .literal (.bool value), .boolean other => do
          requireCheck (decide (value = other))
          pure (.primitive .boolean)
      | .literal (.string value), .string other => do
          requireCheck (decide (value = other))
          pure (.primitive .string)
      | .ident name, .read binder arguments => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder))
          requireCheck (decide (binding.kind ≠ .schema))
          requireCheck (instantiationKeysValid binding.scheme arguments)
          requireCheck (instantiationSortsValid solution arguments)
          let instantiation := schemeSubstitution binding.scheme (witnessSubstitution arguments)
          let requirements := binding.scheme.requirements.map (mapConstraint instantiation)
          requireCheck (checksConstraints solution requirements)
          let ty := substitute solution (substitute instantiation binding.scheme.body)
          requireCheck (checksWellFormed ty && decide (typeSort ty = .value))
          pure ty
      | .binary operator left right, .binary recorded first second => do
          requireCheck (decide (operator = recorded))
          let some (leftType, rightType, result) := binarySignature operator | throw .unsupported
          let actualLeft ← validateExpressionIn fuel context scope solution left first
          let actualRight ← validateExpressionIn fuel context scope solution right second
          requireCheck (decide (actualLeft = leftType) && decide (actualRight = rightType))
          pure result
      | .call callee arguments, .call target recordedArguments => do
          let calleeType ← validateExpressionIn fuel context scope solution callee target
          let argumentTypes ← validateArgumentsIn fuel context scope solution arguments recordedArguments
          match calleeType with
          | .function parameters result => do
              requireCheck (decide (parameters = argumentTypes))
              pure result
          | _ => throw .mismatch
      | .record fields, .record recordedFields => do
          let types ← validateRecordFieldsIn fuel context scope solution fields recordedFields
          requireCheck (decide (types.map Prod.fst).Nodup)
          pure (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
      | .field base name, .field recorded recordedName => do
          requireCheck (decide (name = recordedName))
          let baseType ← validateExpressionIn fuel context scope solution base recorded
          match baseType with
          | .record row =>
              let some ty := fieldAt? name row | throw .mismatch
              pure ty
          | _ => throw .mismatch
      | .ifExpr branches otherwise, .conditional recorded fallback => do
          let result ← validateExpressionIn fuel context scope solution otherwise fallback
          validateBranchesIn fuel context scope solution result branches recorded
          pure result
      | .require value (some (.named name)), .requireValue binder identity recorded => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .schema))
          requireCheck (binding.scheme.quantified.isEmpty && binding.scheme.requirements.isEmpty)
          let .nominal expected (.record row) := binding.scheme.body | throw .mismatch
          requireCheck (decide (identity = expected) && checksWellFormed (.nominal expected (.record row)))
          let _ ← validateExpressionIn fuel context scope solution value recorded
          pure (.result (.nominal expected (.record (substitute solution row))) (.primitive .error))
      | .tryExpr value, .propagate recorded expectedError => do
          let result ← validateExpressionIn fuel context scope solution value recorded
          match result with
          | .result ty error => do
              requireCheck (decide (error = substitute solution expectedError))
              pure ty
          | _ => throw .mismatch
      | .valueBlock body, .valueBlock rawCompletion nodes => do
          let some (names, functionResult, binderTypes) := context | throw .unsupported
          let completion := substitute solution rawCompletion
          validateBlockCompletion fuel names scope solution binderTypes functionResult (some completion) body nodes
          pure completion
      | _, _ => throw .mismatch

  def validateArgumentsIn : Nat → ValidationContext → Scope → Substitution → List CallArgument → List Node → Except ValidationError MonoType
    | 0, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, [], [] => .ok .argumentsEmpty
    | fuel + 1, context, scope, solution, .positional value :: rest, node :: nodes => do
        let ty ← validateExpressionIn fuel context scope solution value node
        let others ← validateArgumentsIn fuel context scope solution rest nodes
        pure (.argument ty others)
    | _ + 1, _, _, _, _, _ => .error .mismatch

  def validateRecordFieldsIn : Nat → ValidationContext → Scope → Substitution → List RecordField → List (NameId × Node) →
      Except ValidationError (List (NameId × MonoType))
    | 0, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, [], [] => .ok []
    | fuel + 1, context, scope, solution, .named name value :: rest, (recordedName, node) :: nodes => do
        requireCheck (decide (name = recordedName))
        let ty ← validateExpressionIn fuel context scope solution value node
        let others ← validateRecordFieldsIn fuel context scope solution rest nodes
        pure ((name, ty) :: others)
    | _ + 1, _, _, _, _, _ => .error .mismatch

  def validateBranchesIn : Nat → ValidationContext → Scope → Substitution → MonoType → List (Expr × Expr) →
      List (Node × Node) → Except ValidationError Unit
    | 0, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, [], [] => .ok ()
    | fuel + 1, context, scope, solution, result, (condition, value) :: rest, (guard, node) :: nodes => do
        let guardType ← validateExpressionIn fuel context scope solution condition guard
        requireCheck (decide (guardType = .primitive .boolean))
        let branchType ← validateExpressionIn fuel context scope solution value node
        requireCheck (decide (branchType = result))
        validateBranchesIn fuel context scope solution result rest nodes
    | _ + 1, _, _, _, _, _, _ => .error .mismatch

  def validateBlockCompletion : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      Option MonoType → List Statement → List StatementNode → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, completion, [], [] => requireCheck (checkCompletion completion (.primitive .unit))
    | fuel + 1, names, scope, solution, binderTypes, result, completion, statement :: rest, node :: nodes => do
      let context := some (names, result, binderTypes)
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind storage candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          requireCheck (decide (storage = rawType))
          let valueType ← validateExpressionIn fuel context scope solution initializer candidate
          requireCheck (checksStoreCompatible (substitute solution rawType) valueType)
          requireCheck (checkAnnotation names solution declared (substitute solution rawType))
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateBlockCompletion fuel names (binding :: scope) solution binderTypes result completion rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateExpressionIn fuel context scope solution value candidate
          requireCheck (checksStoreCompatible (substitute solution binding.scheme.body) valueType)
          validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateExpressionIn fuel context scope solution value candidate
          requireCheck (decide (valueType = result))
          validateBlockCompletion fuel names scope solution binderTypes result none rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck (rest.isEmpty && nodes.isEmpty)
          let valueType ← validateExpressionIn fuel context scope solution (.ident name) candidate
          requireCheck (checkCompletion completion valueType)
      | .expression value, .evaluated candidate => do
          let valueType ← validateExpressionIn fuel context scope solution value candidate
          if rest.isEmpty then do
            requireCheck nodes.isEmpty
            requireCheck (checkCompletion completion valueType)
          else validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          let branchCompletion := if rest.isEmpty then completion else none
          validateStatementBranchesCompletion fuel names scope solution binderTypes result branchCompletion branches candidates
          match otherwise with
          | none => do
              requireCheck fallback.isEmpty
              requireCheck (checkCompletion branchCompletion (.primitive .unit))
          | some body => validateBlockCompletion fuel names scope solution binderTypes result branchCompletion body fallback
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateBlockCompletion fuel names scope solution binderTypes result completion rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _, _ => .error .mismatch

  def validateStatementBranchesCompletion : Nat → TypeNames → Scope → Substitution → BinderTypes → MonoType →
      Option MonoType → List (Expr × List Statement) → List (Node × List StatementNode) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, binderTypes, result, completion,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateExpressionIn fuel (some (names, result, binderTypes)) scope solution condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateBlockCompletion fuel names scope solution binderTypes result completion body nodes
      validateStatementBranchesCompletion fuel names scope solution binderTypes result completion rest candidates
    | _ + 1, _, _, _, _, _, _, _, _ => .error .mismatch
end

def validateExpression (fuel : Nat) (scope : Scope) (solution : Substitution) (source : Expr) (candidate : Node) :
    Except ValidationError MonoType := validateExpressionIn fuel none scope solution source candidate

def validateArguments (fuel : Nat) (scope : Scope) (solution : Substitution) (source : List CallArgument)
    (candidate : List Node) : Except ValidationError MonoType := validateArgumentsIn fuel none scope solution source candidate

def validateRecordFields (fuel : Nat) (scope : Scope) (solution : Substitution) (source : List RecordField)
    (candidate : List (NameId × Node)) : Except ValidationError (List (NameId × MonoType)) :=
  validateRecordFieldsIn fuel none scope solution source candidate

def validateBranches (fuel : Nat) (scope : Scope) (solution : Substitution) (result : MonoType)
    (source : List (Expr × Expr)) (candidate : List (Node × Node)) : Except ValidationError Unit :=
  validateBranchesIn fuel none scope solution result source candidate

theorem except_bind_ok {ε α β : Type} (action : Except ε α) (next : α → Except ε β) (out : β) :
    (action >>= next) = .ok out ↔ ∃ value, action = .ok value ∧ next value = .ok out := by
  cases action with
  | error error =>
    constructor
    · intro impossible; cases impossible
    · rintro ⟨_, impossible, _⟩; cases impossible
  | ok value =>
    constructor
    · intro result; exact ⟨value, rfl, result⟩
    · rintro ⟨_, equal, result⟩; cases equal; exact result

theorem except_map_ok {ε α β : Type} (f : α → β) (action : Except ε α) (out : β) :
    (f <$> action) = .ok out ↔ ∃ value, action = .ok value ∧ f value = out := by
  cases action with
  | error error =>
    constructor
    · intro impossible; cases impossible
    · rintro ⟨_, impossible, _⟩; cases impossible
  | ok value =>
    constructor
    · intro result; cases result; exact ⟨value, rfl, rfl⟩
    · rintro ⟨_, equal, result⟩; cases equal; cases result; rfl

theorem exists_require_ok (condition : Bool) :
    (∃ result, requireCheck condition = .ok result) ↔ condition = true := by
  cases condition <;> simp [requireCheck]

def ExpressionSound (fuel : Nat) : Prop := ∀ scope solution source node ty,
  validateExpression fuel scope solution source node = .ok ty → ExpressionHasType scope solution source ty

def ArgumentsSound (fuel : Nat) : Prop := ∀ scope solution arguments nodes types,
  validateArguments fuel scope solution arguments nodes = .ok types → ArgumentsHaveTypes scope solution arguments types

def FieldsSound (fuel : Nat) : Prop := ∀ scope solution fields nodes types,
  validateRecordFields fuel scope solution fields nodes = .ok types → RecordFieldsHaveTypes scope solution fields types

def BranchesSound (fuel : Nat) : Prop := ∀ scope solution result branches nodes,
  validateBranches fuel scope solution result branches nodes = .ok () → BranchesHaveType scope solution branches result

theorem validate_read_sound (fuel : Nat) (scope : Scope) (solution : Substitution) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.ident name) (.read binder arguments) = .ok ty) :
    ExpressionHasType scope solution (.ident name) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateExpression, validateExpressionIn, found] at accepted
  | some binding =>
    simp [validateExpression, validateExpressionIn, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, checks_constraints_iff] at accepted
    obtain ⟨_, notSchema, _, _, solved, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    exact .read found notSchema ⟨rfl, rfl⟩ solved wellFormed sorted

theorem validate_binary_sound (fuel : Nat) (ih : ExpressionSound fuel) (scope : Scope)
    (solution : Substitution) (operator recorded : BinaryOp) (left right : Expr)
    (first second : Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.binary operator left right)
      (.binary recorded first second) = .ok ty) :
    ExpressionHasType scope solution (.binary operator left right) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, accepted⟩ := accepted
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih scope solution left first _ leftAccepted)
      (ih scope solution right second _ rightAccepted) (binary_signature_sound found)

def identityCallSource : Expr :=
  .call (.ident identityBinding.name) [.positional (.literal (.integer "7"))]

def identityCallCandidate : Node :=
  .call (.read identityBinding.binder [(identityVariable, .variable ⟨10, .value⟩)]) [.integer "7"]

def identityCallSolution (id : Variable) : MonoType :=
  if id = ⟨10, .value⟩ then .primitive .integer else .variable id

def identityCallAccepted : Bool :=
  match validateExpression 12 [identityBinding] identityCallSolution identityCallSource identityCallCandidate with
  | .ok (.primitive .integer) => true
  | _ => false

theorem independent_validator_accepts_identity_call : identityCallAccepted = true := by
  decide +kernel

def changedIdentityBinding : Node :=
  .call (.read ⟨⟨1, by decide⟩⟩ [(identityVariable, .variable ⟨10, .value⟩)]) [.integer "7"]

def changedIdentityLiteral : Node :=
  .call (.read identityBinding.binder [(identityVariable, .variable ⟨10, .value⟩)]) [.integer "8"]

def rejectsIdentityCandidate (candidate : Node) : Bool :=
  match validateExpression 12 [identityBinding] identityCallSolution identityCallSource candidate with
  | .error .mismatch => true
  | _ => false

theorem independent_validator_rejects_changed_binding_and_literal :
    rejectsIdentityCandidate changedIdentityBinding = true ∧
      rejectsIdentityCandidate changedIdentityLiteral = true := by
  constructor <;> decide +kernel

end Xsh.Typing.Connected
