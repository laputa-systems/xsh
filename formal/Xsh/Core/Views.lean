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

def eraseValidations : {kind : TypeKind} → TypeVariableFreeTerm 0 String Declaration kind →
    TypeVariableFreeTerm 0 String Declaration kind
  | _, .variable id => nomatch id
  | _, .primitive p => .primitive p
  | _, .list item => .list (eraseValidations item)
  | _, .map key value => .map (eraseValidations key) (eraseValidations value)
  | _, .set item => .set (eraseValidations item)
  | _, .stream item => .stream (eraseValidations item)
  | _, .record fields => .record (eraseValidations fields)
  | _, .module closure exports => .module closure (eraseValidations exports)
  | _, .result value error => .result (eraseValidations value) (eraseValidations error)
  | _, .optional value => .optional (eraseValidations value)
  | _, .union first second rest => .union (eraseValidations first) (eraseValidations second) (eraseValidations rest)
  | _, .functionClass kind => .functionClass kind
  | _, .callable signature => .callable (eraseValidations signature)
  | _, .schemeCallable declaration => .schemeCallable declaration
  | _, .tagged declaration => .tagged declaration
  | _, .errorFamily declaration => .errorFamily declaration
  | _, .errorVariant family variant => .errorVariant family variant
  | _, .errorFacet facet => .errorFacet facet
  | _, .nonEmpty element => .list (eraseValidations element)
  | _, .relativePath => .primitive .path
  | _, .nominal _ fields => .record (eraseValidations fields)
  | _, .boundedInteger bounds => IntegerBaseType bounds.base
  | _, .rowEmpty => .rowEmpty
  | _, .rowField name value rest => .rowField name (eraseValidations value) (eraseValidations rest)
  | _, .argumentsEmpty => .argumentsEmpty
  | _, .parameter requirement name type rest =>
      .parameter requirement name (eraseValidations type) (eraseValidations rest)
  | _, .restParameter name element => .restParameter name (eraseValidations element)
  | _, .signature policy parameters result => .signature policy (eraseValidations parameters) (eraseValidations result)
  | _, .membersEmpty => .membersEmpty
  | _, .member value rest => .member (eraseValidations value) (eraseValidations rest)
  | _, .exportsEmpty => .exportsEmpty
  | _, .exportValue name presence value rest => .exportValue name presence (eraseValidations value) (eraseValidations rest)
  | _, .exportCallable name presence signature rest =>
      .exportCallable name presence (eraseValidations signature) (eraseValidations rest)
  | _, .exportGenericCallable name presence kind rest => .exportGenericCallable name presence kind (eraseValidations rest)

def RowsDistinct : {kind : TypeKind} → TypeVariableFreeTerm 0 String Declaration kind → Prop
  | _, .variable id => nomatch id
  | _, .list item | _, .set item | _, .stream item | _, .optional item | _, .nonEmpty item =>
      RowsDistinct item
  | _, .map key value | _, .result key value => RowsDistinct key ∧ RowsDistinct value
  | _, .record fields | _, .nominal _ fields => RowsDistinct fields
  | _, .module _ exports => RowsDistinct exports
  | _, .union first second rest => RowsDistinct first ∧ RowsDistinct second ∧ RowsDistinct rest
  | _, .callable signature => RowsDistinct signature
  | _, .rowField name type rest =>
      name ∉ (rowEntries rest).map Prod.fst ∧ RowsDistinct type ∧ RowsDistinct rest
  | _, .parameter _ _ type rest => RowsDistinct type ∧ RowsDistinct rest
  | _, .restParameter _ type => RowsDistinct type
  | _, .signature _ parameters result => RowsDistinct parameters ∧ RowsDistinct result
  | _, .member type rest | _, .exportValue _ _ type rest => RowsDistinct type ∧ RowsDistinct rest
  | _, .exportCallable _ _ signature rest => RowsDistinct signature ∧ RowsDistinct rest
  | _, .exportGenericCallable _ _ _ rest => RowsDistinct rest
  | _, _ => True

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
        (sourceFirst :: sourceSecond :: tailMembers sourceRest).length =
          (targetFirst :: targetSecond :: tailMembers targetRest).length →
        TargetMembersCovered (targetFirst :: targetSecond :: tailMembers targetRest)
          (sourceFirst :: sourceSecond :: tailMembers sourceRest) →
        MembersPaired (sourceFirst :: sourceSecond :: tailMembers sourceRest)
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

  inductive TargetMemberCovered : ViewType Declaration → List (ViewType Declaration) → Prop where
    | here {source target : ViewType Declaration} (rest : List (ViewType Declaration)) :
        BaseView source target → TargetMemberCovered target (source :: rest)
    | there {target : ViewType Declaration} (other : ViewType Declaration) {rest : List (ViewType Declaration)} :
        TargetMemberCovered target rest → TargetMemberCovered target (other :: rest)

  inductive TargetMembersCovered : List (ViewType Declaration) → List (ViewType Declaration) → Prop where
    | nil (source : List (ViewType Declaration)) : TargetMembersCovered [] source
    | cons {member : ViewType Declaration} {rest source : List (ViewType Declaration)} :
        TargetMemberCovered member source → TargetMembersCovered rest source → TargetMembersCovered (member :: rest) source

  inductive MemberRemoved : ViewType Declaration → List (ViewType Declaration) → List (ViewType Declaration) → Prop where
    | here (item : ViewType Declaration) (rest : List (ViewType Declaration)) :
        MemberRemoved item (item :: rest) rest
    | there (other : ViewType Declaration) {item : ViewType Declaration} {source rest : List (ViewType Declaration)} :
        MemberRemoved item source rest → MemberRemoved item (other :: source) (other :: rest)

  inductive MembersPaired : List (ViewType Declaration) → List (ViewType Declaration) → Prop where
    | nil : MembersPaired [] []
    | cons {source target : ViewType Declaration} {sources targets rest : List (ViewType Declaration)} :
        BaseView source target → MemberRemoved target targets rest → MembersPaired sources rest →
          MembersPaired (source :: sources) targets

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

/-- Erased types agree up to the order of record fields and union members.
This is the least equivalence that is closed under the positions a base view
descends through (list elements, map values, stream elements, record fields,
optionals, both sides of a result, union members and callable results) and
that swaps two adjacent record fields or union members. A union is compared
through the members-kind term that lists all of its members. The relation
relates only types that view each other: `erasedEquiv_mutual_views` gives view
chains in both directions, so equivalent types have the same Core members in
every world (`Widen.equivalent_members`), and `erasedEquiv_recordWidth` shows
that a record keeps its field count. -/
inductive ErasedEquiv : {kind : TypeKind} → TypeVariableFreeTerm 0 String Declaration kind →
    TypeVariableFreeTerm 0 String Declaration kind → Prop where
  | refl {kind : TypeKind} (type : TypeVariableFreeTerm 0 String Declaration kind) : ErasedEquiv type type
  | symm {kind : TypeKind} {left right : TypeVariableFreeTerm 0 String Declaration kind} :
      ErasedEquiv left right → ErasedEquiv right left
  | trans {kind : TypeKind} {first middle last : TypeVariableFreeTerm 0 String Declaration kind} :
      ErasedEquiv first middle → ErasedEquiv middle last → ErasedEquiv first last
  | list {source target : ViewType Declaration} :
      ErasedEquiv source target → ErasedEquiv (.list source) (.list target)
  | map (key : ViewType Declaration) {source target : ViewType Declaration} :
      ErasedEquiv source target → ErasedEquiv (.map key source) (.map key target)
  | stream {source target : ViewType Declaration} :
      ErasedEquiv source target → ErasedEquiv (.stream source) (.stream target)
  | record {source target : ViewRow Declaration} :
      ErasedEquiv source target → ErasedEquiv (.record source) (.record target)
  | optional {source target : ViewType Declaration} :
      ErasedEquiv source target → ErasedEquiv (.optional source) (.optional target)
  | result {sourceValue targetValue sourceError targetError : ViewType Declaration} :
      ErasedEquiv sourceValue targetValue → ErasedEquiv sourceError targetError →
        ErasedEquiv (.result sourceValue sourceError) (.result targetValue targetError)
  | union {sourceFirst sourceSecond targetFirst targetSecond : ViewType Declaration}
      {sourceRest targetRest : ViewMembers Declaration} :
      ErasedEquiv (.member sourceFirst (.member sourceSecond sourceRest))
          (.member targetFirst (.member targetSecond targetRest)) →
        ErasedEquiv (.union sourceFirst sourceSecond sourceRest) (.union targetFirst targetSecond targetRest)
  | callable {source target : ViewSignature Declaration} :
      ErasedEquiv source target → ErasedEquiv (.callable source) (.callable target)
  | signature (policy : InvocationPolicy 0) (parameters : TypeVariableFreeTerm 0 String Declaration .arguments)
      {source target : ViewType Declaration} :
      ErasedEquiv source target →
        ErasedEquiv (.signature policy parameters source) (.signature policy parameters target)
  | rowField (name : String) {source target : ViewType Declaration} {sourceRest targetRest : ViewRow Declaration} :
      ErasedEquiv source target → ErasedEquiv sourceRest targetRest →
        ErasedEquiv (.rowField name source sourceRest) (.rowField name target targetRest)
  | swapFields (first second : String) (firstType secondType : ViewType Declaration) (rest : ViewRow Declaration) :
      ErasedEquiv (.rowField first firstType (.rowField second secondType rest))
        (.rowField second secondType (.rowField first firstType rest))
  | member {source target : ViewType Declaration} {sourceRest targetRest : ViewMembers Declaration} :
      ErasedEquiv source target → ErasedEquiv sourceRest targetRest →
        ErasedEquiv (.member source sourceRest) (.member target targetRest)
  | swapMembers (first second : ViewType Declaration) (rest : ViewMembers Declaration) :
      ErasedEquiv (.member first (.member second rest)) (.member second (.member first rest))

instance erasedEquivHasEquiv {kind : TypeKind} : HasEquiv (TypeVariableFreeTerm 0 String Declaration kind) := ⟨ErasedEquiv⟩

theorem erasedEquiv_equivalence {kind : TypeKind} :
    Equivalence (ErasedEquiv (Declaration := Declaration) (kind := kind)) :=
  ⟨.refl, .symm, .trans⟩

/-- The number of fields of a record or row; zero for every other type. -/
def recordWidth : {kind : TypeKind} → TypeVariableFreeTerm 0 String Declaration kind → Nat
  | _, .record fields => recordWidth fields
  | _, .rowField _ _ rest => recordWidth rest + 1
  | _, _ => 0

theorem erasedEquiv_recordWidth {kind : TypeKind} {source target : TypeVariableFreeTerm 0 String Declaration kind}
    (equiv : ErasedEquiv source target) : recordWidth source = recordWidth target := by
  induction equiv <;> simp_all [recordWidth]

def rowOf : List (String × ViewType Declaration) → ViewRow Declaration
  | [] => .rowEmpty
  | (name, type) :: rest => .rowField name type (rowOf rest)

def membersOf : List (ViewType Declaration) → ViewMembers Declaration
  | [] => .membersEmpty
  | type :: rest => .member type (membersOf rest)

theorem rowOf_rowEntries : ∀ row : ViewRow Declaration, rowOf (rowEntries row) = row
  | .variable id => nomatch id
  | .rowEmpty => by simp [rowEntries, rowOf]
  | .rowField name type rest => by simp [rowEntries, rowOf, rowOf_rowEntries rest]

theorem membersOf_tailMembers : ∀ members : ViewMembers Declaration, membersOf (tailMembers members) = members
  | .variable id => nomatch id
  | .membersEmpty => by simp [tailMembers, membersOf]
  | .member type rest => by simp [tailMembers, membersOf, membersOf_tailMembers rest]

theorem rowEntries_erase : ∀ row : ViewRow Declaration,
    rowEntries (eraseValidations row) = (rowEntries row).map (fun entry => (entry.1, eraseValidations entry.2))
  | .variable id => nomatch id
  | .rowEmpty => by simp [rowEntries, eraseValidations]
  | .rowField name type rest => by simp [rowEntries, eraseValidations, rowEntries_erase rest]

theorem tailMembers_erase : ∀ members : ViewMembers Declaration,
    tailMembers (eraseValidations members) = (tailMembers members).map eraseValidations
  | .variable id => nomatch id
  | .membersEmpty => by simp [tailMembers, eraseValidations]
  | .member type rest => by simp [tailMembers, eraseValidations, tailMembers_erase rest]

theorem rowOf_perm {left right : List (String × ViewType Declaration)} (perm : left.Perm right) :
    ErasedEquiv (rowOf left) (rowOf right) := by
  induction perm with
  | nil => exact .refl _
  | cons entry _ ih => exact .rowField entry.1 (.refl _) ih
  | swap first second rest => exact .swapFields second.1 first.1 second.2 first.2 (rowOf rest)
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂

theorem rowsDistinct_row : ∀ row : ViewRow Declaration, RowsDistinct row →
    ((rowEntries row).map Prod.fst).Nodup ∧ ∀ entry ∈ rowEntries row, RowsDistinct entry.2
  | .variable id, _ => nomatch id
  | .rowEmpty, _ => by simp [rowEntries]
  | .rowField name type rest, ⟨fresh, here, later⟩ => by
      obtain ⟨nodup, each⟩ := rowsDistinct_row rest later
      simp only [rowEntries, List.map_cons, List.nodup_cons, List.mem_cons, forall_eq_or_imp]
      exact ⟨⟨fresh, nodup⟩, here, each⟩

theorem rowsDistinct_members : ∀ members : ViewMembers Declaration, RowsDistinct members →
    ∀ member ∈ tailMembers members, RowsDistinct member
  | .variable id, _ => nomatch id
  | .membersEmpty, _ => by simp [tailMembers]
  | .member type rest, ⟨here, later⟩ => by
      simp only [tailMembers, List.mem_cons, forall_eq_or_imp]
      exact ⟨here, rowsDistinct_members rest later⟩

theorem rowsDistinct_base {validated baseType : ViewType Declaration} (hasBase : base validated = some baseType)
    (distinct : RowsDistinct validated) : RowsDistinct baseType := by
  cases validated <;> simp only [base, reduceCtorEq, Option.some.injEq] at hasBase <;> subst hasBase
  case nonEmpty element => exact distinct
  case relativePath => trivial
  case nominal declaration fields => exact distinct
  case boundedInteger bounds => cases bounds.base <;> trivial

theorem erase_base {validated baseType : ViewType Declaration} (hasBase : base validated = some baseType) :
    eraseValidations validated = eraseValidations baseType := by
  cases validated <;> simp only [base, reduceCtorEq, Option.some.injEq] at hasBase <;> subst hasBase
  case boundedInteger bounds => cases h : bounds.base <;> simp [eraseValidations, IntegerBaseType, h]
  all_goals rfl

theorem rowOf_cover : ∀ (sources targets : List (String × ViewType Declaration)),
    (sources.map Prod.fst).Nodup → (targets.map Prod.fst).Nodup →
    (∀ name ∈ sources.map Prod.fst, name ∈ targets.map Prod.fst) →
    (∀ target ∈ targets, ∃ source, (target.1, source) ∈ sources ∧ ErasedEquiv source target.2) →
      ErasedEquiv (rowOf sources) (rowOf targets)
  | [], [], _, _, _, _ => .refl _
  | [], target :: _, _, _, _, covered => by
      obtain ⟨_, absent, _⟩ := covered target (by simp)
      simp at absent
  | (name, source) :: sources, targets, sourceNodup, targetNodup, names, covered => by
      obtain ⟨⟨found, target⟩, present, same⟩ := List.mem_map.mp (names name (by simp))
      simp only at same
      subst same
      obtain ⟨before, after, split⟩ := List.append_of_mem present
      have perm : targets.Perm ((found, target) :: (before ++ after)) := split ▸ List.perm_middle
      simp only [List.map_cons, List.nodup_cons] at sourceNodup
      have restNodup := (perm.map Prod.fst).nodup_iff.mp targetNodup
      simp only [List.map_cons, List.nodup_cons] at restNodup
      obtain ⟨partner, partnerPresent, partnerEquiv⟩ := covered (found, target) present
      have headEquiv : ErasedEquiv source target := by
        simp only [List.mem_cons, Prod.mk.injEq] at partnerPresent
        rcases partnerPresent with ⟨_, same⟩ | later
        · exact same ▸ partnerEquiv
        · exact absurd (List.mem_map.mpr ⟨_, later, rfl⟩) sourceNodup.1
      have tailEquiv := rowOf_cover sources (before ++ after) sourceNodup.2 restNodup.2
        (fun other otherPresent => by
          obtain ⟨entry, entryPresent, entryName⟩ :=
            List.mem_map.mp (names other (List.mem_cons_of_mem _ otherPresent))
          rcases List.mem_cons.mp (perm.mem_iff.mp entryPresent) with same | later
          · subst same
            subst entryName
            exact absurd otherPresent sourceNodup.1
          · exact List.mem_map.mpr ⟨entry, later, entryName⟩)
        (fun entry entryPresent => by
          obtain ⟨partner, partnerPresent, partnerEquiv⟩ := covered entry (perm.mem_iff.mpr (by simp [entryPresent]))
          simp only [List.mem_cons, Prod.mk.injEq] at partnerPresent
          rcases partnerPresent with ⟨same, _⟩ | later
          · exact absurd (List.mem_map.mpr ⟨entry, entryPresent, same⟩) restNodup.1
          · exact ⟨partner, later, partnerEquiv⟩)
      exact (ErasedEquiv.rowField found headEquiv tailEquiv).trans (rowOf_perm perm.symm)

theorem removed_perm : ∀ {item : ViewType Declaration} {source rest : List (ViewType Declaration)},
    MemberRemoved item source rest → source.Perm (item :: rest)
  | _, _, _, .here _ _ => .refl _
  | _, _, _, .there other removed => ((removed_perm removed).cons other).trans (.swap _ _ _)

theorem removed_erased : ∀ {item : ViewType Declaration} {source rest : List (ViewType Declaration)},
    MemberRemoved item source rest →
      ErasedEquiv (membersOf (source.map eraseValidations))
        (.member (eraseValidations item) (membersOf (rest.map eraseValidations)))
  | _, _, _, .here _ _ => .refl _
  | _, _, _, .there other removed =>
      (ErasedEquiv.member (.refl _) (removed_erased removed)).trans (.swapMembers _ _ _)

theorem erased_names (entries : List (String × ViewType Declaration)) :
    (entries.map fun entry => (entry.1, eraseValidations entry.2)).map Prod.fst = entries.map Prod.fst := by
  induction entries <;> simp_all

mutual
  theorem view_erased : ∀ {source target : ViewType Declaration}, BaseView source target →
      RowsDistinct source → RowsDistinct target →
        ErasedEquiv (eraseValidations source) (eraseValidations target)
    | _, _, .refl _, _, _ => .refl _
    | _, _, .validated hasBase view, sourceDistinct, targetDistinct => by
        rw [erase_base hasBase]
        exact view_erased view (rowsDistinct_base hasBase sourceDistinct) targetDistinct
    | _, _, .nonEmpty view, sourceDistinct, targetDistinct =>
        .list (view_erased view sourceDistinct targetDistinct)
    | _, _, .list view, sourceDistinct, targetDistinct =>
        .list (view_erased view sourceDistinct targetDistinct)
    | _, _, .map _ view, sourceDistinct, targetDistinct =>
        .map _ (view_erased view sourceDistinct.2 targetDistinct.2)
    | _, _, .stream view, sourceDistinct, targetDistinct =>
        .stream (view_erased view sourceDistinct targetDistinct)
    | _, _, .record (source := sourceRow) (target := targetRow) covered names, sourceDistinct, targetDistinct => by
        obtain ⟨sourceNodup, sourceEach⟩ := rowsDistinct_row sourceRow sourceDistinct
        obtain ⟨targetNodup, targetEach⟩ := rowsDistinct_row targetRow targetDistinct
        have matched := entries_erased covered sourceEach targetEach
        refine .record ?_
        rw [← rowOf_rowEntries (eraseValidations sourceRow), ← rowOf_rowEntries (eraseValidations targetRow),
          rowEntries_erase, rowEntries_erase]
        refine rowOf_cover _ _ (by rwa [erased_names]) (by rwa [erased_names])
          (by rwa [erased_names, erased_names]) ?_
        intro entry present
        obtain ⟨original, originalPresent, same⟩ := List.mem_map.mp present
        subst same
        obtain ⟨partner, partnerPresent, partnerEquiv⟩ := matched original originalPresent
        exact ⟨eraseValidations partner, List.mem_map.mpr ⟨_, partnerPresent, rfl⟩, partnerEquiv⟩
    | _, _, .optional view, sourceDistinct, targetDistinct =>
        .optional (view_erased view sourceDistinct targetDistinct)
    | _, _, .result valueView errorView, sourceDistinct, targetDistinct =>
        .result (view_erased valueView sourceDistinct.1 targetDistinct.1)
          (view_erased errorView sourceDistinct.2 targetDistinct.2)
    | _, _, .union (sourceFirst := sourceFirst) (sourceSecond := sourceSecond) (sourceRest := sourceRest)
        (targetFirst := targetFirst) (targetSecond := targetSecond) (targetRest := targetRest)
        _ _ _ paired, sourceDistinct, targetDistinct => by
        obtain ⟨sourceFirstDistinct, sourceSecondDistinct, sourceRestDistinct⟩ := sourceDistinct
        obtain ⟨targetFirstDistinct, targetSecondDistinct, targetRestDistinct⟩ := targetDistinct
        have equiv := paired_erased paired
          (by
            simp only [List.mem_cons, forall_eq_or_imp]
            exact ⟨sourceFirstDistinct, sourceSecondDistinct, rowsDistinct_members _ sourceRestDistinct⟩)
          (by
            simp only [List.mem_cons, forall_eq_or_imp]
            exact ⟨targetFirstDistinct, targetSecondDistinct, rowsDistinct_members _ targetRestDistinct⟩)
        simp only [List.map_cons, membersOf, ← tailMembers_erase, membersOf_tailMembers] at equiv
        exact .union equiv
    | _, _, .callable (.result _ _ view), sourceDistinct, targetDistinct =>
        .callable (.signature _ _ (view_erased view sourceDistinct.2 targetDistinct.2))

  theorem entry_erased : ∀ {entry : String × ViewType Declaration} {sources : List (String × ViewType Declaration)},
      EntryCovered entry sources → (∀ source ∈ sources, RowsDistinct source.2) → RowsDistinct entry.2 →
        ∃ source, (entry.1, source) ∈ sources ∧
          ErasedEquiv (eraseValidations source) (eraseValidations entry.2)
    | _, _, .here _ _ view, sourceDistinct, targetDistinct =>
        ⟨_, List.mem_cons_self, view_erased view (sourceDistinct _ List.mem_cons_self) targetDistinct⟩
    | _, _, .there _ covered, sourceDistinct, targetDistinct => by
        obtain ⟨source, present, equiv⟩ :=
          entry_erased covered (fun source present => sourceDistinct source (List.mem_cons_of_mem _ present))
            targetDistinct
        exact ⟨source, List.mem_cons_of_mem _ present, equiv⟩

  theorem entries_erased : ∀ {targets sources : List (String × ViewType Declaration)},
      EntriesCovered targets sources → (∀ source ∈ sources, RowsDistinct source.2) →
        (∀ target ∈ targets, RowsDistinct target.2) →
          ∀ target ∈ targets, ∃ source, (target.1, source) ∈ sources ∧
            ErasedEquiv (eraseValidations source) (eraseValidations target.2)
    | _, _, .nil _, _, _ => by simp
    | _, _, .cons here rest, sourceDistinct, targetDistinct => by
        intro target present
        rcases List.mem_cons.mp present with same | later
        · exact same ▸ entry_erased here sourceDistinct (targetDistinct _ List.mem_cons_self)
        · exact entries_erased rest sourceDistinct
            (fun other otherPresent => targetDistinct other (List.mem_cons_of_mem _ otherPresent)) target later

  theorem paired_erased : ∀ {sources targets : List (ViewType Declaration)}, MembersPaired sources targets →
      (∀ source ∈ sources, RowsDistinct source) → (∀ target ∈ targets, RowsDistinct target) →
        ErasedEquiv (membersOf (sources.map eraseValidations)) (membersOf (targets.map eraseValidations))
    | _, _, .nil, _, _ => .refl _
    | _, _, .cons view removed rest, sourceDistinct, targetDistinct => by
        have perm := removed_perm removed
        exact (ErasedEquiv.member
            (view_erased view (sourceDistinct _ List.mem_cons_self) (targetDistinct _ (perm.mem_iff.mpr List.mem_cons_self)))
            (paired_erased rest (fun other otherPresent => sourceDistinct other (List.mem_cons_of_mem _ otherPresent))
              (fun other otherPresent => targetDistinct other
                (perm.mem_iff.mpr (List.mem_cons_of_mem _ otherPresent))))).trans
          (removed_erased removed).symm
end

/-- Erased-position precision. When every row of both types has distinct
labels, a base view changes only validated positions: erasing validations on
both sides gives equivalent types, which differ at most in the order of record
fields and union members. The label premise stands for the checker's rejection
of duplicate record fields; without it a record view can drop a duplicate
label. -/
theorem view_precision {source target : ViewType Declaration} :
    RowsDistinct source → RowsDistinct target → BaseView source target →
      eraseValidations source ≈ eraseValidations target :=
  fun sourceDistinct targetDistinct view => view_erased view sourceDistinct targetDistinct

/-! Equivalent erased types view each other. -/

theorem ViewChain.single {source target : ViewType Declaration} (view : BaseView source target) :
    ViewChain source target :=
  .step (.refl _) view

theorem ViewChain.trans {first middle last : ViewType Declaration} (front : ViewChain first middle)
    (back : ViewChain middle last) : ViewChain first last := by
  induction back with
  | refl => exact front
  | step _ view ih => exact .step ih view

theorem SignatureChain.trans {first middle last : ViewSignature Declaration} (front : SignatureChain first middle)
    (back : SignatureChain middle last) : SignatureChain first last := by
  induction back with
  | refl => exact front
  | step _ view ih => exact .step ih view

theorem ViewChain.lift {f : ViewType Declaration → ViewType Declaration}
    (lift : ∀ {source target}, BaseView source target → BaseView (f source) (f target))
    {source target : ViewType Declaration} (chain : ViewChain source target) : ViewChain (f source) (f target) := by
  induction chain with
  | refl => exact .refl _
  | step _ view ih => exact .step ih (lift view)

theorem ViewChain.liftSignature {f : ViewType Declaration → ViewSignature Declaration}
    (lift : ∀ {source target}, BaseView source target → SignatureView (f source) (f target))
    {source target : ViewType Declaration} (chain : ViewChain source target) :
    SignatureChain (f source) (f target) := by
  induction chain with
  | refl => exact .refl _
  | step _ view ih => exact .step ih (lift view)

theorem SignatureChain.lift {f : ViewSignature Declaration → ViewType Declaration}
    (lift : ∀ {source target}, SignatureView source target → BaseView (f source) (f target))
    {source target : ViewSignature Declaration} (chain : SignatureChain source target) :
    ViewChain (f source) (f target) := by
  induction chain with
  | refl => exact .refl _
  | step _ view ih => exact .step ih (lift view)

theorem entries_weaken : ∀ {targets sources : List (String × ViewType Declaration)}
    (other : String × ViewType Declaration), EntriesCovered targets sources →
      EntriesCovered targets (other :: sources)
  | _, _, _, .nil _ => .nil _
  | _, _, other, .cons here rest => .cons (.there other here) (entries_weaken other rest)

theorem entries_self : ∀ entries : List (String × ViewType Declaration), EntriesCovered entries entries
  | [] => .nil _
  | (name, type) :: rest => .cons (.here name rest (.refl type)) (entries_weaken _ (entries_self rest))

theorem record_view_cons (name : String) (type : ViewType Declaration) {row : ViewRow Declaration}
    {target : ViewType Declaration} (view : BaseView (.record row) target) :
    ∃ targetRow, target = .record targetRow ∧
      BaseView (.record (.rowField name type row)) (.record (.rowField name type targetRow)) := by
  cases view with
  | refl => exact ⟨row, rfl, .refl _⟩
  | validated hasBase _ => simp [base] at hasBase
  | record covered names =>
    refine ⟨_, rfl, .record ?_ ?_⟩
    · simp only [rowEntries]
      exact .cons (.here name _ (.refl type)) (entries_weaken _ covered)
    · simp only [rowEntries, List.map_cons, List.mem_cons]
      rintro other (same | later)
      · exact Or.inl same
      · exact Or.inr (names other later)

theorem record_chain_cons (name : String) (type : ViewType Declaration) {row : ViewRow Declaration}
    {target : ViewType Declaration} (chain : ViewChain (.record row) target) :
    ∃ targetRow, target = .record targetRow ∧
      ViewChain (.record (.rowField name type row)) (.record (.rowField name type targetRow)) := by
  induction chain with
  | refl => exact ⟨row, rfl, .refl _⟩
  | step _ view ih =>
    obtain ⟨middleRow, same, front⟩ := ih
    subst same
    obtain ⟨targetRow, same, back⟩ := record_view_cons name type view
    exact ⟨targetRow, same, .step front back⟩

theorem record_chain_tail (name : String) (type : ViewType Declaration) {row target : ViewRow Declaration}
    (chain : ViewChain (.record row) (.record target)) :
    ViewChain (.record (.rowField name type row)) (.record (.rowField name type target)) := by
  obtain ⟨targetRow, same, lifted⟩ := record_chain_cons name type chain
  cases same
  exact lifted

theorem record_chain_field (name : String) {source target : ViewType Declaration} (rest : ViewRow Declaration)
    (chain : ViewChain source target) :
    ViewChain (.record (.rowField name source rest)) (.record (.rowField name target rest)) :=
  ViewChain.lift (f := fun type => .record (.rowField name type rest))
    (fun view => .record (by simp only [rowEntries]; exact .cons (.here name _ view) (entries_weaken _ (entries_self _)))
      (by simp [rowEntries]))
    chain

theorem record_swap_view (first second : String) (firstType secondType : ViewType Declaration)
    (rest : ViewRow Declaration) :
    BaseView (.record (.rowField first firstType (.rowField second secondType rest)))
      (.record (.rowField second secondType (.rowField first firstType rest))) := by
  refine .record ?_ ?_
  · simp only [rowEntries]
    exact .cons (.there _ (.here second _ (.refl secondType)))
      (.cons (.here first _ (.refl firstType)) (entries_weaken _ (entries_weaken _ (entries_self _))))
  · simp only [rowEntries, List.map_cons, List.mem_cons]
    rintro other (same | same | later)
    · exact Or.inr (Or.inl same)
    · exact Or.inl same
    · exact Or.inr (Or.inr later)

inductive MembersChain : List (ViewType Declaration) → List (ViewType Declaration) → Prop where
  | refl (members : List (ViewType Declaration)) : MembersChain members members
  | step {first middle last : List (ViewType Declaration)} :
      MembersChain first middle → MembersPaired middle last → MembersChain first last

theorem MembersChain.trans {first middle last : List (ViewType Declaration)} (front : MembersChain first middle)
    (back : MembersChain middle last) : MembersChain first last := by
  induction back with
  | refl => exact front
  | step _ paired ih => exact .step ih paired

theorem paired_self : ∀ members : List (ViewType Declaration), MembersPaired members members
  | [] => .nil
  | member :: rest => .cons (.refl member) (.here member rest) (paired_self rest)

theorem members_chain_cons (member : ViewType Declaration) {source target : List (ViewType Declaration)}
    (chain : MembersChain source target) : MembersChain (member :: source) (member :: target) := by
  induction chain with
  | refl => exact .refl _
  | step _ paired ih => exact .step ih (.cons (.refl member) (.here member _) paired)

theorem members_chain_head {source target : ViewType Declaration} (rest : List (ViewType Declaration))
    (chain : ViewChain source target) : MembersChain (source :: rest) (target :: rest) := by
  induction chain with
  | refl => exact .refl _
  | step _ view ih => exact .step ih (.cons view (.here _ rest) (paired_self rest))

theorem paired_facts : ∀ {sources targets : List (ViewType Declaration)}, MembersPaired sources targets →
    (∀ source ∈ sources, ∃ target ∈ targets, BaseView source target) ∧
      (∀ target ∈ targets, ∃ source ∈ sources, BaseView source target) ∧ sources.length = targets.length
  | _, _, .nil => by simp
  | _, _, .cons view removed rest => by
      have perm := removed_perm removed
      obtain ⟨forward, backward, lengths⟩ := paired_facts rest
      refine ⟨?_, ?_, by simp [perm.length_eq, lengths]⟩
      · intro source present
        rcases List.mem_cons.mp present with same | later
        · exact ⟨_, perm.mem_iff.mpr List.mem_cons_self, same ▸ view⟩
        · obtain ⟨target, targetPresent, targetView⟩ := forward source later
          exact ⟨target, perm.mem_iff.mpr (List.mem_cons_of_mem _ targetPresent), targetView⟩
      · intro target present
        rcases List.mem_cons.mp (perm.mem_iff.mp present) with same | later
        · exact ⟨_, List.mem_cons_self, same ▸ view⟩
        · obtain ⟨source, sourcePresent, sourceView⟩ := backward target later
          exact ⟨source, List.mem_cons_of_mem _ sourcePresent, sourceView⟩

theorem member_covered_of {source target : ViewType Declaration} :
    ∀ {targets : List (ViewType Declaration)}, target ∈ targets → BaseView source target →
      MemberCovered source targets
  | _ :: _, present, view => by
      rcases List.mem_cons.mp present with same | later
      · exact same ▸ .here _ view
      · exact .there _ (member_covered_of later view)

theorem members_covered_of {targets : List (ViewType Declaration)} :
    ∀ {sources : List (ViewType Declaration)}, (∀ source ∈ sources, ∃ target ∈ targets, BaseView source target) →
      MembersCovered sources targets
  | [], _ => .nil _
  | _ :: _, covered => by
      obtain ⟨target, present, view⟩ := covered _ List.mem_cons_self
      exact .cons (member_covered_of present view)
        (members_covered_of fun source later => covered source (List.mem_cons_of_mem _ later))

theorem target_member_covered_of {source target : ViewType Declaration} :
    ∀ {sources : List (ViewType Declaration)}, source ∈ sources → BaseView source target →
      TargetMemberCovered target sources
  | _ :: _, present, view => by
      rcases List.mem_cons.mp present with same | later
      · exact same ▸ .here _ view
      · exact .there _ (target_member_covered_of later view)

theorem target_members_covered_of {sources : List (ViewType Declaration)} :
    ∀ {targets : List (ViewType Declaration)}, (∀ target ∈ targets, ∃ source ∈ sources, BaseView source target) →
      TargetMembersCovered targets sources
  | [], _ => .nil _
  | _ :: _, covered => by
      obtain ⟨source, present, view⟩ := covered _ List.mem_cons_self
      exact .cons (target_member_covered_of present view)
        (target_members_covered_of fun target later => covered target (List.mem_cons_of_mem _ later))

theorem tailMembers_membersOf : ∀ members : List (ViewType Declaration), tailMembers (membersOf members) = members
  | [] => by simp [membersOf, tailMembers]
  | member :: rest => by simp [membersOf, tailMembers, tailMembers_membersOf rest]

theorem union_paired_view {sourceFirst sourceSecond targetFirst targetSecond : ViewType Declaration}
    {sourceRest targetRest : List (ViewType Declaration)}
    (paired : MembersPaired (sourceFirst :: sourceSecond :: sourceRest) (targetFirst :: targetSecond :: targetRest)) :
    BaseView (.union sourceFirst sourceSecond (membersOf sourceRest))
      (.union targetFirst targetSecond (membersOf targetRest)) := by
  obtain ⟨forward, backward, lengths⟩ := paired_facts paired
  refine .union ?_ ?_ ?_ ?_ <;> rw [tailMembers_membersOf, tailMembers_membersOf]
  · exact members_covered_of forward
  · exact lengths
  · exact target_members_covered_of backward
  · exact paired

theorem union_chain {first second : ViewType Declaration} {rest : ViewMembers Declaration}
    {target : List (ViewType Declaration)} (chain : MembersChain (first :: second :: tailMembers rest) target) :
    ∀ {targetFirst targetSecond : ViewType Declaration} {targetRest : List (ViewType Declaration)},
      target = targetFirst :: targetSecond :: targetRest →
        ViewChain (.union first second rest) (.union targetFirst targetSecond (membersOf targetRest)) := by
  induction chain with
  | refl =>
    intro targetFirst targetSecond targetRest same
    simp only [List.cons.injEq] at same
    obtain ⟨rfl, rfl, same⟩ := same
    rw [← same, membersOf_tailMembers]
    exact .refl _
  | @step middle last _ paired ih =>
    intro targetFirst targetSecond targetRest same
    subst same
    have lengths := (paired_facts paired).2.2
    obtain ⟨middleFirst, middleSecond, middleRest, rfl⟩ :
        ∃ middleFirst middleSecond middleRest, middle = middleFirst :: middleSecond :: middleRest := by
      match middle, lengths with
      | [], lengths => simp at lengths
      | [_], lengths => simp at lengths
      | middleFirst :: middleSecond :: middleRest, _ => exact ⟨middleFirst, middleSecond, middleRest, rfl⟩
    exact .step (ih rfl) (union_paired_view paired)

def EquivViews : (kind : TypeKind) → TypeVariableFreeTerm 0 String Declaration kind →
    TypeVariableFreeTerm 0 String Declaration kind → Prop
  | .value, source, target => ViewChain source target ∧ ViewChain target source
  | .row, source, target => ViewChain (.record source) (.record target) ∧ ViewChain (.record target) (.record source)
  | .members, source, target =>
      MembersChain (tailMembers source) (tailMembers target) ∧ MembersChain (tailMembers target) (tailMembers source)
  | .signature, source, target => SignatureChain source target ∧ SignatureChain target source
  | .arguments, source, target => source = target
  | .exports, source, target => source = target

theorem erasedEquiv_views {kind : TypeKind} {source target : TypeVariableFreeTerm 0 String Declaration kind}
    (equiv : ErasedEquiv source target) : EquivViews kind source target := by
  induction equiv with
  | @refl kind type =>
    cases kind <;> simp only [EquivViews] <;> first | exact ⟨.refl _, .refl _⟩ | rfl
  | @symm kind _ _ _ ih =>
    cases kind <;> simp only [EquivViews] at ih ⊢ <;> first | exact ⟨ih.2, ih.1⟩ | exact ih.symm
  | @trans kind _ _ _ _ _ ih₁ ih₂ =>
    cases kind <;> simp only [EquivViews] at ih₁ ih₂ ⊢ <;>
      first | exact ⟨ih₁.1.trans ih₂.1, ih₂.2.trans ih₁.2⟩ | exact ih₁.trans ih₂
  | list _ ih => exact ⟨ih.1.lift .list, ih.2.lift .list⟩
  | map key _ ih => exact ⟨ih.1.lift (.map key), ih.2.lift (.map key)⟩
  | stream _ ih => exact ⟨ih.1.lift .stream, ih.2.lift .stream⟩
  | record _ ih => exact ih
  | optional _ ih => exact ⟨ih.1.lift .optional, ih.2.lift .optional⟩
  | result _ _ valueIh errorIh =>
    exact ⟨(valueIh.1.lift (fun view => .result view (.refl _))).trans (errorIh.1.lift (fun view => .result (.refl _) view)),
      (valueIh.2.lift (fun view => .result view (.refl _))).trans (errorIh.2.lift (fun view => .result (.refl _) view))⟩
  | union _ ih =>
    simp only [EquivViews, tailMembers] at ih
    exact ⟨(union_chain ih.1 rfl).trans (by rw [membersOf_tailMembers]; exact .refl _),
      (union_chain ih.2 rfl).trans (by rw [membersOf_tailMembers]; exact .refl _)⟩
  | callable _ ih => exact ⟨ih.1.lift .callable, ih.2.lift .callable⟩
  | signature policy parameters _ ih => exact ⟨ih.1.liftSignature (.result policy parameters), ih.2.liftSignature (.result policy parameters)⟩
  | rowField name _ _ valueIh restIh =>
    exact ⟨(record_chain_field name _ valueIh.1).trans (record_chain_tail name _ restIh.1),
      (record_chain_field name _ valueIh.2).trans (record_chain_tail name _ restIh.2)⟩
  | swapFields first second firstType secondType rest =>
    exact ⟨.single (record_swap_view _ _ _ _ _), .single (record_swap_view _ _ _ _ _)⟩
  | member _ _ valueIh restIh =>
    simp only [EquivViews, tailMembers] at restIh ⊢
    exact ⟨(members_chain_head _ valueIh.1).trans (members_chain_cons _ restIh.1),
      (members_chain_head _ valueIh.2).trans (members_chain_cons _ restIh.2)⟩
  | swapMembers first second rest =>
    simp only [EquivViews, tailMembers]
    exact ⟨.step (.refl _) (.cons (.refl first) (.there second (.here first _)) (paired_self _)),
      .step (.refl _) (.cons (.refl second) (.there first (.here second _)) (paired_self _))⟩

/-- Equivalent types view each other in both directions. -/
theorem erasedEquiv_mutual_views {source target : ViewType Declaration} (equiv : source ≈ target) :
    ViewChain source target ∧ ViewChain target source :=
  erasedEquiv_views equiv

end Xsh.Core
