import Xsh.Typing.Connected.NodeJudgment
import Xsh.Typing.Connected.QualifiedJudgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive QualifiedNodeIn (principal : Substitution) (requirements : List Constraint) :
      Option MonoType → Scope →
      Node → MonoType → Prop where
    | integer {lexeme : String} {value : Xsh.Values.IntValue} :
        parseInteger lexeme = some value → QualifiedNodeIn principal requirements context scope (.integer lexeme) (.primitive .integer)
    | boolean {value : Bool} : QualifiedNodeIn principal requirements context scope (.boolean value) (.primitive .boolean)
    | string {value : String} : QualifiedNodeIn principal requirements context scope (.string value) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : List (Variable × MonoType)}
        {ty : MonoType} {instantiatedRequirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema → instantiationKeysValid binding.scheme arguments = true →
        instantiationSortsValid principal arguments = true →
        Instantiates binding.scheme (witnessSubstitution arguments) ty instantiatedRequirements →
        (∀ constraint ∈ instantiatedRequirements, mapConstraint principal constraint ∈ requirements) →
        WellFormed (substitute principal ty) → typeSort (substitute principal ty) = .value →
        QualifiedNodeIn principal requirements context scope (.read binding.binder arguments) (substitute principal ty)
    | binary {operator : BinaryOp} {left right : Node} {leftType rightType result : MonoType} :
        QualifiedNodeIn principal requirements context scope left leftType → QualifiedNodeIn principal requirements context scope right rightType →
        BinaryTyping operator leftType rightType result →
        QualifiedNodeIn principal requirements context scope (.binary operator left right) result
    | call {callee : Node} {arguments : List Node} {argumentTypes result : MonoType} :
        QualifiedNodeIn principal requirements context scope callee (.function argumentTypes result) →
        QualifiedNodesIn principal requirements context scope arguments argumentTypes →
        QualifiedNodeIn principal requirements context scope (.call callee arguments) result
    | record {fields : List (NameId × Node)} {types : List (NameId × MonoType)} :
        QualifiedNodeFieldsIn principal requirements context scope fields types → (types.map Prod.fst).Nodup →
        QualifiedNodeIn principal requirements context scope (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Node} {name : NameId} {record result : MonoType} :
        QualifiedNodeIn principal requirements context scope base record → Constraint.field record name result ∈ requirements →
        QualifiedNodeIn principal requirements context scope (.field base name) result
    | conditional {branches : List (Node × Node)} {otherwise : Node} {result : MonoType} :
        QualifiedNodeBranchesIn principal requirements context scope branches result →
        QualifiedNodeIn principal requirements context scope otherwise result →
        QualifiedNodeIn principal requirements context scope (.conditional branches otherwise) result
    | requireValue {value : Node} {name : NameId} {binding : Binding}
        {identity : Xsh.Validation.Nominal.NominalId} {row input : MonoType} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        QualifiedNodeIn principal requirements context scope value input →
        QualifiedNodeIn principal requirements context scope (.requireValue binding.binder identity value)
          (.result (.nominal identity (.record (substitute principal row))) (.primitive .error))
    | propagate {value : Node} {result error expectedError : MonoType} :
        QualifiedNodeIn principal requirements context scope value (.result result error) →
        error = substitute principal expectedError →
        QualifiedNodeIn principal requirements context scope (.propagate value expectedError) result

    | valueBlock {functionResult scope body rawCompletion} :
        QualifiedNodeBlockHasCompletionIn principal requirements scope functionResult (some (substitute principal rawCompletion)) body →
        QualifiedNodeIn principal requirements (some functionResult) scope (.valueBlock rawCompletion body) (substitute principal rawCompletion)

  inductive QualifiedNodesIn (principal : Substitution) (requirements : List Constraint) :
      Option MonoType → Scope →
      List Node → MonoType → Prop where
    | nil : QualifiedNodesIn principal requirements context scope [] .argumentsEmpty
    | cons {node : Node} {rest : List Node} {ty types : MonoType} :
        QualifiedNodeIn principal requirements context scope node ty → QualifiedNodesIn principal requirements context scope rest types →
        QualifiedNodesIn principal requirements context scope (node :: rest) (.argument ty types)

  inductive QualifiedNodeFieldsIn (principal : Substitution) (requirements : List Constraint) :
      Option MonoType → Scope →
      List (NameId × Node) → List (NameId × MonoType) → Prop where
    | nil : QualifiedNodeFieldsIn principal requirements context scope [] []
    | cons {name : NameId} {node : Node} {rest : List (NameId × Node)}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        QualifiedNodeIn principal requirements context scope node ty → QualifiedNodeFieldsIn principal requirements context scope rest types →
        QualifiedNodeFieldsIn principal requirements context scope ((name, node) :: rest) ((name, ty) :: types)

  inductive QualifiedNodeBranchesIn (principal : Substitution) (requirements : List Constraint) :
      Option MonoType → Scope →
      List (Node × Node) → MonoType → Prop where
    | nil {result : MonoType} : QualifiedNodeBranchesIn principal requirements context scope [] result
    | cons {condition branch : Node} {rest : List (Node × Node)} {result : MonoType} :
        QualifiedNodeIn principal requirements context scope condition (.primitive .boolean) →
        QualifiedNodeIn principal requirements context scope branch result →
        QualifiedNodeBranchesIn principal requirements context scope rest result →
        QualifiedNodeBranchesIn principal requirements context scope ((condition, branch) :: rest) result
  inductive QualifiedNodeBlockHasCompletionIn (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → Option MonoType → List StatementNode → Prop where
    | nil {scope result completion} :
        CompletionAccepts completion (.primitive .unit) →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion []
    | binding {scope result completion name binder kind rawType initializer rest} :
        QualifiedNodeIn principal requirements (some result) scope initializer (substitute principal rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        QualifiedNodeBlockHasCompletionIn principal requirements (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (.binding binder kind initializer :: rest)
    | assignment {scope result completion name binding value rest} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        QualifiedNodeIn principal requirements (some result) scope value (substitute principal binding.scheme.body) →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion rest →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (.assignment binding.binder value :: rest)
    | returned {scope result completion value rest} :
        QualifiedNodeIn principal requirements (some result) scope value result → QualifiedNodeBlockHasCompletionIn principal requirements scope result none rest →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (.returned value :: rest)
    | evaluatedTail {scope result completion value ty} :
        QualifiedNodeIn principal requirements (some result) scope value ty → CompletionAccepts completion ty →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion [.evaluated value]
    | evaluatedNext {scope result completion value next rest ty} :
        QualifiedNodeIn principal requirements (some result) scope value ty →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (next :: rest) →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (.evaluated value :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        QualifiedNodeBranchesHaveCompletionIn principal requirements scope result completion branches →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion fallback →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion [.conditional branches fallback]
    | conditionalNext {scope result completion branches fallback next rest} :
        QualifiedNodeBranchesHaveCompletionIn principal requirements scope result none branches →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result none fallback →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (next :: rest) →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion (.conditional branches fallback :: next :: rest)

  inductive QualifiedNodeBranchesHaveCompletionIn (principal : Substitution) (requirements : List Constraint) :
      Scope → MonoType → Option MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result completion} : QualifiedNodeBranchesHaveCompletionIn principal requirements scope result completion []
    | cons {scope result completion condition body rest} :
        QualifiedNodeIn principal requirements (some result) scope condition (.primitive .boolean) →
        QualifiedNodeBlockHasCompletionIn principal requirements scope result completion body →
        QualifiedNodeBranchesHaveCompletionIn principal requirements scope result completion rest →
        QualifiedNodeBranchesHaveCompletionIn principal requirements scope result completion ((condition, body) :: rest)
end


abbrev QualifiedNode (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodeIn principal requirements none scope
abbrev QualifiedNodes (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodesIn principal requirements none scope
abbrev QualifiedNodeFields (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodeFieldsIn principal requirements none scope
abbrev QualifiedNodeBranches (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodeBranchesIn principal requirements none scope
abbrev QualifiedNodeBlockHasCompletion (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodeBlockHasCompletionIn principal requirements
abbrev QualifiedNodeBranchesHaveCompletion (principal : Substitution) (requirements : List Constraint) :=
  QualifiedNodeBranchesHaveCompletionIn principal requirements
abbrev QualifiedNodeBlock (principal : Substitution) (requirements : List Constraint) (scope : Scope) (result : MonoType)
    (nodes : List StatementNode) : Prop :=
  QualifiedNodeBlockHasCompletion principal requirements scope result (some result) nodes
abbrev QualifiedNodeStatementBranches (principal : Substitution) (requirements : List Constraint) (scope : Scope) (result : MonoType)
    (branches : List (Node × List StatementNode)) : Prop :=
  QualifiedNodeBranchesHaveCompletion principal requirements scope result (some result) branches

theorem qualified_context_node_instantiates (principal later : Substitution) (requirements : List Constraint)
    (context : Option MonoType) (scope : Scope) (source : Node) (ty : MonoType)
    (typed : QualifiedNodeIn principal requirements context scope source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeHasTypeIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty) := by
  refine @QualifiedNodeIn.rec principal requirements
    (fun context scope source ty _ => NodeHasTypeIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun context scope source ty _ => NodesHaveTypesIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun context scope source types _ => NodeFieldsHaveTypesIn (compose later principal) (context.map (substitute later)) scope source (substituteFields later types))
    (fun context scope source ty _ => NodeBranchesHaveTypeIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun scope result completion nodes _ => NodeBlockHasCompletion (compose later principal) scope (substitute later result) (completion.map (substitute later)) nodes)
    (fun scope result completion nodes _ => NodeBranchesHaveCompletion (compose later principal) scope (substitute later result) (completion.map (substitute later)) nodes)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ context scope source ty typed
  · intro context scope lexeme value parsed
    exact .integer parsed
  · intro context scope value
    exact .boolean
  · intro context scope value
    exact .string
  · intro context scope name binding arguments raw instantiatedRequirements found notSchema keys sorts instantiated included wellFormed sort
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
  · intro context scope operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro context scope callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro context scope fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply NodeHasTypeIn.record fieldsIH
    simpa [substituteFields, List.map_map, Function.comp_def] using unique
  · intro context scope base name record result baseTyped member baseIH
    obtain ⟨row, same, field⟩ := solves _ member
    rw [same] at baseIH
    exact .field baseIH field
  · intro context scope branches otherwise result branchesTyped otherwiseTyped branchesIH otherwiseIH
    exact .conditional branchesIH otherwiseIH
  · intro context scope value name binding identity row input found schema quant empty shape wellFormed typed valueIH
    simp only [substitute, ← substitute_compose]
    exact .requireValue found schema quant empty shape wellFormed valueIH
  · intro context scope value result error expectedError typed same valueIH
    apply NodeHasTypeIn.propagate valueIH
    rw [substitute_compose]
    exact congrArg (substitute later) same
  · intro result scope body raw bodyTyped bodyIH
    simp only [Option.map_some, ← substitute_compose] at bodyIH ⊢
    exact .valueBlock bodyIH
  · intros; exact .nil
  · intro context scope expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · intros; exact .nil
  · intro context scope name expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · intro context scope result
    exact .nil
  · intro context scope condition branch rest result conditionTyped branchTyped restTyped conditionIH branchIH restIH
    exact .cons conditionIH branchIH restIH

  · intro scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro scope result completion name binder kind rawType initializer rest value fresh restTyped valueIH restIH
    have valueTyped := valueIH
    rw [← substitute_compose] at valueTyped
    exact .binding valueTyped fresh restIH
  · intro scope result completion name binding value rest found mutable valueTyped restTyped valueIH restIH
    have valueType := valueIH
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro scope result completion value rest valueTyped restTyped valueIH restIH
    exact .returned valueIH restIH
  · intro scope result completion value ty valueTyped accepted valueIH
    exact .evaluatedTail valueIH
      (completion_instantiates later completion ty accepted)
  · intro scope result completion value next rest ty valueTyped restTyped valueIH restIH
    exact .evaluatedNext valueIH restIH
  · intro scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro scope result completion
    exact .nil
  · intro scope result completion condition body rest conditionTyped bodyTyped restTyped conditionIH bodyIH restIH
    exact .cons conditionIH bodyIH restIH

theorem qualified_node_instantiates (scope : Scope) (principal later : Substitution)
    (requirements : List Constraint) (source : Node) (ty : MonoType)
    (typed : QualifiedNode scope principal requirements source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeHasType scope (compose later principal) source (substitute later ty) :=
  qualified_context_node_instantiates principal later requirements none scope source ty typed sorted solves

theorem qualified_node_completion_instantiates (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (nodes : List StatementNode)
    (typed : QualifiedNodeBlockHasCompletion principal requirements scope result completion nodes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasCompletion (compose later principal) scope (substitute later result)
      (completion.map (substitute later)) nodes := by
  refine @QualifiedNodeBlockHasCompletionIn.rec principal requirements
    (fun context scope source ty _ => NodeHasTypeIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun context scope source ty _ => NodesHaveTypesIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun context scope source types _ => NodeFieldsHaveTypesIn (compose later principal) (context.map (substitute later)) scope source (substituteFields later types))
    (fun context scope source ty _ => NodeBranchesHaveTypeIn (compose later principal) (context.map (substitute later)) scope source (substitute later ty))
    (fun scope result completion nodes _ => NodeBlockHasCompletion (compose later principal) scope (substitute later result) (completion.map (substitute later)) nodes)
    (fun scope result completion nodes _ => NodeBranchesHaveCompletion (compose later principal) scope (substitute later result) (completion.map (substitute later)) nodes)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ scope result completion nodes typed
  · intro context scope lexeme value parsed
    exact .integer parsed
  · intro context scope value
    exact .boolean
  · intro context scope value
    exact .string
  · intro context scope name binding arguments raw instantiatedRequirements found notSchema keys sorts instantiated included wellFormed sort
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
  · intro context scope operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro context scope callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro context scope fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply NodeHasTypeIn.record fieldsIH
    simpa [substituteFields, List.map_map, Function.comp_def] using unique
  · intro context scope base name record result baseTyped member baseIH
    obtain ⟨row, same, field⟩ := solves _ member
    rw [same] at baseIH
    exact .field baseIH field
  · intro context scope branches otherwise result branchesTyped otherwiseTyped branchesIH otherwiseIH
    exact .conditional branchesIH otherwiseIH
  · intro context scope value name binding identity row input found schema quant empty shape wellFormed typed valueIH
    simp only [substitute, ← substitute_compose]
    exact .requireValue found schema quant empty shape wellFormed valueIH
  · intro context scope value result error expectedError typed same valueIH
    apply NodeHasTypeIn.propagate valueIH
    rw [substitute_compose]
    exact congrArg (substitute later) same
  · intro result scope body raw bodyTyped bodyIH
    simp only [Option.map_some, ← substitute_compose] at bodyIH ⊢
    exact .valueBlock bodyIH
  · intros; exact .nil
  · intro context scope expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · intros; exact .nil
  · intro context scope name expression rest ty types first remaining firstIH remainingIH
    exact .cons firstIH remainingIH
  · intro context scope result
    exact .nil
  · intro context scope condition branch rest result conditionTyped branchTyped restTyped conditionIH branchIH restIH
    exact .cons conditionIH branchIH restIH

  · intro scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro scope result completion name binder kind rawType initializer rest value fresh restTyped valueIH restIH
    have valueTyped := valueIH
    rw [← substitute_compose] at valueTyped
    exact .binding valueTyped fresh restIH
  · intro scope result completion name binding value rest found mutable valueTyped restTyped valueIH restIH
    have valueType := valueIH
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro scope result completion value rest valueTyped restTyped valueIH restIH
    exact .returned valueIH restIH
  · intro scope result completion value ty valueTyped accepted valueIH
    exact .evaluatedTail valueIH
      (completion_instantiates later completion ty accepted)
  · intro scope result completion value next rest ty valueTyped restTyped valueIH restIH
    exact .evaluatedNext valueIH restIH
  · intro scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro scope result completion
    exact .nil
  · intro scope result completion condition body rest conditionTyped bodyTyped restTyped conditionIH bodyIH restIH
    exact .cons conditionIH bodyIH restIH

theorem qualified_node_block_instantiates (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (nodes : List StatementNode)
    (typed : QualifiedNodeBlock principal requirements scope result nodes)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeBlockHasType (compose later principal) scope (substitute later result) nodes := by
  exact qualified_node_completion_instantiates principal later requirements scope result (some result) nodes typed sorted solves

end Xsh.Typing.Connected
