import Xsh.Core.Membership

namespace Xsh.Core

/-- A caller supplies decisions only for the retained reference contracts.
The structural membership decision does not inspect or execute those objects. -/
structure ContractDecisions {Declaration Identity Source : Type}
    (world : MembershipWorld Declaration Identity Source) where
  stream : ∀ reference type, Decidable (world.stream reference type)
  callable : ∀ reference type, Decidable (world.callable reference type)
  scheme : ∀ reference declaration, Decidable (world.scheme reference declaration)
  generic : ∀ reference kind, Decidable (world.generic reference kind)

namespace Membership

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

def decidePrimitive (type : PrimitiveType) (candidate : Value Declaration Identity Source) :
    Decidable (primitive type candidate) := by
  unfold primitive
  split <;> infer_instance

def decideCallable (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : SignatureType Declaration) (candidate : Value Declaration Identity Source) :
    Decidable (callable world type candidate) := by
  unfold callable
  split
  · rename_i reference actual _
    letI := contracts.callable reference type
    infer_instance
  · infer_instance

def decideGeneric (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (kind : CallableKind) (candidate : Value Declaration Identity Source) :
    Decidable (generic world kind candidate) := by
  unfold generic
  split
  · rename_i reference actual _
    letI := contracts.generic reference kind
    infer_instance
  · infer_instance

def decideKey (type : ValueType Declaration) (candidate : MapKey) : Decidable (key type candidate) := by
  unfold key
  split <;> (try split) <;> first | exact decidePrimitive _ _ | infer_instance

def Decision (world : MembershipWorld Declaration Identity Source) {kind : TypeKind}
    (type : VariableFreeTypeTerm String Declaration kind) : Type :=
  match kind, type with
  | .value, type => ∀ candidate : Value Declaration Identity Source, Decidable (value world type candidate)
  | .row, type => ∀ fields : List (String × Value Declaration Identity Source), Decidable (row world type fields)
  | .exports, type => ∀ fields : List (String × Value Declaration Identity Source), Decidable (exports world type fields)
  | .members, type => ∀ candidate : Value Declaration Identity Source, Decidable (members world type candidate)
  | .arguments, _ | .signature, _ => Unit

def decideTerm (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    {kind : TypeKind} (type : VariableFreeTypeTerm String Declaration kind) : Decision world type :=
  match type with
    | .variable id => nomatch id
    | .primitive type => fun candidate => by
        simpa only [value] using decidePrimitive type candidate
    | .list element => fun candidate => by
        letI : DecidablePred (value world element) := fun item => decideTerm world contracts element item
        unfold value
        split <;> infer_instance
    | .map keyType element => fun candidate => by
        letI : DecidablePred (value world element) := fun item => decideTerm world contracts element item
        letI : DecidablePred (key keyType) := decideKey keyType
        unfold value
        split <;> infer_instance
    | .set element => fun candidate => by
        letI : DecidablePred (key element) := decideKey element
        unfold value
        split <;> infer_instance
    | .stream element => fun candidate => by
        unfold value
        split
        · exact contracts.stream _ _
        · infer_instance
    | .record fields => fun candidate => by
        letI : DecidablePred (row world fields) := fun actual => decideTerm world contracts fields actual
        unfold value
        split <;> infer_instance
    | .module closure required => fun candidate => by
        letI : DecidablePred (exports world required) := fun actual => decideTerm world contracts required actual
        unfold value
        split <;> infer_instance
    | .result success failure => fun candidate => by
        letI : DecidablePred (value world success) := fun item => decideTerm world contracts success item
        letI : DecidablePred (value world failure) := fun item => decideTerm world contracts failure item
        unfold value
        split <;> infer_instance
    | .optional element => fun candidate => by
        letI := decidePrimitive (Declaration := Declaration) (Identity := Identity) (Source := Source) .null candidate
        letI := decideTerm world contracts element candidate
        unfold value
        infer_instance
    | .union first second rest => fun candidate => by
        letI := decideTerm world contracts first candidate
        letI := decideTerm world contracts second candidate
        letI := decideTerm world contracts rest candidate
        unfold value
        infer_instance
    | .functionClass kind => fun candidate => by
        unfold value
        split <;> infer_instance
    | .callable signature => fun candidate => by
        simpa only [value] using decideCallable world contracts signature candidate
    | .schemeCallable declaration => fun candidate => by
        unfold value
        split
        · exact contracts.scheme _ _
        · infer_instance
    | .tagged declaration => fun candidate => by
        unfold value
        split <;> infer_instance
    | .errorFamily declaration => fun candidate => by
        unfold value
        split <;> infer_instance
    | .errorVariant family variant => fun candidate => by
        unfold value
        split <;> infer_instance
    | .errorFacet facet => fun candidate => by
        unfold value
        split <;> infer_instance
    | .nonEmpty element => fun candidate => by
        letI : DecidablePred (value world element) := fun item => decideTerm world contracts element item
        unfold value
        split <;> infer_instance
    | .relativePath => fun candidate => by
        unfold value
        split <;> infer_instance
    | .nominal declaration fields => fun candidate => by
        letI : DecidablePred (row world fields) := fun actual => decideTerm world contracts fields actual
        unfold value
        split <;> (try split) <;> infer_instance
    | .boundedInteger bounds => fun candidate => by
        unfold value
        split <;> infer_instance
    | .rowEmpty => fun _ => by simp only [row]; infer_instance
    | .rowField name type rest => fun fields => by
        letI : DecidablePred (value world type) := fun item => decideTerm world contracts type item
        letI := decideTerm world contracts rest fields
        unfold row
        split <;> infer_instance
    | .exportsEmpty => fun _ => by simp only [exports]; infer_instance
    | .exportValue name presence type rest => fun fields => by
        letI : DecidablePred (value world type) := fun item => decideTerm world contracts type item
        letI := decideTerm world contracts rest fields
        unfold exports
        split
        · infer_instance
        · cases presence <;> simp only [absentAllowed] <;> infer_instance
    | .exportCallable name presence signature rest => fun fields => by
        letI : DecidablePred (callable world signature) := decideCallable world contracts signature
        letI := decideTerm world contracts rest fields
        unfold exports
        split
        · infer_instance
        · cases presence <;> simp only [absentAllowed] <;> infer_instance
    | .exportGenericCallable name presence kind rest => fun fields => by
        letI : DecidablePred (generic world kind) := decideGeneric world contracts kind
        letI := decideTerm world contracts rest fields
        unfold exports
        split
        · infer_instance
        · cases presence <;> simp only [absentAllowed] <;> infer_instance
    | .membersEmpty => fun _ => by simp only [members]; infer_instance
    | .member type rest => fun candidate => by
        letI := decideTerm world contracts type candidate
        letI := decideTerm world contracts rest candidate
        unfold members
        infer_instance
    | .argumentsEmpty | .parameter _ _ _ _ | .restParameter _ _ | .signature _ _ _ => ()
  termination_by structural type

def check (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Bool :=
  @decide (value world type candidate) (decideTerm world contracts type candidate)

theorem check_iff (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    check world contracts type candidate = true ↔ value world type candidate := by
  letI := decideTerm world contracts type candidate
  exact decide_eq_true_iff

end Membership

end Xsh.Core
