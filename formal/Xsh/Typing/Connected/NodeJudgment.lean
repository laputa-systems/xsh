import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem instantiation_sorts_compose (principal later : Substitution)
    (arguments : List (Variable × MonoType)) (sorted : WellSorted later)
    (valid : instantiationSortsValid principal arguments = true) :
    instantiationSortsValid (compose later principal) arguments = true := by
  simp only [instantiationSortsValid, List.all_eq_true] at valid ⊢
  intro pair member
  obtain ⟨id, ty⟩ := pair
  have prior := valid (id, ty) member
  simp only [Bool.and_eq_true, decide_eq_true_eq, checks_well_formed_iff] at prior ⊢
  rw [substitute_compose, substitute_sort later sorted]
  exact ⟨substitute_well_formed later sorted _ prior.1, prior.2⟩

mutual
  inductive NodeHasTypeIn (solution : Substitution) :
      Option MonoType → Scope → Node → MonoType → Prop where
    | integer {context scope lexeme value} :
        parseInteger lexeme = some value →
        NodeHasTypeIn solution context scope (.integer lexeme) (.primitive .integer)
    | boolean {context scope value} :
        NodeHasTypeIn solution context scope (.boolean value) (.primitive .boolean)
    | string {context scope value} :
        NodeHasTypeIn solution context scope (.string value) (.primitive .string)
    | read {context scope name binding arguments ty requirements} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema →
        instantiationKeysValid binding.scheme arguments = true →
        instantiationSortsValid solution arguments = true →
        Instantiates binding.scheme (witnessSubstitution arguments) ty requirements →
        Solves requirements solution → WellFormed (substitute solution ty) →
        typeSort (substitute solution ty) = .value →
        NodeHasTypeIn solution context scope (.read binding.binder arguments) (substitute solution ty)
    | binary {context scope operator left right leftType rightType result} :
        NodeHasTypeIn solution context scope left leftType → NodeHasTypeIn solution context scope right rightType →
        BinaryTyping operator leftType rightType result →
        NodeHasTypeIn solution context scope (.binary operator left right) result
    | call {context scope callee arguments argumentTypes result} :
        NodeHasTypeIn solution context scope callee (.function argumentTypes result) →
        NodesHaveTypesIn solution context scope arguments argumentTypes →
        NodeHasTypeIn solution context scope (.call callee arguments) result
    | record {context scope fields types} :
        NodeFieldsHaveTypesIn solution context scope fields types → (types.map Prod.fst).Nodup →
        NodeHasTypeIn solution context scope (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {context scope base name row result} :
        NodeHasTypeIn solution context scope base (.record row) → FieldOccurs name result row →
        NodeHasTypeIn solution context scope (.field base name) result
    | conditional {context scope branches otherwise result} :
        NodeBranchesHaveTypeIn solution context scope branches result →
        NodeHasTypeIn solution context scope otherwise result →
        NodeHasTypeIn solution context scope (.conditional branches otherwise) result
    | requireValue {context scope value name binding identity row input} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        NodeHasTypeIn solution context scope value input →
        NodeHasTypeIn solution context scope (.requireValue binding.binder identity value)
          (.result (.nominal identity (.record (substitute solution row))) (.primitive .error))
    | propagate {context scope value result error expectedError} :
        NodeHasTypeIn solution context scope value (.result result error) →
        error = substitute solution expectedError →
        NodeHasTypeIn solution context scope (.propagate value expectedError) result
    | valueBlock {scope functionResult body rawCompletion} :
        NodeBlockHasCompletion solution scope functionResult (some (substitute solution rawCompletion)) body →
        NodeHasTypeIn solution (some functionResult) scope (.valueBlock rawCompletion body) (substitute solution rawCompletion)

  inductive NodesHaveTypesIn (solution : Substitution) :
      Option MonoType → Scope → List Node → MonoType → Prop where
    | nil {context scope} : NodesHaveTypesIn solution context scope [] .argumentsEmpty
    | cons {context scope node rest ty types} :
        NodeHasTypeIn solution context scope node ty → NodesHaveTypesIn solution context scope rest types →
        NodesHaveTypesIn solution context scope (node :: rest) (.argument ty types)

  inductive NodeFieldsHaveTypesIn (solution : Substitution) :
      Option MonoType → Scope → List (NameId × Node) → List (NameId × MonoType) → Prop where
    | nil {context scope} : NodeFieldsHaveTypesIn solution context scope [] []
    | cons {context scope name node rest ty types} :
        NodeHasTypeIn solution context scope node ty → NodeFieldsHaveTypesIn solution context scope rest types →
        NodeFieldsHaveTypesIn solution context scope ((name, node) :: rest) ((name, ty) :: types)

  inductive NodeBranchesHaveTypeIn (solution : Substitution) :
      Option MonoType → Scope → List (Node × Node) → MonoType → Prop where
    | nil {context scope result} : NodeBranchesHaveTypeIn solution context scope [] result
    | cons {context scope condition branch rest result} :
        NodeHasTypeIn solution context scope condition (.primitive .boolean) →
        NodeHasTypeIn solution context scope branch result → NodeBranchesHaveTypeIn solution context scope rest result →
        NodeBranchesHaveTypeIn solution context scope ((condition, branch) :: rest) result

  inductive NodeBlockHasCompletion (solution : Substitution) :
      Scope → MonoType → Option MonoType → List StatementNode → Prop where
    | nil {scope result completion} :
        CompletionAccepts completion (.primitive .unit) →
        NodeBlockHasCompletion solution scope result completion []
    | binding {scope result completion name binder kind rawType initializer rest valueType} :
        NodeHasTypeIn solution (some result) scope initializer valueType →
        StoreCompatible (substitute solution rawType) valueType →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        NodeBlockHasCompletion solution (⟨name, binder, kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        NodeBlockHasCompletion solution scope result completion (.binding binder kind rawType initializer :: rest)
    | assignment {scope result completion name binding value rest valueType} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        NodeHasTypeIn solution (some result) scope value valueType →
        StoreCompatible (substitute solution binding.scheme.body) valueType →
        NodeBlockHasCompletion solution scope result completion rest →
        NodeBlockHasCompletion solution scope result completion (.assignment binding.binder value :: rest)
    | returned {scope result completion value rest} :
        NodeHasTypeIn solution (some result) scope value result →
        NodeBlockHasCompletion solution scope result none rest →
        NodeBlockHasCompletion solution scope result completion (.returned value :: rest)
    | evaluatedTail {scope result completion value ty} :
        NodeHasTypeIn solution (some result) scope value ty → CompletionAccepts completion ty →
        NodeBlockHasCompletion solution scope result completion [.evaluated value]
    | evaluatedNext {scope result completion value next rest ty} :
        NodeHasTypeIn solution (some result) scope value ty →
        NodeBlockHasCompletion solution scope result completion (next :: rest) →
        NodeBlockHasCompletion solution scope result completion (.evaluated value :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        NodeBranchesHaveCompletion solution scope result completion branches →
        NodeBlockHasCompletion solution scope result completion fallback →
        NodeBlockHasCompletion solution scope result completion [.conditional branches fallback]
    | conditionalNext {scope result completion branches fallback next rest} :
        NodeBranchesHaveCompletion solution scope result none branches →
        NodeBlockHasCompletion solution scope result none fallback →
        NodeBlockHasCompletion solution scope result completion (next :: rest) →
        NodeBlockHasCompletion solution scope result completion (.conditional branches fallback :: next :: rest)

  inductive NodeBranchesHaveCompletion (solution : Substitution) :
      Scope → MonoType → Option MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result completion} : NodeBranchesHaveCompletion solution scope result completion []
    | cons {scope result completion condition body rest} :
        NodeHasTypeIn solution (some result) scope condition (.primitive .boolean) →
        NodeBlockHasCompletion solution scope result completion body →
        NodeBranchesHaveCompletion solution scope result completion rest →
        NodeBranchesHaveCompletion solution scope result completion ((condition, body) :: rest)
end

abbrev NodeHasType (scope : Scope) (solution : Substitution) (node : Node) (ty : MonoType) : Prop :=
  NodeHasTypeIn solution none scope node ty
abbrev NodesHaveTypes (scope : Scope) (solution : Substitution) (nodes : List Node) (ty : MonoType) : Prop :=
  NodesHaveTypesIn solution none scope nodes ty
abbrev NodeFieldsHaveTypes (scope : Scope) (solution : Substitution) (fields : List (NameId × Node))
    (types : List (NameId × MonoType)) : Prop :=
  NodeFieldsHaveTypesIn solution none scope fields types
abbrev NodeBranchesHaveType (scope : Scope) (solution : Substitution) (branches : List (Node × Node))
    (ty : MonoType) : Prop := NodeBranchesHaveTypeIn solution none scope branches ty
abbrev NodeBlockHasType (solution : Substitution) (scope : Scope) (result : MonoType)
    (nodes : List StatementNode) : Prop :=
  NodeBlockHasCompletion solution scope result (some result) nodes
abbrev NodeStatementBranchesHaveType (solution : Substitution) (scope : Scope) (result : MonoType)
    (branches : List (Node × List StatementNode)) : Prop :=
  NodeBranchesHaveCompletion solution scope result (some result) branches

end Xsh.Typing.Connected
