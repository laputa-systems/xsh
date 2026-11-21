import Xsh.Core.Types

namespace Xsh.Core

variable {Declaration : Type}

abbrev ViewType (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .value
abbrev ViewRow (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .row
abbrev ViewMembers (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .members
abbrev ViewSignature (Declaration : Type) := TypeVariableFreeTerm 0 String Declaration .signature

inductive ValidationTag (Declaration : Type) where
  | nonEmpty
  | relativePath
  | nominal (declaration : Declaration)
  | bounded (bounds : Xsh.Validation.BoundedSpec)
  deriving DecidableEq

def validationTag : ViewType Declaration → Option (ValidationTag Declaration)
  | .nonEmpty _ => some .nonEmpty
  | .relativePath => some .relativePath
  | .nominal declaration _ => some (.nominal declaration)
  | .boundedInteger bounds => some (.bounded bounds)
  | _ => none

def IntegerBaseType : Xsh.Validation.IntegerBase → ViewType Declaration
  | .int => .primitive .integer
  | .uint => .primitive .unsignedInteger

def base : ViewType Declaration → Option (ViewType Declaration)
  | .nonEmpty element => some (.list element)
  | .relativePath => some (.primitive .path)
  | .nominal _ fields => some (.record fields)
  | .boundedInteger bounds => some (IntegerBaseType bounds.base)
  | _ => none

def rowEntries : ViewRow Declaration → List (String × ViewType Declaration)
  | .rowField name type rest => (name, type) :: rowEntries rest
  | _ => []

def tailMembers : ViewMembers Declaration → List (ViewType Declaration)
  | .member type rest => type :: tailMembers rest
  | _ => []

def unionMembers : ViewType Declaration → Option (List (ViewType Declaration))
  | .union first second rest => some (first :: second :: tailMembers rest)
  | _ => none

mutual
  inductive BaseView : ViewType Declaration → ViewType Declaration → Prop where
    | refl (type : ViewType Declaration) : BaseView type type
    | validated {validatedType baseType target : ViewType Declaration} :
        base validatedType = some baseType → BaseView baseType target → BaseView validatedType target
    | nonEmpty {source target : ViewType Declaration} :
        BaseView source target → BaseView (.nonEmpty source) (.nonEmpty target)
    | list {source target : ViewType Declaration} : BaseView source target → BaseView (.list source) (.list target)
    | map (key : ViewType Declaration) {source target : ViewType Declaration} :
        BaseView source target → BaseView (.map key source) (.map key target)
    | stream {source target : ViewType Declaration} :
        BaseView source target → BaseView (.stream source) (.stream target)
    | record {source target : ViewRow Declaration} :
        EntriesCovered (rowEntries target) (rowEntries source) →
        (∀ name ∈ (rowEntries source).map Prod.fst, name ∈ (rowEntries target).map Prod.fst) →
          BaseView (.record source) (.record target)
    | optional {source target : ViewType Declaration} :
        BaseView source target → BaseView (.optional source) (.optional target)
    | result {sourceValue targetValue sourceError targetError : ViewType Declaration} :
        BaseView sourceValue targetValue → BaseView sourceError targetError →
          BaseView (.result sourceValue sourceError) (.result targetValue targetError)
    | union {sourceFirst sourceSecond targetFirst targetSecond : ViewType Declaration}
        {sourceRest targetRest : ViewMembers Declaration} :
        MembersCovered (sourceFirst :: sourceSecond :: tailMembers sourceRest)
          (targetFirst :: targetSecond :: tailMembers targetRest) →
          BaseView (.union sourceFirst sourceSecond sourceRest) (.union targetFirst targetSecond targetRest)
    | callable {source target : ViewSignature Declaration} :
        SignatureView source target → BaseView (.callable source) (.callable target)

  inductive EntryCovered : String × ViewType Declaration → List (String × ViewType Declaration) → Prop where
    | here (name : String) {source target : ViewType Declaration} (rest : List (String × ViewType Declaration)) :
        BaseView source target → EntryCovered (name, target) ((name, source) :: rest)
    | there {entry : String × ViewType Declaration} (other : String × ViewType Declaration)
        {rest : List (String × ViewType Declaration)} :
        EntryCovered entry rest → EntryCovered entry (other :: rest)

  inductive EntriesCovered : List (String × ViewType Declaration) → List (String × ViewType Declaration) → Prop where
    | nil (source : List (String × ViewType Declaration)) : EntriesCovered [] source
    | cons {entry : String × ViewType Declaration} {rest source : List (String × ViewType Declaration)} :
        EntryCovered entry source → EntriesCovered rest source → EntriesCovered (entry :: rest) source

  inductive MemberCovered : ViewType Declaration → List (ViewType Declaration) → Prop where
    | here {source target : ViewType Declaration} (rest : List (ViewType Declaration)) :
        BaseView source target → MemberCovered source (target :: rest)
    | there {source : ViewType Declaration} (other : ViewType Declaration) {rest : List (ViewType Declaration)} :
        MemberCovered source rest → MemberCovered source (other :: rest)

  inductive MembersCovered : List (ViewType Declaration) → List (ViewType Declaration) → Prop where
    | nil (target : List (ViewType Declaration)) : MembersCovered [] target
    | cons {member : ViewType Declaration} {rest target : List (ViewType Declaration)} :
        MemberCovered member target → MembersCovered rest target → MembersCovered (member :: rest) target

  inductive SignatureView : ViewSignature Declaration → ViewSignature Declaration → Prop where
    | result (policy : InvocationPolicy 0) (parameters : TypeVariableFreeTerm 0 String Declaration .arguments)
        {source target : ViewType Declaration} :
        BaseView source target →
          SignatureView (.signature policy parameters source) (.signature policy parameters target)
end

inductive ViewChain : ViewType Declaration → ViewType Declaration → Prop where
  | refl (type : ViewType Declaration) : ViewChain type type
  | step {first middle last : ViewType Declaration} :
      ViewChain first middle → BaseView middle last → ViewChain first last

inductive SignatureChain : ViewSignature Declaration → ViewSignature Declaration → Prop where
  | refl (signature : ViewSignature Declaration) : SignatureChain signature signature
  | step {first middle last : ViewSignature Declaration} :
      SignatureChain first middle → SignatureView middle last → SignatureChain first last

theorem base_unvalidated_primitive (primitive : PrimitiveType) :
    base (.primitive primitive : ViewType Declaration) = none := rfl

theorem view_from_primitive {primitive : PrimitiveType} {target : ViewType Declaration}
    (view : BaseView (.primitive primitive) target) : target = .primitive primitive := by
  cases view with
  | refl => rfl
  | validated hasBase _ => cases hasBase

theorem chain_from_primitive {primitive : PrimitiveType} {target : ViewType Declaration}
    (chain : ViewChain (.primitive primitive) target) : target = .primitive primitive := by
  induction chain with
  | refl => rfl
  | step _ view ih => subst ih; exact view_from_primitive view

theorem signature_chain_from_primitive_result {policy : InvocationPolicy 0}
    {parameters : TypeVariableFreeTerm 0 String Declaration .arguments} {primitive : PrimitiveType}
    {target : ViewSignature Declaration}
    (chain : SignatureChain (.signature policy parameters (.primitive primitive)) target) :
    target = .signature policy parameters (.primitive primitive) := by
  induction chain with
  | refl => rfl
  | step _ view ih =>
    subst ih
    cases view with
    | result _ _ resultView => rw [view_from_primitive resultView]

theorem chain_primitive_iff {primitive : PrimitiveType} {target : ViewType Declaration} :
    ViewChain (.primitive primitive) target ↔ target = .primitive primitive :=
  ⟨chain_from_primitive, fun same => same ▸ .refl _⟩

theorem signature_chain_primitive_iff {policy : InvocationPolicy 0}
    {parameters : TypeVariableFreeTerm 0 String Declaration .arguments} {primitive : PrimitiveType}
    {target : ViewSignature Declaration} :
    SignatureChain (.signature policy parameters (.primitive primitive)) target ↔
      target = .signature policy parameters (.primitive primitive) :=
  ⟨signature_chain_from_primitive_result, fun same => same ▸ .refl _⟩

end Xsh.Core
