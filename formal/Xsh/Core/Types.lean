import Xsh.Core.SymbolicEffects
import Xsh.Validation.IntegerBounds

namespace Xsh.Core

inductive TypeKind where
  | value
  | row
  | arguments
  | exports
  | members
  | signature
  deriving DecidableEq

inductive PrimitiveType where
  | any
  | null
  | boolean
  | integer
  | unsignedInteger
  | float
  | duration
  | string
  | bytes
  | digest
  | regex
  | path
  | erasedRecord
  | dynamicModule
  | status
  | environmentPaths
  | error
  | processError
  | command
  | processHandle
  | networkJob
  | filesystemRoot
  | filesystemLock
  | sshSession
  | unit
  deriving DecidableEq

inductive CallableKind where
  | pure
  | procedure
  | producer
  deriving DecidableEq

inductive InvocationPolicy (effectParameters : Nat) where
  | pure
  | procedure (effects : EffectBound effectParameters)
  | producer (effects : EffectBound effectParameters)
  deriving DecidableEq

def InvocationPolicy.kind : InvocationPolicy effectParameters → CallableKind
  | .pure => .pure
  | .procedure _ => .procedure
  | .producer _ => .producer

def InvocationPolicy.substituteEffects (replacement : Fin sourceEffects → SymbolicContract targetEffects) :
    InvocationPolicy sourceEffects → InvocationPolicy targetEffects
  | .pure => .pure
  | .procedure effects => .procedure (effects.substitute replacement)
  | .producer effects => .producer (effects.substitute replacement)

theorem InvocationPolicy.substituteEffects_kind
    (replacement : Fin sourceEffects → SymbolicContract targetEffects)
    (policy : InvocationPolicy sourceEffects) :
    (policy.substituteEffects replacement).kind = policy.kind := by
  cases policy <;> rfl

inductive ParameterRequirement where
  | required
  | defaulted
  deriving DecidableEq

inductive ExportPresence where
  | required
  | optional
  deriving DecidableEq

inductive ExportClosure where
  | open
  | exact
  deriving DecidableEq

/-- Kinded type terms retain callable labels, defaults, effects, module exports,
and declaration identities. The effect scope is separate from type variables.
Names and variables are semantic parameters.
Recovery markers are not types. Union admissibility, row uniqueness and scheme
authentication require separate judgments; construction alone proves no typing
or Rust correspondence. -/
inductive TypeTerm (effectParameters : Nat) (Label Declaration Variable : Type) : TypeKind → Type where
  | variable {kind : TypeKind} (id : Variable) : TypeTerm effectParameters Label Declaration Variable kind
  | primitive (type : PrimitiveType) : TypeTerm effectParameters Label Declaration Variable .value
  | list (element : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | map (key value : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | set (element : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | stream (element : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | record (fields : TypeTerm effectParameters Label Declaration Variable .row) : TypeTerm effectParameters Label Declaration Variable .value
  | module (closure : ExportClosure) (exports : TypeTerm effectParameters Label Declaration Variable .exports) :
      TypeTerm effectParameters Label Declaration Variable .value
  | result (value error : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | optional (value : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | union (first second : TypeTerm effectParameters Label Declaration Variable .value)
      (rest : TypeTerm effectParameters Label Declaration Variable .members) : TypeTerm effectParameters Label Declaration Variable .value
  | functionClass (kind : CallableKind) : TypeTerm effectParameters Label Declaration Variable .value
  | callable (signature : TypeTerm effectParameters Label Declaration Variable .signature) : TypeTerm effectParameters Label Declaration Variable .value
  | schemeCallable (declaration : Declaration) : TypeTerm effectParameters Label Declaration Variable .value
  | tagged (declaration : Declaration) : TypeTerm effectParameters Label Declaration Variable .value
  | errorFamily (declaration : Declaration) : TypeTerm effectParameters Label Declaration Variable .value
  | errorVariant (family : Declaration) (variant : Label) : TypeTerm effectParameters Label Declaration Variable .value
  | errorFacet (facet : Label) : TypeTerm effectParameters Label Declaration Variable .value
  | nonEmpty (element : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .value
  | relativePath : TypeTerm effectParameters Label Declaration Variable .value
  | nominal (declaration : Declaration) (fields : TypeTerm effectParameters Label Declaration Variable .row) :
      TypeTerm effectParameters Label Declaration Variable .value
  | boundedInteger (bounds : Xsh.Validation.BoundedSpec) : TypeTerm effectParameters Label Declaration Variable .value
  | rowEmpty : TypeTerm effectParameters Label Declaration Variable .row
  | rowField (name : Label) (value : TypeTerm effectParameters Label Declaration Variable .value)
      (rest : TypeTerm effectParameters Label Declaration Variable .row) : TypeTerm effectParameters Label Declaration Variable .row
  | argumentsEmpty : TypeTerm effectParameters Label Declaration Variable .arguments
  | parameter (requirement : ParameterRequirement) (name : Label)
      (type : TypeTerm effectParameters Label Declaration Variable .value)
      (rest : TypeTerm effectParameters Label Declaration Variable .arguments) : TypeTerm effectParameters Label Declaration Variable .arguments
  | restParameter (name : Label) (element : TypeTerm effectParameters Label Declaration Variable .value) :
      TypeTerm effectParameters Label Declaration Variable .arguments
  | signature (policy : InvocationPolicy effectParameters) (parameters : TypeTerm effectParameters Label Declaration Variable .arguments)
      (result : TypeTerm effectParameters Label Declaration Variable .value) : TypeTerm effectParameters Label Declaration Variable .signature
  | membersEmpty : TypeTerm effectParameters Label Declaration Variable .members
  | member (value : TypeTerm effectParameters Label Declaration Variable .value)
      (rest : TypeTerm effectParameters Label Declaration Variable .members) : TypeTerm effectParameters Label Declaration Variable .members
  | exportsEmpty : TypeTerm effectParameters Label Declaration Variable .exports
  | exportValue (name : Label) (presence : ExportPresence) (value : TypeTerm effectParameters Label Declaration Variable .value)
      (rest : TypeTerm effectParameters Label Declaration Variable .exports) : TypeTerm effectParameters Label Declaration Variable .exports
  | exportCallable (name : Label) (presence : ExportPresence)
      (signature : TypeTerm effectParameters Label Declaration Variable .signature)
      (rest : TypeTerm effectParameters Label Declaration Variable .exports) : TypeTerm effectParameters Label Declaration Variable .exports
  | exportGenericCallable (name : Label) (presence : ExportPresence) (kind : CallableKind)
      (rest : TypeTerm effectParameters Label Declaration Variable .exports) :
      TypeTerm effectParameters Label Declaration Variable .exports

  deriving DecidableEq

abbrev TypeVariableFreeTerm (effectParameters : Nat) (Label Declaration : Type) (kind : TypeKind) :=
  TypeTerm effectParameters Label Declaration Empty kind

abbrev TypeSubstitution (effectParameters : Nat) (Label Declaration Source Target : Type) :=
  {kind : TypeKind} → Source → TypeTerm effectParameters Label Declaration Target kind

namespace TypeTerm

variable {Label Declaration Source Middle Target : Type} {effectParameters : Nat}

private def transform
    (replacement : TypeSubstitution targetEffects Label Declaration Source Target)
    (mapPolicy : InvocationPolicy sourceEffects → InvocationPolicy targetEffects) :
    {kind : TypeKind} → TypeTerm sourceEffects Label Declaration Source kind →
      TypeTerm targetEffects Label Declaration Target kind
  | _, .variable id => replacement id
  | _, .primitive type => .primitive type
  | _, .list element => .list (transform replacement mapPolicy element)
  | _, .map key value => .map (transform replacement mapPolicy key) (transform replacement mapPolicy value)
  | _, .set element => .set (transform replacement mapPolicy element)
  | _, .stream element => .stream (transform replacement mapPolicy element)
  | _, .record fields => .record (transform replacement mapPolicy fields)
  | _, .module closure exports => .module closure (transform replacement mapPolicy exports)
  | _, .result value error => .result (transform replacement mapPolicy value) (transform replacement mapPolicy error)
  | _, .optional value => .optional (transform replacement mapPolicy value)
  | _, .union first second rest =>
      .union (transform replacement mapPolicy first) (transform replacement mapPolicy second) (transform replacement mapPolicy rest)
  | _, .functionClass kind => .functionClass kind
  | _, .callable contract => .callable (transform replacement mapPolicy contract)
  | _, .schemeCallable declaration => .schemeCallable declaration
  | _, .tagged declaration => .tagged declaration
  | _, .errorFamily declaration => .errorFamily declaration
  | _, .errorVariant family variant => .errorVariant family variant
  | _, .errorFacet facet => .errorFacet facet
  | _, .nonEmpty element => .nonEmpty (transform replacement mapPolicy element)
  | _, .relativePath => .relativePath
  | _, .nominal declaration fields => .nominal declaration (transform replacement mapPolicy fields)
  | _, .boundedInteger bounds => .boundedInteger bounds
  | _, .rowEmpty => .rowEmpty
  | _, .rowField name value rest => .rowField name (transform replacement mapPolicy value) (transform replacement mapPolicy rest)
  | _, .argumentsEmpty => .argumentsEmpty
  | _, .parameter requirement name type rest =>
      .parameter requirement name (transform replacement mapPolicy type) (transform replacement mapPolicy rest)
  | _, .restParameter name element => .restParameter name (transform replacement mapPolicy element)
  | _, .signature policy parameters output =>
      .signature (mapPolicy policy) (transform replacement mapPolicy parameters) (transform replacement mapPolicy output)
  | _, .membersEmpty => .membersEmpty
  | _, .member value rest => .member (transform replacement mapPolicy value) (transform replacement mapPolicy rest)
  | _, .exportsEmpty => .exportsEmpty
  | _, .exportValue name presence value rest =>
      .exportValue name presence (transform replacement mapPolicy value) (transform replacement mapPolicy rest)
  | _, .exportCallable name presence contract rest =>
      .exportCallable name presence (transform replacement mapPolicy contract) (transform replacement mapPolicy rest)
  | _, .exportGenericCallable name presence kind rest =>
      .exportGenericCallable name presence kind (transform replacement mapPolicy rest)

def substitute (replacement : TypeSubstitution effectParameters Label Declaration Source Target)
    {kind : TypeKind} (type : TypeTerm effectParameters Label Declaration Source kind) :
    TypeTerm effectParameters Label Declaration Target kind :=
  transform replacement id type

def substituteEffects (replacement : Fin sourceEffects → SymbolicContract targetEffects)
    {kind : TypeKind} (type : TypeTerm sourceEffects Label Declaration Source kind) :
    TypeTerm targetEffects Label Declaration Source kind :=
  transform (fun id => .variable id) (InvocationPolicy.substituteEffects replacement) type

def identity : TypeSubstitution effectParameters Label Declaration Source Source := fun id => .variable id

def compose (outer : TypeSubstitution effectParameters Label Declaration Middle Target)
    (inner : TypeSubstitution effectParameters Label Declaration Source Middle) : TypeSubstitution effectParameters Label Declaration Source Target :=
  fun id => substitute outer (inner id)

theorem substitute_identity {kind : TypeKind} (type : TypeTerm effectParameters Label Declaration Source kind) :
    substitute identity type = type := by
  induction type <;> simp_all [substitute, transform, identity]

theorem substitute_compose (outer : TypeSubstitution effectParameters Label Declaration Middle Target)
    (inner : TypeSubstitution effectParameters Label Declaration Source Middle) {kind : TypeKind}
    (type : TypeTerm effectParameters Label Declaration Source kind) :
    substitute outer (substitute inner type) = substitute (compose outer inner) type := by
  induction type <;> simp_all [substitute, transform, compose]

theorem substitute_congr (first second : TypeSubstitution effectParameters Label Declaration Source Target)
    (same : ∀ {kind : TypeKind} (id : Source), first (kind := kind) id = second id)
    {kind : TypeKind} (type : TypeTerm effectParameters Label Declaration Source kind) :
    substitute first type = substitute second type := by
  induction type <;> simp_all [substitute, transform]

theorem type_variable_free_substitution_irrelevant
    (first second : TypeSubstitution effectParameters Label Declaration Empty Target)
    {kind : TypeKind} (type : TypeVariableFreeTerm effectParameters Label Declaration kind) :
    substitute first type = substitute second type := by
  exact substitute_congr first second (fun id => Empty.elim id) type

theorem effect_type_substitution_commutes
    (effects : Fin sourceEffects → SymbolicContract targetEffects)
    (types : TypeSubstitution sourceEffects Label Declaration Source Target)
    {kind : TypeKind} (type : TypeTerm sourceEffects Label Declaration Source kind) :
    substituteEffects effects (substitute types type) =
      substitute (fun id => substituteEffects effects (types id)) (substituteEffects effects type) := by
  induction type <;> simp_all [substitute, substituteEffects, transform]

theorem effect_type_substitution_controls :
    let source : TypeTerm 2 String Nat Nat .signature :=
      .signature (.procedure (.known ((EffectExpression.parameter 0).masked
        (EffectSet.full.without (EffectSet.ofList [.error])))))
        (.parameter .defaulted "callback"
          (.callable (.signature (.producer (.known (EffectExpression.parameter 1)))
            .argumentsEmpty (.variable 0))) .argumentsEmpty)
        (.list (.variable 1))
    let values : TypeSubstitution 2 String Nat Nat Empty := fun {kind} id =>
      match kind with
      | .value => if id = 0 then .primitive .unsignedInteger else .primitive .string
      | .row => .rowEmpty
      | .arguments => .argumentsEmpty
      | .exports => .exportsEmpty
      | .members => .membersEmpty
      | .signature => .signature .pure .argumentsEmpty (.primitive .integer)
    let effects : Fin 2 → SymbolicContract 0 := fun parameter =>
      .finite (EffectSet.ofList (if parameter = 0 then [.fs, .error] else [.net]))
    let expected : TypeVariableFreeTerm 0 String Nat .signature :=
      .signature (.procedure (.known (.finite (EffectSet.ofList [.fs]))))
        (.parameter .defaulted "callback"
          (.callable (.signature (.producer (.known (.finite (EffectSet.ofList [.net]))))
            .argumentsEmpty (.primitive .unsignedInteger))) .argumentsEmpty)
        (.list (.primitive .string))
    let unknown : TypeVariableFreeTerm 0 String Nat .signature :=
      .signature (.procedure .unknown)
        (.parameter .defaulted "callback"
          (.callable (.signature (.producer .unknown)
            .argumentsEmpty (.primitive .unsignedInteger))) .argumentsEmpty)
        (.list (.primitive .string))
    substituteEffects effects (substitute values source) = expected ∧
      substituteEffects (fun _ => SymbolicContract.unknown) (substitute values source) = unknown := by
  decide +kernel

end TypeTerm

end Xsh.Core
