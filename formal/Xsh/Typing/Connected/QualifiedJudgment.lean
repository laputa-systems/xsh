import Xsh.Typing.Connected.Qualified

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

mutual
  inductive QualifiedExpression (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      Expr → MonoType → Prop where
    | integer {lexeme : String} {value : Xsh.Values.IntValue} :
        parseInteger lexeme = some value → QualifiedExpression scope principal requirements (.literal (.integer lexeme)) (.primitive .integer)
    | boolean {value : Bool} : QualifiedExpression scope principal requirements (.literal (.bool value)) (.primitive .boolean)
    | string {value : String} : QualifiedExpression scope principal requirements (.literal (.string value)) (.primitive .string)
    | read {name : NameId} {binding : Binding} {arguments : Substitution}
        {ty : MonoType} {instantiatedRequirements : List Constraint} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind ≠ .schema → Instantiates binding.scheme arguments ty instantiatedRequirements →
        (∀ constraint ∈ instantiatedRequirements, mapConstraint principal constraint ∈ requirements) →
        WellFormed (substitute principal ty) → typeSort (substitute principal ty) = .value →
        QualifiedExpression scope principal requirements (.ident name) (substitute principal ty)
    | binary {operator : BinaryOp} {left right : Expr} {leftType rightType result : MonoType} :
        QualifiedExpression scope principal requirements left leftType →
        QualifiedExpression scope principal requirements right rightType →
        BinaryTyping operator leftType rightType result →
        QualifiedExpression scope principal requirements (.binary operator left right) result
    | call {callee : Expr} {arguments : List CallArgument} {argumentTypes result : MonoType} :
        QualifiedExpression scope principal requirements callee (.function argumentTypes result) →
        QualifiedArguments scope principal requirements arguments argumentTypes →
        QualifiedExpression scope principal requirements (.call callee arguments) result
    | record {fields : List RecordField} {types : List (NameId × MonoType)} :
        QualifiedRecordFields scope principal requirements fields types → (types.map Prod.fst).Nodup →
        QualifiedExpression scope principal requirements (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {base : Expr} {name : NameId} {record result : MonoType} :
        QualifiedExpression scope principal requirements base record →
        Constraint.field record name result ∈ requirements →
        QualifiedExpression scope principal requirements (.field base name) result
    | conditional {branches : List (Expr × Expr)} {otherwise : Expr} {result : MonoType} :
        QualifiedBranches scope principal requirements branches result →
        QualifiedExpression scope principal requirements otherwise result →
        QualifiedExpression scope principal requirements (.ifExpr branches otherwise) result
    | requireValue {value : Expr} {name : NameId} {binding : Binding}
        {identity : Xsh.Validation.Nominal.NominalId} {row input : MonoType} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        QualifiedExpression scope principal requirements value input →
        QualifiedExpression scope principal requirements (.require value (some (.named name)))
          (.result (.nominal identity (.record (substitute principal row))) (.primitive .error))
    | propagate {value : Expr} {result error : MonoType} :
        QualifiedExpression scope principal requirements value (.result result error) →
        QualifiedExpression scope principal requirements (.tryExpr value) result

  inductive QualifiedArguments (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List CallArgument → MonoType → Prop where
    | nil : QualifiedArguments scope principal requirements [] .argumentsEmpty
    | cons {expression : Expr} {rest : List CallArgument} {ty types : MonoType} :
        QualifiedExpression scope principal requirements expression ty →
        QualifiedArguments scope principal requirements rest types →
        QualifiedArguments scope principal requirements (.positional expression :: rest) (.argument ty types)

  inductive QualifiedRecordFields (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List RecordField → List (NameId × MonoType) → Prop where
    | nil : QualifiedRecordFields scope principal requirements [] []
    | cons {name : NameId} {expression : Expr} {rest : List RecordField}
        {ty : MonoType} {types : List (NameId × MonoType)} :
        QualifiedExpression scope principal requirements expression ty →
        QualifiedRecordFields scope principal requirements rest types →
        QualifiedRecordFields scope principal requirements (.named name expression :: rest) ((name, ty) :: types)

  inductive QualifiedBranches (scope : Scope) (principal : Substitution) (requirements : List Constraint) :
      List (Expr × Expr) → MonoType → Prop where
    | nil {result : MonoType} : QualifiedBranches scope principal requirements [] result
    | cons {condition branch : Expr} {rest : List (Expr × Expr)} {result : MonoType} :
        QualifiedExpression scope principal requirements condition (.primitive .boolean) →
        QualifiedExpression scope principal requirements branch result →
        QualifiedBranches scope principal requirements rest result →
        QualifiedBranches scope principal requirements ((condition, branch) :: rest) result
end

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

theorem qualified_expression_instantiates (scope : Scope) (principal later : Substitution)
    (requirements : List Constraint) (source : Expr) (ty : MonoType)
    (typed : QualifiedExpression scope principal requirements source ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ExpressionHasType scope (compose later principal) source (substitute later ty) := by
  refine @QualifiedExpression.rec scope principal requirements
    (fun source ty _ => ExpressionHasType scope (compose later principal) source (substitute later ty))
    (fun source types _ => ArgumentsHaveTypes scope (compose later principal) source (substitute later types))
    (fun source types _ => RecordFieldsHaveTypes scope (compose later principal) source (substituteFields later types))
    (fun source ty _ => BranchesHaveType scope (compose later principal) source (substitute later ty))
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ source ty typed
  · intro lexeme value parsed
    exact .integer parsed
  · intro value
    exact .boolean
  · intro value
    exact .string
  · intro name binding arguments raw instantiatedRequirements found notSchema instantiated included wellFormed sort
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
  · intro operator left right leftType rightType result leftTyped rightTyped binary leftIH rightIH
    exact .binary leftIH rightIH (binary_typing_substitutes later binary)
  · intro callee arguments argumentTypes result calleeTyped argumentsTyped calleeIH argumentsIH
    exact .call calleeIH argumentsIH
  · intro fields types fieldsTyped unique fieldsIH
    rw [substituted_record_fields]
    apply ExpressionHasType.record fieldsIH
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
  · intro value result error typed valueIH
    exact .propagate valueIH
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


mutual
  inductive QualifiedBlockHasCompletion (names : TypeNames) (principal : Substitution)
      (requirements : List Constraint) : Scope → MonoType → Option MonoType → List Statement → Prop where
    | nil {scope result completion} : CompletionAccepts completion (.primitive .unit) →
        QualifiedBlockHasCompletion names principal requirements scope result completion []
    | binding {scope result completion kind name declared initializer rest binder rawType} :
        QualifiedExpression scope principal requirements initializer (substitute principal rawType) →
        AnnotationAgrees names principal declared (substitute principal rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        QualifiedBlockHasCompletion names principal requirements
          (⟨name, binder, bindingKind kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        QualifiedBlockHasCompletion names principal requirements scope result completion
          (.binding kind (.name name) declared initializer :: rest)
    | assignment {scope result completion name value rest binding} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        QualifiedExpression scope principal requirements value (substitute principal binding.scheme.body) →
        QualifiedBlockHasCompletion names principal requirements scope result completion rest →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.assign (.name name) .set value :: rest)
    | returned {scope result completion value rest} :
        QualifiedExpression scope principal requirements value result →
        QualifiedBlockHasCompletion names principal requirements scope result none rest →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.returnValue (some value) :: rest)
    | tailName {scope result completion name ty} :
        QualifiedExpression scope principal requirements (.ident name) ty → CompletionAccepts completion ty →
        QualifiedBlockHasCompletion names principal requirements scope result completion [.tailName name]
    | expressionTail {scope result completion value ty} :
        QualifiedExpression scope principal requirements value ty → CompletionAccepts completion ty →
        QualifiedBlockHasCompletion names principal requirements scope result completion [.expression value]
    | expressionNext {scope result completion value next rest ty} :
        QualifiedExpression scope principal requirements value ty →
        QualifiedBlockHasCompletion names principal requirements scope result completion (next :: rest) →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.expression value :: next :: rest)
    | scopedTail {scope result completion body} :
        QualifiedBlockHasCompletion names principal requirements scope result completion body →
        QualifiedBlockHasCompletion names principal requirements scope result completion [.expression (.valueBlock body)]
    | scopedNext {scope result completion body next rest} :
        QualifiedBlockHasCompletion names principal requirements scope result none body →
        QualifiedBlockHasCompletion names principal requirements scope result completion (next :: rest) →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.expression (.valueBlock body) :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        QualifiedBranchesHaveCompletion names principal requirements scope result completion branches →
        QualifiedBlockHasCompletion names principal requirements scope result completion fallback →
        QualifiedBlockHasCompletion names principal requirements scope result completion [.ifStmt branches (some fallback)]
    | conditionalNoFallbackTail {scope result completion branches} :
        QualifiedBranchesHaveCompletion names principal requirements scope result completion branches →
        CompletionAccepts completion (.primitive .unit) →
        QualifiedBlockHasCompletion names principal requirements scope result completion [.ifStmt branches none]
    | conditionalNext {scope result completion branches fallback next rest} :
        QualifiedBranchesHaveCompletion names principal requirements scope result none branches →
        QualifiedBlockHasCompletion names principal requirements scope result none fallback →
        QualifiedBlockHasCompletion names principal requirements scope result completion (next :: rest) →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.ifStmt branches (some fallback) :: next :: rest)
    | conditionalNoFallbackNext {scope result completion branches next rest} :
        QualifiedBranchesHaveCompletion names principal requirements scope result none branches →
        QualifiedBlockHasCompletion names principal requirements scope result completion (next :: rest) →
        QualifiedBlockHasCompletion names principal requirements scope result completion (.ifStmt branches none :: next :: rest)

  inductive QualifiedBranchesHaveCompletion (names : TypeNames) (principal : Substitution)
      (requirements : List Constraint) : Scope → MonoType → Option MonoType → List (Expr × List Statement) → Prop where
    | nil {scope result completion} : QualifiedBranchesHaveCompletion names principal requirements scope result completion []
    | cons {scope result completion condition body rest} :
        QualifiedExpression scope principal requirements condition (.primitive .boolean) →
        QualifiedBlockHasCompletion names principal requirements scope result completion body →
        QualifiedBranchesHaveCompletion names principal requirements scope result completion rest →
        QualifiedBranchesHaveCompletion names principal requirements scope result completion ((condition, body) :: rest)
end

abbrev QualifiedBlock (names : TypeNames) (principal : Substitution) (requirements : List Constraint)
    (scope : Scope) (result : MonoType) (source : List Statement) : Prop :=
  QualifiedBlockHasCompletion names principal requirements scope result (some result) source

abbrev QualifiedStatementBranches (names : TypeNames) (principal : Substitution) (requirements : List Constraint)
    (scope : Scope) (result : MonoType) (source : List (Expr × List Statement)) : Prop :=
  QualifiedBranchesHaveCompletion names principal requirements scope result (some result) source

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

theorem qualified_completion_instantiates (names : TypeNames) (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (source : List Statement)
    (typed : QualifiedBlockHasCompletion names principal requirements scope result completion source)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    SourceBlockHasCompletion names (compose later principal) scope (substitute later result)
      (completion.map (substitute later)) source := by
  refine @QualifiedBlockHasCompletion.rec names principal requirements
    (fun scope result completion source _ => SourceBlockHasCompletion names (compose later principal) scope
      (substitute later result) (completion.map (substitute later)) source)
    (fun scope result completion source _ => SourceBranchesHaveCompletion names (compose later principal) scope
      (substitute later result) (completion.map (substitute later)) source)
    ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ ?_ scope result completion source typed
  · intro scope result completion accepted
    exact .nil (completion_instantiates later completion (.primitive .unit) accepted)
  · intro scope result completion kind name declared initializer rest binder rawType value annotation fresh restTyped restIH
    have valueTyped := qualified_expression_instantiates scope principal later requirements _ _ value sorted solves
    have annotationTyped := qualified_annotation_instantiates names principal later _ _ annotation
    rw [← substitute_compose] at valueTyped annotationTyped
    exact .binding valueTyped annotationTyped fresh restIH
  · intro scope result completion name value rest binding found mutable valueTyped restTyped restIH
    have valueType := qualified_expression_instantiates scope principal later requirements _ _ valueTyped sorted solves
    rw [← substitute_compose] at valueType
    exact .assignment found mutable valueType restIH
  · intro scope result completion value rest valueTyped restTyped restIH
    exact .returned (qualified_expression_instantiates scope principal later requirements _ _ valueTyped sorted solves) restIH
  · intro scope result completion name ty valueTyped accepted
    exact .tailName (qualified_expression_instantiates scope principal later requirements _ _ valueTyped sorted solves)
      (completion_instantiates later completion ty accepted)
  · intro scope result completion value ty valueTyped accepted
    exact .expressionTail (qualified_expression_instantiates scope principal later requirements _ _ valueTyped sorted solves)
      (completion_instantiates later completion ty accepted)
  · intro scope result completion value next rest ty valueTyped restTyped restIH
    exact .expressionNext (qualified_expression_instantiates scope principal later requirements _ _ valueTyped sorted solves) restIH
  · intro scope result completion body typed bodyIH
    exact .scopedTail bodyIH
  · intro scope result completion body next rest bodyTyped restTyped bodyIH restIH
    exact .scopedNext bodyIH restIH
  · intro scope result completion branches fallback branchesTyped fallbackTyped branchesIH fallbackIH
    exact .conditionalTail branchesIH fallbackIH
  · intro scope result completion branches branchesTyped accepted branchesIH
    exact .conditionalNoFallbackTail branchesIH (completion_instantiates later completion (.primitive .unit) accepted)
  · intro scope result completion branches fallback next rest branchesTyped fallbackTyped restTyped branchesIH fallbackIH restIH
    exact .conditionalNext branchesIH fallbackIH restIH
  · intro scope result completion branches next rest branchesTyped restTyped branchesIH restIH
    exact .conditionalNoFallbackNext branchesIH restIH
  · intro scope result completion
    exact .nil
  · intro scope result completion condition body rest conditionTyped bodyTyped restTyped bodyIH restIH
    exact .cons (qualified_expression_instantiates scope principal later requirements _ _ conditionTyped sorted solves) bodyIH restIH

theorem qualified_block_instantiates (names : TypeNames) (principal later : Substitution)
    (requirements : List Constraint) (scope : Scope) (result : MonoType) (source : List Statement)
    (typed : QualifiedBlock names principal requirements scope result source)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    BlockHasType names (compose later principal) scope (substitute later result) source := by
  exact qualified_completion_instantiates names principal later requirements scope result (some result) source typed sorted solves


end Xsh.Typing.Connected
