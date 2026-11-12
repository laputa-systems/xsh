import Xsh.Typing.Connected.QualifiedValidate
import Xsh.Typing.Connected.QualifiedNode

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def CQualifiedExpressionSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedExpressionIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedExpressionIn principal requirements (sourceTypeContext context) scope source ty
def CQualifiedArgumentsSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedArgumentsIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedArgumentsIn principal requirements (sourceTypeContext context) scope source ty
def CQualifiedFieldsSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedFieldsIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedRecordFieldsIn principal requirements (sourceTypeContext context) scope source ty
def CQualifiedBranchesSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields result source candidate,
 validateQualifiedBranchesIn fuel context scope principal requirements fields result source candidate = .ok () → QualifiedBranchesIn principal requirements (sourceTypeContext context) scope source result
theorem c_validate_qualified_read_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.ident name) (.read binder arguments) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.ident name) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpressionIn, found] at accepted
  | some binding =>
    simp [validateQualifiedExpressionIn, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, List.all_eq_true] at accepted
    obtain ⟨_, notSchema, _, _, included, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    apply QualifiedExpressionIn.read found notSchema ⟨rfl, rfl⟩ _ wellFormed sorted
    intro constraint member
    obtain ⟨original, present, rfl⟩ := List.mem_map.mp member
    exact included original present

theorem c_validate_qualified_field_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (base : Expr) (name recordedName : NameId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.field base name) (.field node recordedName) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.field base name) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, baseType, baseAccepted, rest⟩ := accepted
  cases found : fields (.field node recordedName) with
  | none => simp [found] at rest
  | some result =>
    simp [found, except_map_ok, exists_require_ok] at rest
    obtain ⟨member, rfl⟩ := rest
    exact .field (ih context scope principal requirements fields base node _ baseAccepted) member

theorem c_validate_qualified_call_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedExpressionSound fuel)
    (argsIH : CQualifiedArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (callee : Expr) (arguments : List CallArgument)
    (target : Node) (nodes : List Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.call callee arguments) (.call target nodes) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.call callee arguments) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok] at accepted
  obtain ⟨calleeType, calleeAccepted, argumentTypes, argumentsAccepted, rest⟩ := accepted
  cases calleeType <;> simp [except_map_ok, exists_require_ok] at rest
  case function parameters result =>
    obtain ⟨same, rfl⟩ := rest
    subst parameters
    exact .call (exprIH context scope principal requirements fields callee target _ calleeAccepted)
      (argsIH context scope principal requirements fields arguments nodes _ argumentsAccepted)

theorem c_validate_qualified_record_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedFieldsSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : List RecordField) (nodes : List (NameId × Node)) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.record source) (.record nodes) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.record source) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨types, fieldsAccepted, nodup, rfl⟩ := accepted
  exact .record (ih context scope principal requirements fields source nodes types fieldsAccepted) nodup

theorem c_validate_qualified_conditional_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedExpressionSound fuel)
    (branchesIH : CQualifiedBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (branches : List (Expr × Expr))
    (otherwise : Expr) (nodes : List (Node × Node)) (fallback : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.ifExpr branches otherwise) (.conditional nodes fallback) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.ifExpr branches otherwise) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨fallbackAccepted, _, branchAccepted⟩ := accepted
  exact .conditional (branchesIH context scope principal requirements fields ty branches nodes branchAccepted)
    (exprIH context scope principal requirements fields otherwise fallback ty fallbackAccepted)

theorem c_validate_qualified_propagate_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (node : Node) (expectedError ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.tryExpr value) (.propagate node expectedError) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.tryExpr value) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok] at accepted
  obtain ⟨valueType, valueAccepted, rest⟩ := accepted
  cases valueType <;> simp at rest
  case result inner error =>
    simp [except_map_ok, exists_require_ok] at rest
    obtain ⟨_, rfl⟩ := rest
    exact .propagate (ih context scope principal requirements fields value node _ valueAccepted)

theorem c_validate_qualified_require_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (name : NameId) (binder : BinderId)
    (identity : Xsh.Validation.Nominal.NominalId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.require value (some (.named name))) (.requireValue binder identity node) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.require value (some (.named name))) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpressionIn, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [validateQualifiedExpressionIn, found, body, except_bind_ok, except_map_ok, exists_require_ok,
          Bool.and_eq_true, checks_well_formed_iff] at accepted
        obtain ⟨⟨_, kind⟩, ⟨quantified, empty⟩, ⟨_, wellFormed⟩, ⟨inner, innerAccepted⟩, rfl⟩ := accepted
        exact .requireValue found kind quantified empty body wellFormed
          (ih context scope principal requirements fields value node inner innerAccepted)
      | _ => simp [validateQualifiedExpressionIn, found, body, except_bind_ok] at accepted
    | _ => simp [validateQualifiedExpressionIn, found, body, except_bind_ok] at accepted

theorem c_validate_qualified_argument_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedExpressionSound fuel)
    (argsIH : CQualifiedArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value : Expr) (rest : List CallArgument)
    (node : Node) (nodes : List Node) (types : MonoType)
    (accepted : validateQualifiedArgumentsIn (fuel + 1) context scope principal requirements fields
      (.positional value :: rest) (node :: nodes) = .ok types) :
    QualifiedArgumentsIn principal requirements (sourceTypeContext context) scope (.positional value :: rest) types := by
  simp [validateQualifiedArgumentsIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH context scope principal requirements fields value node _ firstAccepted)
    (argsIH context scope principal requirements fields rest nodes _ othersAccepted)

theorem c_validate_qualified_field_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedExpressionSound fuel)
    (fieldsIH : CQualifiedFieldsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name recorded : NameId) (value : Expr)
    (rest : List RecordField) (node : Node) (nodes : List (NameId × Node)) (types : List (NameId × MonoType))
    (accepted : validateQualifiedFieldsIn (fuel + 1) context scope principal requirements fields
      (.named name value :: rest) ((recorded, node) :: nodes) = .ok types) :
    QualifiedRecordFieldsIn principal requirements (sourceTypeContext context) scope (.named name value :: rest) types := by
  simp [validateQualifiedFieldsIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH context scope principal requirements fields value node _ firstAccepted)
    (fieldsIH context scope principal requirements fields rest nodes _ othersAccepted)

theorem c_validate_qualified_branch_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedExpressionSound fuel)
    (branchesIH : CQualifiedBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (result : MonoType)
    (condition value : Expr) (rest : List (Expr × Expr)) (guard node : Node) (nodes : List (Node × Node))
    (accepted : validateQualifiedBranchesIn (fuel + 1) context scope principal requirements fields result
      ((condition, value) :: rest) ((guard, node) :: nodes) = .ok ()) :
    QualifiedBranchesIn principal requirements (sourceTypeContext context) scope ((condition, value) :: rest) result := by
  simp [validateQualifiedBranchesIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, _, valueAccepted, rfl, remainingAccepted⟩ := accepted
  exact .cons (exprIH context scope principal requirements fields condition guard _ guardAccepted)
    (exprIH context scope principal requirements fields value node _ valueAccepted)
    (branchesIH context scope principal requirements fields _ rest nodes remainingAccepted)

theorem c_validate_qualified_integer_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (lexeme candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.integer lexeme)) (.integer candidate) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.literal (.integer lexeme)) ty := by
  cases decimal : decimalLexeme lexeme with
  | false =>
    simp only [validateQualifiedExpressionIn, decimal, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    cases parsed : parseInteger lexeme with
    | none => simp [validateQualifiedExpressionIn, decimal, parsed] at accepted
    | some value =>
      simp [validateQualifiedExpressionIn, decimal, parsed, except_map_ok, exists_require_ok] at accepted
      obtain ⟨_, rfl⟩ := accepted
      exact .integer parsed

theorem c_validate_qualified_boolean_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : Bool) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.bool value)) (.boolean candidate) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.literal (.bool value)) ty := by
  simp [validateQualifiedExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .boolean

theorem c_validate_qualified_string_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.string value)) (.string candidate) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.literal (.string value)) ty := by
  simp [validateQualifiedExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .string

theorem c_validate_qualified_binary_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (operator recorded : BinaryOp) (left right : Expr) (first second : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.binary operator left right) (.binary recorded first second) = .ok ty) :
    QualifiedExpressionIn principal requirements (sourceTypeContext context) scope (.binary operator left right) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, accepted⟩ := accepted
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih context scope principal requirements fields left first _ leftAccepted)
      (ih context scope principal requirements fields right second _ rightAccepted) (binary_signature_sound found)

def QualifiedCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedBlockHasCompletionIn solution requirements names scope result completion source

def QualifiedBranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedBranchesHaveCompletionIn solution requirements names scope result completion source

theorem qualified_return_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.returnValue (some value) :: rest) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨expressionType, expressionAccepted, same, remainingAccepted⟩ := accepted
  subst expressionType
  exact .returned (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node _ expressionAccepted)
    (ih names scope solution requirements fields binderTypes _ none rest nodes remainingAccepted)

theorem qualified_assignment_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.assign (.name name) .set value :: rest) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedBlockCompletion, found] at accepted
  | some binding =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨_, mutable⟩, _, expressionAccepted, compatible, remainingAccepted⟩ := accepted
    exact .assignment found mutable
      (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node _ expressionAccepted)
      (by simpa [Bool.or_eq_true, checks_store_compatible_iff] using compatible)
      (ih names scope solution requirements fields binderTypes result completion rest nodes remainingAccepted)

theorem qualified_binding_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (storage : MonoType) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind storage node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.binding kind (.name name) declared initializer :: rest) := by
  cases found : binderTypes binder with
  | none => simp [validateQualifiedBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, storageEqual, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    subst storage
    apply QualifiedBlockHasCompletionIn.binding (rawType := rawType)
    · exact exprIH (some (names, result, binderTypes)) scope solution requirements fields initializer node _ expressionAccepted
    · simpa [Bool.or_eq_true, checks_store_compatible_iff] using valueEqual
    · exact check_annotation_sound names solution declared _ annotationAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution requirements fields binderTypes result completion rest nodes remainingAccepted

theorem qualified_tail_name_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.tailName name :: rest) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨empty, _⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst rest
  simp [requireCheck] at completionAccepted
  exact .tailName (exprIH (some (names, result, binderTypes)) scope solution requirements fields (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem qualified_evaluated_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel) (ih : QualifiedCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.expression value :: rest) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, _, completionAccepted⟩ := accepted
    exact .expressionTail (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    exact .expressionNext (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node ty expressionAccepted)
      (ih names scope solution requirements fields binderTypes result completion (next :: rest) nodes restAccepted)

theorem qualified_branch_completion_sound (fuel : Nat) (exprIH : CQualifiedExpressionSound fuel) (blockIH : QualifiedCompletionSound fuel)
    (branchesIH : QualifiedBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateQualifiedBranchesCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    QualifiedBranchesHaveCompletionIn solution requirements names scope result completion ((condition, body) :: rest) := by
  simp [validateQualifiedBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (exprIH (some (names, result, binderTypes)) scope solution requirements fields condition guard _ guardAccepted)
    (blockIH names scope solution requirements fields binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution requirements fields binderTypes result completion rest candidates remainingAccepted)

theorem qualified_conditional_completion_sound (fuel : Nat) (_exprIH : CQualifiedExpressionSound fuel) (blockIH : QualifiedCompletionSound fuel)
    (branchesIH : QualifiedBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedBlockHasCompletionIn solution requirements names scope result completion (.ifStmt branches otherwise :: rest) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, _⟩ := accepted
      exact .conditionalNoFallbackTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (check_completion_sound completion (.primitive .unit) completionAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, _⟩ := accepted
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, _, _, remainingAccepted⟩ := accepted
      exact .conditionalNoFallbackNext
        (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) nodes remainingAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      exact .conditionalNext
        (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result none body fallback bodyAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) nodes remainingAccepted)

theorem c_validate_qualified_all_sound : ∀ fuel,
    CQualifiedExpressionSound fuel ∧ CQualifiedArgumentsSound fuel ∧ CQualifiedFieldsSound fuel ∧ CQualifiedBranchesSound fuel ∧ QualifiedCompletionSound fuel ∧ QualifiedBranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [CQualifiedExpressionSound, CQualifiedArgumentsSound, CQualifiedFieldsSound, CQualifiedBranchesSound, QualifiedCompletionSound, QualifiedBranchCompletionSound, validateQualifiedBlockCompletion, validateQualifiedBranchesCompletion, validateQualifiedExpressionIn,
      validateQualifiedArgumentsIn, validateQualifiedFieldsIn, validateQualifiedBranchesIn]
  | succ fuel ih =>
    obtain ⟨exprIH, argsIH, fieldsIH, branchesIH, blockIH, statementBranchesIH⟩ := ih
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
    · intro context scope principal requirements fieldResults source node ty accepted
      cases source with
      | literal value =>
        cases value <;> cases node <;> first
          | exact c_validate_qualified_integer_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | exact c_validate_qualified_boolean_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | exact c_validate_qualified_string_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | cases accepted
      | ident name =>
        cases node <;> first
          | exact c_validate_qualified_read_sound fuel context scope principal requirements fieldResults name _ _ ty accepted
          | cases accepted
      | binary operator left right =>
        cases node <;> first
          | exact c_validate_qualified_binary_sound fuel context exprIH scope principal requirements fieldResults operator _ left right _ _ ty accepted
          | cases accepted
      | call callee arguments =>
        cases node <;> first
          | exact c_validate_qualified_call_sound fuel context exprIH argsIH scope principal requirements fieldResults callee arguments _ _ ty accepted
          | cases accepted
      | record fields =>
        cases node <;> first
          | exact c_validate_qualified_record_sound fuel context fieldsIH scope principal requirements fieldResults fields _ ty accepted
          | cases accepted
      | field base name =>
        cases node <;> first
          | exact c_validate_qualified_field_sound fuel context exprIH scope principal requirements fieldResults base name _ _ ty accepted
          | cases accepted
      | ifExpr branches otherwise =>
        cases node <;> first
          | exact c_validate_qualified_conditional_sound fuel context exprIH branchesIH scope principal requirements fieldResults branches otherwise _ _ ty accepted
          | cases accepted
      | require value schema =>
        cases schema with
        | none => cases node <;> cases accepted
        | some annotation =>
          cases annotation <;> cases node <;> first
            | exact c_validate_qualified_require_sound fuel context exprIH scope principal requirements fieldResults value _ _ _ _ ty accepted
            | cases accepted
      | tryExpr value =>
        cases node <;> first
          | exact c_validate_qualified_propagate_sound fuel context exprIH scope principal requirements fieldResults value _ _ ty accepted
          | cases accepted
      | valueBlock body =>
        cases node with
        | valueBlock rawCompletion nodes =>
          cases context with
          | none => cases accepted
          | some context =>
            obtain ⟨names, result, binderTypes⟩ := context
            simp [validateQualifiedExpressionIn, except_map_ok] at accepted
            obtain ⟨⟨⟨⟩, bodyAccepted⟩, rfl⟩ := accepted
            exact .valueBlock (blockIH names scope principal requirements fieldResults binderTypes result _ body nodes bodyAccepted)
        | _ => cases accepted
      | _ => cases node <;> cases accepted
    · intro context scope principal requirements fieldResults arguments nodes types accepted
      cases arguments with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons argument rest =>
        cases argument <;> cases nodes <;> first
          | exact c_validate_qualified_argument_cons_sound fuel context exprIH argsIH scope principal requirements fieldResults _ rest _ _ types accepted
          | cases accepted
    · intro context scope principal requirements fieldResults fields nodes types accepted
      cases fields with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons field rest =>
        cases field <;> cases nodes with
        | nil => cases accepted
        | cons pair tail =>
          cases pair with
          | mk name node =>
            first
            | exact c_validate_qualified_field_cons_sound fuel context exprIH fieldsIH scope principal requirements fieldResults _ name _ rest node tail types accepted
            | cases accepted
    · intro context scope principal requirements fieldResults result branches nodes accepted
      cases branches with
      | nil =>
        cases nodes with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons branch rest =>
        cases nodes with
        | nil => cases accepted
        | cons pair tail =>
          cases branch with
          | mk condition value =>
            cases pair with
            | mk guard node =>
              exact c_validate_qualified_branch_cons_sound fuel context exprIH branchesIH scope principal requirements fieldResults result
                condition value rest guard node tail accepted


    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil =>
          simp [validateQualifiedBlockCompletion, requireCheck] at accepted
          exact .nil (check_completion_sound completion (.primitive .unit) accepted)
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact qualified_binding_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  kind _ declared initializer rest _ _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact qualified_assignment_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact qualified_return_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact qualified_tail_name_completion_sound fuel exprIH names scope solution requirements fields binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact qualified_evaluated_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                value rest node nodes accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact qualified_conditional_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution requirements fields binderTypes
                  result completion branches otherwise rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons branch rest =>
        cases candidate with
        | nil => cases accepted
        | cons candidate candidates =>
          cases branch with
          | mk condition body =>
            cases candidate with
            | mk guard nodes =>
              exact qualified_branch_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution requirements fields binderTypes result
                completion condition body rest guard nodes candidates accepted



def CQualifiedNodeExpressionSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedExpressionIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedNodeIn principal requirements (nodeTypeContext context) scope candidate ty
def CQualifiedNodeArgumentsSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedArgumentsIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedNodesIn principal requirements (nodeTypeContext context) scope candidate ty
def CQualifiedNodeFieldsSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields source candidate ty,
 validateQualifiedFieldsIn fuel context scope principal requirements fields source candidate = .ok ty → QualifiedNodeFieldsIn principal requirements (nodeTypeContext context) scope candidate ty
def CQualifiedNodeBranchesSound (fuel : Nat) : Prop := ∀ context scope principal requirements fields result source candidate,
 validateQualifiedBranchesIn fuel context scope principal requirements fields result source candidate = .ok () → QualifiedNodeBranchesIn principal requirements (nodeTypeContext context) scope candidate result
theorem c_validate_qualified_node_read_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.ident name) (.read binder arguments) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.read binder arguments) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpressionIn, found] at accepted
  | some binding =>
    simp [validateQualifiedExpressionIn, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, List.all_eq_true] at accepted
    obtain ⟨same, notSchema, keys, sorts, included, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    subst binder
    apply QualifiedNodeIn.read found notSchema keys sorts ⟨rfl, rfl⟩ _ wellFormed sorted
    intro constraint member
    obtain ⟨original, present, rfl⟩ := List.mem_map.mp member
    exact included original present

theorem c_validate_qualified_node_field_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedNodeExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (base : Expr) (name recordedName : NameId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.field base name) (.field node recordedName) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.field node recordedName) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨same, baseType, baseAccepted, rest⟩ := accepted
  subst recordedName
  cases found : fields (.field node name) with
  | none => simp [found] at rest
  | some result =>
    simp [found, except_map_ok, exists_require_ok] at rest
    obtain ⟨member, rfl⟩ := rest
    exact .field (ih context scope principal requirements fields base node _ baseAccepted) member

theorem c_validate_qualified_node_call_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedNodeExpressionSound fuel)
    (argsIH : CQualifiedNodeArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (callee : Expr) (arguments : List CallArgument)
    (target : Node) (nodes : List Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.call callee arguments) (.call target nodes) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.call target nodes) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok] at accepted
  obtain ⟨calleeType, calleeAccepted, argumentTypes, argumentsAccepted, rest⟩ := accepted
  cases calleeType <;> simp [except_map_ok, exists_require_ok] at rest
  case function parameters result =>
    obtain ⟨same, rfl⟩ := rest
    subst parameters
    exact .call (exprIH context scope principal requirements fields callee target _ calleeAccepted)
      (argsIH context scope principal requirements fields arguments nodes _ argumentsAccepted)

theorem c_validate_qualified_node_record_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedNodeFieldsSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : List RecordField) (nodes : List (NameId × Node)) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.record source) (.record nodes) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.record nodes) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨types, fieldsAccepted, nodup, rfl⟩ := accepted
  exact .record (ih context scope principal requirements fields source nodes types fieldsAccepted) nodup

theorem c_validate_qualified_node_conditional_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedNodeExpressionSound fuel)
    (branchesIH : CQualifiedNodeBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (branches : List (Expr × Expr))
    (otherwise : Expr) (nodes : List (Node × Node)) (fallback : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.ifExpr branches otherwise) (.conditional nodes fallback) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.conditional nodes fallback) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨fallbackAccepted, _, branchAccepted⟩ := accepted
  exact .conditional (branchesIH context scope principal requirements fields ty branches nodes branchAccepted)
    (exprIH context scope principal requirements fields otherwise fallback ty fallbackAccepted)

theorem c_validate_qualified_node_propagate_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedNodeExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (node : Node) (expectedError ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.tryExpr value) (.propagate node expectedError) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.propagate node expectedError) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok] at accepted
  obtain ⟨valueType, valueAccepted, rest⟩ := accepted
  cases valueType <;> simp at rest
  case result inner error =>
    simp [except_map_ok, exists_require_ok] at rest
    obtain ⟨same, rfl⟩ := rest
    exact .propagate (ih context scope principal requirements fields value node _ valueAccepted) same

theorem c_validate_qualified_node_require_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedNodeExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (name : NameId) (binder : BinderId)
    (identity : Xsh.Validation.Nominal.NominalId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.require value (some (.named name))) (.requireValue binder identity node) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.requireValue binder identity node) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpressionIn, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [validateQualifiedExpressionIn, found, body, except_bind_ok, except_map_ok, exists_require_ok,
          Bool.and_eq_true, checks_well_formed_iff] at accepted
        obtain ⟨⟨sameBinder, kind⟩, ⟨quantified, empty⟩, ⟨sameIdentity, wellFormed⟩, ⟨inner, innerAccepted⟩, rfl⟩ := accepted
        subst binder
        subst identity
        exact .requireValue found kind quantified empty body wellFormed
          (ih context scope principal requirements fields value node inner innerAccepted)
      | _ => simp [validateQualifiedExpressionIn, found, body, except_bind_ok] at accepted
    | _ => simp [validateQualifiedExpressionIn, found, body, except_bind_ok] at accepted

theorem c_validate_qualified_node_argument_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedNodeExpressionSound fuel)
    (argsIH : CQualifiedNodeArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value : Expr) (rest : List CallArgument)
    (node : Node) (nodes : List Node) (types : MonoType)
    (accepted : validateQualifiedArgumentsIn (fuel + 1) context scope principal requirements fields
      (.positional value :: rest) (node :: nodes) = .ok types) :
    QualifiedNodesIn principal requirements (nodeTypeContext context) scope (node :: nodes) types := by
  simp [validateQualifiedArgumentsIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH context scope principal requirements fields value node _ firstAccepted)
    (argsIH context scope principal requirements fields rest nodes _ othersAccepted)

theorem c_validate_qualified_node_field_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedNodeExpressionSound fuel)
    (fieldsIH : CQualifiedNodeFieldsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name recorded : NameId) (value : Expr)
    (rest : List RecordField) (node : Node) (nodes : List (NameId × Node)) (types : List (NameId × MonoType))
    (accepted : validateQualifiedFieldsIn (fuel + 1) context scope principal requirements fields
      (.named name value :: rest) ((recorded, node) :: nodes) = .ok types) :
    QualifiedNodeFieldsIn principal requirements (nodeTypeContext context) scope ((recorded, node) :: nodes) types := by
  simp [validateQualifiedFieldsIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨same, first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  subst recorded
  exact .cons (exprIH context scope principal requirements fields value node _ firstAccepted)
    (fieldsIH context scope principal requirements fields rest nodes _ othersAccepted)

theorem c_validate_qualified_node_branch_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : CQualifiedNodeExpressionSound fuel)
    (branchesIH : CQualifiedNodeBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (result : MonoType)
    (condition value : Expr) (rest : List (Expr × Expr)) (guard node : Node) (nodes : List (Node × Node))
    (accepted : validateQualifiedBranchesIn (fuel + 1) context scope principal requirements fields result
      ((condition, value) :: rest) ((guard, node) :: nodes) = .ok ()) :
    QualifiedNodeBranchesIn principal requirements (nodeTypeContext context) scope ((guard, node) :: nodes) result := by
  simp [validateQualifiedBranchesIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, _, valueAccepted, rfl, remainingAccepted⟩ := accepted
  exact .cons (exprIH context scope principal requirements fields condition guard _ guardAccepted)
    (exprIH context scope principal requirements fields value node _ valueAccepted)
    (branchesIH context scope principal requirements fields _ rest nodes remainingAccepted)

theorem c_validate_qualified_node_integer_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (lexeme candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.integer lexeme)) (.integer candidate) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.integer candidate) ty := by
  cases decimal : decimalLexeme lexeme with
  | false =>
    simp only [validateQualifiedExpressionIn, decimal, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    cases parsed : parseInteger lexeme with
    | none => simp [validateQualifiedExpressionIn, decimal, parsed] at accepted
    | some value =>
      simp [validateQualifiedExpressionIn, decimal, parsed, except_map_ok, exists_require_ok] at accepted
      obtain ⟨same, rfl⟩ := accepted
      subst candidate
      exact .integer parsed

theorem c_validate_qualified_node_boolean_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : Bool) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.bool value)) (.boolean candidate) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.boolean candidate) ty := by
  simp [validateQualifiedExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .boolean

theorem c_validate_qualified_node_string_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.literal (.string value)) (.string candidate) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.string candidate) ty := by
  simp [validateQualifiedExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .string

theorem c_validate_qualified_node_binary_sound (fuel : Nat) (context : ValidationContext) (ih : CQualifiedNodeExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (operator recorded : BinaryOp) (left right : Expr) (first second : Node) (ty : MonoType)
    (accepted : validateQualifiedExpressionIn (fuel + 1) context scope principal requirements fields
      (.binary operator left right) (.binary recorded first second) = .ok ty) :
    QualifiedNodeIn principal requirements (nodeTypeContext context) scope (.binary recorded first second) ty := by
  simp [validateQualifiedExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨same, accepted⟩ := accepted
  subst recorded
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih context scope principal requirements fields left first _ leftAccepted)
      (ih context scope principal requirements fields right second _ rightAccepted) (binary_signature_sound found)

def QualifiedNodeCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion candidate

def QualifiedNodeBranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution requirements fields binderTypes result completion source candidate,
    validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion source candidate = .ok () →
    QualifiedNodeBranchesHaveCompletionIn solution requirements scope result completion candidate

theorem qualified_node_return_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.returned node :: nodes) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨expressionType, expressionAccepted, same, remainingAccepted⟩ := accepted
  subst expressionType
  exact .returned (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node _ expressionAccepted)
    (ih names scope solution requirements fields binderTypes _ none rest nodes remainingAccepted)

theorem qualified_node_assignment_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.assignment binder node :: nodes) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedBlockCompletion, found] at accepted
  | some binding =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨same, mutable⟩, _, expressionAccepted, compatible, remainingAccepted⟩ := accepted
    subst binder
    exact .assignment found mutable
      (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node _ expressionAccepted)
      (by simpa [Bool.or_eq_true, checks_store_compatible_iff] using compatible)
      (ih names scope solution requirements fields binderTypes result completion rest nodes remainingAccepted)

theorem qualified_node_binding_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (storage : MonoType) (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind storage node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.binding binder recordedKind storage node :: nodes) := by
  cases found : binderTypes binder with
  | none => simp [validateQualifiedBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateQualifiedBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, storageEqual, _, expressionAccepted, valueEqual, _, remainingAccepted⟩ := accepted
    subst recordedKind
    subst storage
    apply QualifiedNodeBlockHasCompletionIn.binding (rawType := rawType) (name := name)
    · exact exprIH (some (names, result, binderTypes)) scope solution requirements fields initializer node _ expressionAccepted
    · simpa [Bool.or_eq_true, checks_store_compatible_iff] using valueEqual
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution requirements fields binderTypes result completion rest nodes remainingAccepted

theorem qualified_node_tail_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.evaluated node :: nodes) := by
  simp [validateQualifiedBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨_, empty⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst nodes
  simp [requireCheck] at completionAccepted
  exact .evaluatedTail (exprIH (some (names, result, binderTypes)) scope solution requirements fields (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem qualified_node_evaluated_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel) (ih : QualifiedNodeCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.evaluated node :: nodes) := by
  cases rest with
  | nil =>
    simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, empty, completionAccepted⟩ := accepted
    subst nodes
    exact .evaluatedTail (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    cases nodes with
    | nil => cases fuel <;> cases restAccepted
    | cons candidate candidates =>
      exact .evaluatedNext (exprIH (some (names, result, binderTypes)) scope solution requirements fields value node ty expressionAccepted)
        (ih names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates) restAccepted)

theorem qualified_node_branch_completion_sound (fuel : Nat) (exprIH : CQualifiedNodeExpressionSound fuel) (blockIH : QualifiedNodeCompletionSound fuel)
    (branchesIH : QualifiedNodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateQualifiedBranchesCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    QualifiedNodeBranchesHaveCompletionIn solution requirements scope result completion ((guard, nodes) :: candidates) := by
  simp [validateQualifiedBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (exprIH (some (names, result, binderTypes)) scope solution requirements fields condition guard _ guardAccepted)
    (blockIH names scope solution requirements fields binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution requirements fields binderTypes result completion rest candidates remainingAccepted)

theorem qualified_node_conditional_completion_sound (fuel : Nat) (_exprIH : CQualifiedNodeExpressionSound fuel) (blockIH : QualifiedNodeCompletionSound fuel)
    (branchesIH : QualifiedNodeBranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateQualifiedBlockCompletion (fuel + 1) names scope solution requirements fields binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    QualifiedNodeBlockHasCompletionIn solution requirements scope result completion (.conditional candidates fallback :: nodes) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (.nil (check_completion_sound completion (.primitive .unit) completionAccepted))
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, empty⟩ := accepted
      subst nodes
      exact .conditionalTail
        (branchesIH names scope solution requirements fields binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution requirements fields binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateQualifiedBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, fallbackEmpty, _, remainingAccepted⟩ := accepted
      subst fallback
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
          (.nil (Or.inl rfl))
          (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)
    | some body =>
      simp [validateQualifiedBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      cases nodes with
      | nil => cases fuel <;> cases remainingAccepted
      | cons candidate candidates' =>
        exact .conditionalNext
          (branchesIH names scope solution requirements fields binderTypes result none branches candidates branchAccepted)
          (blockIH names scope solution requirements fields binderTypes result none body fallback bodyAccepted)
          (blockIH names scope solution requirements fields binderTypes result completion (next :: rest) (candidate :: candidates') remainingAccepted)

theorem c_validate_qualified_node_all_sound : ∀ fuel,
    CQualifiedNodeExpressionSound fuel ∧ CQualifiedNodeArgumentsSound fuel ∧ CQualifiedNodeFieldsSound fuel ∧ CQualifiedNodeBranchesSound fuel ∧ QualifiedNodeCompletionSound fuel ∧ QualifiedNodeBranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [CQualifiedNodeExpressionSound, CQualifiedNodeArgumentsSound, CQualifiedNodeFieldsSound, CQualifiedNodeBranchesSound, QualifiedNodeCompletionSound, QualifiedNodeBranchCompletionSound, validateQualifiedBlockCompletion, validateQualifiedBranchesCompletion, validateQualifiedExpressionIn,
      validateQualifiedArgumentsIn, validateQualifiedFieldsIn, validateQualifiedBranchesIn]
  | succ fuel ih =>
    obtain ⟨exprIH, argsIH, fieldsIH, branchesIH, blockIH, statementBranchesIH⟩ := ih
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
    · intro context scope principal requirements fieldResults source node ty accepted
      cases source with
      | literal value =>
        cases value <;> cases node <;> first
          | exact c_validate_qualified_node_integer_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | exact c_validate_qualified_node_boolean_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | exact c_validate_qualified_node_string_sound fuel context scope principal requirements fieldResults _ _ ty accepted
          | cases accepted
      | ident name =>
        cases node <;> first
          | exact c_validate_qualified_node_read_sound fuel context scope principal requirements fieldResults name _ _ ty accepted
          | cases accepted
      | binary operator left right =>
        cases node <;> first
          | exact c_validate_qualified_node_binary_sound fuel context exprIH scope principal requirements fieldResults operator _ left right _ _ ty accepted
          | cases accepted
      | call callee arguments =>
        cases node <;> first
          | exact c_validate_qualified_node_call_sound fuel context exprIH argsIH scope principal requirements fieldResults callee arguments _ _ ty accepted
          | cases accepted
      | record fields =>
        cases node <;> first
          | exact c_validate_qualified_node_record_sound fuel context fieldsIH scope principal requirements fieldResults fields _ ty accepted
          | cases accepted
      | field base name =>
        cases node <;> first
          | exact c_validate_qualified_node_field_sound fuel context exprIH scope principal requirements fieldResults base name _ _ ty accepted
          | cases accepted
      | ifExpr branches otherwise =>
        cases node <;> first
          | exact c_validate_qualified_node_conditional_sound fuel context exprIH branchesIH scope principal requirements fieldResults branches otherwise _ _ ty accepted
          | cases accepted
      | require value schema =>
        cases schema with
        | none => cases node <;> cases accepted
        | some annotation =>
          cases annotation <;> cases node <;> first
            | exact c_validate_qualified_node_require_sound fuel context exprIH scope principal requirements fieldResults value _ _ _ _ ty accepted
            | cases accepted
      | tryExpr value =>
        cases node <;> first
          | exact c_validate_qualified_node_propagate_sound fuel context exprIH scope principal requirements fieldResults value _ _ ty accepted
          | cases accepted
      | valueBlock body =>
        cases node with
        | valueBlock rawCompletion nodes =>
          cases context with
          | none => cases accepted
          | some context =>
            obtain ⟨names, result, binderTypes⟩ := context
            simp [validateQualifiedExpressionIn, except_map_ok] at accepted
            obtain ⟨⟨⟨⟩, bodyAccepted⟩, rfl⟩ := accepted
            exact .valueBlock (blockIH names scope principal requirements fieldResults binderTypes result _ body nodes bodyAccepted)
        | _ => cases accepted
      | _ => cases node <;> cases accepted
    · intro context scope principal requirements fieldResults arguments nodes types accepted
      cases arguments with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons argument rest =>
        cases argument <;> cases nodes <;> first
          | exact c_validate_qualified_node_argument_cons_sound fuel context exprIH argsIH scope principal requirements fieldResults _ rest _ _ types accepted
          | cases accepted
    · intro context scope principal requirements fieldResults fields nodes types accepted
      cases fields with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons field rest =>
        cases field <;> cases nodes with
        | nil => cases accepted
        | cons pair tail =>
          cases pair with
          | mk name node =>
            first
            | exact c_validate_qualified_node_field_cons_sound fuel context exprIH fieldsIH scope principal requirements fieldResults _ name _ rest node tail types accepted
            | cases accepted
    · intro context scope principal requirements fieldResults result branches nodes accepted
      cases branches with
      | nil =>
        cases nodes with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons branch rest =>
        cases nodes with
        | nil => cases accepted
        | cons pair tail =>
          cases branch with
          | mk condition value =>
            cases pair with
            | mk guard node =>
              exact c_validate_qualified_node_branch_cons_sound fuel context exprIH branchesIH scope principal requirements fieldResults result
                condition value rest guard node tail accepted


    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil =>
          simp [validateQualifiedBlockCompletion, requireCheck] at accepted
          exact .nil (check_completion_sound completion (.primitive .unit) accepted)
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact qualified_node_binding_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  kind _ declared initializer rest _ _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact qualified_node_assignment_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact qualified_node_return_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact qualified_node_tail_completion_sound fuel exprIH names scope solution requirements fields binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact qualified_node_evaluated_completion_sound fuel exprIH blockIH names scope solution requirements fields binderTypes result completion
                value rest node nodes accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact qualified_node_conditional_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution requirements fields binderTypes
                  result completion branches otherwise rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution requirements fields binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil => exact .nil
        | cons _ _ => cases accepted
      | cons branch rest =>
        cases candidate with
        | nil => cases accepted
        | cons candidate candidates =>
          cases branch with
          | mk condition body =>
            cases candidate with
            | mk guard nodes =>
              exact qualified_node_branch_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution requirements fields binderTypes result
                completion condition body rest guard nodes candidates accepted


end Xsh.Typing.Connected
