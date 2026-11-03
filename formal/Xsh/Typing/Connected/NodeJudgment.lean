import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive NodeHasType (scope : Scope) (solution : Substitution) : Node → MonoType → Prop where
    | integer {lexeme : String} {value : Xsh.Values.IntValue} :
        parseInteger lexeme = some value → NodeHasType scope solution (.integer lexeme) (.primitive .integer)
    | boolean {value : Bool} : NodeHasType scope solution (.boolean value) (.primitive .boolean)
    | string {value : String} : NodeHasType scope solution (.string value) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : List (Variable × MonoType)}
        {ty : MonoType} {requirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema →
        instantiationKeysValid binding.scheme arguments = true →
        instantiationSortsValid solution arguments = true →
        Instantiates binding.scheme (witnessSubstitution arguments) ty requirements →
        Solves requirements solution → WellFormed (substitute solution ty) →
        typeSort (substitute solution ty) = .value →
        NodeHasType scope solution (.read binding.binder arguments) (substitute solution ty)
    | binary {operator : BinaryOp} {left right : Node} {leftType rightType result : MonoType} :
        NodeHasType scope solution left leftType → NodeHasType scope solution right rightType →
        BinaryTyping operator leftType rightType result →
        NodeHasType scope solution (.binary operator left right) result
    | call {callee : Node} {arguments : List Node} {argumentTypes result : MonoType} :
        NodeHasType scope solution callee (.function argumentTypes result) →
        NodesHaveTypes scope solution arguments argumentTypes →
        NodeHasType scope solution (.call callee arguments) result
    | record {fields : List (NameId × Node)} {types : List (NameId × MonoType)} :
        NodeFieldsHaveTypes scope solution fields types → (types.map Prod.fst).Nodup →
        NodeHasType scope solution (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Node} {name : NameId} {row result : MonoType} :
        NodeHasType scope solution base (.record row) → FieldOccurs name result row →
        NodeHasType scope solution (.field base name) result
    | conditional {branches : List (Node × Node)} {otherwise : Node} {result : MonoType} :
        NodeBranchesHaveType scope solution branches result → NodeHasType scope solution otherwise result →
        NodeHasType scope solution (.conditional branches otherwise) result
    | requireValue {value : Node} {name : NameId} {binding : Binding}
        {identity : Xsh.Validation.Nominal.NominalId} {row input : MonoType} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        NodeHasType scope solution value input →
        NodeHasType scope solution (.requireValue binding.binder identity value)
          (.result (.nominal identity (.record (substitute solution row))) (.primitive .error))
    | propagate {value : Node} {result error expectedError : MonoType} :
        NodeHasType scope solution value (.result result error) →
        error = substitute solution expectedError →
        NodeHasType scope solution (.propagate value expectedError) result

  inductive NodesHaveTypes (scope : Scope) (solution : Substitution) : List Node → MonoType → Prop where
    | nil : NodesHaveTypes scope solution [] .argumentsEmpty
    | cons {node : Node} {rest : List Node} {ty types : MonoType} :
        NodeHasType scope solution node ty → NodesHaveTypes scope solution rest types →
        NodesHaveTypes scope solution (node :: rest) (.argument ty types)

  inductive NodeFieldsHaveTypes (scope : Scope) (solution : Substitution) :
      List (NameId × Node) → List (NameId × MonoType) → Prop where
    | nil : NodeFieldsHaveTypes scope solution [] []
    | cons {name : NameId} {node : Node} {rest : List (NameId × Node)}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        NodeHasType scope solution node ty → NodeFieldsHaveTypes scope solution rest types →
        NodeFieldsHaveTypes scope solution ((name, node) :: rest) ((name, ty) :: types)

  inductive NodeBranchesHaveType (scope : Scope) (solution : Substitution) :
      List (Node × Node) → MonoType → Prop where
    | nil {result : MonoType} : NodeBranchesHaveType scope solution [] result
    | cons {condition branch : Node} {rest : List (Node × Node)} {result : MonoType} :
        NodeHasType scope solution condition (.primitive .boolean) →
        NodeHasType scope solution branch result → NodeBranchesHaveType scope solution rest result →
        NodeBranchesHaveType scope solution ((condition, branch) :: rest) result
end

end Xsh.Typing.Connected
