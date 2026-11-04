import Xsh.Typing.Connected.BlockValidate
import Xsh.Typing.Connected.Sound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def ContextExpressionSound (fuel : Nat) : Prop := ∀ context scope solution source node ty,
  validateExpressionIn fuel context scope solution source node = .ok ty → ExpressionHasTypeIn solution (sourceTypeContext context) scope source ty
def ContextArgumentsSound (fuel : Nat) : Prop := ∀ context scope solution source node ty,
  validateArgumentsIn fuel context scope solution source node = .ok ty → ArgumentsHaveTypesIn solution (sourceTypeContext context) scope source ty
def ContextFieldsSound (fuel : Nat) : Prop := ∀ context scope solution source node ty,
  validateRecordFieldsIn fuel context scope solution source node = .ok ty → RecordFieldsHaveTypesIn solution (sourceTypeContext context) scope source ty
def ContextBranchesSound (fuel : Nat) : Prop := ∀ context scope solution result source node,
  validateBranchesIn fuel context scope solution result source node = .ok () → BranchesHaveTypeIn solution (sourceTypeContext context) scope source result

theorem context_validate_read_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (solution : Substitution) (name : NameId)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.ident name) (.read binder arguments) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.ident name) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateExpressionIn, found] at accepted
  | some binding =>
    simp [validateExpressionIn, found, except_bind_ok, except_map_ok, exists_require_ok,
      Bool.and_eq_true, checks_well_formed_iff, checks_constraints_iff] at accepted
    obtain ⟨_, notSchema, _, _, solved, ⟨wellFormed, sorted⟩, rfl⟩ := accepted
    exact .read found notSchema ⟨rfl, rfl⟩ solved wellFormed sorted

theorem context_validate_binary_sound (fuel : Nat) (context : ValidationContext) (ih : ContextExpressionSound fuel) (scope : Scope)
    (solution : Substitution) (operator recorded : BinaryOp) (left right : Expr)
    (first second : Node) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.binary operator left right)
      (.binary recorded first second) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.binary operator left right) ty := by
  simp [validateExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, accepted⟩ := accepted
  cases found : binarySignature operator with
  | none => simp [found] at accepted
  | some signature =>
    obtain ⟨leftType, rightType, result⟩ := signature
    simp [found, except_bind_ok, except_map_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨_, leftAccepted, _, rightAccepted, ⟨rfl, rfl⟩, rfl⟩ := accepted
    exact .binary (ih context scope solution left first _ leftAccepted)
      (ih context scope solution right second _ rightAccepted) (binary_signature_sound found)

theorem context_validate_integer_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (solution : Substitution)
    (lexeme candidate : String) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution
      (.literal (.integer lexeme)) (.integer candidate) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.literal (.integer lexeme)) ty := by
  cases decimal : decimalLexeme lexeme with
  | false =>
    simp only [validateExpressionIn, decimal, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    cases parsed : parseInteger lexeme with
    | none => simp [validateExpressionIn, decimal, parsed] at accepted
    | some value =>
      simp [validateExpressionIn, decimal, parsed, except_map_ok, exists_require_ok] at accepted
      obtain ⟨_, rfl⟩ := accepted
      exact .integer parsed

theorem context_validate_boolean_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (solution : Substitution)
    (value candidate : Bool) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution
      (.literal (.bool value)) (.boolean candidate) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.literal (.bool value)) ty := by
  simp [validateExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .boolean

theorem context_validate_string_sound (fuel : Nat) (context : ValidationContext) (scope : Scope) (solution : Substitution)
    (value candidate : String) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution
      (.literal (.string value)) (.string candidate) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.literal (.string value)) ty := by
  simp [validateExpressionIn, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, rfl⟩ := accepted
  exact .string

theorem context_validate_call_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel) (argsIH : ContextArgumentsSound fuel)
    (scope : Scope) (solution : Substitution) (callee : Expr) (arguments : List CallArgument)
    (target : Node) (nodes : List Node) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.call callee arguments)
      (.call target nodes) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.call callee arguments) ty := by
  simp [validateExpressionIn, except_bind_ok] at accepted
  obtain ⟨calleeType, calleeAccepted, argumentTypes, argumentsAccepted, rest⟩ := accepted
  cases calleeType <;> simp [except_map_ok, exists_require_ok] at rest
  case function parameters result =>
    obtain ⟨same, rfl⟩ := rest
    subst parameters
    exact .call (exprIH context scope solution callee target _ calleeAccepted)
      (argsIH context scope solution arguments nodes _ argumentsAccepted)

theorem context_validate_record_sound (fuel : Nat) (context : ValidationContext) (fieldsIH : ContextFieldsSound fuel)
    (scope : Scope) (solution : Substitution) (fields : List RecordField)
    (nodes : List (NameId × Node)) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.record fields) (.record nodes) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.record fields) ty := by
  simp [validateExpressionIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨types, fieldsAccepted, nodup, rfl⟩ := accepted
  exact .record (fieldsIH context scope solution fields nodes types fieldsAccepted) nodup

theorem context_validate_conditional_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (branchesIH : ContextBranchesSound fuel) (scope : Scope) (solution : Substitution)
    (branches : List (Expr × Expr)) (otherwise : Expr) (nodes : List (Node × Node))
    (fallback : Node) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.ifExpr branches otherwise)
      (.conditional nodes fallback) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.ifExpr branches otherwise) ty := by
  simp [validateExpressionIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨fallbackAccepted, _, branchAccepted⟩ := accepted
  exact .conditional (branchesIH context scope solution ty branches nodes branchAccepted)
    (exprIH context scope solution otherwise fallback ty fallbackAccepted)

theorem context_validate_field_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (base : Expr) (name recordedName : NameId)
    (node : Node) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.field base name)
      (.field node recordedName) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.field base name) ty := by
  simp [validateExpressionIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, baseType, baseAccepted, rest⟩ := accepted
  cases baseType <;> simp at rest
  case record row =>
    cases found : fieldAt? name row with
    | none => simp [found] at rest
    | some field =>
      simp [found] at rest
      change Except.ok field = .ok ty at rest
      cases rest
      exact .field (exprIH context scope solution base node _ baseAccepted) (field_occurs_iff.mpr found)

theorem context_validate_propagate_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (value : Expr) (node : Node) (expectedError ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.tryExpr value)
      (.propagate node expectedError) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.tryExpr value) ty := by
  simp [validateExpressionIn, except_bind_ok] at accepted
  obtain ⟨valueType, valueAccepted, rest⟩ := accepted
  cases valueType <;> simp at rest
  case result inner error =>
    simp [except_map_ok, exists_require_ok] at rest
    obtain ⟨_, rfl⟩ := rest
    exact .propagate (exprIH context scope solution value node _ valueAccepted)

theorem context_validate_require_sound (fuel : Nat) (context : ValidationContext) (ih : ContextExpressionSound fuel)
    (scope : Scope) (solution : Substitution) (value : Expr) (name : NameId)
    (binder : BinderId) (identity : Xsh.Validation.Nominal.NominalId) (node : Node) (ty : MonoType)
    (accepted : validateExpressionIn (fuel + 1) context scope solution (.require value (some (.named name)))
      (.requireValue binder identity node) = .ok ty) :
    ExpressionHasTypeIn solution (sourceTypeContext context) scope (.require value (some (.named name))) ty := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateExpressionIn, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [validateExpressionIn, found, body, except_bind_ok, except_map_ok, exists_require_ok,
          Bool.and_eq_true, checks_well_formed_iff] at accepted
        obtain ⟨⟨_, kind⟩, ⟨quantified, requirements⟩, ⟨_, wellFormed⟩, ⟨inner, innerAccepted⟩, rfl⟩ := accepted
        exact .requireValue found kind quantified requirements body wellFormed
          (ih context scope solution value node inner innerAccepted)
      | _ => simp [validateExpressionIn, found, body, except_bind_ok] at accepted
    | _ => simp [validateExpressionIn, found, body, except_bind_ok] at accepted

theorem context_validate_argument_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (argsIH : ContextArgumentsSound fuel) (scope : Scope) (solution : Substitution)
    (value : Expr) (rest : List CallArgument) (node : Node) (nodes : List Node) (types : MonoType)
    (accepted : validateArgumentsIn (fuel + 1) context scope solution (.positional value :: rest)
      (node :: nodes) = .ok types) :
    ArgumentsHaveTypesIn solution (sourceTypeContext context) scope (.positional value :: rest) types := by
  simp [validateArgumentsIn, except_bind_ok, except_map_ok] at accepted
  obtain ⟨first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH context scope solution value node _ firstAccepted)
    (argsIH context scope solution rest nodes _ othersAccepted)

theorem context_validate_field_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (fieldsIH : ContextFieldsSound fuel) (scope : Scope) (solution : Substitution)
    (name recorded : NameId) (value : Expr) (rest : List RecordField)
    (node : Node) (nodes : List (NameId × Node)) (types : List (NameId × MonoType))
    (accepted : validateRecordFieldsIn (fuel + 1) context scope solution (.named name value :: rest)
      ((recorded, node) :: nodes) = .ok types) :
    RecordFieldsHaveTypesIn solution (sourceTypeContext context) scope (.named name value :: rest) types := by
  simp [validateRecordFieldsIn, except_bind_ok, except_map_ok, exists_require_ok] at accepted
  obtain ⟨_, first, firstAccepted, others, othersAccepted, rfl⟩ := accepted
  exact .cons (exprIH context scope solution value node _ firstAccepted)
    (fieldsIH context scope solution rest nodes _ othersAccepted)

theorem context_validate_branch_cons_sound (fuel : Nat) (context : ValidationContext) (exprIH : ContextExpressionSound fuel)
    (branchesIH : ContextBranchesSound fuel) (scope : Scope) (solution : Substitution) (result : MonoType)
    (condition value : Expr) (rest : List (Expr × Expr)) (guard node : Node)
    (nodes : List (Node × Node))
    (accepted : validateBranchesIn (fuel + 1) context scope solution result ((condition, value) :: rest)
      ((guard, node) :: nodes) = .ok ()) :
    BranchesHaveTypeIn solution (sourceTypeContext context) scope ((condition, value) :: rest) result := by
  simp [validateBranchesIn, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, _, valueAccepted, rfl, remainingAccepted⟩ := accepted
  exact .cons (exprIH context scope solution condition guard _ guardAccepted)
    (exprIH context scope solution value node _ valueAccepted)
    (branchesIH context scope solution _ rest nodes remainingAccepted)



def BlockCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result completion source candidate,
    validateBlockCompletion fuel names scope solution binderTypes result completion source candidate = .ok () →
    SourceBlockHasCompletion solution names scope result completion source

def BranchCompletionSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result completion source candidate,
    validateStatementBranchesCompletion fuel names scope solution binderTypes result completion source candidate = .ok () →
    SourceBranchesHaveCompletion solution names scope result completion source

theorem return_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel) (ih : BlockCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.returnValue (some value) :: rest) (.returned node :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.returnValue (some value) :: rest) := by
  simp [validateBlockCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨expressionType, expressionAccepted, same, remainingAccepted⟩ := accepted
  subst expressionType
  exact .returned (exprIH (some (names, result, binderTypes)) scope solution value node _ expressionAccepted)
    (ih names scope solution binderTypes _ none rest nodes remainingAccepted)

theorem assignment_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel) (ih : BlockCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (value : Expr)
    (rest : List Statement) (binder : BinderId) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.assign (.name name) .set value :: rest) (.assignment binder node :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.assign (.name name) .set value :: rest) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [validateBlockCompletion, found] at accepted
  | some binding =>
    simp [validateBlockCompletion, found, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
    obtain ⟨⟨_, mutable⟩, _, expressionAccepted, rfl, remainingAccepted⟩ := accepted
    exact .assignment found mutable
      (exprIH (some (names, result, binderTypes)) scope solution value node _ expressionAccepted)
      (ih names scope solution binderTypes result completion rest nodes remainingAccepted)

theorem binding_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel) (ih : BlockCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (kind : Xsh.Syntax.BindingKind)
    (name : NameId) (declared : Option TypeExpr) (initializer : Expr) (rest : List Statement)
    (binder : BinderId) (recordedKind : BindingKind) (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.binding kind (.name name) declared initializer :: rest) (.binding binder recordedKind node :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.binding kind (.name name) declared initializer :: rest) := by
  cases found : binderTypes binder with
  | none => simp [validateBlockCompletion, found, except_bind_ok] at accepted
  | some rawType =>
    simp [validateBlockCompletion, found, except_bind_ok, exists_require_ok] at accepted
    obtain ⟨kindEqual, fresh, _, expressionAccepted, valueEqual, annotationAccepted, remainingAccepted⟩ := accepted
    subst recordedKind
    apply SourceBlockHasCompletion.binding (rawType := rawType)
    · exact valueEqual ▸ exprIH (some (names, result, binderTypes)) scope solution initializer node _ expressionAccepted
    · exact valueEqual ▸ check_annotation_sound names solution declared _ annotationAccepted
    · intro binding member same
      exact fresh binding member same
    · exact ih names _ solution binderTypes result completion rest nodes remainingAccepted

theorem tail_name_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (name : NameId) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.tailName name :: rest) (.evaluated node :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.tailName name :: rest) := by
  simp [validateBlockCompletion, except_bind_ok, exists_require_ok, Bool.and_eq_true] at accepted
  obtain ⟨⟨empty, _⟩, ty, expressionAccepted, completionAccepted⟩ := accepted
  subst rest
  simp [requireCheck] at completionAccepted
  exact .tailName (exprIH (some (names, result, binderTypes)) scope solution (.ident name) node ty expressionAccepted)
    (check_completion_sound completion ty completionAccepted)

theorem evaluated_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel) (ih : BlockCompletionSound fuel)
    (names : TypeNames) (scope : Scope) (solution : Substitution) (binderTypes : BinderTypes)
    (result : MonoType) (completion : Option MonoType) (value : Expr) (rest : List Statement)
    (node : Node) (nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.expression value :: rest) (.evaluated node :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.expression value :: rest) := by
  cases rest with
  | nil =>
    simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
    obtain ⟨ty, expressionAccepted, _, completionAccepted⟩ := accepted
    exact .expressionTail (exprIH (some (names, result, binderTypes)) scope solution value node ty expressionAccepted)
      (check_completion_sound completion ty completionAccepted)
  | cons next rest =>
    simp [validateBlockCompletion, except_bind_ok] at accepted
    obtain ⟨⟨ty, expressionAccepted⟩, restAccepted⟩ := accepted
    exact .expressionNext (exprIH (some (names, result, binderTypes)) scope solution value node ty expressionAccepted)
      (ih names scope solution binderTypes result completion (next :: rest) nodes restAccepted)

theorem branch_completion_sound (fuel : Nat) (exprIH : ContextExpressionSound fuel) (blockIH : BlockCompletionSound fuel)
    (branchesIH : BranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (condition : Expr) (body : List Statement)
    (rest : List (Expr × List Statement)) (guard : Node) (nodes : List StatementNode)
    (candidates : List (Node × List StatementNode))
    (accepted : validateStatementBranchesCompletion (fuel + 1) names scope solution binderTypes result completion
      ((condition, body) :: rest) ((guard, nodes) :: candidates) = .ok ()) :
    SourceBranchesHaveCompletion solution names scope result completion ((condition, body) :: rest) := by
  simp [validateStatementBranchesCompletion, except_bind_ok, exists_require_ok] at accepted
  obtain ⟨_, guardAccepted, rfl, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
  exact .cons (exprIH (some (names, result, binderTypes)) scope solution condition guard _ guardAccepted)
    (blockIH names scope solution binderTypes result completion body nodes bodyAccepted)
    (branchesIH names scope solution binderTypes result completion rest candidates remainingAccepted)

theorem conditional_completion_sound (fuel : Nat) (_exprIH : ContextExpressionSound fuel) (blockIH : BlockCompletionSound fuel)
    (branchesIH : BranchCompletionSound fuel) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (completion : Option MonoType) (branches : List (Expr × List Statement))
    (otherwise : Option (List Statement)) (rest : List Statement)
    (candidates : List (Node × List StatementNode)) (fallback nodes : List StatementNode)
    (accepted : validateBlockCompletion (fuel + 1) names scope solution binderTypes result completion
      (.ifStmt branches otherwise :: rest) (.conditional candidates fallback :: nodes) = .ok ()) :
    SourceBlockHasCompletion solution names scope result completion (.ifStmt branches otherwise :: rest) := by
  cases rest with
  | nil =>
    cases otherwise with
    | none =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨_, fallbackEmpty⟩, completionAccepted, _⟩ := accepted
      exact .conditionalNoFallbackTail
        (branchesIH names scope solution binderTypes result completion branches candidates branchAccepted)
        (check_completion_sound completion (.primitive .unit) completionAccepted)
    | some body =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, _⟩ := accepted
      exact .conditionalTail
        (branchesIH names scope solution binderTypes result completion branches candidates branchAccepted)
        (blockIH names scope solution binderTypes result completion body fallback bodyAccepted)
  | cons next rest =>
    cases otherwise with
    | none =>
      simp [validateBlockCompletion, except_bind_ok, requireCheck] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, _, _, remainingAccepted⟩ := accepted
      exact .conditionalNoFallbackNext
        (branchesIH names scope solution binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution binderTypes result completion (next :: rest) nodes remainingAccepted)
    | some body =>
      simp [validateBlockCompletion, except_bind_ok] at accepted
      obtain ⟨⟨⟨⟩, branchAccepted⟩, ⟨⟨⟩, bodyAccepted⟩, remainingAccepted⟩ := accepted
      exact .conditionalNext
        (branchesIH names scope solution binderTypes result none branches candidates branchAccepted)
        (blockIH names scope solution binderTypes result none body fallback bodyAccepted)
        (blockIH names scope solution binderTypes result completion (next :: rest) nodes remainingAccepted)

theorem context_validate_all_sound : ∀ fuel,
    ContextExpressionSound fuel ∧ ContextArgumentsSound fuel ∧ ContextFieldsSound fuel ∧ ContextBranchesSound fuel ∧ BlockCompletionSound fuel ∧ BranchCompletionSound fuel := by
  intro fuel
  induction fuel with
  | zero =>
    simp [ContextExpressionSound, ContextArgumentsSound, ContextFieldsSound, ContextBranchesSound, BlockCompletionSound, BranchCompletionSound, validateBlockCompletion, validateStatementBranchesCompletion, validateExpressionIn,
      validateArgumentsIn, validateRecordFieldsIn, validateBranchesIn]
  | succ fuel ih =>
    obtain ⟨exprIH, argsIH, fieldsIH, branchesIH, blockIH, statementBranchesIH⟩ := ih
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
    · intro context scope solution source node ty accepted
      cases source with
      | literal value =>
        cases value <;> cases node <;> first
          | exact context_validate_integer_sound fuel context scope solution _ _ ty accepted
          | exact context_validate_boolean_sound fuel context scope solution _ _ ty accepted
          | exact context_validate_string_sound fuel context scope solution _ _ ty accepted
          | cases accepted
      | ident name =>
        cases node <;> first
          | exact context_validate_read_sound fuel context scope solution name _ _ ty accepted
          | cases accepted
      | binary operator left right =>
        cases node <;> first
          | exact context_validate_binary_sound fuel context exprIH scope solution operator _ left right _ _ ty accepted
          | cases accepted
      | call callee arguments =>
        cases node <;> first
          | exact context_validate_call_sound fuel context exprIH argsIH scope solution callee arguments _ _ ty accepted
          | cases accepted
      | record fields =>
        cases node <;> first
          | exact context_validate_record_sound fuel context fieldsIH scope solution fields _ ty accepted
          | cases accepted
      | field base name =>
        cases node <;> first
          | exact context_validate_field_sound fuel context exprIH scope solution base name _ _ ty accepted
          | cases accepted
      | ifExpr branches otherwise =>
        cases node <;> first
          | exact context_validate_conditional_sound fuel context exprIH branchesIH scope solution branches otherwise _ _ ty accepted
          | cases accepted
      | require value schema =>
        cases schema with
        | none => cases node <;> cases accepted
        | some annotation =>
          cases annotation <;> cases node <;> first
            | exact context_validate_require_sound fuel context exprIH scope solution value _ _ _ _ ty accepted
            | cases accepted
      | tryExpr value =>
        cases node <;> first
          | exact context_validate_propagate_sound fuel context exprIH scope solution value _ _ ty accepted
          | cases accepted
      | valueBlock body =>
        cases node with
        | valueBlock rawCompletion nodes =>
          cases context with
          | none => cases accepted
          | some context =>
            obtain ⟨names, result, binderTypes⟩ := context
            simp [validateExpressionIn, except_map_ok] at accepted
            obtain ⟨⟨⟨⟩, bodyAccepted⟩, rfl⟩ := accepted
            exact .valueBlock (blockIH names scope solution binderTypes result _ body nodes bodyAccepted)
        | _ => cases accepted
      | _ => cases node <;> cases accepted
    · intro context scope solution arguments nodes types accepted
      cases arguments with
      | nil =>
        cases nodes with
        | nil => cases accepted; exact .nil
        | cons _ _ => cases accepted
      | cons argument rest =>
        cases argument <;> cases nodes <;> first
          | exact context_validate_argument_cons_sound fuel context exprIH argsIH scope solution _ rest _ _ types accepted
          | cases accepted
    · intro context scope solution fields nodes types accepted
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
            | exact context_validate_field_cons_sound fuel context exprIH fieldsIH scope solution _ name _ rest node tail types accepted
            | cases accepted
    · intro context scope solution result branches nodes accepted
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
              exact context_validate_branch_cons_sound fuel context exprIH branchesIH scope solution result
                condition value rest guard node tail accepted


    · intro names scope solution binderTypes result completion source candidate accepted
      cases source with
      | nil =>
        cases candidate with
        | nil =>
          simp [validateBlockCompletion, requireCheck] at accepted
          exact .nil (check_completion_sound completion (.primitive .unit) accepted)
        | cons _ _ => cases accepted
      | cons statement rest =>
        cases candidate with
        | nil => cases accepted
        | cons node nodes =>
          cases statement with
          | binding kind target declared initializer =>
            cases target <;> cases node <;> first
              | exact binding_completion_sound fuel exprIH blockIH names scope solution binderTypes result completion
                  kind _ declared initializer rest _ _ _ nodes accepted
              | cases accepted
          | assign target op value =>
            cases target <;> cases op <;> cases node <;> first
              | exact assignment_completion_sound fuel exprIH blockIH names scope solution binderTypes result completion
                  _ value rest _ _ nodes accepted
              | cases accepted
          | returnValue value =>
            cases value <;> cases node <;> first
              | exact return_completion_sound fuel exprIH blockIH names scope solution binderTypes result completion
                  _ rest _ nodes accepted
              | cases accepted
          | tailName name =>
            cases node <;> first
              | exact tail_name_completion_sound fuel exprIH names scope solution binderTypes result completion
                  name rest _ nodes accepted
              | cases accepted
          | expression value =>
            cases node with
            | evaluated node =>
              exact evaluated_completion_sound fuel exprIH blockIH names scope solution binderTypes result completion
                value rest node nodes accepted
            | _ => cases value <;> cases accepted
          | ifStmt branches otherwise =>
            cases node <;> first
              | exact conditional_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution binderTypes
                  result completion branches otherwise rest _ _ nodes accepted
              | cases accepted
          | _ => cases node <;> cases accepted
    · intro names scope solution binderTypes result completion source candidate accepted
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
              exact branch_completion_sound fuel exprIH blockIH statementBranchesIH names scope solution binderTypes result
                completion condition body rest guard nodes candidates accepted


theorem validate_blocks_sound (fuel : Nat) : BlockSound fuel ∧ StatementBranchesSound fuel := by
  constructor
  · intro names scope solution binderTypes result source candidate accepted
    exact (context_validate_all_sound fuel).2.2.2.2.1 names scope solution binderTypes result (some result)
      source candidate accepted
  · intro names scope solution binderTypes result source candidate accepted
    exact (context_validate_all_sound fuel).2.2.2.2.2 names scope solution binderTypes result (some result)
      source candidate accepted

theorem accepted_block_has_type (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode)
    (accepted : validateBlock fuel names scope solution binderTypes result source candidate = .ok ()) :
    BlockHasType names solution scope result source :=
  (validate_blocks_sound fuel).1 names scope solution binderTypes result source candidate accepted

end Xsh.Typing.Connected
