import Xsh.Typing.Connected.NodeJudgment
import Xsh.Typing.Connected.QualifiedJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive QualifiedNode (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      Node → MonoType → Prop where
    | integer {lexeme : String} {value : Xsh.Values.IntValue} :
        parseInteger lexeme = some value → QualifiedNode scope principal requirements (.integer lexeme) (.primitive .integer)
    | boolean {value : Bool} : QualifiedNode scope principal requirements (.boolean value) (.primitive .boolean)
    | string {value : String} : QualifiedNode scope principal requirements (.string value) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : List (Variable × MonoType)}
        {ty : MonoType} {instantiatedRequirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema → instantiationKeysValid binding.scheme arguments = true →
        instantiationSortsValid principal arguments = true →
        Instantiates binding.scheme (witnessSubstitution arguments) ty instantiatedRequirements →
        (∀ constraint ∈ instantiatedRequirements, mapConstraint principal constraint ∈ requirements) →
        WellFormed (substitute principal ty) → typeSort (substitute principal ty) = .value →
        QualifiedNode scope principal requirements (.read binding.binder arguments) (substitute principal ty)
    | binary {operator : BinaryOp} {left right : Node} {leftType rightType result : MonoType} :
        QualifiedNode scope principal requirements left leftType → QualifiedNode scope principal requirements right rightType →
        BinaryTyping operator leftType rightType result →
        QualifiedNode scope principal requirements (.binary operator left right) result
    | call {callee : Node} {arguments : List Node} {argumentTypes result : MonoType} :
        QualifiedNode scope principal requirements callee (.function argumentTypes result) →
        QualifiedNodes scope principal requirements arguments argumentTypes →
        QualifiedNode scope principal requirements (.call callee arguments) result
    | record {fields : List (NameId × Node)} {types : List (NameId × MonoType)} :
        QualifiedNodeFields scope principal requirements fields types → (types.map Prod.fst).Nodup →
        QualifiedNode scope principal requirements (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Node} {name : NameId} {record result : MonoType} :
        QualifiedNode scope principal requirements base record → Constraint.field record name result ∈ requirements →
        QualifiedNode scope principal requirements (.field base name) result
    | conditional {branches : List (Node × Node)} {otherwise : Node} {result : MonoType} :
        QualifiedNodeBranches scope principal requirements branches result →
        QualifiedNode scope principal requirements otherwise result →
        QualifiedNode scope principal requirements (.conditional branches otherwise) result
    | requireValue {value : Node} {name : NameId} {binding : Binding}
        {identity : Xsh.Validation.Nominal.NominalId} {row input : MonoType} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        QualifiedNode scope principal requirements value input →
        QualifiedNode scope principal requirements (.requireValue binding.binder identity value)
          (.result (.nominal identity (.record (substitute principal row))) (.primitive .error))
    | propagate {value : Node} {result error expectedError : MonoType} :
        QualifiedNode scope principal requirements value (.result result error) →
        error = substitute principal expectedError →
        QualifiedNode scope principal requirements (.propagate value expectedError) result

  inductive QualifiedNodes (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List Node → MonoType → Prop where
    | nil : QualifiedNodes scope principal requirements [] .argumentsEmpty
    | cons {node : Node} {rest : List Node} {ty types : MonoType} :
        QualifiedNode scope principal requirements node ty → QualifiedNodes scope principal requirements rest types →
        QualifiedNodes scope principal requirements (node :: rest) (.argument ty types)

  inductive QualifiedNodeFields (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List (NameId × Node) → List (NameId × MonoType) → Prop where
    | nil : QualifiedNodeFields scope principal requirements [] []
    | cons {name : NameId} {node : Node} {rest : List (NameId × Node)}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        QualifiedNode scope principal requirements node ty → QualifiedNodeFields scope principal requirements rest types →
        QualifiedNodeFields scope principal requirements ((name, node) :: rest) ((name, ty) :: types)

  inductive QualifiedNodeBranches (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List (Node × Node) → MonoType → Prop where
    | nil {result : MonoType} : QualifiedNodeBranches scope principal requirements [] result
    | cons {condition branch : Node} {rest : List (Node × Node)} {result : MonoType} :
        QualifiedNode scope principal requirements condition (.primitive .boolean) →
        QualifiedNode scope principal requirements branch result →
        QualifiedNodeBranches scope principal requirements rest result →
        QualifiedNodeBranches scope principal requirements ((condition, branch) :: rest) result
end


theorem qualified_node_instantiates (scope : Scope) (principal later : Substitution)
    (requirements : List Constraint) (source : Node) (ty : MonoType)
    (typed : QualifiedNode scope principal requirements source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeHasType scope (compose later principal) source (substitute later ty) := by
  refine @QualifiedNode.rec scope principal requirements
    (fun source ty _ => NodeHasType scope (compose later principal) source (substitute later ty))
    (fun source types _ => NodesHaveTypes scope (compose later principal) source (substitute later types))
    (fun source types _ => NodeFieldsHaveTypes scope (compose later principal) source (substituteFields later types))
    (fun source ty _ => NodeBranchesHaveType scope (compose later principal) source (substitute later ty))
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ source ty typed
  · intro lexeme value parsed
    exact .integer parsed
  · intro value
    exact .boolean
  · intro value
    exact .string
  · intro name binding arguments raw instantiatedRequirements found notSchema keys sorts instantiated included wellFormed sort
    have mapped : Solves (instantiatedRequirements.map (mapConstraint principal)) later := by
      intro constraint member
      obtain ⟨original, present, rfl⟩ := List.mem_map.mp member
      exact solves _ (included original present)
    have requirementsSolved := (solves_mapped instantiatedRequirements principal later).mp mapped
    have resultingWellFormed : WellFormed (substitute (compose later principal) raw) := by
      rw [substitute_compose]
      exact substitute_well_formed later sorted _ wellFormed
    have resultingSort : typeSort (substitute (compose later principal) raw) = .value := by
      rw [substitute_compose, substitute_sort later sorted]
      exact sort
    rw [← substitute_compose]
    exact .read found notSchema keys (instantiation_sorts_compose principal later arguments sorted sorts) instantiated requirementsSolved resultingWellFormed resultingSort
  · intro operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply NodeHasType.record fieldsIH
    simpa [substituteFields, List.map_map, Function.comp_def] using unique
  · intro base name record result baseTyped member baseIH
    obtain ⟨row, same, field⟩ := solves _ member
    rw [same] at baseIH
    exact .field baseIH field
  · intro branches otherwise result branchesTyped otherwiseTyped branchesIH otherwiseIH
    exact .conditional branchesIH otherwiseIH
  · intro value name binding identity row input found schema quant empty shape wellFormed typed valueIH
    simp only [substitute, ← substitute_compose]
    exact .requireValue found schema quant empty shape wellFormed valueIH
  · intro value result error expectedError typed same valueIH
    apply NodeHasType.propagate valueIH
    rw [substitute_compose]
    exact congrArg (substitute later) same
  · exact .nil
  · intro expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · exact .nil
  · intro name expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · intro result
    exact .nil
  · intro condition branch rest result conditionTyped branchTyped restTyped conditionIH branchIH restIH
    exact .cons conditionIH branchIH restIH


end Xsh.Typing.Connected
