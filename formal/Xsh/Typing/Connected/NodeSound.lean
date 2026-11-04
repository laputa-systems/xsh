import Xsh.Typing.Connected.NodeJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def NodeExpressionSound (fuel : Nat) : Prop := ∀ scope solution source node ty,
  validateExpression fuel scope solution source node = .ok ty → NodeHasType scope solution node ty

def NodeArgumentsSound (fuel : Nat) : Prop := ∀ scope solution arguments nodes types,
  validateArguments fuel scope solution arguments nodes = .ok types → NodesHaveTypes scope solution nodes types

def NodeFieldsSound (fuel : Nat) : Prop := ∀ scope solution fields nodes types,
  validateRecordFields fuel scope solution fields nodes = .ok types → NodeFieldsHaveTypes scope solution nodes types

def NodeBranchesSound (fuel : Nat) : Prop := ∀ scope solution result branches nodes,
  validateBranches fuel scope solution result branches nodes = .ok () → NodeBranchesHaveType scope solution nodes result

end Xsh.Typing.Connected

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem validate_node_integer_sound (fuel : Nat) (scope : Scope) (solution : Substitution)
    (lexeme candidate : String) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution
      (.literal (.integer lexeme)) (.integer candidate) = .ok ty) :
    NodeHasType scope solution (.integer candidate) ty := by
  cases decimal : decimalLexeme lexeme with
  | false =>
    simp only [validateExpression, validateExpressionIn, decimal, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    cases parsed : parseInteger lexeme with
    | none => simp [validateExpression, validateExpressionIn, decimal, parsed] at accepted
    | some value =>
      simp [validateExpression, validateExpressionIn, decimal, parsed, except_map_ok, exists_require_ok] at accepted
      obtain ⟨same, rfl⟩ := accepted
      subst candidate
      exact .integer parsed

theorem validate_node_boolean_sound (fuel : Nat) (scope : Scope) (solution : Substitution)
    (value candidate : Bool) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution
      (.literal (.bool value)) (.boolean candidate) = .ok ty) :
    NodeHasType scope solution (.boolean candidate) ty := by
  simp [validateExpression, validateExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .boolean

theorem validate_node_string_sound (fuel : Nat) (scope : Scope) (solution : Substitution)
    (value candidate : String) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution
      (.literal (.string value)) (.string candidate) = .ok ty) :
    NodeHasType scope solution (.string candidate) ty := by
  simp [validateExpression, validateExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .string

theorem validate_node_read_sound (fuel : Nat) (scope : Scope) (solution : Substitution) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.ident name) (.read binder arguments) = .ok ty) :
    NodeHasType scope solution (.read binder arguments) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateExpression, validateExpressionIn, found] at accepted
  | some binding =>
    simp [validateExpression, validateExpressionIn, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, checks_constraints_iff] at accepted
    obtain ⟨same, notSchema, keys, sorts, solved, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    subst binder
    exact .read found notSchema keys sorts ⟨rfl, rfl⟩ solved wellFormed sorted

theorem validate_node_binary_sound (fuel : Nat) (ih : NodeExpressionSound fuel) (scope : Scope)
    (solution : Substitution) (operator recorded : BinaryOp) (left right : Expr)
    (first second : Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.binary operator left right)
      (.binary recorded first second) = .ok ty) :
    NodeHasType scope solution (.binary recorded first second) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨same, accepted⟩ := accepted
  subst recorded
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih scope solution left first _ leftAccepted)
      (ih scope solution right second _ rightAccepted) (binary_signature_sound found)

theorem validate_node_call_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel) (argsIH : NodeArgumentsSound fuel)
    (scope : Scope) (solution : Substitution) (callee : Expr) (arguments : List CallArgument)
    (target : Node) (nodes : List Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.call callee arguments)
      (.call target nodes) = .ok ty) :
    NodeHasType scope solution (.call target nodes) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok] at accepted
  obtain ⟨calleeType, calleeAccepted, argumentTypes, argumentsAccepted, rest⟩ := accepted
  cases calleeType <;> simp [except_map_ok, exists_require_ok] at rest
  case function parameters result =>
    obtain ⟨same, rfl⟩ := rest
    subst parameters
    exact .call (exprIH scope solution callee target _ calleeAccepted)
      (argsIH scope solution arguments nodes _ argumentsAccepted)

theorem validate_node_record_sound (fuel : Nat) (fieldsIH : NodeFieldsSound fuel)
    (scope : Scope) (solution : Substitution) (fields : List RecordField)
    (nodes : List (NameId × Node)) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.record fields) (.record nodes) = .ok ty) :
    NodeHasType scope solution (.record nodes) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨types, fieldsAccepted, nodup, rfl⟩ := accepted
  exact .record (fieldsIH scope solution fields nodes types fieldsAccepted) nodup

theorem validate_node_conditional_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (branchesIH : NodeBranchesSound fuel) (scope : Scope) (solution : Substitution)
    (branches : List (Expr × Expr)) (otherwise : Expr) (nodes : List (Node × Node))
    (fallback : Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.ifExpr branches otherwise)
      (.conditional nodes fallback) = .ok ty) :
    NodeHasType scope solution (.conditional nodes fallback) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨fallbackAccepted, _, branchAccepted⟩ := accepted
  exact .conditional (branchesIH scope solution ty branches nodes branchAccepted)
    (exprIH scope solution otherwise fallback ty fallbackAccepted)

theorem validate_node_field_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (base : Expr) (name recordedName : NameId)
    (node : Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.field base name)
      (.field node recordedName) = .ok ty) :
    NodeHasType scope solution (.field node recordedName) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨same, baseType, baseAccepted, rest⟩ := accepted
  subst recordedName
  cases baseType <;> simp at rest
  case record row =>
    cases found : fieldAt? name row with
    | none => simp [found] at rest
    | some field =>
      simp [found] at rest
      change Except.ok field = .ok ty at rest
      cases rest
      exact .field (exprIH scope solution base node _ baseAccepted) (field_occurs_iff.mpr found)

theorem validate_node_propagate_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (value : Expr) (node : Node) (expectedError ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.tryExpr value)
      (.propagate node expectedError) = .ok ty) :
    NodeHasType scope solution (.propagate node expectedError) ty := by
  simp [validateExpression, validateExpressionIn, except_bind_ok] at accepted
  obtain ⟨valueType, valueAccepted, rest⟩ := accepted
  cases valueType <;> simp at rest
  case result inner error =>
    simp [except_map_ok, exists_require_ok] at rest
    obtain ⟨same, rfl⟩ := rest
    exact .propagate (exprIH scope solution value node _ valueAccepted) same

theorem validate_node_require_sound (fuel : Nat) (ih : NodeExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (value : Expr) (name : NameId)
    (binder : BinderId) (identity : Xsh.Validation.Nominal.NominalId) (node : Node) (ty : MonoType)
    (accepted : validateExpression (fuel + 1) scope solution (.require value (some (.named name)))
      (.requireValue binder identity node) = .ok ty) :
    NodeHasType scope solution (.requireValue binder identity node) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateExpression, validateExpressionIn, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [validateExpression, validateExpressionIn, found, body, except_bind_ok, except_map_ok, exists_require_ok,
          Bool.and_eq_true, checks_well_formed_iff] at accepted
        obtain ⟨⟨sameBinder, kind⟩, ⟨quantified, requirements⟩, ⟨sameIdentity, wellFormed⟩, ⟨inner, innerAccepted⟩, rfl⟩ := accepted
        subst binder
        subst identity
        exact .requireValue found kind quantified requirements body wellFormed
          (ih scope solution value node inner innerAccepted)
      | _ => simp [validateExpression, validateExpressionIn, found, body, except_bind_ok] at accepted
    | _ => simp [validateExpression, validateExpressionIn, found, body, except_bind_ok] at accepted

theorem validate_node_argument_cons_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (argsIH : NodeArgumentsSound fuel) (scope : Scope) (solution : Substitution)
    (value : Expr) (rest : List CallArgument) (node : Node) (nodes : List Node) (types : MonoType)
    (accepted : validateArguments (fuel + 1) scope solution (.positional value :: rest)
      (node :: nodes) = .ok types) :
    NodesHaveTypes scope solution (node :: nodes) types := by
  simp [validateArguments, validateArgumentsIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH scope solution value node _ firstAccepted)
    (argsIH scope solution rest nodes _ othersAccepted)

theorem validate_node_field_cons_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (fieldsIH : NodeFieldsSound fuel) (scope : Scope) (solution : Substitution)
    (name recorded : NameId) (value : Expr) (rest : List RecordField)
    (node : Node) (nodes : List (NameId × Node)) (types : List (NameId × MonoType))
    (accepted : validateRecordFields (fuel + 1) scope solution (.named name value :: rest)
      ((recorded, node) :: nodes) = .ok types) :
    NodeFieldsHaveTypes scope solution ((recorded, node) :: nodes) types := by
  simp [validateRecordFields, validateRecordFieldsIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨same, first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  subst recorded
  exact .cons (exprIH scope solution value node _ firstAccepted)
    (fieldsIH scope solution rest nodes _ othersAccepted)

theorem validate_node_branch_cons_sound (fuel : Nat) (exprIH : NodeExpressionSound fuel)
    (branchesIH : NodeBranchesSound fuel) (scope : Scope) (solution : Substitution) (result : MonoType)
    (condition value : Expr) (rest : List (Expr × Expr)) (guard node : Node)
    (nodes : List (Node × Node))
    (accepted : validateBranches (fuel + 1) scope solution result ((condition, value) :: rest)
      ((guard, node) :: nodes) = .ok ()) :
    NodeBranchesHaveType scope solution ((guard, node) :: nodes) result := by
  simp [validateBranches, validateBranchesIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, _, valueAccepted, rfl, remainingAccepted⟩ := accepted
  exact .cons (exprIH scope solution condition guard _ guardAccepted)
    (exprIH scope solution value node _ valueAccepted)
    (branchesIH scope solution _ rest nodes remainingAccepted)


theorem validate_nodes_sound : ∀ fuel,
    NodeExpressionSound fuel ∧ NodeArgumentsSound fuel ∧ NodeFieldsSound fuel ∧ NodeBranchesSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [NodeExpressionSound, NodeArgumentsSound, NodeFieldsSound, NodeBranchesSound, validateExpression, validateExpressionIn,
      validateArguments, validateArgumentsIn, validateRecordFields, validateRecordFieldsIn, validateBranches, validateBranchesIn]
  | succ fuel ih =>
    obtain ⟨exprIH, argsIH, fieldsIH, branchesIH⟩ := ih
    refine ⟨?_, ?_, ?_, ?_⟩
    · intro scope solution source node ty accepted
      cases source with
      | literal value =>
        cases value <;> cases node <;> first
          | exact validate_node_integer_sound fuel scope solution _ _ ty accepted
          | exact validate_node_boolean_sound fuel scope solution _ _ ty accepted
          | exact validate_node_string_sound fuel scope solution _ _ ty accepted
          | cases accepted
      | ident name =>
        cases node <;> first
          | exact validate_node_read_sound fuel scope solution name _ _ ty accepted
          | cases accepted
      | binary operator left right =>
        cases node <;> first
          | exact validate_node_binary_sound fuel exprIH scope solution operator _ left right _ _ ty accepted
          | cases accepted
      | call callee arguments =>
        cases node <;> first
          | exact validate_node_call_sound fuel exprIH argsIH scope solution callee arguments _ _ ty accepted
          | cases accepted
      | record fields =>
        cases node <;> first
          | exact validate_node_record_sound fuel fieldsIH scope solution fields _ ty accepted
          | cases accepted
      | field base name =>
        cases node <;> first
          | exact validate_node_field_sound fuel exprIH scope solution base name _ _ ty accepted
          | cases accepted
      | ifExpr branches otherwise =>
        cases node <;> first
          | exact validate_node_conditional_sound fuel exprIH branchesIH scope solution branches otherwise _ _ ty accepted
          | cases accepted
      | require value schema =>
        cases schema with
        | none => cases node <;> cases accepted
        | some annotation =>
          cases annotation <;> cases node <;> first
            | exact validate_node_require_sound fuel exprIH scope solution value _ _ _ _ ty accepted
            | cases accepted
      | tryExpr value =>
        cases node <;> first
          | exact validate_node_propagate_sound fuel exprIH scope solution value _ _ ty accepted
          | cases accepted
      | _ => cases node <;> cases accepted
    · intro scope solution arguments nodes types accepted
      cases arguments with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons argument rest =>
        cases argument <;> cases nodes <;> first
          | exact validate_node_argument_cons_sound fuel exprIH argsIH scope solution _ rest _ _ types accepted
          | cases accepted
    · intro scope solution fields nodes types accepted
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
            | exact validate_node_field_cons_sound fuel exprIH fieldsIH scope solution _ name _ rest node tail types accepted
            | cases accepted
    · intro scope solution result branches nodes accepted
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
              exact validate_node_branch_cons_sound fuel exprIH branchesIH scope solution result
                condition value rest guard node tail accepted


theorem accepted_node_has_type (fuel : Nat) (scope : Scope) (solution : Substitution)
    (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateExpression fuel scope solution source candidate = .ok ty) :
    NodeHasType scope solution candidate ty :=
  (validate_nodes_sound fuel).1 scope solution source candidate ty accepted

end Xsh.Typing.Connected
