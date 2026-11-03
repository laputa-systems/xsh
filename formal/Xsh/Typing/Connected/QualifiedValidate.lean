import Xsh.Typing.Connected.QualifiedJudgment
import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

abbrev FieldResults := Node → Option MonoType

mutual
  def validateQualifiedExpression : Nat → Scope → Substitution → List Constraint → FieldResults →
      Expr → Node → Except ValidationError MonoType
    | 0, _, _, _, _, _, _ => .error .exhausted
    | fuel + 1, scope, principal, requirements, fieldResults, source, candidate => do
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
          let actualLeft ← validateQualifiedExpression fuel scope principal requirements fieldResults left first
          let actualRight ← validateQualifiedExpression fuel scope principal requirements fieldResults right second
          requireCheck (decide (actualLeft = leftType) && decide (actualRight = rightType))
          pure result
      | .call callee arguments, .call target recordedArguments => do
          let calleeType ← validateQualifiedExpression fuel scope principal requirements fieldResults callee target
          let argumentTypes ← validateQualifiedArguments fuel scope principal requirements fieldResults arguments recordedArguments
          match calleeType with
          | .function parameters result => do
              requireCheck (decide (parameters = argumentTypes))
              pure result
          | _ => throw .mismatch
      | .record fields, .record recordedFields => do
          let types ← validateQualifiedFields fuel scope principal requirements fieldResults fields recordedFields
          requireCheck (decide (types.map Prod.fst).Nodup)
          pure (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
      | .field base name, .field recorded recordedName => do
          requireCheck (decide (name = recordedName))
          let baseType ← validateQualifiedExpression fuel scope principal requirements fieldResults base recorded
          let some result := fieldResults candidate | throw .mismatch
          requireCheck (decide (Constraint.field baseType name result ∈ requirements))
          pure result
      | .ifExpr branches otherwise, .conditional recorded fallback => do
          let result ← validateQualifiedExpression fuel scope principal requirements fieldResults otherwise fallback
          validateQualifiedBranches fuel scope principal requirements fieldResults result branches recorded
          pure result
      | .require value (some (.named name)), .requireValue binder identity recorded => do
          let some binding := scope.find? (fun item => decide (item.name = name)) | throw .unresolvedName
          requireCheck (decide (binding.binder = binder) && decide (binding.kind = .schema))
          requireCheck (binding.scheme.quantified.isEmpty && binding.scheme.requirements.isEmpty)
          let .nominal expected (.record row) := binding.scheme.body | throw .mismatch
          requireCheck (decide (identity = expected) && checksWellFormed (.nominal expected (.record row)))
          let _ ← validateQualifiedExpression fuel scope principal requirements fieldResults value recorded
          pure (.result (.nominal expected (.record (substitute principal row))) (.primitive .error))
      | .tryExpr value, .propagate recorded expectedError => do
          let result ← validateQualifiedExpression fuel scope principal requirements fieldResults value recorded
          match result with
          | .result ty error => do
              requireCheck (decide (error = substitute principal expectedError))
              pure ty
          | _ => throw .mismatch
      | _, _ => throw .mismatch

  def validateQualifiedArguments : Nat → Scope → Substitution → List Constraint → FieldResults →
      List CallArgument → List Node → Except ValidationError MonoType
    | 0, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, [], [] => .ok .argumentsEmpty
    | fuel + 1, scope, principal, requirements, fields, .positional value :: rest, node :: nodes => do
        let ty ← validateQualifiedExpression fuel scope principal requirements fields value node
        let others ← validateQualifiedArguments fuel scope principal requirements fields rest nodes
        pure (.argument ty others)
    | _ + 1, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedFields : Nat → Scope → Substitution → List Constraint → FieldResults →
      List RecordField → List (NameId × Node) → Except ValidationError (List (NameId × MonoType))
    | 0, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, [], [] => .ok []
    | fuel + 1, scope, principal, requirements, fields, .named name value :: rest, (recorded, node) :: nodes => do
        requireCheck (decide (name = recorded))
        let ty ← validateQualifiedExpression fuel scope principal requirements fields value node
        let others ← validateQualifiedFields fuel scope principal requirements fields rest nodes
        pure ((name, ty) :: others)
    | _ + 1, _, _, _, _, _, _ => .error .mismatch

  def validateQualifiedBranches : Nat → Scope → Substitution → List Constraint → FieldResults → MonoType →
      List (Expr × Expr) → List (Node × Node) → Except ValidationError Unit
    | 0, _, _, _, _, _, _, _ => .error .exhausted
    | _ + 1, _, _, _, _, _, [], [] => .ok ()
    | fuel + 1, scope, principal, requirements, fields, result, (condition, value) :: rest, (guard, node) :: nodes => do
        let guardType ← validateQualifiedExpression fuel scope principal requirements fields condition guard
        requireCheck (decide (guardType = .primitive .boolean))
        let branchType ← validateQualifiedExpression fuel scope principal requirements fields value node
        requireCheck (decide (branchType = result))
        validateQualifiedBranches fuel scope principal requirements fields result rest nodes
    | _ + 1, _, _, _, _, _, _, _ => .error .mismatch
end

end Xsh.Typing.Connected
