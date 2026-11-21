import Xsh.Validation.Core.Union

namespace Xsh.Validation.Core.Require

open Xsh.Core
open Xsh.Validation.Core
open Xsh.Validation.Core.Union

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

def decideMember (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    Option (Value Declaration Identity Source) :=
  letI := Membership.decideTerm world contracts type candidate
  if Membership.value world type candidate then some candidate else none

def replaceField (name : String) (replacement : Value Declaration Identity Source) :
    List (String × Value Declaration Identity Source) → List (String × Value Declaration Identity Source)
  | [] => []
  | (other, value) :: rest =>
    if other = name then (other, replacement) :: rest else (other, value) :: replaceField name replacement rest

def peel : Value Declaration Identity Source → List Declaration × Value Declaration Identity Source
  | .nominal declaration contents => ((declaration :: (peel contents).1), (peel contents).2)
  | candidate => ([], candidate)

def wrap : List Declaration → Value Declaration Identity Source → Value Declaration Identity Source
  | [], candidate => candidate
  | declaration :: rest, candidate => .nominal declaration (wrap rest candidate)

def rewrap (original : Value Declaration Identity Source) (rebuilt : Value Declaration Identity Source) :
    Value Declaration Identity Source :=
  wrap (peel original).1 rebuilt

mutual
  def require (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      ValueType Declaration → Value Declaration Identity Source → Option (Value Declaration Identity Source)
    | .variable id, _ => nomatch id
    | .list element, candidate =>
      match candidate.eraseNominal with
      | .list items => (items.mapM (require world contracts element)).map (fun items => rewrap candidate (.list items))
      | _ => decideMember world contracts (.list element) candidate
    | .nonEmpty element, candidate =>
      match candidate.eraseNominal with
      | .list [] => none
      | .list items => (items.mapM (require world contracts element)).map (fun items => rewrap candidate (.list items))
      | _ => decideMember world contracts (.nonEmpty element) candidate
    | .map keyType element, candidate =>
      match candidate.eraseNominal with
      | .map entries =>
        (entries.mapM (fun entry =>
          letI := Membership.decideKey keyType entry.1
          if Membership.key keyType entry.1 then
            (require world contracts element entry.2).map (fun value => (entry.1, value))
          else none)).map (fun entries => rewrap candidate (.map entries))
      | .record fields =>
        if keyType = .primitive .string then
          (fields.mapM (fun field =>
            (require world contracts element field.2).map (fun value => (field.1, value)))).map
              (fun fields => rewrap candidate (.record fields))
        else none
      | _ => none
    | .record fields, candidate =>
      match candidate.eraseNominal with
      | .record actual => (requireRow world contracts fields actual).map (fun fields => rewrap candidate (.record fields))
      | _ => decideMember world contracts (.record fields) candidate
    | .nominal declaration fields, candidate =>
      match candidate.eraseNominal with
      | .record actual => (requireRow world contracts fields actual).map (fun rebuilt => .nominal declaration (.record rebuilt))
      | _ => (decideMember world contracts (.record fields) candidate).map
          (fun accepted => .nominal declaration accepted.eraseNominal)
    | .optional element, candidate =>
      letI := Membership.decidePrimitive (Declaration := Declaration) .null candidate
      if Membership.primitive .null candidate then some candidate else require world contracts element candidate
    | .union first second rest, candidate =>
      match require world contracts first candidate with
      | some converted => some converted
      | none =>
        match require world contracts second candidate with
        | some converted => some converted
        | none => requireMembers world contracts rest candidate
    | .result success failure, candidate =>
      match candidate.eraseNominal with
      | .ok item => (require world contracts success item).map (fun item => rewrap candidate (.ok item))
      | .err item => (require world contracts failure item).map (fun item => rewrap candidate (.err item))
      | _ => none
    | .primitive type, candidate => decideMember world contracts (.primitive type) candidate
    | .set element, candidate => decideMember world contracts (.set element) candidate
    | .stream element, candidate => decideMember world contracts (.stream element) candidate
    | .module closure exports, candidate => decideMember world contracts (.module closure exports) candidate
    | .functionClass kind, candidate => decideMember world contracts (.functionClass kind) candidate
    | .callable signature, candidate => decideMember world contracts (.callable signature) candidate
    | .schemeCallable declaration, candidate => decideMember world contracts (.schemeCallable declaration) candidate
    | .tagged declaration, candidate => decideMember world contracts (.tagged declaration) candidate
    | .errorFamily declaration, candidate => decideMember world contracts (.errorFamily declaration) candidate
    | .errorVariant family variant, candidate => decideMember world contracts (.errorVariant family variant) candidate
    | .errorFacet facet, candidate => decideMember world contracts (.errorFacet facet) candidate
    | .relativePath, candidate => decideMember world contracts .relativePath candidate
    | .boundedInteger bounds, candidate => decideMember world contracts (.boundedInteger bounds) candidate

  def requireRow (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      RecordType Declaration → List (String × Value Declaration Identity Source) →
        Option (List (String × Value Declaration Identity Source))
    | .variable id, _ => nomatch id
    | .rowEmpty, actual => some actual
    | .rowField name type rest, actual =>
      match actual.lookup name with
      | none => none
      | some found =>
        match require world contracts type found with
        | none => none
        | some converted => requireRow world contracts rest (replaceField name converted actual)

  def requireMembers (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      UnionTail Declaration → Value Declaration Identity Source → Option (Value Declaration Identity Source)
    | .variable id, _ => nomatch id
    | .membersEmpty, _ => none
    | .member type rest, candidate =>
      match require world contracts type candidate with
      | some converted => some converted
      | none => requireMembers world contracts rest candidate
end

def rowNames : RecordType Declaration → List String
  | .rowField name _ rest => name :: rowNames rest
  | _ => []

mutual
  def Admissible : ValueType Declaration → Prop
    | .list element | .nonEmpty element | .optional element => Admissible element
    | .map _ element => Admissible element
    | .record fields | .nominal _ fields => RowAdmissible fields
    | .union first second rest => Admissible first ∧ Admissible second ∧ MembersAdmissible rest
    | .result success failure => Admissible success ∧ Admissible failure
    | _ => True

  def RowAdmissible : RecordType Declaration → Prop
    | .rowField name type rest => name ∉ rowNames rest ∧ Admissible type ∧ RowAdmissible rest
    | _ => True

  def MembersAdmissible : UnionTail Declaration → Prop
    | .member type rest => Admissible type ∧ MembersAdmissible rest
    | _ => True
end

mutual
  def NominalFree : ValueType Declaration → Prop
    | .list element | .nonEmpty element | .optional element => NominalFree element
    | .map _ element => NominalFree element
    | .record fields => RowNominalFree fields
    | .nominal _ _ => False
    | .union first second rest => NominalFree first ∧ NominalFree second ∧ MembersNominalFree rest
    | .result success failure => NominalFree success ∧ NominalFree failure
    | _ => True

  def RowNominalFree : RecordType Declaration → Prop
    | .rowField _ type rest => NominalFree type ∧ RowNominalFree rest
    | _ => True

  def MembersNominalFree : UnionTail Declaration → Prop
    | .member type rest => NominalFree type ∧ MembersNominalFree rest
    | _ => True
end

def requireResult (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (schemaError : Value Declaration Identity Source) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) : Value Declaration Identity Source :=
  match require world contracts type candidate with
  | some converted => .ok converted
  | none => .err schemaError


omit [DecidableEq Declaration] in
theorem eraseNominal_wrap (declarations : List Declaration) (candidate : Value Declaration Identity Source) :
    (wrap declarations candidate).eraseNominal = candidate.eraseNominal := by
  induction declarations with
  | nil => rfl
  | cons head rest ih => simp [wrap, Value.eraseNominal, ih]

omit [DecidableEq Declaration] in
theorem eraseNominal_rewrap (original rebuilt : Value Declaration Identity Source) :
    (rewrap original rebuilt).eraseNominal = rebuilt.eraseNominal :=
  eraseNominal_wrap _ _

omit [DecidableEq Declaration] in
theorem wrap_peel (candidate : Value Declaration Identity Source) :
    wrap (peel candidate).1 (peel candidate).2 = candidate := by
  cases candidate with
  | nominal declaration contents => simp [peel, wrap, wrap_peel contents]
  | _ => rfl

omit [DecidableEq Declaration] in
theorem peel_eraseNominal (candidate : Value Declaration Identity Source) :
    (peel candidate).2 = candidate.eraseNominal := by
  cases candidate with
  | nominal declaration contents => simp [peel, Value.eraseNominal, peel_eraseNominal contents]
  | _ => rfl

omit [DecidableEq Declaration] in
theorem rewrap_erased (candidate : Value Declaration Identity Source) :
    rewrap candidate candidate.eraseNominal = candidate := by
  unfold rewrap
  rw [← peel_eraseNominal, wrap_peel]

theorem decideMember_some {world : MembershipWorld Declaration Identity Source} {contracts : ContractDecisions world}
    {type : ValueType Declaration} {candidate converted : Value Declaration Identity Source}
    (decided : decideMember world contracts type candidate = some converted) :
    converted = candidate ∧ Membership.value world type candidate := by
  unfold decideMember at decided
  letI := Membership.decideTerm world contracts type candidate
  by_cases member : Membership.value world type candidate
  · simp only [member, ite_true, Option.some.injEq] at decided
    exact ⟨decided.symm, member⟩
  · simp [member] at decided

theorem decideMember_of_member {world : MembershipWorld Declaration Identity Source}
    {contracts : ContractDecisions world} {type : ValueType Declaration} {candidate : Value Declaration Identity Source}
    (member : Membership.value world type candidate) : decideMember world contracts type candidate = some candidate := by
  unfold decideMember
  letI := Membership.decideTerm world contracts type candidate
  simp [member]

omit [DecidableEq Declaration] in
theorem mapM_some_mem {α β : Type} {f : α → Option β} :
    ∀ {inputs : List α} {outputs : List β}, inputs.mapM f = some outputs →
      ∀ output ∈ outputs, ∃ input ∈ inputs, f input = some output
  | [], outputs, mapped => by
    simp at mapped
    subst mapped
    simp
  | input :: rest, outputs, mapped => by
    simp only [List.mapM_cons, Option.bind_eq_bind, Option.pure_def] at mapped
    cases head : f input with
    | none => simp [head] at mapped
    | some converted =>
      cases tail : rest.mapM f with
      | none => simp [head, tail] at mapped
      | some others =>
        simp [head, tail] at mapped
        subst mapped
        intro output present
        simp only [List.mem_cons] at present
        rcases present with same | later
        · exact ⟨input, by simp, same ▸ head⟩
        · obtain ⟨found, foundPresent, foundMapped⟩ := mapM_some_mem tail output later
          exact ⟨found, List.mem_cons_of_mem _ foundPresent, foundMapped⟩

omit [DecidableEq Declaration] in
theorem mapM_some_nonempty {α β : Type} {f : α → Option β} {input : α} {rest : List α} {outputs : List β}
    (mapped : (input :: rest).mapM f = some outputs) : outputs ≠ [] := by
  simp only [List.mapM_cons, Option.bind_eq_bind, Option.pure_def] at mapped
  cases head : f input with
  | none => simp [head] at mapped
  | some converted =>
    cases tail : rest.mapM f with
    | none => simp [head, tail] at mapped
    | some others =>
      simp [head, tail] at mapped
      subst mapped
      simp

omit [DecidableEq Declaration] in
theorem mapM_identity {α : Type} {f : α → Option α} (identity : ∀ input output, f input = some output → output = input) :
    ∀ {inputs outputs : List α}, inputs.mapM f = some outputs → outputs = inputs
  | [], outputs, mapped => by simp at mapped; exact mapped
  | input :: rest, outputs, mapped => by
    simp only [List.mapM_cons, Option.bind_eq_bind, Option.pure_def] at mapped
    cases head : f input with
    | none => simp [head] at mapped
    | some converted =>
      cases tail : rest.mapM f with
      | none => simp [head, tail] at mapped
      | some others =>
        simp [head, tail] at mapped
        subst mapped
        rw [identity input converted head, mapM_identity identity tail]

omit [DecidableEq Declaration] in
theorem mapM_of_identity {α : Type} {f : α → Option α} :
    ∀ {inputs : List α}, (∀ input ∈ inputs, f input = some input) → inputs.mapM f = some inputs
  | [], _ => rfl
  | input :: rest, identity => by
    simp only [List.mapM_cons, Option.bind_eq_bind, Option.pure_def]
    rw [identity input (by simp), mapM_of_identity (fun other present => identity other (List.mem_cons_of_mem _ present))]
    rfl

omit [DecidableEq Declaration] in
theorem lookup_replaceField_same (name : String) (replacement : Value Declaration Identity Source) :
    ∀ {fields : List (String × Value Declaration Identity Source)} {found : Value Declaration Identity Source},
      fields.lookup name = some found → (replaceField name replacement fields).lookup name = some replacement
  | [], _, present => by simp at present
  | (other, value) :: rest, found, present => by
    by_cases same : other = name
    · subst same
      simp [replaceField]
    · have differentNames : (name == other) = false := by simpa using Ne.symm same
      simp only [List.lookup, differentNames] at present
      simp only [replaceField, same, ite_false, List.lookup, differentNames]
      exact lookup_replaceField_same name replacement present

omit [DecidableEq Declaration] in
theorem lookup_replaceField_other (name other : String) (replacement : Value Declaration Identity Source)
    (different : other ≠ name) :
    ∀ fields : List (String × Value Declaration Identity Source),
      (replaceField name replacement fields).lookup other = fields.lookup other
  | [] => rfl
  | (key, value) :: rest => by
    by_cases same : key = name
    · subst same
      have differentNames : (other == key) = false := by simpa using different
      simp [replaceField, List.lookup, differentNames]
    · simp only [replaceField, same, ite_false, List.lookup]
      split <;> simp_all [lookup_replaceField_other name other replacement different rest]

omit [DecidableEq Declaration] in
theorem replaceField_self (name : String) :
    ∀ {fields : List (String × Value Declaration Identity Source)} {found : Value Declaration Identity Source},
      fields.lookup name = some found → replaceField name found fields = fields
  | [], _, present => by simp at present
  | (other, value) :: rest, found, present => by
    by_cases same : other = name
    · subst same
      simp [List.lookup] at present
      subst present
      simp [replaceField]
    · have differentNames : (name == other) = false := by simpa using Ne.symm same
      simp only [List.lookup, differentNames] at present
      simp only [replaceField, same, ite_false, replaceField_self name present]

theorem requireRow_keeps_others (world : MembershipWorld Declaration Identity Source)
    (contracts : ContractDecisions world) :
    ∀ (row : RecordType Declaration) (fields rebuilt : List (String × Value Declaration Identity Source)) (name : String),
      name ∉ rowNames row → requireRow world contracts row fields = some rebuilt →
        rebuilt.lookup name = fields.lookup name
  | .variable id, _, _, _, _, _ => nomatch id
  | .rowEmpty, fields, rebuilt, _, _, required => by
    simp only [requireRow, Option.some.injEq] at required
    rw [required]
  | .rowField other type rest, fields, rebuilt, name, absent, required => by
    simp only [rowNames, List.mem_cons, not_or] at absent
    simp only [requireRow] at required
    cases found : fields.lookup other with
    | none => simp [found] at required
    | some value =>
      cases converted : require world contracts type value with
      | none => simp [found, converted] at required
      | some replacement =>
        simp only [found, converted] at required
        rw [requireRow_keeps_others world contracts rest _ rebuilt name absent.2 required,
          lookup_replaceField_other other name replacement absent.1]

mutual
  theorem require_sound (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      ∀ (type : ValueType Declaration), Admissible type → ∀ (candidate converted : Value Declaration Identity Source),
        require world contracts type candidate = some converted → Membership.value world type converted
    | .variable id, _, _, _, _ => nomatch id
    | .list element, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case list items =>
        cases mapped : items.mapM (require world contracts element) with
        | none => simp [mapped] at required
        | some outputs =>
          simp only [mapped, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
          intro item present
          obtain ⟨input, _, inputRequired⟩ := mapM_some_mem mapped item present
          exact require_sound world contracts element admissible input item inputRequired
      all_goals
        obtain ⟨same, member⟩ := decideMember_some required
        exact same ▸ member
    | .nonEmpty element, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case list items =>
        cases items with
        | nil => simp at required
        | cons first rest =>
          simp only at required
          cases mapped : (first :: rest).mapM (require world contracts element) with
          | none => simp [mapped] at required
          | some outputs =>
            simp only [mapped, Option.map_some, Option.some.injEq] at required
            subst required
            simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
            refine ⟨mapM_some_nonempty mapped, ?_⟩
            intro item present
            obtain ⟨input, _, inputRequired⟩ := mapM_some_mem mapped item present
            exact require_sound world contracts element admissible input item inputRequired
      all_goals
        obtain ⟨same, member⟩ := decideMember_some required
        exact same ▸ member
    | .map keyType element, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at required
      case map entries =>
        cases mapped : entries.mapM (fun entry =>
            letI := Membership.decideKey keyType entry.1
            if Membership.key keyType entry.1 then
              (require world contracts element entry.2).map (fun value => (entry.1, value))
            else none) with
        | none => simp [mapped] at required
        | some outputs =>
          simp only [mapped, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
          intro output present
          obtain ⟨input, _, inputRequired⟩ := mapM_some_mem mapped output present
          letI := Membership.decideKey keyType input.1
          by_cases keyed : Membership.key keyType input.1
          · simp only [keyed, ite_true] at inputRequired
            cases elementRequired : require world contracts element input.2 with
            | none => simp [elementRequired] at inputRequired
            | some value =>
              simp only [elementRequired, Option.map_some, Option.some.injEq] at inputRequired
              subst inputRequired
              exact ⟨keyed, require_sound world contracts element admissible input.2 value elementRequired⟩
          · simp [keyed] at inputRequired
      case record fields =>
        by_cases stringKeys : keyType = .primitive .string
        · simp only [stringKeys, ite_true] at required
          cases mapped : fields.mapM (fun field =>
              (require world contracts element field.2).map (fun value => (field.1, value))) with
          | none => simp [mapped] at required
          | some outputs =>
            simp only [mapped, Option.map_some, Option.some.injEq] at required
            subst required
            simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
            refine ⟨stringKeys, ?_⟩
            intro output present
            obtain ⟨input, _, inputRequired⟩ := mapM_some_mem mapped output present
            cases elementRequired : require world contracts element input.2 with
            | none => simp [elementRequired] at inputRequired
            | some value =>
              simp only [elementRequired, Option.map_some, Option.some.injEq] at inputRequired
              subst inputRequired
              exact require_sound world contracts element admissible input.2 value elementRequired
        · simp [stringKeys] at required
    | .record fields, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case record actual =>
        cases rebuilt : requireRow world contracts fields actual with
        | none => simp [rebuilt] at required
        | some outputs =>
          simp only [rebuilt, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, Membership.recordFields, eraseNominal_rewrap, Value.eraseNominal]
          exact requireRow_sound world contracts fields admissible actual outputs rebuilt
      all_goals
        obtain ⟨same, member⟩ := decideMember_some required
        exact same ▸ member
    | .nominal declaration fields, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case record actual =>
        cases rebuilt : requireRow world contracts fields actual with
        | none => simp [rebuilt] at required
        | some outputs =>
          simp only [rebuilt, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, Membership.recordFields, Value.eraseNominal]
          exact ⟨by trivial, requireRow_sound world contracts fields admissible actual outputs rebuilt⟩
      all_goals
        cases decided : decideMember world contracts (.record fields) candidate with
        | none => simp [decided] at required
        | some same =>
          simp only [decided, Option.map_some, Option.some.injEq] at required
          subst required
          obtain ⟨sameValue, member⟩ := decideMember_some decided
          subst sameValue
          simp only [Membership.value] at member ⊢
          refine ⟨by trivial, ?_⟩
          simpa [Membership.recordFields, Value.eraseNominal_idempotent] using member
    | .optional element, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      letI := Membership.decidePrimitive (Declaration := Declaration) (Identity := Identity) (Source := Source)
        .null candidate
      by_cases isNull : Membership.primitive .null candidate
      · simp only [isNull, ite_true, Option.some.injEq] at required
        subst required
        simp only [Membership.value]
        exact Or.inl isNull
      · simp only [isNull, ite_false] at required
        simp only [Membership.value]
        exact Or.inr (require_sound world contracts element admissible candidate converted required)
    | .union first second rest, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      simp only [Membership.value]
      cases firstRequired : require world contracts first candidate with
      | some value =>
        simp only [firstRequired, Option.some.injEq] at required
        subst required
        exact Or.inl (require_sound world contracts first admissible.1 candidate value firstRequired)
      | none =>
        cases secondRequired : require world contracts second candidate with
        | some value =>
          simp only [firstRequired, secondRequired, Option.some.injEq] at required
          subst required
          exact Or.inr (Or.inl (require_sound world contracts second admissible.2.1 candidate value secondRequired))
        | none =>
          simp only [firstRequired, secondRequired] at required
          exact Or.inr (Or.inr (requireMembers_sound world contracts rest admissible.2.2 candidate converted required))
    | .result success failure, admissible, candidate, converted, required => by
      simp only [Admissible] at admissible
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at required
      case ok item =>
        cases itemRequired : require world contracts success item with
        | none => simp [itemRequired] at required
        | some value =>
          simp only [itemRequired, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
          exact require_sound world contracts success admissible.1 item value itemRequired
      case err item =>
        cases itemRequired : require world contracts failure item with
        | none => simp [itemRequired] at required
        | some value =>
          simp only [itemRequired, Option.map_some, Option.some.injEq] at required
          subst required
          simp only [Membership.value, eraseNominal_rewrap, Value.eraseNominal]
          exact require_sound world contracts failure admissible.2 item value itemRequired
    | .primitive _, _, _, _, required | .set _, _, _, _, required | .stream _, _, _, _, required
    | .module _ _, _, _, _, required | .functionClass _, _, _, _, required | .callable _, _, _, _, required
    | .schemeCallable _, _, _, _, required | .tagged _, _, _, _, required | .errorFamily _, _, _, _, required
    | .errorVariant _ _, _, _, _, required | .errorFacet _, _, _, _, required | .relativePath, _, _, _, required
    | .boundedInteger _, _, _, _, required => by
      simp only [require] at required
      obtain ⟨same, member⟩ := decideMember_some required
      exact same ▸ member

  theorem requireRow_sound (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      ∀ (row : RecordType Declaration), RowAdmissible row →
        ∀ (fields rebuilt : List (String × Value Declaration Identity Source)),
          requireRow world contracts row fields = some rebuilt → Membership.row world row rebuilt
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, _ => by simp [Membership.row]
    | .rowField name type rest, admissible, fields, rebuilt, required => by
      simp only [RowAdmissible] at admissible
      simp only [requireRow] at required
      cases found : fields.lookup name with
      | none => simp [found] at required
      | some value =>
        cases converted : require world contracts type value with
        | none => simp [found, converted] at required
        | some replacement =>
          simp only [found, converted] at required
          simp only [Membership.row]
          refine ⟨?_, requireRow_sound world contracts rest admissible.2.2 _ rebuilt required⟩
          rw [requireRow_keeps_others world contracts rest _ rebuilt name admissible.1 required,
            lookup_replaceField_same name replacement found]
          exact require_sound world contracts type admissible.2.1 value replacement converted

  theorem requireMembers_sound (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (rest : UnionTail Declaration), MembersAdmissible rest →
        ∀ (candidate converted : Value Declaration Identity Source),
          requireMembers world contracts rest candidate = some converted → Membership.members world rest converted
    | .variable id, _, _, _, _ => nomatch id
    | .membersEmpty, _, _, _, required => by simp [requireMembers] at required
    | .member type tail, admissible, candidate, converted, required => by
      simp only [MembersAdmissible] at admissible
      simp only [requireMembers] at required
      simp only [Membership.members]
      cases here : require world contracts type candidate with
      | some value =>
        simp only [here, Option.some.injEq] at required
        subst required
        exact Or.inl (require_sound world contracts type admissible.1 candidate value here)
      | none =>
        simp only [here] at required
        exact Or.inr (requireMembers_sound world contracts tail admissible.2 candidate converted required)
end


mutual
  theorem require_identity (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world) :
      ∀ (type : ValueType Declaration), NominalFree type → ∀ (candidate converted : Value Declaration Identity Source),
        require world contracts type candidate = some converted → converted = candidate
    | .variable id, _, _, _, _ => nomatch id
    | .list element, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case list items =>
        cases mapped : items.mapM (require world contracts element) with
        | none => simp [mapped] at required
        | some outputs =>
          simp only [mapped, Option.map_some, Option.some.injEq] at required
          subst required
          rw [mapM_identity (require_identity world contracts element free) mapped, ← erased, rewrap_erased]
      all_goals exact (decideMember_some required).1
    | .nonEmpty element, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case list items =>
        cases items with
        | nil => simp at required
        | cons first rest =>
          simp only at required
          cases mapped : (first :: rest).mapM (require world contracts element) with
          | none => simp [mapped] at required
          | some outputs =>
            simp only [mapped, Option.map_some, Option.some.injEq] at required
            subst required
            rw [mapM_identity (require_identity world contracts element free) mapped, ← erased, rewrap_erased]
      all_goals exact (decideMember_some required).1
    | .map keyType element, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at required
      case map entries =>
        cases mapped : entries.mapM (fun entry =>
            letI := Membership.decideKey keyType entry.1
            if Membership.key keyType entry.1 then
              (require world contracts element entry.2).map (fun value => (entry.1, value))
            else none) with
        | none => simp [mapped] at required
        | some outputs =>
          simp only [mapped, Option.map_some, Option.some.injEq] at required
          subst required
          have same : outputs = entries := by
            apply mapM_identity _ mapped
            intro input output inputRequired
            letI := Membership.decideKey keyType input.1
            by_cases keyed : Membership.key keyType input.1
            · simp only [keyed, ite_true] at inputRequired
              cases elementRequired : require world contracts element input.2 with
              | none => simp [elementRequired] at inputRequired
              | some value =>
                simp only [elementRequired, Option.map_some, Option.some.injEq] at inputRequired
                subst inputRequired
                rw [require_identity world contracts element free input.2 value elementRequired]
            · simp [keyed] at inputRequired
          rw [same, ← erased, rewrap_erased]
      case record fields =>
        by_cases stringKeys : keyType = .primitive .string
        · simp only [stringKeys, ite_true] at required
          cases mapped : fields.mapM (fun field =>
              (require world contracts element field.2).map (fun value => (field.1, value))) with
          | none => simp [mapped] at required
          | some outputs =>
            simp only [mapped, Option.map_some, Option.some.injEq] at required
            subst required
            have same : outputs = fields := by
              apply mapM_identity _ mapped
              intro input output inputRequired
              cases elementRequired : require world contracts element input.2 with
              | none => simp [elementRequired] at inputRequired
              | some value =>
                simp only [elementRequired, Option.map_some, Option.some.injEq] at inputRequired
                subst inputRequired
                rw [require_identity world contracts element free input.2 value elementRequired]
            rw [same, ← erased, rewrap_erased]
        · simp [stringKeys] at required
    | .record fields, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased] at required
      case record actual =>
        cases rebuilt : requireRow world contracts fields actual with
        | none => simp [rebuilt] at required
        | some outputs =>
          simp only [rebuilt, Option.map_some, Option.some.injEq] at required
          subst required
          rw [requireRow_identity world contracts fields free actual outputs rebuilt, ← erased, rewrap_erased]
      all_goals exact (decideMember_some required).1
    | .nominal _ _, free, _, _, _ => by simp [NominalFree] at free
    | .optional element, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      letI := Membership.decidePrimitive (Declaration := Declaration) (Identity := Identity) (Source := Source)
        .null candidate
      by_cases isNull : Membership.primitive .null candidate
      · simp only [isNull, ite_true, Option.some.injEq] at required
        exact required.symm
      · simp only [isNull, ite_false] at required
        exact require_identity world contracts element free candidate converted required
    | .union first second rest, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases firstRequired : require world contracts first candidate with
      | some value =>
        simp only [firstRequired, Option.some.injEq] at required
        subst required
        exact require_identity world contracts first free.1 candidate value firstRequired
      | none =>
        cases secondRequired : require world contracts second candidate with
        | some value =>
          simp only [firstRequired, secondRequired, Option.some.injEq] at required
          subst required
          exact require_identity world contracts second free.2.1 candidate value secondRequired
        | none =>
          simp only [firstRequired, secondRequired] at required
          exact requireMembers_identity world contracts rest free.2.2 candidate converted required
    | .result success failure, free, candidate, converted, required => by
      simp only [NominalFree] at free
      simp only [require] at required
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at required
      case ok item =>
        cases itemRequired : require world contracts success item with
        | none => simp [itemRequired] at required
        | some value =>
          simp only [itemRequired, Option.map_some, Option.some.injEq] at required
          subst required
          rw [require_identity world contracts success free.1 item value itemRequired, ← erased, rewrap_erased]
      case err item =>
        cases itemRequired : require world contracts failure item with
        | none => simp [itemRequired] at required
        | some value =>
          simp only [itemRequired, Option.map_some, Option.some.injEq] at required
          subst required
          rw [require_identity world contracts failure free.2 item value itemRequired, ← erased, rewrap_erased]
    | .primitive _, _, _, _, required | .set _, _, _, _, required | .stream _, _, _, _, required
    | .module _ _, _, _, _, required | .functionClass _, _, _, _, required | .callable _, _, _, _, required
    | .schemeCallable _, _, _, _, required | .tagged _, _, _, _, required | .errorFamily _, _, _, _, required
    | .errorVariant _ _, _, _, _, required | .errorFacet _, _, _, _, required | .relativePath, _, _, _, required
    | .boundedInteger _, _, _, _, required => by
      simp only [require] at required
      exact (decideMember_some required).1

  theorem requireRow_identity (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (row : RecordType Declaration), RowNominalFree row →
        ∀ (fields rebuilt : List (String × Value Declaration Identity Source)),
          requireRow world contracts row fields = some rebuilt → rebuilt = fields
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, required => by
      simp only [requireRow, Option.some.injEq] at required
      exact required.symm
    | .rowField name type rest, free, fields, rebuilt, required => by
      simp only [RowNominalFree] at free
      simp only [requireRow] at required
      cases found : fields.lookup name with
      | none => simp [found] at required
      | some value =>
        cases converted : require world contracts type value with
        | none => simp [found, converted] at required
        | some replacement =>
          simp only [found, converted] at required
          have same := require_identity world contracts type free.1 value replacement converted
          subst same
          rw [replaceField_self name found] at required
          exact requireRow_identity world contracts rest free.2 fields rebuilt required

  theorem requireMembers_identity (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (rest : UnionTail Declaration), MembersNominalFree rest →
        ∀ (candidate converted : Value Declaration Identity Source),
          requireMembers world contracts rest candidate = some converted → converted = candidate
    | .variable id, _, _, _, _ => nomatch id
    | .membersEmpty, _, _, _, required => by simp [requireMembers] at required
    | .member type tail, free, candidate, converted, required => by
      simp only [MembersNominalFree] at free
      simp only [requireMembers] at required
      cases here : require world contracts type candidate with
      | some value =>
        simp only [here, Option.some.injEq] at required
        subst required
        exact require_identity world contracts type free.1 candidate value here
      | none =>
        simp only [here] at required
        exact requireMembers_identity world contracts tail free.2 candidate converted required
end

mutual
  theorem require_of_member (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (type : ValueType Declaration), NominalFree type → ∀ (candidate : Value Declaration Identity Source),
        Membership.value world type candidate → require world contracts type candidate = some candidate
    | .variable id, _, _, _ => nomatch id
    | .list element, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      have original := member
      simp only [Membership.value] at member
      cases erased : candidate.eraseNominal <;> simp only [erased] at member ⊢
      case list items =>
        rw [mapM_of_identity (fun item present => require_of_member world contracts element free item (member item present))]
        simp only [Option.map_some]
        rw [← erased, rewrap_erased]
      all_goals first | exact member.elim | exact decideMember_of_member original
    | .nonEmpty element, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      have original := member
      simp only [Membership.value] at member
      cases erased : candidate.eraseNominal <;> simp only [erased] at member ⊢
      case list items =>
        cases items with
        | nil => exact absurd rfl member.1
        | cons first rest =>
          simp only
          rw [mapM_of_identity (fun item present => require_of_member world contracts element free item (member.2 item present))]
          simp only [Option.map_some]
          rw [← erased, rewrap_erased]
      all_goals first | exact member.elim | exact decideMember_of_member original
    | .map keyType element, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      simp only [Membership.value] at member
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at member ⊢
      case map entries =>
        rw [mapM_of_identity]
        · simp only [Option.map_some]
          rw [← erased, rewrap_erased]
        · intro entry present
          letI := Membership.decideKey keyType entry.1
          simp only [(member entry present).1, ite_true,
            require_of_member world contracts element free entry.2 (member entry present).2, Option.map_some]
      case record fields =>
        simp only [member.1, ite_true]
        rw [mapM_of_identity]
        · simp only [Option.map_some]
          rw [← erased, rewrap_erased]
        · intro field present
          simp only [require_of_member world contracts element free field.2 (member.2 field present), Option.map_some]
      all_goals exact member.elim
    | .record fields, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      have original := member
      simp only [Membership.value, Membership.recordFields] at member
      cases erased : candidate.eraseNominal <;> simp only [erased] at member ⊢
      case record actual =>
        rw [requireRow_of_member world contracts fields free actual member]
        simp only [Option.map_some]
        rw [← erased, rewrap_erased]
      all_goals first | exact member.elim | exact decideMember_of_member original
    | .nominal _ _, free, _, _ => by simp [NominalFree] at free
    | .optional element, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      letI := Membership.decidePrimitive (Declaration := Declaration) (Identity := Identity) (Source := Source)
        .null candidate
      by_cases isNull : Membership.primitive .null candidate
      · simp [isNull]
      · simp only [isNull, ite_false]
        simp only [Membership.value] at member
        rcases member with nullMember | elementMember
        · exact absurd nullMember isNull
        · exact require_of_member world contracts element free candidate elementMember
    | .union first second rest, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      simp only [Membership.value] at member
      cases firstRequired : require world contracts first candidate with
      | some value =>
        rw [require_identity world contracts first free.1 candidate value firstRequired]
      | none =>
        cases secondRequired : require world contracts second candidate with
        | some value =>
          simp only
          rw [require_identity world contracts second free.2.1 candidate value secondRequired]
        | none =>
          simp only
          rcases member with firstMember | secondMember | restMember
          · rw [require_of_member world contracts first free.1 candidate firstMember] at firstRequired
            cases firstRequired
          · rw [require_of_member world contracts second free.2.1 candidate secondMember] at secondRequired
            cases secondRequired
          · exact requireMembers_of_member world contracts rest free.2.2 candidate restMember
    | .result success failure, free, candidate, member => by
      simp only [NominalFree] at free
      simp only [require]
      simp only [Membership.value] at member
      cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq] at member ⊢
      case ok item =>
        rw [require_of_member world contracts success free.1 item member]
        simp only [Option.map_some]
        rw [← erased, rewrap_erased]
      case err item =>
        rw [require_of_member world contracts failure free.2 item member]
        simp only [Option.map_some]
        rw [← erased, rewrap_erased]
      all_goals exact member.elim
    | .primitive _, _, _, member | .set _, _, _, member | .stream _, _, _, member
    | .module _ _, _, _, member | .functionClass _, _, _, member | .callable _, _, _, member
    | .schemeCallable _, _, _, member | .tagged _, _, _, member | .errorFamily _, _, _, member
    | .errorVariant _ _, _, _, member | .errorFacet _, _, _, member | .relativePath, _, _, member
    | .boundedInteger _, _, _, member => by
      simp only [require]
      exact decideMember_of_member member

  theorem requireRow_of_member (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (row : RecordType Declaration), RowNominalFree row → ∀ (fields : List (String × Value Declaration Identity Source)),
        Membership.row world row fields → requireRow world contracts row fields = some fields
    | .variable id, _, _, _ => nomatch id
    | .rowEmpty, _, _, _ => by simp [requireRow]
    | .rowField name type rest, free, fields, member => by
      simp only [RowNominalFree] at free
      simp only [Membership.row] at member
      simp only [requireRow]
      cases found : fields.lookup name with
      | none => simp [found] at member
      | some value =>
        simp only [found] at member
        simp only [require_of_member world contracts type free.1 value member.1]
        rw [replaceField_self name found]
        exact requireRow_of_member world contracts rest free.2 fields member.2

  theorem requireMembers_of_member (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) :
      ∀ (rest : UnionTail Declaration), MembersNominalFree rest → ∀ (candidate : Value Declaration Identity Source),
        Membership.members world rest candidate → requireMembers world contracts rest candidate = some candidate
    | .variable id, _, _, _ => nomatch id
    | .membersEmpty, _, _, member => by simp [Membership.members] at member
    | .member type tail, free, candidate, member => by
      simp only [MembersNominalFree] at free
      simp only [requireMembers]
      simp only [Membership.members] at member
      cases here : require world contracts type candidate with
      | some value =>
        rw [require_identity world contracts type free.1 candidate value here]
      | none =>
        simp only
        rcases member with hereMember | tailMember
        · rw [require_of_member world contracts type free.1 candidate hereMember] at here
          cases here
        · exact requireMembers_of_member world contracts tail free.2 candidate tailMember
end

theorem require_nominal (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (declaration : Declaration) (fields : RecordType Declaration) (candidate : Value Declaration Identity Source) :
    require world contracts (.nominal declaration fields) candidate =
      (require world contracts (.record fields) candidate).map
        (fun accepted => .nominal declaration accepted.eraseNominal) := by
  simp only [require]
  cases erased : candidate.eraseNominal <;> simp only [erased]
  case record actual =>
    cases rebuilt : requireRow world contracts fields actual <;> simp [eraseNominal_rewrap, Value.eraseNominal]

theorem require_union_order (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (first second : ValueType Declaration) (rest : UnionTail Declaration)
    (candidate converted : Value Declaration Identity Source) :
    require world contracts (.union first second rest) candidate = some converted ↔
      require world contracts first candidate = some converted ∨
        (require world contracts first candidate = none ∧
          (require world contracts second candidate = some converted ∨
            (require world contracts second candidate = none ∧
              requireMembers world contracts rest candidate = some converted))) := by
  simp only [require]
  cases firstRequired : require world contracts first candidate <;>
    cases secondRequired : require world contracts second candidate <;> simp

/-- Recursive `.require` over every Core type, value and world. On an
admissible target (record schemas list each field once), an accepted result is
a member of the target. On a target without nominal positions, `.require`
accepts exactly the members and returns them unchanged. A nominal target
checks the fields as its structural schema does and introduces the declaration
on the accepted record, which recursive record, list, map, optional, union and
result targets do at every nested nominal slot. A union asks its members in
written order and returns the first acceptance. A rejection anywhere in the
value rejects the whole value, which `.require` returns as recoverable `Err`
data rather than raising. -/
theorem recursive_require (world : MembershipWorld Declaration Identity Source)
    (contracts : ContractDecisions world) :
    (∀ (type : ValueType Declaration) candidate converted, Admissible type →
      require world contracts type candidate = some converted → Membership.value world type converted) ∧
    (∀ (type : ValueType Declaration) candidate converted, NominalFree type → Admissible type →
      (require world contracts type candidate = some converted ↔
        Membership.value world type candidate ∧ converted = candidate)) ∧
    (∀ declaration (fields : RecordType Declaration) candidate,
      require world contracts (.nominal declaration fields) candidate =
        (require world contracts (.record fields) candidate).map
          (fun accepted => .nominal declaration accepted.eraseNominal)) ∧
    (∀ (first second : ValueType Declaration) rest candidate converted,
      require world contracts (.union first second rest) candidate = some converted ↔
        require world contracts first candidate = some converted ∨
          (require world contracts first candidate = none ∧
            (require world contracts second candidate = some converted ∨
              (require world contracts second candidate = none ∧
                requireMembers world contracts rest candidate = some converted)))) ∧
    (∀ (schemaError : Value Declaration Identity Source) type candidate,
      (require world contracts type candidate = none ↔
        requireResult world contracts schemaError type candidate = .err schemaError) ∧
      ∀ converted, require world contracts type candidate = some converted ↔
        requireResult world contracts schemaError type candidate = .ok converted) := by
  refine ⟨fun type candidate converted admissible required =>
      require_sound world contracts type admissible candidate converted required, ?_,
    require_nominal world contracts, require_union_order world contracts, ?_⟩
  · intro type candidate converted free admissible
    constructor
    · intro required
      have same := require_identity world contracts type free candidate converted required
      subst same
      exact ⟨require_sound world contracts type admissible _ _ required, rfl⟩
    · rintro ⟨member, rfl⟩
      exact require_of_member world contracts type free converted member
  · intro schemaError type candidate
    unfold requireResult
    cases require world contracts type candidate <;> simp


namespace Witness

open Xsh.Validation.Core.Widen.Witness
open Xsh.Validation.Core.Union.Witness

def nameField : RecordType Unit := .rowField "name" (.primitive .string) .rowEmpty
def packageType : WitnessType := .nominal () nameField
def manifestType : WitnessType := .record (.rowField "pkg" packageType .rowEmpty)
def erasedManifestType : WitnessType := .record (.rowField "pkg" (.record nameField) .rowEmpty)

def nameValue : WitnessValue := .scalar (.string "x")
def packageRecord : WitnessValue := .record [("name", nameValue)]
def extra : WitnessValue := .scalar (.integer ⟨1, by decide⟩)
def manifest : WitnessValue := .record [("pkg", packageRecord), ("extra", extra)]
def badManifest : WitnessValue := .record [("pkg", .record [("name", extra)]), ("extra", extra)]
def schemaError : WitnessValue := .scalar (.string "schema")

theorem name_accepted : require openWorld openContracts (.primitive .string) nameValue = some nameValue := by
  simp only [require]
  exact decideMember_of_member (by simp [nameValue, Membership.value, Membership.primitive, Value.eraseNominal])

theorem name_rejects_integer : require openWorld openContracts (.primitive .string) extra = none := by
  simp only [require, decideMember]
  simp [extra, Membership.value, Membership.primitive, Value.eraseNominal]

theorem nested_nominal_slot_is_introduced_and_extra_fields_kept :
    require openWorld openContracts manifestType manifest =
      some (.record [("pkg", .nominal () (.record [("name", nameValue)])), ("extra", extra)]) := by
  simp [require, requireRow, manifestType, packageType, nameField, manifest, packageRecord, List.lookup,
    replaceField, name_accepted, rewrap, peel, wrap, Value.eraseNominal]

theorem nested_failure_publishes_nothing :
    require openWorld openContracts manifestType badManifest = none ∧
      requireResult openWorld openContracts schemaError manifestType badManifest = .err schemaError := by
  have rejected : require openWorld openContracts manifestType badManifest = none := by
    simp [require, requireRow, manifestType, packageType, nameField, badManifest, List.lookup,
      name_rejects_integer, Value.eraseNominal]
  exact ⟨rejected, by simp [requireResult, rejected]⟩

theorem unsigned_require_is_recoverable :
    requireResult openWorld openContracts schemaError (.primitive .unsignedInteger) minusFive = .err schemaError ∧
      requireResult openWorld openContracts schemaError (.primitive .unsignedInteger) five = .ok five := by
  have rejected : require openWorld openContracts (.primitive .unsignedInteger) minusFive = none := by
    simp only [require, decideMember]
    simp [minusFive, Membership.value, Membership.primitive, Value.eraseNominal]
  have accepted : require openWorld openContracts (.primitive .unsignedInteger) five = some five := by
    simp only [require]
    exact decideMember_of_member (by simp [five, Membership.value, Membership.primitive, Value.eraseNominal])
  exact ⟨by simp [requireResult, rejected], by simp [requireResult, accepted]⟩

theorem union_order_decides_conversion :
    require openWorld openContracts (.union packageType (.record nameField) .membersEmpty) packageRecord =
        some (.nominal () packageRecord) ∧
      require openWorld openContracts (.union (.record nameField) packageType .membersEmpty) packageRecord =
        some packageRecord := by
  constructor <;>
    simp [require, requireRow, packageType, nameField, packageRecord, List.lookup, replaceField, name_accepted,
      rewrap, peel, wrap, Value.eraseNominal]

theorem check_without_conversion_is_detected :
    decideMember openWorld openContracts erasedManifestType manifest = some manifest ∧
      ¬ Membership.value openWorld manifestType manifest := by
  constructor
  · apply decideMember_of_member
    simp [erasedManifestType, nameField, manifest, packageRecord, nameValue, Membership.value, Membership.row,
      Membership.recordFields, Membership.primitive, Value.eraseNominal, List.lookup]
  · simp [manifestType, packageType, nameField, manifest, packageRecord, Membership.value, Membership.row,
      Membership.recordFields, Value.eraseNominal, List.lookup]

end Witness

end Xsh.Validation.Core.Require
