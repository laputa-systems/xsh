import Xsh.Typing.Connected.QualifiedValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def QualifiedExpressionSound (fuel : Nat) : Prop :=
  ∀ scope principal requirements fields source candidate ty,
    validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty →
    QualifiedExpression scope principal requirements source ty

def QualifiedArgumentsSound (fuel : Nat) : Prop :=
  ∀ scope principal requirements fields source candidate ty,
    validateQualifiedArguments fuel scope principal requirements fields source candidate = .ok ty →
    QualifiedArguments scope principal requirements source ty

def QualifiedFieldsSound (fuel : Nat) : Prop :=
  ∀ scope principal requirements fields source candidate ty,
    validateQualifiedFields fuel scope principal requirements fields source candidate = .ok ty →
    QualifiedRecordFields scope principal requirements source ty

def QualifiedBranchesSound (fuel : Nat) : Prop :=
  ∀ scope principal requirements fields result source candidate,
    validateQualifiedBranches fuel scope principal requirements fields result source candidate = .ok () →
    QualifiedBranches scope principal requirements source result

theorem validate_qualified_read_sound (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.ident name) (.read binder arguments) = .ok ty) :
    QualifiedExpression scope principal requirements (.ident name) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpression, found] at accepted
  | some binding =>
    simp [validateQualifiedExpression, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, List.all_eq_true] at accepted
    obtain ⟨_, notSchema, _, _, included, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    apply QualifiedExpression.read found notSchema ⟨rfl, rfl⟩ _ wellFormed sorted
    intro constraint member
    obtain ⟨original, present, rfl⟩ := List.mem_map.mp member
    exact included original present

theorem validate_qualified_field_sound (fuel : Nat) (ih : QualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (base : Expr) (name recordedName : NameId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.field base name) (.field node recordedName) = .ok ty) :
    QualifiedExpression scope principal requirements (.field base name) ty := by
  simp [validateQualifiedExpression, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, baseType, baseAccepted, rest⟩ := accepted
  cases found : fields (.field node recordedName) with
  | none => simp [found] at rest
  | some result =>
    simp [found, except_map_ok, exists_require_ok] at rest
    obtain ⟨member, rfl⟩ := rest
    exact .field (ih scope principal requirements fields base node _ baseAccepted) member

theorem validate_qualified_call_sound (fuel : Nat) (exprIH : QualifiedExpressionSound fuel)
    (argsIH : QualifiedArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (callee : Expr) (arguments : List CallArgument)
    (target : Node) (nodes : List Node) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.call callee arguments) (.call target nodes) = .ok ty) :
    QualifiedExpression scope principal requirements (.call callee arguments) ty := by
  simp [validateQualifiedExpression, except_bind_ok] at accepted
  obtain ⟨calleeType, calleeAccepted, argumentTypes, argumentsAccepted, rest⟩ := accepted
  cases calleeType <;> simp [except_map_ok, exists_require_ok] at rest
  case function parameters result =>
    obtain ⟨same, rfl⟩ := rest
    subst parameters
    exact .call (exprIH scope principal requirements fields callee target _ calleeAccepted)
      (argsIH scope principal requirements fields arguments nodes _ argumentsAccepted)

theorem validate_qualified_record_sound (fuel : Nat) (ih : QualifiedFieldsSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : List RecordField) (nodes : List (NameId × Node)) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.record source) (.record nodes) = .ok ty) :
    QualifiedExpression scope principal requirements (.record source) ty := by
  simp [validateQualifiedExpression, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨types, fieldsAccepted, nodup, rfl⟩ := accepted
  exact .record (ih scope principal requirements fields source nodes types fieldsAccepted) nodup

theorem validate_qualified_conditional_sound (fuel : Nat) (exprIH : QualifiedExpressionSound fuel)
    (branchesIH : QualifiedBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (branches : List (Expr × Expr))
    (otherwise : Expr) (nodes : List (Node × Node)) (fallback : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.ifExpr branches otherwise) (.conditional nodes fallback) = .ok ty) :
    QualifiedExpression scope principal requirements (.ifExpr branches otherwise) ty := by
  simp [validateQualifiedExpression, except_bind_ok, except_map_ok] at accepted
  obtain ⟨fallbackAccepted, _, branchAccepted⟩ := accepted
  exact .conditional (branchesIH scope principal requirements fields ty branches nodes branchAccepted)
    (exprIH scope principal requirements fields otherwise fallback ty fallbackAccepted)

theorem validate_qualified_propagate_sound (fuel : Nat) (ih : QualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (node : Node) (expectedError ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.tryExpr value) (.propagate node expectedError) = .ok ty) :
    QualifiedExpression scope principal requirements (.tryExpr value) ty := by
  simp [validateQualifiedExpression, except_bind_ok] at accepted
  obtain ⟨valueType, valueAccepted, rest⟩ := accepted
  cases valueType <;> simp at rest
  case result inner error =>
    simp [except_map_ok, exists_require_ok] at rest
    obtain ⟨_, rfl⟩ := rest
    exact .propagate (ih scope principal requirements fields value node _ valueAccepted)

theorem validate_qualified_require_sound (fuel : Nat) (ih : QualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (value : Expr) (name : NameId) (binder : BinderId)
    (identity : Xsh.Validation.Nominal.NominalId) (node : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.require value (some (.named name))) (.requireValue binder identity node) = .ok ty) :
    QualifiedExpression scope principal requirements (.require value (some (.named name))) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateQualifiedExpression, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [validateQualifiedExpression, found, body, except_bind_ok, except_map_ok, exists_require_ok,
          Bool.and_eq_true, checks_well_formed_iff] at accepted
        obtain ⟨⟨_, kind⟩, ⟨quantified, empty⟩, ⟨_, wellFormed⟩, ⟨inner, innerAccepted⟩, rfl⟩ := accepted
        exact .requireValue found kind quantified empty body wellFormed
          (ih scope principal requirements fields value node inner innerAccepted)
      | _ => simp [validateQualifiedExpression, found, body, except_bind_ok] at accepted
    | _ => simp [validateQualifiedExpression, found, body, except_bind_ok] at accepted

theorem validate_qualified_argument_cons_sound (fuel : Nat) (exprIH : QualifiedExpressionSound fuel)
    (argsIH : QualifiedArgumentsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value : Expr) (rest : List CallArgument)
    (node : Node) (nodes : List Node) (types : MonoType)
    (accepted : validateQualifiedArguments (fuel + 1) scope principal requirements fields
      (.positional value :: rest) (node :: nodes) = .ok types) :
    QualifiedArguments scope principal requirements (.positional value :: rest) types := by
  simp [validateQualifiedArguments, except_bind_ok, except_map_ok] at accepted
  obtain ⟨first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH scope principal requirements fields value node _ firstAccepted)
    (argsIH scope principal requirements fields rest nodes _ othersAccepted)

theorem validate_qualified_field_cons_sound (fuel : Nat) (exprIH : QualifiedExpressionSound fuel)
    (fieldsIH : QualifiedFieldsSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (name recorded : NameId) (value : Expr)
    (rest : List RecordField) (node : Node) (nodes : List (NameId × Node)) (types : List (NameId × MonoType))
    (accepted : validateQualifiedFields (fuel + 1) scope principal requirements fields
      (.named name value :: rest) ((recorded, node) :: nodes) = .ok types) :
    QualifiedRecordFields scope principal requirements (.named name value :: rest) types := by
  simp [validateQualifiedFields, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH scope principal requirements fields value node _ firstAccepted)
    (fieldsIH scope principal requirements fields rest nodes _ othersAccepted)

theorem validate_qualified_branch_cons_sound (fuel : Nat) (exprIH : QualifiedExpressionSound fuel)
    (branchesIH : QualifiedBranchesSound fuel) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (result : MonoType)
    (condition value : Expr) (rest : List (Expr × Expr)) (guard node : Node) (nodes : List (Node × Node))
    (accepted : validateQualifiedBranches (fuel + 1) scope principal requirements fields result
      ((condition, value) :: rest) ((guard, node) :: nodes) = .ok ()) :
    QualifiedBranches scope principal requirements ((condition, value) :: rest) result := by
  simp [validateQualifiedBranches, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, _, valueAccepted, rfl, remainingAccepted⟩ := accepted
  exact .cons (exprIH scope principal requirements fields condition guard _ guardAccepted)
    (exprIH scope principal requirements fields value node _ valueAccepted)
    (branchesIH scope principal requirements fields _ rest nodes remainingAccepted)

theorem validate_qualified_integer_sound (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (lexeme candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.literal (.integer lexeme)) (.integer candidate) = .ok ty) :
    QualifiedExpression scope principal requirements (.literal (.integer lexeme)) ty := by
  cases decimal : decimalLexeme lexeme with
  | false =>
    simp only [validateQualifiedExpression, decimal, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    cases parsed : parseInteger lexeme with
    | none => simp [validateQualifiedExpression, decimal, parsed] at accepted
    | some value =>
      simp [validateQualifiedExpression, decimal, parsed, except_map_ok, exists_require_ok] at accepted
      obtain ⟨_, rfl⟩ := accepted
      exact .integer parsed

theorem validate_qualified_boolean_sound (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : Bool) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.literal (.bool value)) (.boolean candidate) = .ok ty) :
    QualifiedExpression scope principal requirements (.literal (.bool value)) ty := by
  simp [validateQualifiedExpression, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .boolean

theorem validate_qualified_string_sound (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (value candidate : String) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.literal (.string value)) (.string candidate) = .ok ty) :
    QualifiedExpression scope principal requirements (.literal (.string value)) ty := by
  simp [validateQualifiedExpression, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .string

theorem validate_qualified_binary_sound (fuel : Nat) (ih : QualifiedExpressionSound fuel)
    (scope : Scope) (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (operator recorded : BinaryOp) (left right : Expr) (first second : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression (fuel + 1) scope principal requirements fields
      (.binary operator left right) (.binary recorded first second) = .ok ty) :
    QualifiedExpression scope principal requirements (.binary operator left right) ty := by
  simp [validateQualifiedExpression, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, accepted⟩ := accepted
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih scope principal requirements fields left first _ leftAccepted)
      (ih scope principal requirements fields right second _ rightAccepted) (binary_signature_sound found)

theorem validate_qualified_expressions_sound : ∀ fuel,
    QualifiedExpressionSound fuel ∧ QualifiedArgumentsSound fuel ∧ QualifiedFieldsSound fuel ∧ QualifiedBranchesSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [QualifiedExpressionSound, QualifiedArgumentsSound, QualifiedFieldsSound, QualifiedBranchesSound, validateQualifiedExpression,
      validateQualifiedArguments, validateQualifiedFields, validateQualifiedBranches]
  | succ fuel ih =>
    obtain ⟨exprIH, argsIH, fieldsIH, branchesIH⟩ := ih
    refine ⟨?_, ?_, ?_, ?_⟩
    · intro scope principal requirements fieldResults source node ty accepted
      cases source with
      | literal value =>
        cases value <;> cases node <;> first
          | exact validate_qualified_integer_sound fuel scope principal requirements fieldResults _ _ ty accepted
          | exact validate_qualified_boolean_sound fuel scope principal requirements fieldResults _ _ ty accepted
          | exact validate_qualified_string_sound fuel scope principal requirements fieldResults _ _ ty accepted
          | cases accepted
      | ident name =>
        cases node <;> first
          | exact validate_qualified_read_sound fuel scope principal requirements fieldResults name _ _ ty accepted
          | cases accepted
      | binary operator left right =>
        cases node <;> first
          | exact validate_qualified_binary_sound fuel exprIH scope principal requirements fieldResults operator _ left right _ _ ty accepted
          | cases accepted
      | call callee arguments =>
        cases node <;> first
          | exact validate_qualified_call_sound fuel exprIH argsIH scope principal requirements fieldResults callee arguments _ _ ty accepted
          | cases accepted
      | record fields =>
        cases node <;> first
          | exact validate_qualified_record_sound fuel fieldsIH scope principal requirements fieldResults fields _ ty accepted
          | cases accepted
      | field base name =>
        cases node <;> first
          | exact validate_qualified_field_sound fuel exprIH scope principal requirements fieldResults base name _ _ ty accepted
          | cases accepted
      | ifExpr branches otherwise =>
        cases node <;> first
          | exact validate_qualified_conditional_sound fuel exprIH branchesIH scope principal requirements fieldResults branches otherwise _ _ ty accepted
          | cases accepted
      | require value schema =>
        cases schema with
        | none => cases node <;> cases accepted
        | some annotation =>
          cases annotation <;> cases node <;> first
            | exact validate_qualified_require_sound fuel exprIH scope principal requirements fieldResults value _ _ _ _ ty accepted
            | cases accepted
      | tryExpr value =>
        cases node <;> first
          | exact validate_qualified_propagate_sound fuel exprIH scope principal requirements fieldResults value _ _ ty accepted
          | cases accepted
      | _ => cases node <;> cases accepted
    · intro scope principal requirements fieldResults arguments nodes types accepted
      cases arguments with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons argument rest =>
        cases argument <;> cases nodes <;> first
          | exact validate_qualified_argument_cons_sound fuel exprIH argsIH scope principal requirements fieldResults _ rest _ _ types accepted
          | cases accepted
    · intro scope principal requirements fieldResults fields nodes types accepted
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
            | exact validate_qualified_field_cons_sound fuel exprIH fieldsIH scope principal requirements fieldResults _ name _ rest node tail types accepted
            | cases accepted
    · intro scope principal requirements fieldResults result branches nodes accepted
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
              exact validate_qualified_branch_cons_sound fuel exprIH branchesIH scope principal requirements fieldResults result
                condition value rest guard node tail accepted


theorem validated_qualified_expression_instantiates (fuel : Nat) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ExpressionHasType scope (compose later principal) source (substitute later ty) :=
  qualified_expression_instantiates scope principal later requirements source ty
    ((validate_qualified_expressions_sound fuel).1 scope principal requirements fields source candidate ty accepted)
    sorted solves

theorem accepted_qualified_expression_has_type (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty) :
    QualifiedExpression scope principal requirements source ty :=
  (validate_qualified_expressions_sound fuel).1 scope principal requirements fields source candidate ty accepted

end Xsh.Typing.Connected
