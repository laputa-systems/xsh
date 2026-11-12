import Xsh.Core.Effects
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

inductive InvocationPolicy where
  | pure
  | procedure (effects : EffectBound)
  | producer (effects : EffectBound)
  deriving DecidableEq

def InvocationPolicy.kind : InvocationPolicy → CallableKind
  | .pure => .pure
  | .procedure _ => .procedure
  | .producer _ => .producer

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
and declaration identities. Names and variables are semantic parameters.
Recovery markers are not types. Union admissibility, row uniqueness and scheme
authentication require separate judgments; construction alone proves no typing
or Rust correspondence. -/
inductive TypeTerm (Label Declaration Variable : Type) : TypeKind → Type where
  | variable {kind : TypeKind} (id : Variable) : TypeTerm Label Declaration Variable kind
  | primitive (type : PrimitiveType) : TypeTerm Label Declaration Variable .value
  | list (element : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | map (key value : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | set (element : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | stream (element : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | record (fields : TypeTerm Label Declaration Variable .row) : TypeTerm Label Declaration Variable .value
  | module (closure : ExportClosure) (exports : TypeTerm Label Declaration Variable .exports) :
      TypeTerm Label Declaration Variable .value
  | result (value error : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | optional (value : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | union (first second : TypeTerm Label Declaration Variable .value)
      (rest : TypeTerm Label Declaration Variable .members) : TypeTerm Label Declaration Variable .value
  | functionClass (kind : CallableKind) : TypeTerm Label Declaration Variable .value
  | callable (signature : TypeTerm Label Declaration Variable .signature) : TypeTerm Label Declaration Variable .value
  | schemeCallable (declaration : Declaration) : TypeTerm Label Declaration Variable .value
  | tagged (declaration : Declaration) : TypeTerm Label Declaration Variable .value
  | errorFamily (declaration : Declaration) : TypeTerm Label Declaration Variable .value
  | errorVariant (family : Declaration) (variant : Label) : TypeTerm Label Declaration Variable .value
  | errorFacet (facet : Label) : TypeTerm Label Declaration Variable .value
  | nonEmpty (element : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .value
  | relativePath : TypeTerm Label Declaration Variable .value
  | nominal (declaration : Declaration) (fields : TypeTerm Label Declaration Variable .row) :
      TypeTerm Label Declaration Variable .value
  | boundedInteger (bounds : Xsh.Validation.BoundedSpec) : TypeTerm Label Declaration Variable .value
  | rowEmpty : TypeTerm Label Declaration Variable .row
  | rowField (name : Label) (value : TypeTerm Label Declaration Variable .value)
      (rest : TypeTerm Label Declaration Variable .row) : TypeTerm Label Declaration Variable .row
  | argumentsEmpty : TypeTerm Label Declaration Variable .arguments
  | parameter (requirement : ParameterRequirement) (name : Label)
      (type : TypeTerm Label Declaration Variable .value)
      (rest : TypeTerm Label Declaration Variable .arguments) : TypeTerm Label Declaration Variable .arguments
  | restParameter (name : Label) (element : TypeTerm Label Declaration Variable .value) :
      TypeTerm Label Declaration Variable .arguments
  | signature (policy : InvocationPolicy) (parameters : TypeTerm Label Declaration Variable .arguments)
      (result : TypeTerm Label Declaration Variable .value) : TypeTerm Label Declaration Variable .signature
  | membersEmpty : TypeTerm Label Declaration Variable .members
  | member (value : TypeTerm Label Declaration Variable .value)
      (rest : TypeTerm Label Declaration Variable .members) : TypeTerm Label Declaration Variable .members
  | exportsEmpty : TypeTerm Label Declaration Variable .exports
  | exportValue (name : Label) (presence : ExportPresence) (value : TypeTerm Label Declaration Variable .value)
      (rest : TypeTerm Label Declaration Variable .exports) : TypeTerm Label Declaration Variable .exports
  | exportCallable (name : Label) (presence : ExportPresence)
      (signature : TypeTerm Label Declaration Variable .signature)
      (rest : TypeTerm Label Declaration Variable .exports) : TypeTerm Label Declaration Variable .exports
  | exportGenericCallable (name : Label) (presence : ExportPresence) (kind : CallableKind)
      (rest : TypeTerm Label Declaration Variable .exports) :
      TypeTerm Label Declaration Variable .exports

  deriving DecidableEq

abbrev VariableFreeTypeTerm (Label Declaration : Type) (kind : TypeKind) :=
  TypeTerm Label Declaration Empty kind

abbrev TypeSubstitution (Label Declaration Source Target : Type) :=
  {kind : TypeKind} → Source → TypeTerm Label Declaration Target kind

namespace TypeTerm

variable {Label Declaration Source Middle Target : Type}

def substitute (replacement : TypeSubstitution Label Declaration Source Target) :
    {kind : TypeKind} → TypeTerm Label Declaration Source kind → TypeTerm Label Declaration Target kind
  | _, .variable id => replacement id
  | _, .primitive type => .primitive type
  | _, .list element => .list (substitute replacement element)
  | _, .map key value => .map (substitute replacement key) (substitute replacement value)
  | _, .set element => .set (substitute replacement element)
  | _, .stream element => .stream (substitute replacement element)
  | _, .record fields => .record (substitute replacement fields)
  | _, .module closure exports => .module closure (substitute replacement exports)
  | _, .result value error => .result (substitute replacement value) (substitute replacement error)
  | _, .optional value => .optional (substitute replacement value)
  | _, .union first second rest =>
      .union (substitute replacement first) (substitute replacement second) (substitute replacement rest)
  | _, .functionClass kind => .functionClass kind
  | _, .callable contract => .callable (substitute replacement contract)
  | _, .schemeCallable declaration => .schemeCallable declaration
  | _, .tagged declaration => .tagged declaration
  | _, .errorFamily declaration => .errorFamily declaration
  | _, .errorVariant family variant => .errorVariant family variant
  | _, .errorFacet facet => .errorFacet facet
  | _, .nonEmpty element => .nonEmpty (substitute replacement element)
  | _, .relativePath => .relativePath
  | _, .nominal declaration fields => .nominal declaration (substitute replacement fields)
  | _, .boundedInteger bounds => .boundedInteger bounds
  | _, .rowEmpty => .rowEmpty
  | _, .rowField name value rest => .rowField name (substitute replacement value) (substitute replacement rest)
  | _, .argumentsEmpty => .argumentsEmpty
  | _, .parameter requirement name type rest =>
      .parameter requirement name (substitute replacement type) (substitute replacement rest)
  | _, .restParameter name element => .restParameter name (substitute replacement element)
  | _, .signature policy parameters output =>
      .signature policy (substitute replacement parameters) (substitute replacement output)
  | _, .membersEmpty => .membersEmpty
  | _, .member value rest => .member (substitute replacement value) (substitute replacement rest)
  | _, .exportsEmpty => .exportsEmpty
  | _, .exportValue name presence value rest =>
      .exportValue name presence (substitute replacement value) (substitute replacement rest)
  | _, .exportCallable name presence contract rest =>
      .exportCallable name presence (substitute replacement contract) (substitute replacement rest)
  | _, .exportGenericCallable name presence kind rest =>
      .exportGenericCallable name presence kind (substitute replacement rest)

def identity : TypeSubstitution Label Declaration Source Source := fun id => .variable id

def compose (outer : TypeSubstitution Label Declaration Middle Target)
    (inner : TypeSubstitution Label Declaration Source Middle) : TypeSubstitution Label Declaration Source Target :=
  fun id => substitute outer (inner id)

theorem substitute_identity {kind : TypeKind} (type : TypeTerm Label Declaration Source kind) :
    substitute identity type = type := by
  induction type <;> simp_all [substitute, identity]

theorem substitute_compose (outer : TypeSubstitution Label Declaration Middle Target)
    (inner : TypeSubstitution Label Declaration Source Middle) {kind : TypeKind}
    (type : TypeTerm Label Declaration Source kind) :
    substitute outer (substitute inner type) = substitute (compose outer inner) type := by
  induction type <;> simp_all [substitute, compose]

theorem substitute_congr (first second : TypeSubstitution Label Declaration Source Target)
    (same : ∀ {kind : TypeKind} (id : Source), first (kind := kind) id = second id)
    {kind : TypeKind} (type : TypeTerm Label Declaration Source kind) :
    substitute first type = substitute second type := by
  induction type <;> simp_all [substitute]

theorem variable_free_substitution_irrelevant
    (first second : TypeSubstitution Label Declaration Empty Target)
    {kind : TypeKind} (type : VariableFreeTypeTerm Label Declaration kind) :
    substitute first type = substitute second type := by
  exact substitute_congr first second (fun id => Empty.elim id) type

end TypeTerm

end Xsh.Core
