import Xsh.Foundation

/-!
An initial, closed syntax fragment for binding and name-flow proofs.

`RawName` is a parser/name-table identity. It is not a resolved `BinderId`:
this syntax records where a source name occurs, while semantic resolution
assigns declaration identities later. Numeric literal constructors likewise
retain their lexemes; range checking and interpretation belong to later
phases.

This fragment covers expression, statement, pattern, type, assignment-target,
call-argument, record-field, and comprehension forms used by lexical binding
examples. It deliberately does not claim to enumerate all of `ArenaExprKind`,
`ArenaStmtKind`, imports, declarations, command/run forms, structured stream
stages, or their parser/lowering metadata. In particular, a represented term
is not evidence that the Rust parser produced it or that the checker accepts
it. Record binding targets retain nested field selections; free-name traversal
is fuel-bounded by recursive depth and may omit names from a subtree while
retaining names from later siblings. The examples prove their results for the
chosen bounds; no general adequacy theorem connects the bound to source size
yet. Extend this layer only alongside the current source and owner inventory.
Blocks here have statement bodies but no general parameter list, so callable
defaults, stage parameters, and the error parameter of a `with` handler are
not covered by the free-name function.
-/

namespace Xsh.Syntax

open Xsh.Foundation

abbrev RawName := NameId

inductive Effect where
  | fs
  | net
  | process
  | env
  | time
  | error
  | io
  deriving DecidableEq

inductive UnaryOp where
  | not
  | neg
  deriving DecidableEq

inductive BinaryOp where
  | resultFallback
  | or
  | and
  | eq
  | ne
  | lt
  | le
  | gt
  | ge
  | member
  | notMember
  | add
  | sub
  | mul
  | div
  | rem
  | union
  | intersect
  deriving DecidableEq

inductive AssignOp where
  | set
  | add
  | sub
  | mul
  | div
  | rem
  deriving DecidableEq

inductive Literal where
  | null
  | bool (value : Bool)
  | integer (sourceLexeme : String)
  | float (sourceLexeme : String)
  | duration (sourceLexeme : String)
  | string (value : String)
  | bytes (value : List UInt8)
  | path (sourceLexeme : String)
  | regex (sourceLexeme : String)
  deriving DecidableEq, Repr

/-- Source type syntax; it does not assert that a type name is declared. -/
inductive TypeExpr where
  | named (name : RawName)
  | qualified (path : List RawName)
  | applied (constructor : TypeExpr) (arguments : List TypeExpr)
  | list (element : TypeExpr)
  | map (key : TypeExpr) (value : TypeExpr)
  | stream (element : TypeExpr)
  | module (interface : TypeExpr)
  | result (value : TypeExpr)
  | optional (value : TypeExpr)
  | union (members : List TypeExpr)
  | callable (pure : Bool) (parameters : List TypeExpr)
      (effects : Option (List Effect)) (result : TypeExpr)
  | nonEmpty (element : TypeExpr)
  | set (element : TypeExpr)

/-- A source binding target contains names, not resolved declaration IDs. -/
inductive BindingTarget where
  | name (name : RawName)
  | discard
  | record (fields : List (RawName × BindingTarget)) (hasRest : Bool)

def BindingTarget.namesFuel : Nat → BindingTarget → List RawName
  | 0, _ => []
  | _fuel + 1, .name n => [n]
  | _fuel + 1, .discard => []
  | fuel + 1, .record fields _ => fields.flatMap fun (_, target) =>
      target.namesFuel fuel

inductive TextPatternPart where
  | text (value : String)
  | hole (name : Option RawName) (formatSpec : Option RawName)
  deriving DecidableEq

inductive Pattern where
  | group (inner : Pattern)
  | alias (inner : Pattern) (name : RawName)
  | wildcard
  | testName (name : RawName) (type : TypeExpr)
  | binding (name : RawName)
  | typeBind (name : Option RawName) (type : TypeExpr)
  | literal (value : Literal)
  | record (fields : List (RawName × Pattern)) (hasRest : Bool)
  | list (elements : List Pattern) (rest : Option Pattern)
  | alternation (members : List Pattern)
  | constructor (name : RawName) (argument : Option Pattern)
  | errorVariant (family : RawName) (variant : RawName)
      (fields : List (RawName × Pattern))
  | facet (name : RawName)
  | tuple (elements : List Pattern)
  | text (parts : List TextPatternPart)
  | textHole (name : Option RawName) (formatSpec : Option RawName)

/-- Candidate binders are syntactic spellings, not checker-resolved binders. -/
def Pattern.boundNamesFuel : Nat → Pattern → List RawName
  | 0, _ => []
  | fuel + 1, pattern => match pattern with
    | .group inner => inner.boundNamesFuel fuel
    | .alias inner name => inner.boundNamesFuel fuel ++ [name]
    | .wildcard => []
    | .testName _ _ => []
    | .binding name => [name]
    | .typeBind name _ => name.toList
    | .literal _ => []
    | .record fields _ => fields.flatMap fun (_, inner) => inner.boundNamesFuel fuel
    | .list elements rest =>
        elements.flatMap (fun inner => inner.boundNamesFuel fuel) ++
          rest.toList.flatMap (fun inner => inner.boundNamesFuel fuel)
    | .alternation members => members.flatMap (fun inner => inner.boundNamesFuel fuel)
    | .constructor _ argument => argument.toList.flatMap (fun inner => inner.boundNamesFuel fuel)
    | .errorVariant _ _ fields => fields.flatMap fun (_, inner) => inner.boundNamesFuel fuel
    | .facet _ => []
    | .tuple elements => elements.flatMap (fun inner => inner.boundNamesFuel fuel)
    | .text parts => parts.flatMap fun
        | .text _ => []
        | .hole name _ => name.toList
    | .textHole name _ => name.toList

/-- Names tested by a pattern are reads; constructor and type labels are not. -/
def Pattern.freeReferencesFuel : Nat → Pattern → List RawName
  | 0, _ => []
  | fuel + 1, pattern => match pattern with
    | .group inner => inner.freeReferencesFuel fuel
    | .alias inner _ => inner.freeReferencesFuel fuel
    | .wildcard => []
    | .testName name _ => [name]
    | .binding _ => []
    | .typeBind _ _ => []
    | .literal _ => []
    | .record fields _ => fields.flatMap fun (_, inner) => inner.freeReferencesFuel fuel
    | .list elements rest =>
        elements.flatMap (fun inner => inner.freeReferencesFuel fuel) ++
          rest.toList.flatMap (fun inner => inner.freeReferencesFuel fuel)
    | .alternation members => members.flatMap (fun inner => inner.freeReferencesFuel fuel)
    | .constructor _ argument => argument.toList.flatMap (fun inner => inner.freeReferencesFuel fuel)
    | .errorVariant _ _ fields => fields.flatMap fun (_, inner) => inner.freeReferencesFuel fuel
    | .facet _ => []
    | .tuple elements => elements.flatMap (fun inner => inner.freeReferencesFuel fuel)
    | .text _ => []
    | .textHole _ _ => []

inductive BindingKind where
  | const
  | letBinding
  | var
  deriving DecidableEq, Repr

inductive DeferKind where
  | exit
  | error
  deriving DecidableEq, Repr

inductive ContextScopeKind where
  | cwd
  | env
  | within
  deriving DecidableEq, Repr

inductive RetrySchedule where
  | delays
  | backoff
  deriving DecidableEq, Repr

mutual
  /-- A recursive source term that keeps names and numeric spellings raw. -/
  inductive Expr where
    | literal (value : Literal)
    | ident (name : RawName)
    | valuePipelineHole
    | item
    | lastStatus
    | list (elements : List Expr)
    | set (elements : List Expr)
    | record (fields : List RecordField)
    | ifExpr (branches : List (Expr × Expr)) (otherwise : Expr)
    | matchExpr (subject : Expr) (arms : List (Pattern × Option Expr × Expr))
    | unary (op : UnaryOp) (value : Expr)
    | comparisonChain (first : Expr) (rest : List (BinaryOp × Expr))
    | binary (op : BinaryOp) (left right : Expr)
    | call (callee : Expr) (arguments : List CallArgument)
    | valuePipelineCall (input call : Expr)
    | field (base : Expr) (name : RawName)
    | nullSafeField (base : Expr) (name : RawName)
    | index (base index : Expr) (guarded : Bool)
    | slice (base : Expr) (start finish : Option Expr) (guarded : Bool)
    | envString (key : RawName)
    | envPathList
    | pipeline (input : Expr) (stages : List Expr)
    | structuredPipeline (input : Expr) (stages : List Expr)
    | spawn (command : Expr)
    | wait (target : Expr)
    | builderCall (call : Expr) (body : List Statement)
    | tryExpr (value : Expr)
    | require (value : Expr) (schema : Option TypeExpr)
    | convert (value : Expr) (target : TypeExpr)
    | capture (body : List Statement)
    | collect (body : List Statement)
    | loopExpr (body : List Statement)
    | retry (schedule : RetrySchedule) (delays : List Expr)
        (pattern : Option Pattern) (body : List Statement)
    | valueBlock (body : List Statement)
    | errorContext (message : Expr) (body : List Statement)
    | contextScope (kind : ContextScopeKind) (input : Expr) (body : List Statement)
    | tempDirScope (path : Option Expr) (parameter : Option RawName) (body : List Statement)
    | resourceScope (bindings : List (Option RawName × Expr)) (body : List Statement)
    | listComp (value : Expr) (qualifiers : List Qualifier)
    | mapComp (key value : Expr) (qualifiers : List Qualifier)
    | setComp (value : Expr) (qualifiers : List Qualifier)

  /-- Statements in the lexical fragment; block lists introduce local scope. -/
  inductive Statement where
    | binding (kind : BindingKind) (target : BindingTarget)
        (annotation : Option TypeExpr) (initializer : Expr)
    | assign (target : AssignTarget) (op : AssignOp) (value : Expr)
    | expression (value : Expr)
    | tailName (name : RawName)
    | returnValue (value : Option Expr)
    | yield (value : Expr)
    | yieldDelegate (value : Expr)
    | defer (kind : DeferKind) (action : Expr)
    | ifStmt (branches : List (Expr × List Statement))
        (otherwise : Option (List Statement))
    | whileStmt (condition : Expr) (body : List Statement)
    | forStmt (target : BindingTarget) (iterable : Expr) (body : List Statement)
    | withScope (bindings : List (Option RawName × Expr))
        (body : List Statement) (otherwise : List Statement)
    | loopStmt (body : List Statement)
    | guardStmt (target : BindingTarget) (annotation : Option TypeExpr)
        (initializer : Expr) (otherwise : List Statement)
    | assert (condition : Expr) (message : Option Expr)
    | breakStmt (value : Option Expr)
    | continueStmt
    | exit (status : Expr)
    | matchStmt (subject : Expr)
        (arms : List (Pattern × Option Expr × List Statement))

  /-- Assignment roots are uses; an environment key is not a lexical variable. -/
  inductive AssignTarget where
    | name (name : RawName)
    | field (base : AssignTarget) (fieldName : RawName)
    | index (base : AssignTarget) (index : Expr)
    | env (key : RawName)

  inductive RecordField where
    | computed (key value : Expr)
    | path (names : List RawName) (value : Expr)
    | named (name : RawName) (value : Expr)
    | shorthand (name : RawName)
    | spread (value : Expr)

  inductive CallArgument where
    | positional (value : Expr)
    | namedSpread (value : Expr)
    | splice (value : Expr)
    | named (name : RawName) (value : Expr)

  inductive Qualifier where
    | each (target : BindingTarget) (iterable : Expr)
    | when (condition : Expr)
end

def withoutBoundNames (bound : List RawName) (names : List RawName) : List RawName :=
  names.filter fun name => !(bound.contains name)

mutual
  /-- Depth-bounded traversal keeps this executable even before parser parity. -/
  def freeExprFuel : Nat → List RawName → Expr → List RawName
    | 0, _, _ => []
    | fuel + 1, bound, expression => match expression with
      | .literal _ => []
      | .ident name => withoutBoundNames bound [name]
      | .valuePipelineHole => []
      | .item => []
      | .lastStatus => []
      | .list elements => freeExprsFuel fuel bound elements
      | .set elements => freeExprsFuel fuel bound elements
      | .record fields => freeRecordFieldsFuel fuel bound fields
      | .ifExpr branches otherwise =>
          branches.flatMap (fun (condition, value) =>
            freeExprFuel fuel bound condition ++ freeExprFuel fuel bound value) ++
              freeExprFuel fuel bound otherwise
      | .matchExpr subject arms =>
          freeExprFuel fuel bound subject ++ arms.flatMap (fun (pattern, guard, value) =>
            let armBound := pattern.boundNamesFuel fuel ++ bound
            withoutBoundNames bound (pattern.freeReferencesFuel fuel) ++
              guard.toList.flatMap (freeExprFuel fuel armBound) ++
                freeExprFuel fuel armBound value)
      | .unary _ value => freeExprFuel fuel bound value
      | .comparisonChain first rest =>
          freeExprFuel fuel bound first ++
            rest.flatMap (fun (_, value) => freeExprFuel fuel bound value)
      | .binary _ left right => freeExprFuel fuel bound left ++ freeExprFuel fuel bound right
      | .call callee arguments =>
          freeExprFuel fuel bound callee ++ freeCallArgumentsFuel fuel bound arguments
      | .valuePipelineCall input call =>
          freeExprFuel fuel bound input ++ freeExprFuel fuel bound call
      | .field base _ => freeExprFuel fuel bound base
      | .nullSafeField base _ => freeExprFuel fuel bound base
      | .index base index _ => freeExprFuel fuel bound base ++ freeExprFuel fuel bound index
      | .slice base start finish _ =>
          freeExprFuel fuel bound base ++ start.toList.flatMap (freeExprFuel fuel bound) ++
            finish.toList.flatMap (freeExprFuel fuel bound)
      | .envString _ => []
      | .envPathList => []
      | .pipeline input stages =>
          freeExprFuel fuel bound input ++ freeExprsFuel fuel bound stages
      | .structuredPipeline input stages =>
          freeExprFuel fuel bound input ++ freeExprsFuel fuel bound stages
      | .spawn command => freeExprFuel fuel bound command
      | .wait target => freeExprFuel fuel bound target
      | .builderCall call body =>
          freeExprFuel fuel bound call ++ freeStatementsFuel fuel bound body
      | .tryExpr value => freeExprFuel fuel bound value
      | .require value _ => freeExprFuel fuel bound value
      | .convert value _ => freeExprFuel fuel bound value
      | .capture body => freeStatementsFuel fuel bound body
      | .collect body => freeStatementsFuel fuel bound body
      | .loopExpr body => freeStatementsFuel fuel bound body
      | .retry _ delays pattern body =>
          freeExprsFuel fuel bound delays ++
            pattern.toList.flatMap (fun item =>
              withoutBoundNames bound (item.freeReferencesFuel fuel)) ++
            freeStatementsFuel fuel
              (pattern.toList.flatMap (fun item => item.boundNamesFuel fuel) ++ bound) body
      | .valueBlock body => freeStatementsFuel fuel bound body
      | .errorContext message body =>
          freeExprFuel fuel bound message ++ freeStatementsFuel fuel bound body
      | .contextScope _ input body =>
          freeExprFuel fuel bound input ++ freeStatementsFuel fuel bound body
      | .tempDirScope path parameter body =>
          path.toList.flatMap (freeExprFuel fuel bound) ++
            freeStatementsFuel fuel (parameter.toList ++ bound) body
      | .resourceScope bindings body =>
          let (bindingUses, bodyBound) := freeNamedBindingsFuel fuel bound bindings
          bindingUses ++ freeStatementsFuel fuel bodyBound body
      | .listComp value qualifiers =>
          freeComprehensionFuel fuel bound qualifiers [value]
      | .mapComp key value qualifiers =>
          freeComprehensionFuel fuel bound qualifiers [key, value]
      | .setComp value qualifiers =>
          freeComprehensionFuel fuel bound qualifiers [value]

  /-- Declarations extend only the source-order suffix of their block. -/
  def freeStatementsFuel : Nat → List RawName → List Statement → List RawName
    | 0, _, _ => []
    | _fuel + 1, _, [] => []
    | fuel + 1, bound, statement :: rest => match statement with
      | .binding _ target _ initializer =>
          freeExprFuel fuel bound initializer ++
          freeStatementsFuel fuel (target.namesFuel fuel ++ bound) rest
      | .assign target _ value =>
          freeAssignTargetFuel fuel bound target ++ freeExprFuel fuel bound value ++
            freeStatementsFuel fuel bound rest
      | .expression value =>
          freeExprFuel fuel bound value ++ freeStatementsFuel fuel bound rest
      | .tailName name =>
          withoutBoundNames bound [name] ++ freeStatementsFuel fuel bound rest
      | .returnValue value =>
          value.toList.flatMap (freeExprFuel fuel bound) ++ freeStatementsFuel fuel bound rest
      | .yield value => freeExprFuel fuel bound value ++ freeStatementsFuel fuel bound rest
      | .yieldDelegate value =>
          freeExprFuel fuel bound value ++ freeStatementsFuel fuel bound rest
      | .defer _ action => freeExprFuel fuel bound action ++ freeStatementsFuel fuel bound rest
      | .ifStmt branches otherwise =>
          branches.flatMap (fun (condition, body) =>
            freeExprFuel fuel bound condition ++ freeStatementsFuel fuel bound body) ++
              otherwise.toList.flatMap (freeStatementsFuel fuel bound) ++
                freeStatementsFuel fuel bound rest
      | .whileStmt condition body =>
          freeExprFuel fuel bound condition ++ freeStatementsFuel fuel bound body ++
            freeStatementsFuel fuel bound rest
      | .forStmt target iterable body =>
          freeExprFuel fuel bound iterable ++
            freeStatementsFuel fuel (target.namesFuel fuel ++ bound) body ++
              freeStatementsFuel fuel bound rest
      | .withScope bindings body otherwise =>
          let (bindingUses, bodyBound) := freeNamedBindingsFuel fuel bound bindings
          bindingUses ++ freeStatementsFuel fuel bodyBound body ++
            freeStatementsFuel fuel bound otherwise ++ freeStatementsFuel fuel bound rest
      | .loopStmt body =>
          freeStatementsFuel fuel bound body ++ freeStatementsFuel fuel bound rest
      | .guardStmt target _ initializer otherwise =>
          freeExprFuel fuel bound initializer ++ freeStatementsFuel fuel bound otherwise ++
          freeStatementsFuel fuel (target.namesFuel fuel ++ bound) rest
      | .assert condition message =>
          freeExprFuel fuel bound condition ++
            message.toList.flatMap (freeExprFuel fuel bound) ++
              freeStatementsFuel fuel bound rest
      | .breakStmt value =>
          value.toList.flatMap (freeExprFuel fuel bound) ++ freeStatementsFuel fuel bound rest
      | .continueStmt => freeStatementsFuel fuel bound rest
      | .exit status => freeExprFuel fuel bound status ++ freeStatementsFuel fuel bound rest
      | .matchStmt subject arms =>
          freeExprFuel fuel bound subject ++ arms.flatMap (fun (pattern, guard, body) =>
            let armBound := pattern.boundNamesFuel fuel ++ bound
            withoutBoundNames bound (pattern.freeReferencesFuel fuel) ++
              guard.toList.flatMap (freeExprFuel fuel armBound) ++
                freeStatementsFuel fuel armBound body) ++
            freeStatementsFuel fuel bound rest

  def freeExprsFuel : Nat → List RawName → List Expr → List RawName
    | 0, _, _ => []
    | _fuel + 1, _, [] => []
    | fuel + 1, bound, value :: rest =>
        freeExprFuel fuel bound value ++ freeExprsFuel fuel bound rest

  def freeRecordFieldsFuel : Nat → List RawName → List RecordField → List RawName
    | 0, _, _ => []
    | _fuel + 1, _, [] => []
    | fuel + 1, bound, field :: rest => match field with
      | .computed key value =>
          freeExprFuel fuel bound key ++ freeExprFuel fuel bound value ++
            freeRecordFieldsFuel fuel bound rest
      | .path _ value => freeExprFuel fuel bound value ++ freeRecordFieldsFuel fuel bound rest
      | .named _ value => freeExprFuel fuel bound value ++ freeRecordFieldsFuel fuel bound rest
      | .shorthand name =>
          withoutBoundNames bound [name] ++ freeRecordFieldsFuel fuel bound rest
      | .spread value => freeExprFuel fuel bound value ++ freeRecordFieldsFuel fuel bound rest

  def freeCallArgumentsFuel : Nat → List RawName → List CallArgument → List RawName
    | 0, _, _ => []
    | _fuel + 1, _, [] => []
    | fuel + 1, bound, argument :: rest => match argument with
      | .positional value =>
          freeExprFuel fuel bound value ++ freeCallArgumentsFuel fuel bound rest
      | .namedSpread value =>
          freeExprFuel fuel bound value ++ freeCallArgumentsFuel fuel bound rest
      | .splice value => freeExprFuel fuel bound value ++ freeCallArgumentsFuel fuel bound rest
      | .named _ value => freeExprFuel fuel bound value ++ freeCallArgumentsFuel fuel bound rest

  /-- Iterables run before their target; following qualifiers see that target. -/
  def freeComprehensionFuel : Nat → List RawName → List Qualifier → List Expr → List RawName
    | 0, _, _, _ => []
    | _fuel + 1, bound, [], results => freeExprsFuel _fuel bound results
    | fuel + 1, bound, .each target iterable :: rest, results =>
        freeExprFuel fuel bound iterable ++
          freeComprehensionFuel fuel (target.namesFuel fuel ++ bound) rest results
    | fuel + 1, bound, .when condition :: rest, results =>
        freeExprFuel fuel bound condition ++ freeComprehensionFuel fuel bound rest results

  /-- Resource initializers see only earlier named bindings in their list. -/
  def freeNamedBindingsFuel : Nat → List RawName → List (Option RawName × Expr) →
      List RawName × List RawName
    | 0, bound, _ => ([], bound)
    | _fuel + 1, bound, [] => ([], bound)
    | fuel + 1, bound, (name, initializer) :: rest =>
        let (laterUses, finalBound) := freeNamedBindingsFuel fuel (name.toList ++ bound) rest
        (freeExprFuel fuel bound initializer ++ laterUses, finalBound)

  def freeAssignTargetFuel : Nat → List RawName → AssignTarget → List RawName
    | 0, _, _ => []
    | fuel + 1, bound, target => match target with
      | .name name => withoutBoundNames bound [name]
      | .field base _ => freeAssignTargetFuel fuel bound base
      | .index base index =>
          freeAssignTargetFuel fuel bound base ++ freeExprFuel fuel bound index
      | .env _ => []
end

/-- A caller supplies a recursive-depth bound; exhausted subtrees contribute no names. -/
def freeNamesFuel (fuel : Nat) (expression : Expr) : List RawName :=
  freeExprFuel fuel [] expression

private def witnessName (value : Nat) (fits : value < 4294967296) : RawName :=
  ⟨⟨value, fits⟩⟩

private def nameX : RawName := witnessName 11 (by decide)
private def nameY : RawName := witnessName 12 (by decide)
private def nameZ : RawName := witnessName 13 (by decide)

theorem identifier_is_free (name : RawName) :
    freeNamesFuel 2 (.ident name) = [name] := by
  simp [freeNamesFuel, freeExprFuel, withoutBoundNames]

theorem binding_initializer_is_outside_its_binding :
    freeNamesFuel 16
      (.valueBlock [.binding .letBinding (.name nameX) none (.ident nameX)]) = [nameX] := by
  decide

theorem later_block_use_is_bound :
    freeNamesFuel 24 (.valueBlock [
      .binding .letBinding (.name nameX) none (.literal (.integer "1")),
      .expression (.ident nameX)
    ]) = [] := by
  decide

theorem record_field_label_is_not_a_binding_name :
    freeNamesFuel 24 (.valueBlock [
      .binding .letBinding (.record [(nameX, .name nameY)] false) none
        (.literal (.integer "1")),
      .expression (.ident nameX)
    ]) = [nameX] := by
  decide

theorem shadowing_initializer_sees_outer_binding :
    freeNamesFuel 32 (.valueBlock [
      .binding .letBinding (.name nameX) none (.ident nameY),
      .binding .letBinding (.name nameX) none (.ident nameX),
      .expression (.ident nameX)
    ]) = [nameY] := by
  decide

theorem comprehension_target_does_not_scope_over_iterable :
    freeNamesFuel 16
      (.listComp (.ident nameX) [.each (.name nameX) (.ident nameX)]) = [nameX] := by
  decide

theorem match_pattern_binding_scopes_over_guard_and_result :
    freeNamesFuel 20 (.matchExpr (.ident nameY)
      [(.binding nameX, some (.ident nameX), .ident nameX)]) = [nameY] := by
  decide

theorem guard_binding_is_visible_afterward_but_not_in_else :
    freeNamesFuel 32 (.valueBlock [
      .guardStmt (.name nameX) none (.ident nameY) [.expression (.ident nameX)],
      .expression (.ident nameX)
    ]) = [nameY, nameX] := by
  decide

theorem with_initializers_see_earlier_bindings_only :
    freeNamesFuel 32 (.valueBlock [
      .withScope [(some nameX, .ident nameY), (some nameY, .ident nameX)]
        [.expression (.ident nameY)] []
    ]) = [nameY] := by
  decide

theorem let_discard_does_not_bind :
    freeNamesFuel 16 (.valueBlock [
      .binding .letBinding .discard none (.ident nameY),
      .expression (.ident nameX)
    ]) = [nameY, nameX] := by
  decide

theorem tempdir_discard_does_not_bind :
    freeNamesFuel 16 (.tempDirScope none none [.expression (.ident nameX)]) = [nameX] := by
  decide

theorem resource_discard_does_not_bind :
    freeNamesFuel 24 (.resourceScope [(none, .ident nameY)]
      [.expression (.ident nameX)]) = [nameY, nameX] := by
  decide

theorem with_discard_does_not_bind :
    freeNamesFuel 24 (.valueBlock [
      .withScope [(none, .ident nameY)] [.expression (.ident nameX)] []
    ]) = [nameY, nameX] := by
  decide

theorem value_pipeline_hole_is_not_a_free_name_or_second_visit :
    freeNamesFuel 16 (.valuePipelineCall (.ident nameX)
      (.call (.ident nameY) [.positional .valuePipelineHole])) = [nameX, nameY] := by
  decide

theorem depth_bound_can_omit_earlier_subtree_and_retain_later_siblings :
    freeNamesFuel 3 (.binary .add
      (.unary .not (.unary .not (.ident nameX)))
      (.binary .add (.ident nameY) (.ident nameZ))) = [nameY, nameZ] := by
  decide

theorem name_and_binder_numeric_payloads_may_match :
    NameId.value nameZ = BinderId.value ⟨nameZ.word⟩ := rfl

end Xsh.Syntax
