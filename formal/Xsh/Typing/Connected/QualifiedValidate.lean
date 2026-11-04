import Xsh.Typing.Connected.QualifiedJudgment
import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

abbrev FieldResults := Node → Option MonoType

mutual
  def validateQualifiedExpressionIn : Nat → ValidationContext → Scope → Substitution → List Constraint → FieldResults →
      Expr → Node → Except ValidationError MonoType
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | fuel + 1, context, scope, principal, requirements, fieldResults, source, candidate => do
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
          requireCheck (instantiationSortsValid principal arguments)
          let instantiation := schemeSubstitution binding.scheme (witnessSubstitution arguments)
          let instantiated := binding.scheme.requirements.map (mapConstraint instantiation)
          requireCheck (instantiated.all fun constraint => decide (mapConstraint principal constraint ∈ requirements))
          let ty := substitute principal (substitute instantiation binding.scheme.body)
          requireCheck (checksWellFormed ty && decide (typeSort ty = .value))
          pure ty
      | .binary operator left right, .binary recorded first second => do
          requireCheck (decide (operator = recorded))
          let some (leftType, rightType, result) := binarySignature operator | throw .unsupported
          let actualLeft ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults left first
          let actualRight ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults right second
          requireCheck (decide (actualLeft = leftType) && decide (actualRight = rightType))
          pure result
      | .call callee arguments, .call target recordedArguments => do
          let calleeType ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults callee target
          let argumentTypes ← validateQualifiedArgumentsIn fuel context scope principal requirements fieldResults arguments recordedArguments
          match calleeType with
          | .function parameters result => do
              requireCheck (decide (parameters = argumentTypes))
              pure result
          | _ => throw .mismatch
      | .record fields, .record recordedFields => do
          let types ← validateQualifiedFieldsIn fuel context scope principal requirements fieldResults fields recordedFields
          requireCheck (decide (types.map Prod.fst).Nodup)
          pure (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
      | .field base name, .field recorded recordedName => do
          requireCheck (decide (name = recordedName))
          let baseType ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults base recorded
          let some result := fieldResults candidate | throw .mismatch
          requireCheck (decide (Constraint.field baseType name result ∈ requirements))
          pure result
      | .ifExpr branches otherwise, .conditional recorded fallback => do
          let result ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults otherwise fallback
          validateQualifiedBranchesIn fuel context scope principal requirements fieldResults result branches recorded
          pure result
      | .require value (some (.named name)), .requireValue binder identity recorded => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .schema))
          requireCheck (binding.scheme.quantified.isEmpty && binding.scheme.requirements.isEmpty)
          let .nominal expected (.record row) := binding.scheme.body | throw .mismatch
          requireCheck (decide (identity = expected) && checksWellFormed (.nominal expected (.record row)))
          let _ ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults value recorded
          pure (.result (.nominal expected (.record (substitute principal row))) (.primitive .error))
      | .tryExpr value, .propagate recorded expectedError => do
          let result ← validateQualifiedExpressionIn fuel context scope principal requirements fieldResults value recorded
          match result with
          | .result ty error => do
              requireCheck (decide (error = substitute principal expectedError))
              pure ty
          | _ => throw .mismatch
      | .valueBlock body, .valueBlock rawCompletion nodes => do
          let some (names, result, binderTypes) := context | throw .unsupported
          let completion := substitute principal rawCompletion
          validateQualifiedBlockCompletion fuel names scope principal requirements fieldResults binderTypes result (some completion) body nodes
          pure completion
      | _, _ => throw .mismatch

  def validateQualifiedArgumentsIn : Nat → ValidationContext → Scope → Substitution → List Constraint → FieldResults →
      List CallArgument → List Node → Except ValidationError MonoType
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, [], [] => .ok .argumentsEmpty
    | fuel + 1, context, scope, principal, requirements, fields, .positional value :: rest, node :: nodes => do
        let ty ← validateQualifiedExpressionIn fuel context scope principal requirements fields value node
        let others ← validateQualifiedArgumentsIn fuel context scope principal requirements fields rest nodes
        pure (.argument ty others)
    | _ + 1, _, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedFieldsIn : Nat → ValidationContext → Scope → Substitution → List Constraint → FieldResults →
      List RecordField → List (NameId × Node) → Except ValidationError (List (NameId × MonoType))
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, [], [] => .ok []
    | fuel + 1, context, scope, principal, requirements, fields, .named name value :: rest, (recorded, node) :: nodes => do
        requireCheck (decide (name = recorded))
        let ty ← validateQualifiedExpressionIn fuel context scope principal requirements fields value node
        let others ← validateQualifiedFieldsIn fuel context scope principal requirements fields rest nodes
        pure ((name, ty) :: others)
    | _ + 1, _, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedBranchesIn : Nat → ValidationContext → Scope → Substitution → List Constraint → FieldResults → MonoType →
      List (Expr × Expr) → List (Node × Node) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, context, scope, principal, requirements, fields, result, (condition, value) :: rest, (guard, node) :: nodes => do
        let guardType ← validateQualifiedExpressionIn fuel context scope principal requirements fields condition guard
        requireCheck (decide (guardType = .primitive .boolean))
        let branchType ← validateQualifiedExpressionIn fuel context scope principal requirements fields value node
        requireCheck (decide (branchType = result))
        validateQualifiedBranchesIn fuel context scope principal requirements fields result rest nodes
    | _ + 1, _, _, _, _, _, _, _, _ => .error .mismatch
  def validateQualifiedBlockCompletion : Nat → TypeNames → Scope → Substitution → List Constraint →
      FieldResults → BinderTypes → MonoType → Option MonoType → List Statement → List StatementNode →
      Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, completion, [], [] =>
        requireCheck (checkCompletion completion (.primitive .unit))
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result, completion, statement :: rest, node :: nodes => do
      match statement, node with
      | .binding kind (.name name) declared initializer, .binding binder recordedKind storage candidate => do
          requireCheck (decide (bindingKind kind = recordedKind))
          requireCheck (!(scope.any fun item => decide (item.binder = binder)))
          let some rawType := binderTypes binder | throw .mismatch
          requireCheck (decide (storage = rawType))
          let valueType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields initializer candidate
          requireCheck (checksStoreCompatible (substitute solution rawType) valueType || decide (Constraint.store (substitute solution rawType) valueType ∈ requirements))
          requireCheck (checkAnnotation names solution declared (substitute solution rawType))
          let binding : Binding := ⟨name, binder, recordedKind, ⟨[], rawType, []⟩⟩
          validateQualifiedBlockCompletion fuel names (binding :: scope) solution requirements fields binderTypes result completion rest nodes
      | .assign (.name name) .set value, .assignment binder candidate => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .mutable))
          let valueType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields value candidate
          requireCheck (checksStoreCompatible (substitute solution binding.scheme.body) valueType || decide (Constraint.store (substitute solution binding.scheme.body) valueType ∈ requirements))
          validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | .returnValue (some value), .returned candidate => do
          let valueType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields value candidate
          requireCheck (decide (valueType = result))
          validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result none rest nodes
      | .tailName name, .evaluated candidate => do
          requireCheck (rest.isEmpty && nodes.isEmpty)
          let valueType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields (.ident name) candidate
          requireCheck (checkCompletion completion valueType)
      | .expression value, .evaluated candidate => do
          let valueType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields value candidate
          if rest.isEmpty then do
            requireCheck nodes.isEmpty
            requireCheck (checkCompletion completion valueType)
          else validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | .ifStmt branches otherwise, .conditional candidates fallback => do
          let branchCompletion := if rest.isEmpty then completion else none
          validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result branchCompletion branches candidates
          match otherwise with
          | none => do
              requireCheck fallback.isEmpty
              requireCheck (checkCompletion branchCompletion (.primitive .unit))
          | some body => validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result branchCompletion body fallback
          if rest.isEmpty then requireCheck nodes.isEmpty
          else validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion rest nodes
      | _, _ => throw .mismatch
    | _ + 1, _, _, _, _, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedBranchesCompletion : Nat → TypeNames → Scope → Substitution → List Constraint →
      FieldResults → BinderTypes → MonoType → Option MonoType → List (Expr × List Statement) →
      List (Node × List StatementNode) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, names, scope, solution, requirements, fields, binderTypes, result, completion,
        (condition, body) :: rest, (guard, nodes) :: candidates => do
      let guardType ← validateQualifiedExpressionIn fuel (some (names, result, binderTypes)) scope solution requirements fields condition guard
      requireCheck (decide (guardType = .primitive .boolean))
      validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result completion body nodes
      validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result completion rest candidates
    | _ + 1, _, _, _, _, _, _, _, _, _, _ => .error .mismatch
end

def validateQualifiedExpression (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : Expr) (candidate : Node) : Except ValidationError MonoType :=
  validateQualifiedExpressionIn fuel none scope principal requirements fields source candidate

def validateQualifiedArguments (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : List CallArgument) (candidate : List Node) : Except ValidationError MonoType :=
  validateQualifiedArgumentsIn fuel none scope principal requirements fields source candidate

def validateQualifiedFields (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : List RecordField) (candidate : List (NameId × Node)) : Except ValidationError (List (NameId × MonoType)) :=
  validateQualifiedFieldsIn fuel none scope principal requirements fields source candidate

def validateQualifiedBranches (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (result : MonoType) (source : List (Expr × Expr)) (candidate : List (Node × Node)) : Except ValidationError Unit :=
  validateQualifiedBranchesIn fuel none scope principal requirements fields result source candidate

end Xsh.Typing.Connected
