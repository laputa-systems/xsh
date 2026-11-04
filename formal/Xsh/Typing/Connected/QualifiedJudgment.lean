import Xsh.Typing.Connected.Qualified

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive QualifiedExpressionIn (principal : Substitution) (requirements : List Constraint) :
      Option (TypeNames × MonoType) → Scope →
      Expr → MonoType → Prop where
    | integer {lexeme : String} {value : Xsh.Values.IntValue} :
        parseInteger lexeme = some value → QualifiedExpressionIn principal requirements context scope (.literal (.integer lexeme)) (.primitive .integer)
    | boolean {value : Bool} : QualifiedExpressionIn principal requirements context scope (.literal (.bool value)) (.primitive .boolean)
    | string {value : String} : QualifiedExpressionIn principal requirements context scope (.literal (.string value)) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : Substitution}
        {ty : MonoType} {instantiatedRequirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema → Instantiates binding.scheme arguments ty instantiatedRequirements →
        (∀ constraint ∈ instantiatedRequirements, mapConstraint principal constraint ∈ requirements) →
        WellFormed (substitute principal ty) → typeSort (substitute principal ty) = .value →
        QualifiedExpressionIn principal requirements context scope (.ident name) (substitute principal ty)
    | binary {operator : BinaryOp} {left right : Expr} {leftType rightType result : MonoType} :
        QualifiedExpressionIn principal requirements context scope left leftType →
        QualifiedExpressionIn principal requirements context scope right rightType →
        BinaryTyping operator leftType rightType result →
        QualifiedExpressionIn principal requirements context scope (.binary operator left right) result
    | call {callee : Expr} {arguments : List CallArgument} {argumentTypes result : MonoType} :
        QualifiedExpressionIn principal requirements context scope callee (.function argumentTypes result) →
        QualifiedArgumentsIn principal requirements context scope arguments argumentTypes →
        QualifiedExpressionIn principal requirements context scope (.call callee arguments) result
    | record {fields : List RecordField} {types : List (NameId × MonoType)} :
        QualifiedRecordFieldsIn principal requirements context scope fields types → (types.map Prod.fst).Nodup →
        QualifiedExpressionIn principal requirements context scope (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Expr} {name : NameId} {record result : MonoType} :
        QualifiedExpressionIn principal requirements context scope base record →
        Constraint.field record name result ∈ requirements →
        QualifiedExpressionIn principal requirements context scope (.field base name) result
    | conditional {branches : List (Expr × Expr)} {otherwise : Expr} {result : MonoType} :
        QualifiedBranchesIn principal requirements context scope branches result →
        QualifiedExpressionIn principal requirements context scope otherwise result →
        QualifiedExpressionIn principal requirements context scope (.ifExpr branches otherwise) result
    | requireValue {value : Expr} {name : NameId} {binding : Binding}
        {identity : Xsh.Validation.Nominal.NominalId} {row input : MonoType} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        QualifiedExpressionIn principal requirements context scope value input →
        QualifiedExpressionIn principal requirements context scope (.require value (some (.named name)))
          (.result (.nominal identity (.record (substitute principal row))) (.primitive .error))
    | propagate {value : Expr} {result error : MonoType} :
        QualifiedExpressionIn principal requirements context scope value (.result result error) →
        QualifiedExpressionIn principal requirements context scope (.tryExpr value) result

    | valueBlock {names functionResult scope body result} :
        QualifiedBlockHasCompletionIn principal requirements names scope functionResult (some result) body →
        QualifiedExpressionIn principal requirements (some (names, functionResult)) scope (.valueBlock body) result

  inductive QualifiedArgumentsIn (principal : Substitution) (requirements : List Constraint) :
      Option (TypeNames × MonoType) → Scope →
      List CallArgument → MonoType → Prop where
    | nil : QualifiedArgumentsIn principal requirements context scope [] .argumentsEmpty
    | cons {expression : Expr} {rest : List CallArgument} {ty types : MonoType} :
        QualifiedExpressionIn principal requirements context scope expression ty →
        QualifiedArgumentsIn principal requirements context scope rest types →
        QualifiedArgumentsIn principal requirements context scope (.positional expression :: rest) (.argument ty types)

  inductive QualifiedRecordFieldsIn (principal : Substitution) (requirements : List Constraint) :
      Option (TypeNames × MonoType) → Scope →
      List RecordField → List (NameId × MonoType) → Prop where
    | nil : QualifiedRecordFieldsIn principal requirements context scope [] []
    | cons {name : NameId} {expression : Expr} {rest : List RecordField}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        QualifiedExpressionIn principal requirements context scope expression ty →
        QualifiedRecordFieldsIn principal requirements context scope rest types →
        QualifiedRecordFieldsIn principal requirements context scope (.named name expression :: rest) ((name, ty) :: types)

  inductive QualifiedBranchesIn (principal : Substitution) (requirements : List Constraint) :
      Option (TypeNames × MonoType) → Scope →
      List (Expr × Expr) → MonoType → Prop where
    | nil {result : MonoType} : QualifiedBranchesIn principal requirements context scope [] result
    | cons {condition branch : Expr} {rest : List (Expr × Expr)} {result : MonoType} :
        QualifiedExpressionIn principal requirements context scope condition (.primitive .boolean) →
        QualifiedExpressionIn principal requirements context scope branch result →
        QualifiedBranchesIn principal requirements context scope rest result →
        QualifiedBranchesIn principal requirements context scope ((condition, branch) :: rest) result
  inductive QualifiedBlockHasCompletionIn (principal : Substitution)
      (requirements : List Constraint) : TypeNames → Scope → MonoType → Option MonoType → List Statement → Prop where
    | nil {names scope result completion} : CompletionAccepts completion (.primitive .unit) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion []
    | binding {names scope result completion kind name declared initializer rest binder rawType} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope initializer (substitute principal rawType) →
        AnnotationAgrees names principal declared (substitute principal rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        QualifiedBlockHasCompletionIn principal requirements names
          (⟨name, binder, bindingKind kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion
          (.binding kind (.name name) declared initializer :: rest)
    | assignment {names scope result completion name value rest binding} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        QualifiedExpressionIn principal requirements (some (names, result)) scope value (substitute principal binding.scheme.body) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion rest →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (.assign (.name name) .set value :: rest)
    | returned {names scope result completion value rest} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope value result →
        QualifiedBlockHasCompletionIn principal requirements names scope result none rest →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (.returnValue (some value) :: rest)
    | tailName {names scope result completion name ty} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope (.ident name) ty → CompletionAccepts completion ty →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion [.tailName name]
    | expressionTail {names scope result completion value ty} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope value ty → CompletionAccepts completion ty →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion [.expression value]
    | expressionNext {names scope result completion value next rest ty} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope value ty →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (next :: rest) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (.expression value :: next :: rest)
    | conditionalTail {names scope result completion branches fallback} :
        QualifiedBranchesHaveCompletionIn principal requirements names scope result completion branches →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion fallback →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion [.ifStmt branches (some fallback)]
    | conditionalNoFallbackTail {names scope result completion branches} :
        QualifiedBranchesHaveCompletionIn principal requirements names scope result completion branches →
        CompletionAccepts completion (.primitive .unit) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion [.ifStmt branches none]
    | conditionalNext {names scope result completion branches fallback next rest} :
        QualifiedBranchesHaveCompletionIn principal requirements names scope result none branches →
        QualifiedBlockHasCompletionIn principal requirements names scope result none fallback →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (next :: rest) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (.ifStmt branches (some fallback) :: next :: rest)
    | conditionalNoFallbackNext {names scope result completion branches next rest} :
        QualifiedBranchesHaveCompletionIn principal requirements names scope result none branches →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (next :: rest) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion (.ifStmt branches none :: next :: rest)

  inductive QualifiedBranchesHaveCompletionIn (principal : Substitution)
      (requirements : List Constraint) : TypeNames → Scope → MonoType → Option MonoType → List (Expr × List Statement) → Prop where
    | nil {names scope result completion} : QualifiedBranchesHaveCompletionIn principal requirements names scope result completion []
    | cons {names scope result completion condition body rest} :
        QualifiedExpressionIn principal requirements (some (names, result)) scope condition (.primitive .boolean) →
        QualifiedBlockHasCompletionIn principal requirements names scope result completion body →
        QualifiedBranchesHaveCompletionIn principal requirements names scope result completion rest →
        QualifiedBranchesHaveCompletionIn principal requirements names scope result completion ((condition, body) :: rest)
end


abbrev QualifiedExpression (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedExpressionIn principal requirements none scope
abbrev QualifiedArguments (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedArgumentsIn principal requirements none scope
abbrev QualifiedRecordFields (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedRecordFieldsIn principal requirements none scope
abbrev QualifiedBranches (scope : Scope) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedBranchesIn principal requirements none scope
abbrev QualifiedBlockHasCompletion (names : TypeNames) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedBlockHasCompletionIn principal requirements names
abbrev QualifiedBranchesHaveCompletion (names : TypeNames) (principal : Substitution) (requirements : List Constraint) :=
  QualifiedBranchesHaveCompletionIn principal requirements names
abbrev QualifiedBlock (names : TypeNames) (principal : Substitution) (requirements : List Constraint)
    (scope : Scope) (result : MonoType) (source : List Statement) : Prop :=
  QualifiedBlockHasCompletion names principal requirements scope result (some result) source
abbrev QualifiedStatementBranches (names : TypeNames) (principal : Substitution) (requirements : List Constraint)
    (scope : Scope) (result : MonoType) (source : List (Expr × List Statement)) : Prop :=
  QualifiedBranchesHaveCompletion names principal requirements scope result (some result) source

def substituteFields (substitution : Substitution) (fields : List (NameId × MonoType)) : List (NameId × MonoType) :=
  fields.map fun (name, ty) => (name, substitute substitution ty)

theorem insert_field_substitutes (substitution : Substitution) (field : NameId × MonoType)
    (fields : List (NameId × MonoType)) :
    substituteFields substitution (insertField field fields) =
      insertField (field.1, substitute substitution field.2) (substituteFields substitution fields) := by
  induction fields with
  | nil => rfl
  | cons head rest ih =>
    simp only [insertField, substituteFields, List.map_cons]
    split <;> simp_all [substituteFields]

theorem canonical_fields_substitute (substitution : Substitution) (fields : List (NameId × MonoType)) :
    substituteFields substitution (canonicalFields fields) = canonicalFields (substituteFields substitution fields) := by
  induction fields with
  | nil => rfl
  | cons head rest ih =>
    change substituteFields substitution (insertField head (canonicalFields rest)) =
      insertField (head.1, substitute substitution head.2) (canonicalFields (substituteFields substitution rest))
    rw [insert_field_substitutes, ih]

theorem substituted_record_fields (substitution : Substitution) (fields : List (NameId × MonoType)) :
    substitute substitution (.record ((canonicalFields fields).foldr
      (fun (name, ty) row => .rowField name ty row) .rowEmpty)) =
    .record ((canonicalFields (substituteFields substitution fields)).foldr
      (fun (name, ty) row => .rowField name ty row) .rowEmpty) := by
  have folded : ∀ items : List (NameId × MonoType),
      substitute substitution (items.foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty) =
        (substituteFields substitution items).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty := by
    intro items
    induction items with
    | nil => rfl
    | cons head rest ih => simp [substituteFields, substitute, ih]
  change MonoType.record (substitute substitution ((canonicalFields fields).foldr
    (fun (name, ty) row => .rowField name ty row) .rowEmpty)) = _
  rw [folded, canonical_fields_substitute]

theorem binary_typing_substitutes (substitution : Substitution) {operator : BinaryOp}
    {left right result : MonoType} (typed : BinaryTyping operator left right result) :
    BinaryTyping operator (substitute substitution left) (substitute substitution right)
      (substitute substitution result) := by
  cases typed <;> constructor

theorem qualified_annotation_instantiates (names : TypeNames) (principal later : Substitution)
    (declared : Option TypeExpr) (ty : MonoType) (agrees : AnnotationAgrees names principal declared ty) :
    AnnotationAgrees names (compose later principal) declared (substitute later ty) := by
  cases declared with
  | none => trivial
  | some annotation =>
    obtain ⟨raw, found, same⟩ := agrees
    refine ⟨raw, found, ?_⟩
    rw [substitute_compose, same]

theorem completion_instantiates (later : Substitution) (completion : Option MonoType) (ty : MonoType)
    (accepted : CompletionAccepts completion ty) :
    CompletionAccepts (completion.map (substitute later)) (substitute later ty) := by
  rcases accepted with absent | present
  · simp [CompletionAccepts, absent]
  · simp [CompletionAccepts, present]

def substituteContext (later : Substitution) : Option (TypeNames × MonoType) → Option (TypeNames × MonoType)
  | none => none
  | some (names, result) => some (names, substitute later result)

theorem qualified_context_instantiates (principal later : Substitution) (requirements : List Constraint)
    (context : Option (TypeNames × MonoType)) (scope : Scope) (source : Expr) (ty : MonoType)
    (typed : QualifiedExpressionIn principal requirements context scope source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ExpressionHasTypeIn (compose later principal) (substituteContext later context) scope source (substitute later ty) := by
  refine @QualifiedExpressionIn.rec principal requirements
    (fun context scope source ty _ => ExpressionHasTypeIn (compose later principal) (substituteContext later context) scope source (substitute later ty))
    (fun context scope source types _ => ArgumentsHaveTypesIn (compose later principal) (substituteContext later context) scope source (substitute later types))
    (fun context scope source types _ => RecordFieldsHaveTypesIn (compose later principal) (substituteContext later context) scope source (substituteFields later types))
    (fun context scope source ty _ => BranchesHaveTypeIn (compose later principal) (substituteContext later context) scope source (substitute later ty))
    (fun names scope result completion source _ => SourceBlockHasCompletion (compose later principal) names scope (substitute later result) (completion.map (substitute later)) source)
    (fun names scope result completion source _ => SourceBranchesHaveCompletion (compose later principal) names scope (substitute later result) (completion.map (substitute later)) source)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ context scope source ty typed
  · intro context scope lexeme value parsed
    exact .integer parsed
  · intro context scope value
    exact .boolean
  · intro context scope value
    exact .string
  · intro context scope name binding arguments raw instantiatedRequirements found notSchema instantiated included wellFormed sort
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
    exact .read found notSchema instantiated requirementsSolved resultingWellFormed resultingSort
  · intro context scope operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro context scope callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro context scope fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply ExpressionHasTypeIn.record fieldsIH
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
  · intro context scope value result error typed valueIH
    exact .propagate valueIH
  · intro names result scope body completion bodyTyped bodyIH
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
  · intro names scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro names scope result completion kind name declared initializer rest binder rawType value annotation fresh restTyped valueIH restIH
    have valueTyped := valueIH
    have annotationTyped := qualified_annotation_instantiates names principal later _ _ annotation
    rw [← substitute_compose] at valueTyped annotationTyped
    exact .binding valueTyped annotationTyped fresh restIH
  · intro names scope result completion name value rest binding found mutable valueTyped restTyped valueIH restIH
    have valueType := valueIH
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro names scope result completion value rest valueTyped restTyped valueIH restIH
    exact .returned valueIH restIH
  · intro names scope result completion name ty valueTyped accepted valueIH
    exact .tailName valueIH
      (completion_instantiates later completion ty accepted)
  · intro names scope result completion value ty valueTyped accepted valueIH
    exact .expressionTail valueIH
      (completion_instantiates later completion ty accepted)
  · intro names scope result completion value next rest ty valueTyped restTyped valueIH restIH
    exact .expressionNext valueIH restIH
  · intro names scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro names scope result completion branches branchesTyped accepted branchesIH
    exact .conditionalNoFallbackTail branchesIH (completion_instantiates later completion (.primitive .unit) accepted)
  · intro names scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro names scope result completion branches next rest branchesTyped restTyped branchesIH restIH
    exact .conditionalNoFallbackNext branchesIH restIH
  · intro names scope result completion
    exact .nil
  · intro names scope result completion condition body rest conditionTyped bodyTyped restTyped conditionIH bodyIH restIH
    exact .cons conditionIH bodyIH restIH

theorem qualified_expression_instantiates (scope : Scope) (principal later : Substitution)
    (requirements : List Constraint) (source : Expr) (ty : MonoType)
    (typed : QualifiedExpression scope principal requirements source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ExpressionHasType scope (compose later principal) source (substitute later ty) :=
  qualified_context_instantiates principal later requirements none scope source ty typed sorted solves

theorem qualified_completion_instantiates (names : TypeNames) (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (source : List Statement)
    (typed : QualifiedBlockHasCompletion names principal requirements scope result completion source)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    SourceBlockHasCompletion (compose later principal) names scope (substitute later result)
      (completion.map (substitute later)) source := by
  refine @QualifiedBlockHasCompletionIn.rec principal requirements
    (fun context scope source ty _ => ExpressionHasTypeIn (compose later principal) (substituteContext later context) scope source (substitute later ty))
    (fun context scope source types _ => ArgumentsHaveTypesIn (compose later principal) (substituteContext later context) scope source (substitute later types))
    (fun context scope source types _ => RecordFieldsHaveTypesIn (compose later principal) (substituteContext later context) scope source (substituteFields later types))
    (fun context scope source ty _ => BranchesHaveTypeIn (compose later principal) (substituteContext later context) scope source (substitute later ty))
    (fun names scope result completion source _ => SourceBlockHasCompletion (compose later principal) names scope (substitute later result) (completion.map (substitute later)) source)
    (fun names scope result completion source _ => SourceBranchesHaveCompletion (compose later principal) names scope (substitute later result) (completion.map (substitute later)) source)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ names scope result completion source typed
  · intro context scope lexeme value parsed
    exact .integer parsed
  · intro context scope value
    exact .boolean
  · intro context scope value
    exact .string
  · intro context scope name binding arguments raw instantiatedRequirements found notSchema instantiated included wellFormed sort
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
    exact .read found notSchema instantiated requirementsSolved resultingWellFormed resultingSort
  · intro context scope operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro context scope callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro context scope fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply ExpressionHasTypeIn.record fieldsIH
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
  · intro context scope value result error typed valueIH
    exact .propagate valueIH
  · intro names result scope body completion bodyTyped bodyIH
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
  · intro names scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro names scope result completion kind name declared initializer rest binder rawType value annotation fresh restTyped valueIH restIH
    have valueTyped := valueIH
    have annotationTyped := qualified_annotation_instantiates names principal later _ _ annotation
    rw [← substitute_compose] at valueTyped annotationTyped
    exact .binding valueTyped annotationTyped fresh restIH
  · intro names scope result completion name value rest binding found mutable valueTyped restTyped valueIH restIH
    have valueType := valueIH
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro names scope result completion value rest valueTyped restTyped valueIH restIH
    exact .returned valueIH restIH
  · intro names scope result completion name ty valueTyped accepted valueIH
    exact .tailName valueIH
      (completion_instantiates later completion ty accepted)
  · intro names scope result completion value ty valueTyped accepted valueIH
    exact .expressionTail valueIH
      (completion_instantiates later completion ty accepted)
  · intro names scope result completion value next rest ty valueTyped restTyped valueIH restIH
    exact .expressionNext valueIH restIH
  · intro names scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro names scope result completion branches branchesTyped accepted branchesIH
    exact .conditionalNoFallbackTail branchesIH (completion_instantiates later completion (.primitive .unit) accepted)
  · intro names scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro names scope result completion branches next rest branchesTyped restTyped branchesIH restIH
    exact .conditionalNoFallbackNext branchesIH restIH
  · intro names scope result completion
    exact .nil
  · intro names scope result completion condition body rest conditionTyped bodyTyped restTyped conditionIH bodyIH restIH
    exact .cons conditionIH bodyIH restIH

theorem qualified_block_instantiates (names : TypeNames) (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (source : List Statement)
    (typed : QualifiedBlock names principal requirements scope result source)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    BlockHasType names (compose later principal) scope (substitute later result) source := by
  exact qualified_completion_instantiates names principal later requirements scope result (some result) source typed sorted solves


end Xsh.Typing.Connected
