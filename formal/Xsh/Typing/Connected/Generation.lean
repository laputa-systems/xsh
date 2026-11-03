import Xsh.Typing.Connected.Types
import Xsh.Syntax.Foundation.ResolvedIdentity

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

inductive BindingKind where
  | immutable
  | mutable
  | schema
  deriving DecidableEq

structure Binding where
  name : NameId
  binder : BinderId
  kind : BindingKind
  scheme : Scheme

abbrev Scope := List Binding

inductive Node where
  | integer (lexeme : String)
  | boolean (value : Bool)
  | string (value : String)
  | read (binder : BinderId) (arguments : List (Variable × MonoType))
  | binary (operator : BinaryOp) (left right : Node)
  | call (callee : Node) (arguments : List Node)
  | record (fields : List (NameId × Node))
  | field (base : Node) (name : NameId)
  | conditional (branches : List (Node × Node)) (otherwise : Node)
  | propagate (value : Node) (error : MonoType)
  | requireValue (binder : BinderId) (identity : Xsh.Validation.Nominal.NominalId) (value : Node)


mutual
  def propagationErrors : Node → List MonoType
    | .integer _ | .boolean _ | .string _ | .read _ _ => []
    | .binary _ left right => propagationErrors left ++ propagationErrors right
    | .call target arguments => propagationErrors target ++ argumentErrors arguments
    | .record fields => recordErrors fields
    | .field base _ => propagationErrors base
    | .conditional branches otherwise => branchErrors branches ++ propagationErrors otherwise
    | .propagate value error => error :: propagationErrors value
    | .requireValue _ _ value => propagationErrors value

  def argumentErrors : List Node → List MonoType
    | [] => []
    | node :: rest => propagationErrors node ++ argumentErrors rest

  def recordErrors : List (NameId × Node) → List MonoType
    | [] => []
    | (_, node) :: rest => propagationErrors node ++ recordErrors rest

  def branchErrors : List (Node × Node) → List MonoType
    | [] => []
    | (condition, value) :: rest => propagationErrors condition ++ propagationErrors value ++ branchErrors rest
end


def propagationContext : MonoType → Option MonoType
  | .result _ error => some error
  | _ => none

def checksPropagationErrors (context : Option MonoType) (solution : Substitution) (errors : List MonoType) : Bool :=
  errors.all fun error => match context with
    | none => false
    | some expected => decide (substitute solution error = expected)

def PropagationMatches (context : Option MonoType) (solution : Substitution) (errors : List MonoType) : Prop :=
  ∀ error ∈ errors, ∃ expected, context = some expected ∧ substitute solution error = expected

theorem checks_propagation_errors_iff (context : Option MonoType) (solution : Substitution)
    (errors : List MonoType) : checksPropagationErrors context solution errors = true ↔
      PropagationMatches context solution errors := by
  cases context <;> simp [checksPropagationErrors, PropagationMatches, List.all_eq_true]

structure Generated where
  ty : MonoType
  node : Node

inductive GenerationError where
  | exhausted
  | invalidLiteral
  | unresolvedName
  | unsupportedExpression
  | unsupportedArgument
  | unsupportedRecordField
  | duplicateField
  | immutableAssignment
  | unsupportedStatement
  | unsupportedAnnotation
  | binderLimit
  deriving DecidableEq, Repr

structure GenerationState where
  nextType : Nat
  nextBinder : Nat
  constraints : List Constraint
  deriving DecidableEq

def nextAfter : List Nat → Nat
  | [] => 0
  | id :: rest => max (id + 1) (nextAfter rest)

theorem member_lt_next_after {id : Nat} {ids : List Nat} (member : id ∈ ids) :
    id < nextAfter ids := by
  induction ids with
  | nil => simp at member
  | cons head rest ih =>
    simp only [List.mem_cons] at member
    rcases member with same | later
    · subst head
      exact Nat.lt_of_lt_of_le (Nat.lt_succ_self id) (Nat.le_max_left _ _)
    · exact Nat.lt_of_lt_of_le (ih later) (Nat.le_max_right _ _)

def scopeVariables (scope : Scope) : List Variable :=
  scope.flatMap fun binding => binding.scheme.quantified ++ schemeVariables binding.scheme

def initialState (scope : Scope) : GenerationState :=
  ⟨nextAfter ((scopeVariables scope).map Variable.id),
    nextAfter (scope.map fun binding => binding.binder.value), []⟩


def initialBlockState (scope : Scope) (result : MonoType) : GenerationState :=
  let state := initialState scope
  { state with nextType := max state.nextType (nextAfter ((freeVariables result).map Variable.id)) }

theorem initial_result_ids_are_fresh {scope : Scope} {result : MonoType} {id : Variable}
    (member : id ∈ freeVariables result) : id.id < (initialBlockState scope result).nextType := by
  apply Nat.lt_of_lt_of_le (member_lt_next_after (List.mem_map.mpr ⟨id, member, rfl⟩))
  exact Nat.le_max_right _ _

theorem initial_type_ids_are_fresh {scope : Scope} {varId : Variable}
    (member : varId ∈ scopeVariables scope) : varId.id < (initialState scope).nextType := by
  apply member_lt_next_after
  exact List.mem_map.mpr ⟨varId, member, rfl⟩

theorem initial_binder_ids_are_fresh {scope : Scope} {binding : Binding}
    (member : binding ∈ scope) : binding.binder.value < (initialState scope).nextBinder := by
  apply member_lt_next_after
  exact List.mem_map.mpr ⟨binding, member, rfl⟩

abbrev Generator := StateT GenerationState (Except GenerationError)

def fresh (kind : VariableKind) : Generator MonoType := do
  let state ← get
  set { state with nextType := state.nextType + 1 }
  pure (.variable ⟨state.nextType, kind⟩)

def emit (constraint : Constraint) : Generator Unit := do
  modify fun state => { state with constraints := state.constraints ++ [constraint] }

def freshBinder : Generator BinderId := do
  let state ← get
  let some binder := BinderId.decode? state.nextBinder | throw .binderLimit
  set { state with nextBinder := state.nextBinder + 1 }
  pure binder

def instantiateArguments : List Variable → Generator (List (Variable × MonoType))
  | [] => pure []
  | id :: rest => do
      let ty ← fresh id.kind
      let others ← instantiateArguments rest
      pure ((id, ty) :: others)

def instantiate (scheme : Scheme) : Generator (MonoType × List (Variable × MonoType)) := do
  let arguments ← instantiateArguments scheme.quantified
  let substitution : Substitution := fun id => (arguments.lookup id).getD (.variable id)
  for requirement in scheme.requirements do
    emit (mapConstraint substitution requirement)
  pure (substitute substitution scheme.body, arguments)

def resolve (scope : Scope) (name : NameId) : Generator Binding :=
  match scope.find? (fun binding => decide (binding.name = name)) with
  | some binding => pure binding
  | none => throw .unresolvedName

def constrainArguments (callee : MonoType) (arguments : List MonoType) : Generator MonoType := do
  let result ← fresh .value
  let parameters := arguments.foldr MonoType.argument .argumentsEmpty
  emit (.equal callee (.function parameters result))
  pure result

def binarySignature : BinaryOp → Option (MonoType × MonoType × MonoType)
  | .add | .sub | .mul | .div | .rem =>
      some (.primitive .integer, .primitive .integer, .primitive .integer)
  | .lt | .le | .gt | .ge =>
      some (.primitive .integer, .primitive .integer, .primitive .boolean)
  | .and | .or =>
      some (.primitive .boolean, .primitive .boolean, .primitive .boolean)
  | _ => none

def insertField (field : NameId × MonoType) : List (NameId × MonoType) → List (NameId × MonoType)
  | [] => [field]
  | head :: rest =>
      if field.1.value ≤ head.1.value then field :: head :: rest
      else head :: insertField field rest

theorem insert_field_preserves_membership (field : NameId × MonoType)
    (fields : List (NameId × MonoType)) : (insertField field fields).Perm (field :: fields) := by
  induction fields with
  | nil => rfl
  | cons head rest ih =>
    simp only [insertField]
    split
    · rfl
    · exact (List.Perm.cons head ih).trans (List.Perm.swap field head rest)

def canonicalFields : List (NameId × MonoType) → List (NameId × MonoType)
  | [] => []
  | field :: rest => insertField field (canonicalFields rest)

theorem canonical_fields_preserve_membership (fields : List (NameId × MonoType)) :
    (canonicalFields fields).Perm fields := by
  induction fields with
  | nil => rfl
  | cons field rest ih =>
    exact (insert_field_preserves_membership field (canonicalFields rest)).trans (List.Perm.cons field ih)

theorem two_distinct_fields_have_order_independent_types (first second : NameId × MonoType)
    (ordered : first.1.value < second.1.value) :
    canonicalFields [first, second] = canonicalFields [second, first] := by
  simp [canonicalFields, insertField, Nat.le_of_lt ordered, Nat.not_le_of_gt ordered]

mutual
  def expression : Nat → Scope → Expr → Generator Generated
    | 0, _, _ => throw .exhausted
    | fuel + 1, scope, source => do
      match source with
      | .literal (.integer lexeme) => do
          if decimalLexeme lexeme then pure () else throw .unsupportedExpression
          if (parseInteger lexeme).isSome then pure () else throw .invalidLiteral
          pure ⟨.primitive .integer, .integer lexeme⟩
      | .literal (.bool value) => pure ⟨.primitive .boolean, .boolean value⟩
      | .literal (.string value) => pure ⟨.primitive .string, .string value⟩
      | .ident name => do
          let binding ← resolve scope name
          if binding.kind = .schema then throw .unsupportedExpression else pure ()
          let (ty, arguments) ← instantiate binding.scheme
          pure ⟨ty, .read binding.binder arguments⟩
      | .binary operator left right => do
          let some (leftType, rightType, result) := binarySignature operator
            | throw .unsupportedExpression
          let first ← expression fuel scope left
          let second ← expression fuel scope right
          emit (.equal first.ty leftType)
          emit (.equal second.ty rightType)
          pure ⟨result, .binary operator first.node second.node⟩
      | .call callee arguments => do
          let target ← expression fuel scope callee
          let (types, nodes) ← callArguments fuel scope arguments
          let result ← constrainArguments target.ty types
          pure ⟨result, .call target.node nodes⟩
      | .record fields => do
          let (types, nodes) ← recordFields fuel scope fields
          if (types.map Prod.fst).Nodup then
            pure ⟨.record ((canonicalFields types).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty),
              .record nodes⟩
          else throw .duplicateField
      | .field base name => do
          let record ← expression fuel scope base
          let value ← fresh .value
          emit (.field record.ty name value)
          pure ⟨value, .field record.node name⟩
      | .ifExpr arms otherwise => do
          let result ← fresh .value
          let branches ← conditionalBranches fuel scope result arms
          let fallback ← expression fuel scope otherwise
          emit (.equal fallback.ty result)
          pure ⟨result, .conditional branches fallback.node⟩
      | .require value (some (.named name)) => do
          let binding ← resolve scope name
          if binding.kind = .schema then pure () else throw .unsupportedExpression
          if binding.scheme.quantified.isEmpty && binding.scheme.requirements.isEmpty then pure ()
            else throw .unsupportedExpression
          let .nominal identity (.record row) := binding.scheme.body | throw .unsupportedExpression
          let base := MonoType.record row
          if checksWellFormed (.nominal identity base) then pure () else throw .unsupportedExpression
          let generated ← expression fuel scope value
          pure ⟨.result (.nominal identity base) (.primitive .error),
            .requireValue binding.binder identity generated.node⟩
      | .tryExpr value => do
          let inner ← expression fuel scope value
          let result ← fresh .value
          let error ← fresh .value
          emit (.equal inner.ty (.result result error))
          pure ⟨result, .propagate inner.node error⟩
      | _ => throw .unsupportedExpression

  def callArguments : Nat → Scope → List CallArgument → Generator (List MonoType × List Node)
    | 0, _, _ => throw .exhausted
    | _ + 1, _, [] => pure ([], [])
    | fuel + 1, scope, argument :: rest => do
      let .positional value := argument | throw .unsupportedArgument
      let generated ← expression fuel scope value
      let (types, nodes) ← callArguments fuel scope rest
      pure (generated.ty :: types, generated.node :: nodes)

  def recordFields : Nat → Scope → List RecordField → Generator (List (NameId × MonoType) × List (NameId × Node))
    | 0, _, _ => throw .exhausted
    | _ + 1, _, [] => pure ([], [])
    | fuel + 1, scope, field :: rest => do
      let .named name value := field | throw .unsupportedRecordField
      let generated ← expression fuel scope value
      let (types, nodes) ← recordFields fuel scope rest
      pure ((name, generated.ty) :: types, (name, generated.node) :: nodes)

  def conditionalBranches : Nat → Scope → MonoType → List (Expr × Expr) → Generator (List (Node × Node))
    | 0, _, _, _ => throw .exhausted
    | _ + 1, _, _, [] => pure []
    | fuel + 1, scope, result, (condition, value) :: rest => do
      let condition ← expression fuel scope condition
      let value ← expression fuel scope value
      emit (.equal condition.ty (.primitive .boolean))
      emit (.equal value.ty result)
      let others ← conditionalBranches fuel scope result rest
      pure ((condition.node, value.node) :: others)
end

structure TypeNames where
  integer : NameId
  boolean : NameId
  string : NameId
  result : NameId

def annotation (names : TypeNames) : TypeExpr → Option MonoType
  | .named name =>
      if name = names.integer then some (.primitive .integer)
      else if name = names.boolean then some (.primitive .boolean)
      else if name = names.string then some (.primitive .string)
      else none
  | .applied (.named name) [value, error] => do
      if name = names.result then pure () else none
      some (.result (← annotation names value) (← annotation names error))
  | _ => none

inductive StatementNode where
  | binding (binder : BinderId) (kind : BindingKind) (initializer : Node)
  | assignment (binder : BinderId) (value : Node)
  | evaluated (value : Node)
  | returned (value : Node)
  | conditional (branches : List (Node × List StatementNode)) (otherwise : List StatementNode)


def constrainPropagation (result : MonoType) (node : Node) : Generator Unit := do
  for error in propagationErrors node do
    let value ← fresh .value
    emit (.equal result (.result value error))

mutual
  def block : Nat → TypeNames → Scope → MonoType → List Statement → Generator (List StatementNode)
    | 0, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [] => pure []
    | fuel + 1, names, scope, result, statement :: rest => do
      match statement with
      | .binding kind (.name name) typeAnnotation initializer => do
          let generated ← expression fuel scope initializer
          constrainPropagation result generated.node
          match typeAnnotation with
          | some declared =>
              let some ty := annotation names declared | throw .unsupportedAnnotation
              emit (.equal generated.ty ty)
          | none => pure ()
          let binder ← freshBinder
          let bindingKind := if kind = .var then BindingKind.mutable else .immutable
          let binding : Binding := ⟨name, binder, bindingKind, ⟨[], generated.ty, []⟩⟩
          let following ← block fuel names (binding :: scope) result rest
          pure (.binding binder bindingKind generated.node :: following)
      | .assign (.name name) .set value => do
          let binding ← resolve scope name
          if binding.kind = .mutable then pure () else throw .immutableAssignment
          let generated ← expression fuel scope value
          constrainPropagation result generated.node
          emit (.equal generated.ty binding.scheme.body)
          let following ← block fuel names scope result rest
          pure (.assignment binding.binder generated.node :: following)
      | .returnValue (some value) => do
          let generated ← expression fuel scope value
          constrainPropagation result generated.node
          emit (.equal generated.ty result)
          let following ← block fuel names scope result rest
          pure (.returned generated.node :: following)
      | .expression value => do
          let generated ← expression fuel scope value
          constrainPropagation result generated.node
          if rest.isEmpty then emit (.equal generated.ty result) else pure ()
          let following ← block fuel names scope result rest
          pure (.evaluated generated.node :: following)
      | .ifStmt branches otherwise => do
          let generated ← statementBranches fuel names scope result branches
          let fallback ← match otherwise with
            | some body => block fuel names scope result body
            | none => pure []
          let following ← block fuel names scope result rest
          pure (.conditional generated fallback :: following)
      | _ => throw .unsupportedStatement

  def statementBranches : Nat → TypeNames → Scope → MonoType →
      List (Expr × List Statement) → Generator (List (Node × List StatementNode))
    | 0, _, _, _, _ => throw .exhausted
    | _ + 1, _, _, _, [] => pure []
    | fuel + 1, names, scope, result, (condition, body) :: rest => do
      let condition ← expression fuel scope condition
      constrainPropagation result condition.node
      emit (.equal condition.ty (.primitive .boolean))
      let body ← block fuel names scope result body
      let others ← statementBranches fuel names scope result rest
      pure ((condition.node, body) :: others)
end

def exampleName (id : Nat) (bound : id < 4294967296) : NameId := ⟨⟨id, bound⟩⟩

def identityBinding : Binding :=
  ⟨exampleName 0 (by decide), ⟨⟨0, by decide⟩⟩, .immutable, identityScheme⟩

def independentIdentityCalls : Expr :=
  .record [
    .named (exampleName 1 (by decide))
      (.call (.ident identityBinding.name) [.positional (.literal (.integer "7"))]),
    .named (exampleName 2 (by decide))
      (.call (.ident identityBinding.name) [.positional (.literal (.bool true))])]

def identityCallsConstraints : Except GenerationError (MonoType × List Constraint) := do
  let (generated, state) ← (expression 16 [identityBinding] independentIdentityCalls).run ⟨10, 1, []⟩
  pure (generated.ty, state.constraints)

theorem independent_calls_generate_distinct_instantiations :
    identityCallsConstraints = .ok (
      .record (.rowField (exampleName 1 (by decide)) (.variable ⟨11, .value⟩)
        (.rowField (exampleName 2 (by decide)) (.variable ⟨13, .value⟩) .rowEmpty)),
      [ .equal (.function (.argument (.variable ⟨10, .value⟩) .argumentsEmpty) (.variable ⟨10, .value⟩))
          (.function (.argument (.primitive .integer) .argumentsEmpty) (.variable ⟨11, .value⟩)),
        .equal (.function (.argument (.variable ⟨12, .value⟩) .argumentsEmpty) (.variable ⟨12, .value⟩))
          (.function (.argument (.primitive .boolean) .argumentsEmpty) (.variable ⟨13, .value⟩)) ]) := by
  rfl

def immutableAssignment : Except GenerationError Unit := do
  let names : TypeNames := ⟨exampleName 3 (by decide), exampleName 4 (by decide),
    exampleName 5 (by decide), exampleName 6 (by decide)⟩
  let _ ← (block 8 names [identityBinding] (.primitive .integer)
    [.assign (.name identityBinding.name) .set (.literal (.integer "1"))]).run ⟨10, 1, []⟩
  pure ()

theorem assignment_to_immutable_binding_is_rejected :
    immutableAssignment = .error .immutableAssignment := by
  rfl

def exampleTypeNames : TypeNames :=
  ⟨exampleName 3 (by decide), exampleName 4 (by decide),
    exampleName 5 (by decide), exampleName 6 (by decide)⟩

def resultValueVariable : Variable := ⟨1, .value⟩

def okScheme : Scheme :=
  ⟨[identityVariable, resultValueVariable],
    .function (.argument (.variable identityVariable) .argumentsEmpty)
      (.result (.variable identityVariable) (.variable resultValueVariable)), []⟩

def errorScheme : Scheme :=
  ⟨[identityVariable, resultValueVariable],
    .function (.argument (.variable resultValueVariable) .argumentsEmpty)
      (.result (.variable identityVariable) (.variable resultValueVariable)), []⟩

def composedScope : Scope :=
  [ identityBinding,
    ⟨exampleName 7 (by decide), ⟨⟨7, by decide⟩⟩, .immutable, ⟨[], .primitive .boolean, []⟩⟩,
    ⟨exampleName 8 (by decide), ⟨⟨8, by decide⟩⟩, .immutable, okScheme⟩,
    ⟨exampleName 9 (by decide), ⟨⟨9, by decide⟩⟩, .immutable, errorScheme⟩ ]

def composedBody : List Statement :=
  [ .binding .letBinding (.name (exampleName 10 (by decide))) none
      (.record [
        .named (exampleName 11 (by decide))
          (.call (.ident identityBinding.name) [.positional (.literal (.integer "7"))]),
        .named (exampleName 12 (by decide))
          (.call (.ident identityBinding.name) [.positional (.ident (exampleName 7 (by decide)))])]),
    .binding .var (.name (exampleName 13 (by decide))) none
      (.field (.ident (exampleName 10 (by decide))) (exampleName 11 (by decide))),
    .assign (.name (exampleName 13 (by decide))) .set
      (.binary .add (.ident (exampleName 13 (by decide))) (.literal (.integer "1"))),
    .returnValue (some (.ifExpr
      [(.field (.ident (exampleName 10 (by decide))) (exampleName 12 (by decide)),
        .call (.ident (exampleName 8 (by decide))) [.positional (.ident (exampleName 13 (by decide)))])]
      (.call (.ident (exampleName 9 (by decide))) [.positional (.literal (.string "denied"))]))) ]

def composedBodyShape : Bool :=
  match (block 48 exampleTypeNames composedScope (.result (.primitive .integer) (.primitive .string))
      composedBody).run ⟨20, 20, []⟩ with
  | .ok ([.binding _ .immutable (.record [(_, .call _ _), (_, .call _ _)]),
      .binding _ .mutable (.field _ _), .assignment _ (.binary .add _ _),
      .returned (.conditional [(_, .call _ _)] (.call _ _))], _) => true
  | _ => false

theorem composed_body_preserves_statement_and_branch_structure : composedBodyShape = true := by
  decide +kernel

def composedSolution (id : Variable) : MonoType :=
  match id with
  | ⟨20, .value⟩ | ⟨21, .value⟩ | ⟨24, .value⟩ | ⟨27, .value⟩ | ⟨30, .value⟩ =>
      .primitive .integer
  | ⟨22, .value⟩ | ⟨23, .value⟩ | ⟨26, .value⟩ => .primitive .boolean
  | ⟨28, .value⟩ | ⟨31, .value⟩ => .primitive .string
  | ⟨25, .value⟩ | ⟨29, .value⟩ | ⟨32, .value⟩ =>
      .result (.primitive .integer) (.primitive .string)
  | other => .variable other

def composedConstraintsAccepted : Bool :=
  match (block 48 exampleTypeNames composedScope (.result (.primitive .integer) (.primitive .string))
      composedBody).run ⟨20, 20, []⟩ with
  | .ok (_, state) => checksConstraints composedSolution state.constraints
  | .error _ => false

theorem composed_constraints_accept_a_solution : composedConstraintsAccepted = true := by
  decide +kernel

theorem composed_generated_constraints_are_solved :
    match (block 48 exampleTypeNames composedScope (.result (.primitive .integer) (.primitive .string))
        composedBody).run ⟨20, 20, []⟩ with
    | .ok (_, state) => Solves state.constraints composedSolution
    | .error _ => False := by
  have accepted := composed_constraints_accept_a_solution
  cases result : (block 48 exampleTypeNames composedScope
      (.result (.primitive .integer) (.primitive .string)) composedBody).run ⟨20, 20, []⟩ with
  | error error => simp [composedConstraintsAccepted, result] at accepted
  | ok pair =>
    obtain ⟨generated, state⟩ := pair
    simp only [result]
    have checked : checksConstraints composedSolution state.constraints = true := by
      simpa [composedConstraintsAccepted, result] using accepted
    exact (checks_constraints_iff composedSolution state.constraints).mp checked

end Xsh.Typing.Connected
