import Xsh.Typing.Connected.Generation

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

inductive BinaryTyping : BinaryOp → MonoType → MonoType → MonoType → Prop where
  | add : BinaryTyping .add (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | sub : BinaryTyping .sub (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | mul : BinaryTyping .mul (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | div : BinaryTyping .div (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | rem : BinaryTyping .rem (.primitive .integer) (.primitive .integer) (.primitive .integer)
  | lt : BinaryTyping .lt (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | le : BinaryTyping .le (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | gt : BinaryTyping .gt (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | ge : BinaryTyping .ge (.primitive .integer) (.primitive .integer) (.primitive .boolean)
  | and : BinaryTyping .and (.primitive .boolean) (.primitive .boolean) (.primitive .boolean)
  | or : BinaryTyping .or (.primitive .boolean) (.primitive .boolean) (.primitive .boolean)

theorem binary_signature_sound {operator : BinaryOp} {left right result : MonoType}
    (found : binarySignature operator = some (left, right, result)) :
    BinaryTyping operator left right result := by
  cases operator <;> simp [binarySignature] at found
  all_goals obtain ⟨rfl, rfl, rfl⟩ := found <;> constructor

theorem binary_signature_complete {operator : BinaryOp} {left right result : MonoType}
    (typed : BinaryTyping operator left right result) :
    binarySignature operator = some (left, right, result) := by
  cases typed <;> rfl

def AnnotationAgrees (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType) : Prop :=
  match declared with
  | none => True
  | some source => ∃ annotated, annotation names source = some annotated ∧
      substitute solution annotated = ty

def bindingKind (kind : Xsh.Syntax.BindingKind) : BindingKind :=
  if kind = .var then .mutable else .immutable

mutual
  inductive ExpressionHasTypeIn (solution : Substitution) :
      Option (TypeNames × MonoType) → Scope → Expr → MonoType → Prop where
    | integer {context scope lexeme value} :
        parseInteger lexeme = some value →
        ExpressionHasTypeIn solution context scope (.literal (.integer lexeme)) (.primitive .integer)
    | boolean {context scope value} :
        ExpressionHasTypeIn solution context scope (.literal (.bool value)) (.primitive .boolean)
    | string {context scope value} :
        ExpressionHasTypeIn solution context scope (.literal (.string value)) (.primitive .string)
    | read {context scope name binding arguments ty requirements} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind ≠ .schema →
        Instantiates binding.scheme arguments ty requirements → Solves requirements solution →
        WellFormed (substitute solution ty) → typeSort (substitute solution ty) = .value →
        ExpressionHasTypeIn solution context scope (.ident name) (substitute solution ty)
    | binary {context scope operator left right leftType rightType result} :
        ExpressionHasTypeIn solution context scope left leftType → ExpressionHasTypeIn solution context scope right rightType →
        BinaryTyping operator leftType rightType result →
        ExpressionHasTypeIn solution context scope (.binary operator left right) result
    | call {context scope callee arguments argumentTypes result} :
        ExpressionHasTypeIn solution context scope callee (.function argumentTypes result) →
        ArgumentsHaveTypesIn solution context scope arguments argumentTypes →
        ExpressionHasTypeIn solution context scope (.call callee arguments) result
    | record {context scope fields types} :
        RecordFieldsHaveTypesIn solution context scope fields types → (types.map Prod.fst).Nodup →
        ExpressionHasTypeIn solution context scope (.record fields)
          (.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {context scope base name row result} :
        ExpressionHasTypeIn solution context scope base (.record row) → FieldOccurs name result row →
        ExpressionHasTypeIn solution context scope (.field base name) result
    | conditional {context scope branches otherwise result} :
        BranchesHaveTypeIn solution context scope branches result → ExpressionHasTypeIn solution context scope otherwise result →
        ExpressionHasTypeIn solution context scope (.ifExpr branches otherwise) result
    | requireValue {context scope value name binding identity row input} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .schema → binding.scheme.quantified = [] → binding.scheme.requirements = [] →
        binding.scheme.body = .nominal identity (.record row) → WellFormed (.nominal identity (.record row)) →
        ExpressionHasTypeIn solution context scope value input →
        ExpressionHasTypeIn solution context scope (.require value (some (.named name)))
          (.result (.nominal identity (.record (substitute solution row))) (.primitive .error))
    | propagate {context scope value result error} :
        ExpressionHasTypeIn solution context scope value (.result result error) →
        ExpressionHasTypeIn solution context scope (.tryExpr value) result
    | valueBlock {names functionResult scope body result} :
        SourceBlockHasCompletion solution names scope functionResult (some result) body →
        ExpressionHasTypeIn solution (some (names, functionResult)) scope (.valueBlock body) result

  inductive ArgumentsHaveTypesIn (solution : Substitution) :
      Option (TypeNames × MonoType) → Scope → List CallArgument → MonoType → Prop where
    | nil {context scope} : ArgumentsHaveTypesIn solution context scope [] .argumentsEmpty
    | cons {context scope expression rest ty types} :
        ExpressionHasTypeIn solution context scope expression ty → ArgumentsHaveTypesIn solution context scope rest types →
        ArgumentsHaveTypesIn solution context scope (.positional expression :: rest) (.argument ty types)

  inductive RecordFieldsHaveTypesIn (solution : Substitution) :
      Option (TypeNames × MonoType) → Scope → List RecordField → List (NameId × MonoType) → Prop where
    | nil {context scope} : RecordFieldsHaveTypesIn solution context scope [] []
    | cons {context scope name expression rest ty types} :
        ExpressionHasTypeIn solution context scope expression ty → RecordFieldsHaveTypesIn solution context scope rest types →
        RecordFieldsHaveTypesIn solution context scope (.named name expression :: rest) ((name, ty) :: types)

  inductive BranchesHaveTypeIn (solution : Substitution) :
      Option (TypeNames × MonoType) → Scope → List (Expr × Expr) → MonoType → Prop where
    | nil {context scope result} : BranchesHaveTypeIn solution context scope [] result
    | cons {context scope condition branch rest result} :
        ExpressionHasTypeIn solution context scope condition (.primitive .boolean) →
        ExpressionHasTypeIn solution context scope branch result → BranchesHaveTypeIn solution context scope rest result →
        BranchesHaveTypeIn solution context scope ((condition, branch) :: rest) result

  inductive SourceBlockHasCompletion (solution : Substitution) :
      TypeNames → Scope → MonoType → Option MonoType → List Statement → Prop where
    | nil {names scope result completion} : CompletionAccepts completion (.primitive .unit) →
        SourceBlockHasCompletion solution names scope result completion []
    | binding {names scope result completion kind name declared initializer rest binder rawType} :
        ExpressionHasTypeIn solution (some (names, result)) scope initializer (substitute solution rawType) →
        AnnotationAgrees names solution declared (substitute solution rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        SourceBlockHasCompletion solution names
          (⟨name, binder, bindingKind kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        SourceBlockHasCompletion solution names scope result completion (.binding kind (.name name) declared initializer :: rest)
    | assignment {names scope result completion name value rest binding} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        ExpressionHasTypeIn solution (some (names, result)) scope value (substitute solution binding.scheme.body) →
        SourceBlockHasCompletion solution names scope result completion rest →
        SourceBlockHasCompletion solution names scope result completion (.assign (.name name) .set value :: rest)
    | returned {names scope result completion value rest} :
        ExpressionHasTypeIn solution (some (names, result)) scope value result →
        SourceBlockHasCompletion solution names scope result none rest →
        SourceBlockHasCompletion solution names scope result completion (.returnValue (some value) :: rest)
    | tailName {names scope result completion name ty} :
        ExpressionHasTypeIn solution (some (names, result)) scope (.ident name) ty → CompletionAccepts completion ty →
        SourceBlockHasCompletion solution names scope result completion [.tailName name]
    | expressionTail {names scope result completion value ty} :
        ExpressionHasTypeIn solution (some (names, result)) scope value ty → CompletionAccepts completion ty →
        SourceBlockHasCompletion solution names scope result completion [.expression value]
    | expressionNext {names scope result completion value next rest ty} :
        ExpressionHasTypeIn solution (some (names, result)) scope value ty →
        SourceBlockHasCompletion solution names scope result completion (next :: rest) →
        SourceBlockHasCompletion solution names scope result completion (.expression value :: next :: rest)
    | conditionalTail {names scope result completion branches fallback} :
        SourceBranchesHaveCompletion solution names scope result completion branches →
        SourceBlockHasCompletion solution names scope result completion fallback →
        SourceBlockHasCompletion solution names scope result completion [.ifStmt branches (some fallback)]
    | conditionalNoFallbackTail {names scope result completion branches} :
        SourceBranchesHaveCompletion solution names scope result completion branches →
        CompletionAccepts completion (.primitive .unit) →
        SourceBlockHasCompletion solution names scope result completion [.ifStmt branches none]
    | conditionalNext {names scope result completion branches fallback next rest} :
        SourceBranchesHaveCompletion solution names scope result none branches →
        SourceBlockHasCompletion solution names scope result none fallback →
        SourceBlockHasCompletion solution names scope result completion (next :: rest) →
        SourceBlockHasCompletion solution names scope result completion (.ifStmt branches (some fallback) :: next :: rest)
    | conditionalNoFallbackNext {names scope result completion branches next rest} :
        SourceBranchesHaveCompletion solution names scope result none branches →
        SourceBlockHasCompletion solution names scope result completion (next :: rest) →
        SourceBlockHasCompletion solution names scope result completion (.ifStmt branches none :: next :: rest)

  inductive SourceBranchesHaveCompletion (solution : Substitution) :
      TypeNames → Scope → MonoType → Option MonoType → List (Expr × List Statement) → Prop where
    | nil {names scope result completion} : SourceBranchesHaveCompletion solution names scope result completion []
    | cons {names scope result completion condition body rest} :
        ExpressionHasTypeIn solution (some (names, result)) scope condition (.primitive .boolean) →
        SourceBlockHasCompletion solution names scope result completion body →
        SourceBranchesHaveCompletion solution names scope result completion rest →
        SourceBranchesHaveCompletion solution names scope result completion ((condition, body) :: rest)
end

abbrev ExpressionHasType (scope : Scope) (solution : Substitution) (source : Expr) (ty : MonoType) : Prop :=
  ExpressionHasTypeIn solution none scope source ty
abbrev ArgumentsHaveTypes (scope : Scope) (solution : Substitution) (arguments : List CallArgument) (ty : MonoType) : Prop :=
  ArgumentsHaveTypesIn solution none scope arguments ty
abbrev RecordFieldsHaveTypes (scope : Scope) (solution : Substitution) (fields : List RecordField)
    (types : List (NameId × MonoType)) : Prop := RecordFieldsHaveTypesIn solution none scope fields types
abbrev BranchesHaveType (scope : Scope) (solution : Substitution) (branches : List (Expr × Expr))
    (ty : MonoType) : Prop := BranchesHaveTypeIn solution none scope branches ty
abbrev BlockHasType (names : TypeNames) (solution : Substitution) (scope : Scope)
    (result : MonoType) (source : List Statement) : Prop :=
  SourceBlockHasCompletion solution names scope result (some result) source
abbrev StatementBranchesHaveType (names : TypeNames) (solution : Substitution) (scope : Scope)
    (result : MonoType) (branches : List (Expr × List Statement)) : Prop :=
  SourceBranchesHaveCompletion solution names scope result (some result) branches

end Xsh.Typing.Connected
