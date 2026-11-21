import Xsh.Validation.Core.Require

namespace Xsh.Validation.Core.Nominal

open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Validation.Core
open Xsh.Validation.Core.Require
open Xsh.Validation.Core.WellFormed

variable {Declaration Identity Source : Type} [DecidableEq Declaration]

mutual
  def nominals : Value Declaration Identity Source → List Declaration
    | .nominal declaration contents => declaration :: nominals contents
    | .list items => nominalsOf items
    | .tagged _ _ items => nominalsOf items
    | .record fields => nominalsOfFields fields
    | .module _ fields => nominalsOfFields fields
    | .map entries => nominalsOfEntries entries
    | .ok contents | .err contents => nominals contents
    | .error ⟨_, _, _, _, payload, _, _, _, cause, _⟩ => nominalsOfFields payload ++ nominalsOfCause cause
    | .processError ⟨_, _, _, _, _, cause⟩ => nominalsOfCause cause
    | _ => []
  termination_by candidate => sizeOf candidate

  def nominalsOf : List (Value Declaration Identity Source) → List Declaration
    | [] => []
    | item :: rest => nominals item ++ nominalsOf rest
  termination_by items => sizeOf items

  def nominalsOfFields : List (String × Value Declaration Identity Source) → List Declaration
    | [] => []
    | (_, item) :: rest => nominals item ++ nominalsOfFields rest
  termination_by fields => sizeOf fields

  def nominalsOfEntries : List (MapKey × Value Declaration Identity Source) → List Declaration
    | [] => []
    | (_, item) :: rest => nominals item ++ nominalsOfEntries rest
  termination_by entries => sizeOf entries

  def nominalsOfCause : Option (Value Declaration Identity Source) → List Declaration
    | none => []
    | some item => nominals item
  termination_by cause => sizeOf cause
end

mutual
  def typeNominals : ValueType Declaration → List Declaration
    | .list element | .nonEmpty element | .optional element | .map _ element => typeNominals element
    | .record fields => rowNominals fields
    | .nominal declaration fields => declaration :: rowNominals fields
    | .union first second rest => typeNominals first ++ typeNominals second ++ membersNominals rest
    | .result success failure => typeNominals success ++ typeNominals failure
    | _ => []

  def rowNominals : RecordType Declaration → List Declaration
    | .rowField _ type rest => typeNominals type ++ rowNominals rest
    | _ => []

  def membersNominals : UnionTail Declaration → List Declaration
    | .member type rest => typeNominals type ++ membersNominals rest
    | _ => []
end

inductive Form (Declaration : Type) where
  | scalar (value : Scalar)
  | list (items : List (Form Declaration))
  | record (fields : List (String × Form Declaration))
  | construct (declaration : Declaration) (schema : RecordType Declaration) (fields : List (String × Form Declaration))
  | require (target : ValueType Declaration) (source : Form Declaration)
  | read (cell : CellId)
  | field (source : Form Declaration) (name : String)
  | spread (source : Form Declaration) (updates : List (String × Form Declaration))

mutual
  def introductions : Form Declaration → List Declaration
    | .scalar _ | .read _ => []
    | .list items => introductionsOf items
    | .record fields => introductionsOfFields fields
    | .construct declaration _ fields => declaration :: introductionsOfFields fields
    | .require target source => typeNominals target ++ introductions source
    | .field source _ => introductions source
    | .spread source updates => introductions source ++ introductionsOfFields updates

  def introductionsOf : List (Form Declaration) → List Declaration
    | [] => []
    | item :: rest => introductions item ++ introductionsOf rest

  def introductionsOfFields : List (String × Form Declaration) → List Declaration
    | [] => []
    | (_, item) :: rest => introductions item ++ introductionsOfFields rest
end

abbrev Heap (Declaration Identity Source : Type) := CellId → Option (Value Declaration Identity Source)

mutual
  def evaluate (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
      (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) : Form Declaration → Option (Value Declaration Identity Source)
    | .scalar value => some (.scalar value)
    | .list items => (evaluateAll world contracts enums heap items).map .list
    | .record fields => (evaluateFields world contracts enums heap fields).map .record
    | .construct declaration schema fields =>
      match evaluateFields world contracts enums heap fields with
      | none => none
      | some values =>
        letI := Membership.decideTerm world contracts schema values
        if Membership.row world schema values then some (.nominal declaration (.record values)) else none
    | .require target source =>
      match evaluate world contracts enums heap source with
      | none => none
      | some value => (convert ⟨world, contracts, enums⟩ .keep [] target value).toOption
    | .read cell => heap cell
    | .field source name =>
      match evaluate world contracts enums heap source with
      | none => none
      | some value =>
        match value.eraseNominal with
        | .record fields => fields.lookup name
        | _ => none
    | .spread source updates =>
      match evaluate world contracts enums heap source with
      | none => none
      | some value =>
        match value.eraseNominal with
        | .record fields => (evaluateFields world contracts enums heap updates).map (fun replaced => .record (replaced ++ fields))
        | _ => none

  def evaluateAll (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
      (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) : List (Form Declaration) → Option (List (Value Declaration Identity Source))
    | [] => some []
    | item :: rest =>
      match evaluate world contracts enums heap item, evaluateAll world contracts enums heap rest with
      | some value, some values => some (value :: values)
      | _, _ => none

  def evaluateFields (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
      (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) :
      List (String × Form Declaration) → Option (List (String × Value Declaration Identity Source))
    | [] => some []
    | (name, item) :: rest =>
      match evaluate world contracts enums heap item, evaluateFields world contracts enums heap rest with
      | some value, some values => some ((name, value) :: values)
      | _, _ => none
end


def Bound (allowed : Declaration → Prop) (candidate : Value Declaration Identity Source) : Prop :=
  ∀ declaration ∈ nominals candidate, allowed declaration

omit [DecidableEq Declaration] in
theorem mem_nominalsOf {declaration : Declaration} :
    ∀ items : List (Value Declaration Identity Source),
      declaration ∈ nominalsOf items ↔ ∃ item ∈ items, declaration ∈ nominals item
  | [] => by simp [nominalsOf]
  | item :: rest => by simp [nominalsOf, mem_nominalsOf rest]

omit [DecidableEq Declaration] in
theorem mem_nominalsOfFields {declaration : Declaration} :
    ∀ fields : List (String × Value Declaration Identity Source),
      declaration ∈ nominalsOfFields fields ↔ ∃ field ∈ fields, declaration ∈ nominals field.2
  | [] => by simp [nominalsOfFields]
  | (name, item) :: rest => by simp [nominalsOfFields, mem_nominalsOfFields rest]

omit [DecidableEq Declaration] in
theorem mem_nominalsOfEntries {declaration : Declaration} :
    ∀ entries : List (MapKey × Value Declaration Identity Source),
      declaration ∈ nominalsOfEntries entries ↔ ∃ entry ∈ entries, declaration ∈ nominals entry.2
  | [] => by simp [nominalsOfEntries]
  | (key, item) :: rest => by simp [nominalsOfEntries, mem_nominalsOfEntries rest]

omit [DecidableEq Declaration] in
theorem bound_list {allowed : Declaration → Prop} {items : List (Value Declaration Identity Source)} :
    Bound allowed (.list items) ↔ ∀ item ∈ items, Bound allowed item := by
  simp only [Bound, nominals, mem_nominalsOf]
  constructor
  · intro bound item present declaration found
    exact bound declaration ⟨item, present, found⟩
  · rintro bound declaration ⟨item, present, found⟩
    exact bound item present declaration found

omit [DecidableEq Declaration] in
theorem bound_record {allowed : Declaration → Prop} {fields : List (String × Value Declaration Identity Source)} :
    Bound allowed (.record fields) ↔ ∀ field ∈ fields, Bound allowed field.2 := by
  simp only [Bound, nominals, mem_nominalsOfFields]
  constructor
  · intro bound field present declaration found
    exact bound declaration ⟨field, present, found⟩
  · rintro bound declaration ⟨field, present, found⟩
    exact bound field present declaration found

omit [DecidableEq Declaration] in
theorem bound_map {allowed : Declaration → Prop} {entries : List (MapKey × Value Declaration Identity Source)} :
    Bound allowed (.map entries) ↔ ∀ entry ∈ entries, Bound allowed entry.2 := by
  simp only [Bound, nominals, mem_nominalsOfEntries]
  constructor
  · intro bound entry present declaration found
    exact bound declaration ⟨entry, present, found⟩
  · rintro bound declaration ⟨entry, present, found⟩
    exact bound entry present declaration found

omit [DecidableEq Declaration] in
theorem bound_eraseNominal {allowed : Declaration → Prop} :
    ∀ {candidate : Value Declaration Identity Source}, Bound allowed candidate → Bound allowed candidate.eraseNominal
  | .nominal declaration contents, bound => by
    simp only [Value.eraseNominal]
    exact bound_eraseNominal (fun found present => bound found (by simp [nominals, present]))
  | .scalar _, bound | .list _, bound | .map _, bound | .set _, bound | .stream _, bound | .record _, bound
  | .filesystemEntry _, bound | .module _ _, bound | .ok _, bound | .err _, bound | .status _, bound
  | .environmentPaths _, bound | .error _, bound | .processError _, bound | .callable _ _, bound
  | .command _, bound | .processHandle _, bound | .networkJob _, bound | .filesystemRoot _, bound
  | .filesystemLock _, bound | .sshSession _, bound | .tagged _ _ _, bound => bound

omit [DecidableEq Declaration] in
theorem peel_bound {allowed : Declaration → Prop} :
    ∀ {candidate : Value Declaration Identity Source}, Bound allowed candidate →
      ∀ declaration ∈ (peel candidate).1, allowed declaration
  | .nominal declaration contents, bound => by
    intro found present
    simp only [peel, List.mem_cons] at present
    rcases present with same | later
    · exact bound found (by simp [nominals, same])
    · exact peel_bound (candidate := contents) (fun other otherPresent => bound other (by simp [nominals, otherPresent]))
        found later
  | .scalar _, _ | .list _, _ | .map _, _ | .set _, _ | .stream _, _ | .record _, _
  | .filesystemEntry _, _ | .module _ _, _ | .ok _, _ | .err _, _ | .status _, _
  | .environmentPaths _, _ | .error _, _ | .processError _, _ | .callable _ _, _
  | .command _, _ | .processHandle _, _ | .networkJob _, _ | .filesystemRoot _, _
  | .filesystemLock _, _ | .sshSession _, _ | .tagged _ _ _, _ => by simp [peel]

omit [DecidableEq Declaration] in
theorem bound_wrap {allowed : Declaration → Prop} :
    ∀ (declarations : List Declaration) {candidate : Value Declaration Identity Source},
      (∀ declaration ∈ declarations, allowed declaration) → Bound allowed candidate →
        Bound allowed (wrap declarations candidate)
  | [], _, _, bound => bound
  | head :: rest, candidate, declarations, bound => by
    intro found present
    simp only [wrap, nominals, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ declarations head (by simp)
    · exact bound_wrap rest (fun other otherPresent => declarations other (by simp [otherPresent])) bound found later

omit [DecidableEq Declaration] in
theorem bound_rewrap {allowed : Declaration → Prop} {original rebuilt : Value Declaration Identity Source}
    (originalBound : Bound allowed original) (rebuiltBound : Bound allowed rebuilt) :
    Bound allowed (rewrap original rebuilt) :=
  bound_wrap _ (peel_bound originalBound) rebuiltBound

omit [DecidableEq Declaration] in
theorem bound_lookup {allowed : Declaration → Prop} {fields : List (String × Value Declaration Identity Source)}
    {name : String} {found : Value Declaration Identity Source} (bound : ∀ field ∈ fields, Bound allowed field.2)
    (present : fields.lookup name = some found) : Bound allowed found := by
  induction fields with
  | nil => simp at present
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    by_cases same : name = other
    · subst same
      simp [List.lookup] at present
      subst present
      exact bound (name, value) (by simp)
    · have differentNames : (name == other) = false := by simpa using same
      simp only [List.lookup, differentNames] at present
      exact ih (fun field fieldPresent => bound field (List.mem_cons_of_mem _ fieldPresent)) present

omit [DecidableEq Declaration] in
theorem bound_replaceField {allowed : Declaration → Prop} (name : String) {replacement : Value Declaration Identity Source}
    (replacementBound : Bound allowed replacement) :
    ∀ {fields : List (String × Value Declaration Identity Source)}, (∀ field ∈ fields, Bound allowed field.2) →
      ∀ field ∈ replaceField name replacement fields, Bound allowed field.2
  | [], _ => by simp [replaceField]
  | (other, value) :: rest, bound => by
    intro field present
    by_cases same : other = name
    · simp only [replaceField, same, ite_true, List.mem_cons] at present
      rcases present with here | later
      · subst here
        exact replacementBound
      · exact bound field (List.mem_cons_of_mem _ later)
    · simp only [replaceField, same, ite_false, List.mem_cons] at present
      rcases present with here | later
      · exact bound field (by simp [here])
      · exact bound_replaceField name replacementBound (fun other otherPresent => bound other
          (List.mem_cons_of_mem _ otherPresent)) field later

omit [DecidableEq Declaration] in
theorem items_bound {allowed : Declaration → Prop}
    {step : Nat → Value Declaration Identity Source → Outcome Declaration Identity Source}
    (stepBound : ∀ position item converted, Bound allowed item → step position item = .ok converted →
      Bound allowed converted) :
    ∀ {position : Nat} {values results : List (Value Declaration Identity Source)},
      (∀ value ∈ values, Bound allowed value) → convertItems step position values = .ok results →
        ∀ result ∈ results, Bound allowed result
  | _, [], results, _, converted => by
    simp only [convertItems, Except.ok.injEq] at converted
    subst converted
    simp
  | position, item :: rest, results, bound, converted => by
    simp only [convertItems] at converted
    split at converted
    · simp at converted
    · rename_i head headConverted
      split at converted
      · simp at converted
      · rename_i others othersConverted
        simp only [Except.ok.injEq] at converted
        subst converted
        intro result present
        simp only [List.mem_cons] at present
        rcases present with same | later
        · exact same ▸ stepBound _ _ _ (bound item (by simp)) headConverted
        · exact items_bound stepBound (fun value inside => bound value (List.mem_cons_of_mem _ inside)) othersConverted
            result later

omit [DecidableEq Declaration] in
theorem entries_bound {allowed : Declaration → Prop}
    {step : MapKey → Value Declaration Identity Source → Outcome Declaration Identity Source}
    (stepBound : ∀ key value converted, Bound allowed value → step key value = .ok converted → Bound allowed converted) :
    ∀ {entries results : List (MapKey × Value Declaration Identity Source)},
      (∀ entry ∈ entries, Bound allowed entry.2) → convertEntries step entries = .ok results →
        ∀ result ∈ results, Bound allowed result.2
  | [], results, _, converted => by
    simp only [convertEntries, Except.ok.injEq] at converted
    subst converted
    simp
  | (key, value) :: rest, results, bound, converted => by
    simp only [convertEntries] at converted
    split at converted
    · simp at converted
    · rename_i head headConverted
      split at converted
      · simp at converted
      · rename_i others othersConverted
        simp only [Except.ok.injEq] at converted
        subst converted
        intro result present
        simp only [List.mem_cons] at present
        rcases present with same | later
        · exact same ▸ stepBound _ _ _ (bound (key, value) (by simp)) headConverted
        · exact entries_bound stepBound (fun entry inside => bound entry (List.mem_cons_of_mem _ inside)) othersConverted
            result later

omit [DecidableEq Declaration] in
theorem listItems_bound {allowed : Declaration → Prop} {candidate : Value Declaration Identity Source}
    {values : List (Value Declaration Identity Source)} (bound : Bound allowed candidate)
    (listed : listItems candidate = some values) : ∀ value ∈ values, Bound allowed value := by
  have erasedBound := bound_eraseNominal bound
  unfold listItems at listed
  split at listed
  · rename_i items erased
    simp only [Option.some.injEq] at listed
    subst listed
    rw [erased] at erasedBound
    exact bound_list.mp erasedBound
  · rename_i paths erased
    simp only [Option.some.injEq] at listed
    subst listed
    intro value present
    obtain ⟨path, _, rfl⟩ := List.mem_map.mp present
    simp [Bound, nominals]
  · simp at listed

omit [DecidableEq Declaration] in
theorem fieldsOf_bound {allowed : Declaration → Prop} {boundary : Boundary Declaration Identity Source}
    {policy : FieldPolicy} {candidate : Value Declaration Identity Source}
    {fields : List (String × Value Declaration Identity Source)}
    (entries : policy ≠ .keep → ∀ reference, ∀ field ∈ boundary.world.entryFields reference, Bound allowed field.2)
    (bound : Bound allowed candidate) (fielded : fieldsOf boundary policy candidate = some fields) :
    ∀ field ∈ fields, Bound allowed field.2 := by
  have erasedBound := bound_eraseNominal bound
  unfold fieldsOf at fielded
  split at fielded
  · rename_i present erased
    simp only [Option.some.injEq] at fielded
    subst fielded
    rw [erased] at erasedBound
    exact bound_record.mp erasedBound
  · rename_i reference erased
    split at fielded
    · simp at fielded
    · rename_i notKeep
      simp only [Option.some.injEq] at fielded
      subst fielded
      exact entries (by cases policy <;> simp_all) reference
  · simp at fielded

omit [DecidableEq Declaration] in
theorem publish_bound {allowed : Declaration → Prop} {policy : FieldPolicy}
    {candidate : Value Declaration Identity Source} {fields : List (String × Value Declaration Identity Source)}
    (bound : Bound allowed candidate) (fieldsBound : ∀ field ∈ fields, Bound allowed field.2) :
    Bound allowed (publish policy candidate fields) := by
  unfold publish
  split
  · exact bound_record.mpr fieldsBound
  · exact bound_rewrap bound (bound_record.mpr fieldsBound)
  · exact bound_record.mpr fieldsBound

omit [DecidableEq Declaration] in
theorem recordBody_bound {allowed : Declaration → Prop} {policy : FieldPolicy} {path : List PathStep}
    {names : List String}
    {keep project : List (String × Value Declaration Identity Source) →
      Except Failure (List (String × Value Declaration Identity Source))}
    (keepBound : ∀ fields results, (∀ field ∈ fields, Bound allowed field.2) → keep fields = .ok results →
      ∀ field ∈ results, Bound allowed field.2)
    (projectBound : ∀ fields results, (∀ field ∈ fields, Bound allowed field.2) → project fields = .ok results →
      ∀ field ∈ results, Bound allowed field.2)
    {fields results : List (String × Value Declaration Identity Source)} (bound : ∀ field ∈ fields, Bound allowed field.2)
    (converted : recordBody policy path names keep project fields = .ok results) :
    ∀ field ∈ results, Bound allowed field.2 := by
  cases policy with
  | keep => exact keepBound _ _ bound converted
  | stripped => exact projectBound _ _ bound converted
  | exact =>
    simp only [recordBody] at converted
    split at converted
    · simp at converted
    · exact keepBound _ _ bound converted

omit [DecidableEq Declaration] in
theorem toOption_some {α ε : Type} {outcome : Except ε α} {value : α} (some' : outcome.toOption = some value) :
    outcome = .ok value := by
  cases outcome <;> simp_all [Except.toOption]

mutual
  theorem convert_bound (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
      (allowed : Declaration → Prop)
      (entries : policy ≠ .keep → ∀ reference, ∀ field ∈ boundary.world.entryFields reference, Bound allowed field.2) :
      ∀ (type : ValueType Declaration), (∀ declaration ∈ typeNominals type, allowed declaration) →
        ∀ (path : List PathStep) (candidate converted : Value Declaration Identity Source), Bound allowed candidate →
          convert boundary policy path type candidate = .ok converted → Bound allowed converted
    | .variable id, _, _, _, _, _, _ => nomatch id
    | .list element, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals] at typeBound
      simp only [convert] at required
      split at required
      · rename_i values listed
        cases items : convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item)
            0 values with
        | error failure => simp [items, Except.map] at required
        | ok results =>
          simp only [items, Except.map, Except.ok.injEq] at required
          subst required
          exact bound_list.mpr (items_bound (fun position item result itemBound itemConverted =>
            convert_bound boundary policy allowed entries element typeBound _ item result itemBound itemConverted)
            (listItems_bound bound listed) items)
      · simp at required
    | .nonEmpty element, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals] at typeBound
      simp only [convert] at required
      split at required
      · rename_i first rest listed
        cases items : convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item)
            0 (first :: rest) with
        | error failure => simp [items, Except.map] at required
        | ok results =>
          simp only [items, Except.map, Except.ok.injEq] at required
          subst required
          exact bound_list.mpr (items_bound (fun position item result itemBound itemConverted =>
            convert_bound boundary policy allowed entries element typeBound _ item result itemBound itemConverted)
            (listItems_bound bound listed) items)
      · simp at required
    | .map keyType element, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals] at typeBound
      have erasedBound := bound_eraseNominal bound
      simp only [convert] at required
      split at required
      · rename_i present erased
        rw [erased] at erasedBound
        cases mapped : convertEntries (fun key value =>
            if keyAdmits keyType key then convert boundary policy (path ++ [.key key]) element value
            else .error ⟨path, .keyMismatch key⟩) present with
        | error failure => simp [mapped, Except.map] at required
        | ok results =>
          simp only [mapped, Except.map, Except.ok.injEq] at required
          subst required
          refine bound_map.mpr (entries_bound ?_ (bound_map.mp erasedBound) mapped)
          intro key value result valueBound stepped
          split at stepped
          · exact convert_bound boundary policy allowed entries element typeBound _ value result valueBound stepped
          · simp at stepped
      · rename_i fields erased
        rw [erased] at erasedBound
        split at required
        · cases mapped : convertEntries (fun key value => convert boundary policy (path ++ [.key key]) element value)
              (fields.map recordEntry) with
          | error failure => simp [mapped, Except.map] at required
          | ok results =>
            simp only [mapped, Except.map, Except.ok.injEq] at required
            subst required
            refine bound_map.mpr (entries_bound (fun key value result valueBound stepped =>
              convert_bound boundary policy allowed entries element typeBound _ value result valueBound stepped) ?_ mapped)
            intro entry present
            obtain ⟨field, fieldPresent, rfl⟩ := List.mem_map.mp present
            exact bound_record.mp erasedBound field fieldPresent
        · simp at required
      · simp at required
    | .set element, _, path, candidate, converted, bound, required => by
      simp only [convert] at required
      split at required
      · exact (checkMember_ok.mp required).2 ▸ bound
      · split at required
        · rename_i values listed
          cases keys : setKeys element path 0 [] values with
          | error failure => simp [keys, Except.map] at required
          | ok found =>
            simp only [keys, Except.map, Except.ok.injEq] at required
            subst required
            simp [Bound, nominals]
        · simp at required
    | .record row, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals] at typeBound
      simp only [convert] at required
      split at required
      · rename_i fields fielded
        have fieldsBound := fieldsOf_bound entries bound fielded
        cases body : recordBody policy path (rowNames row) (convertRow boundary policy path row)
            (projectRow boundary policy path row) fields with
        | error failure => simp [body, Except.map] at required
        | ok results =>
          simp only [body, Except.map, Except.ok.injEq] at required
          subst required
          exact publish_bound bound (recordBody_bound
            (convertRow_bound boundary policy allowed entries row typeBound path)
            (projectRow_bound boundary policy allowed entries row typeBound path) fieldsBound body)
      · split at required
        · exact (checkMember_ok.mp required).2 ▸ bound
        · simp at required
    | .nominal declaration row, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals, List.mem_cons, forall_eq_or_imp] at typeBound
      simp only [convert] at required
      split at required
      · rename_i fields fielded
        have fieldsBound := fieldsOf_bound entries bound fielded
        cases body : recordBody policy path (rowNames row) (convertRow boundary policy path row)
            (projectRow boundary policy path row) fields with
        | error failure => simp [body, Except.map] at required
        | ok results =>
          simp only [body, Except.map, Except.ok.injEq] at required
          subst required
          intro found present
          simp only [nominals, List.mem_cons] at present
          rcases present with same | inner
          · exact same ▸ typeBound.1
          · exact bound_record.mpr (recordBody_bound
              (convertRow_bound boundary policy allowed entries row typeBound.2 path)
              (projectRow_bound boundary policy allowed entries row typeBound.2 path) fieldsBound body) found
              (by simpa [nominals] using inner)
      · split at required
        · cases checked : checkMember boundary path (.record row) candidate with
          | error failure => simp [checked, Except.map] at required
          | ok accepted =>
            simp only [checked, Except.map, Except.ok.injEq] at required
            subst required
            have same := (checkMember_ok.mp checked).2
            subst same
            intro found present
            simp only [nominals, List.mem_cons] at present
            rcases present with same | inner
            · exact same ▸ typeBound.1
            · exact bound_eraseNominal bound found inner
        · simp at required
    | .optional element, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals] at typeBound
      simp only [convert] at required
      split at required
      · simp only [Except.ok.injEq] at required
        exact required ▸ bound
      · exact convert_bound boundary policy allowed entries element typeBound path candidate converted bound required
    | .union first second rest, typeBound, path, candidate, converted, bound, required => by
      simp only [typeNominals, List.mem_append] at typeBound
      simp only [convert] at required
      split at required
      · rename_i accepted firstOk
        simp only [Except.ok.injEq] at required
        subst required
        exact convert_bound boundary policy allowed entries first (fun d p => typeBound d (Or.inl (Or.inl p))) path
          candidate _ bound firstOk
      · split at required
        · rename_i accepted secondOk
          simp only [Except.ok.injEq] at required
          subst required
          exact convert_bound boundary policy allowed entries second (fun d p => typeBound d (Or.inl (Or.inr p))) path
            candidate _ bound secondOk
        · split at required
          · rename_i accepted restSome
            simp only [Except.ok.injEq] at required
            subst required
            exact convertMembers_bound boundary policy allowed entries rest (fun d p => typeBound d (Or.inr p)) path
              candidate _ bound restSome
          · simp at required
    | .tagged declaration, _, path, candidate, converted, bound, required => by
      simp only [convert] at required
      split at required
      · exact (checkMember_ok.mp required).2 ▸ bound
      · simp only [wire] at required
        split at required
        · split at required
          · simp only [Except.ok.injEq] at required
            exact required ▸ bound
          · simp at required
        · split at required
          · simp only [Except.ok.injEq] at required
            subst required
            simp [Bound, nominals, nominalsOf]
          · simp at required
        · simp at required
    | .module closure exports, _, path, candidate, converted, bound, required => by
      simp only [convert] at required
      split at required
      · split at required
        · simp only [Except.ok.injEq] at required
          exact required ▸ bound
        · simp at required
      · simp at required
    | .primitive _, _, path, candidate, converted, bound, required
    | .stream _, _, path, candidate, converted, bound, required
    | .result _ _, _, path, candidate, converted, bound, required
    | .functionClass _, _, path, candidate, converted, bound, required
    | .callable _, _, path, candidate, converted, bound, required
    | .schemeCallable _, _, path, candidate, converted, bound, required
    | .errorFamily _, _, path, candidate, converted, bound, required
    | .errorVariant _ _, _, path, candidate, converted, bound, required
    | .errorFacet _, _, path, candidate, converted, bound, required
    | .relativePath, _, path, candidate, converted, bound, required
    | .boundedInteger _, _, path, candidate, converted, bound, required => by
      simp only [convert] at required
      exact (checkMember_ok.mp required).2 ▸ bound

  theorem convertRow_bound (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
      (allowed : Declaration → Prop)
      (entries : policy ≠ .keep → ∀ reference, ∀ field ∈ boundary.world.entryFields reference, Bound allowed field.2) :
      ∀ (row : RecordType Declaration), (∀ declaration ∈ rowNominals row, allowed declaration) →
        ∀ (path : List PathStep) (fields results : List (String × Value Declaration Identity Source)),
          (∀ field ∈ fields, Bound allowed field.2) → convertRow boundary policy path row fields = .ok results →
            ∀ field ∈ results, Bound allowed field.2
    | .variable id, _, _, _, _, _, _ => nomatch id
    | .rowEmpty, _, path, fields, results, bound, required => by
      simp only [convertRow, Except.ok.injEq] at required
      exact required ▸ bound
    | .rowField name type rest, typeBound, path, fields, results, bound, required => by
      simp only [rowNominals, List.mem_append] at typeBound
      simp only [convertRow] at required
      split at required
      · simp at required
      · rename_i found present
        split at required
        · simp at required
        · rename_i replacement replaced
          exact convertRow_bound boundary policy allowed entries rest (fun d p => typeBound d (Or.inr p)) path _ results
            (bound_replaceField name (convert_bound boundary policy allowed entries type
              (fun d p => typeBound d (Or.inl p)) _ found replacement (bound_lookup bound present) replaced) bound)
            required

  theorem projectRow_bound (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
      (allowed : Declaration → Prop)
      (entries : policy ≠ .keep → ∀ reference, ∀ field ∈ boundary.world.entryFields reference, Bound allowed field.2) :
      ∀ (row : RecordType Declaration), (∀ declaration ∈ rowNominals row, allowed declaration) →
        ∀ (path : List PathStep) (fields results : List (String × Value Declaration Identity Source)),
          (∀ field ∈ fields, Bound allowed field.2) → projectRow boundary policy path row fields = .ok results →
            ∀ field ∈ results, Bound allowed field.2
    | .variable id, _, _, _, _, _, _ => nomatch id
    | .rowEmpty, _, path, fields, results, bound, required => by
      simp only [projectRow, Except.ok.injEq] at required
      subst required
      simp
    | .rowField name type rest, typeBound, path, fields, results, bound, required => by
      simp only [rowNominals, List.mem_append] at typeBound
      simp only [projectRow] at required
      split at required
      · simp at required
      · rename_i found present
        split at required
        · simp at required
        · rename_i replacement replaced
          split at required
          · simp at required
          · rename_i others projected
            simp only [Except.ok.injEq] at required
            subst required
            intro field inside
            simp only [List.mem_cons] at inside
            rcases inside with same | later
            · subst same
              exact convert_bound boundary policy allowed entries type (fun d p => typeBound d (Or.inl p)) _ found
                replacement (bound_lookup bound present) replaced
            · exact projectRow_bound boundary policy allowed entries rest (fun d p => typeBound d (Or.inr p)) path fields
                others bound projected field later

  theorem convertMembers_bound (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
      (allowed : Declaration → Prop)
      (entries : policy ≠ .keep → ∀ reference, ∀ field ∈ boundary.world.entryFields reference, Bound allowed field.2) :
      ∀ (rest : UnionTail Declaration), (∀ declaration ∈ membersNominals rest, allowed declaration) →
        ∀ (path : List PathStep) (candidate converted : Value Declaration Identity Source), Bound allowed candidate →
          convertMembers boundary policy path rest candidate = some converted → Bound allowed converted
    | .variable id, _, _, _, _, _, _ => nomatch id
    | .membersEmpty, _, _, _, _, _, required => by simp [convertMembers] at required
    | .member type tail, typeBound, path, candidate, converted, bound, required => by
      simp only [membersNominals, List.mem_append] at typeBound
      simp only [convertMembers] at required
      split at required
      · rename_i accepted here
        simp only [Option.some.injEq] at required
        subst required
        exact convert_bound boundary policy allowed entries type (fun d p => typeBound d (Or.inl p)) path candidate _
          bound here
      · exact convertMembers_bound boundary policy allowed entries tail (fun d p => typeBound d (Or.inr p)) path candidate
          converted bound required
end




mutual
  theorem evaluate_bound (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
      (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) (allowed : Declaration → Prop)
      (heapBound : ∀ cell stored, heap cell = some stored → Bound allowed stored) :
      ∀ (form : Form Declaration), (∀ declaration ∈ introductions form, allowed declaration) →
        ∀ value, evaluate world contracts enums heap form = some value → Bound allowed value
    | .scalar scalar, _, value, evaluated => by
      simp only [evaluate, Option.some.injEq] at evaluated
      subst evaluated
      simp [Bound, nominals]
    | .list items, introduced, value, evaluated => by
      simp only [introductions] at introduced
      simp only [evaluate] at evaluated
      cases all : evaluateAll world contracts enums heap items with
      | none => simp [all] at evaluated
      | some values =>
        simp only [all, Option.map_some, Option.some.injEq] at evaluated
        subst evaluated
        exact bound_list.mpr (evaluateAll_bound world contracts enums heap allowed heapBound items introduced values all)
    | .record fields, introduced, value, evaluated => by
      simp only [introductions] at introduced
      simp only [evaluate] at evaluated
      cases all : evaluateFields world contracts enums heap fields with
      | none => simp [all] at evaluated
      | some values =>
        simp only [all, Option.map_some, Option.some.injEq] at evaluated
        subst evaluated
        exact bound_record.mpr (evaluateFields_bound world contracts enums heap allowed heapBound fields introduced values all)
    | .construct declaration schema fields, introduced, value, evaluated => by
      simp only [introductions, List.mem_cons, forall_eq_or_imp] at introduced
      simp only [evaluate] at evaluated
      cases all : evaluateFields world contracts enums heap fields with
      | none => simp [all] at evaluated
      | some values =>
        letI := Membership.decideTerm world contracts schema values
        by_cases member : Membership.row world schema values
        · simp only [all, member, ite_true, Option.some.injEq] at evaluated
          subst evaluated
          intro found present
          simp only [nominals, List.mem_cons] at present
          rcases present with same | inner
          · exact same ▸ introduced.1
          · exact bound_record.mpr (evaluateFields_bound world contracts enums heap allowed heapBound fields introduced.2
              values all) found (by simpa [nominals] using inner)
        · simp [all, member] at evaluated
    | .require target source, introduced, value, evaluated => by
      simp only [introductions, List.mem_append] at introduced
      simp only [evaluate] at evaluated
      cases inner : evaluate world contracts enums heap source with
      | none => simp [inner] at evaluated
      | some sourceValue =>
        simp only [inner] at evaluated
        exact convert_bound ⟨world, contracts, enums⟩ .keep allowed (fun notKeep => absurd rfl notKeep) target
          (fun d p => introduced d (Or.inl p)) [] sourceValue value
          (evaluate_bound world contracts enums heap allowed heapBound source (fun d p => introduced d (Or.inr p))
            sourceValue inner) (toOption_some evaluated)
    | .read cell, _, value, evaluated => by
      simp only [evaluate] at evaluated
      exact heapBound cell value evaluated
    | .field source name, introduced, value, evaluated => by
      simp only [introductions] at introduced
      simp only [evaluate] at evaluated
      cases inner : evaluate world contracts enums heap source with
      | none => simp [inner] at evaluated
      | some sourceValue =>
        have sourceBound := bound_eraseNominal
          (evaluate_bound world contracts enums heap allowed heapBound source introduced sourceValue inner)
        simp only [inner] at evaluated
        cases erased : sourceValue.eraseNominal <;> simp only [erased, reduceCtorEq] at evaluated sourceBound
        case record fields => exact bound_lookup (bound_record.mp sourceBound) evaluated
    | .spread source updates, introduced, value, evaluated => by
      simp only [introductions, List.mem_append] at introduced
      simp only [evaluate] at evaluated
      cases inner : evaluate world contracts enums heap source with
      | none => simp [inner] at evaluated
      | some sourceValue =>
        have sourceBound := bound_eraseNominal
          (evaluate_bound world contracts enums heap allowed heapBound source (fun d p => introduced d (Or.inl p))
            sourceValue inner)
        simp only [inner] at evaluated
        cases erased : sourceValue.eraseNominal <;> simp only [erased, reduceCtorEq] at evaluated sourceBound
        case record fields =>
          cases all : evaluateFields world contracts enums heap updates with
          | none => simp [all] at evaluated
          | some replaced =>
            simp only [all, Option.map_some, Option.some.injEq] at evaluated
            subst evaluated
            refine bound_record.mpr ?_
            intro field present
            simp only [List.mem_append] at present
            rcases present with updated | kept
            · exact evaluateFields_bound world contracts enums heap allowed heapBound updates
                (fun d p => introduced d (Or.inr p)) replaced all field updated
            · exact bound_record.mp sourceBound field kept

  theorem evaluateAll_bound (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
      (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) (allowed : Declaration → Prop)
      (heapBound : ∀ cell stored, heap cell = some stored → Bound allowed stored) :
      ∀ (items : List (Form Declaration)), (∀ declaration ∈ introductionsOf items, allowed declaration) →
        ∀ values, evaluateAll world contracts enums heap items = some values → ∀ value ∈ values, Bound allowed value
    | [], _, values, evaluated => by
      simp only [evaluateAll, Option.some.injEq] at evaluated
      subst evaluated
      simp
    | item :: rest, introduced, values, evaluated => by
      simp only [introductionsOf, List.mem_append] at introduced
      simp only [evaluateAll] at evaluated
      cases head : evaluate world contracts enums heap item with
      | none => simp [head] at evaluated
      | some value =>
        cases tail : evaluateAll world contracts enums heap rest with
        | none => simp [head, tail] at evaluated
        | some others =>
          simp only [head, tail, Option.some.injEq] at evaluated
          subst evaluated
          intro found present
          simp only [List.mem_cons] at present
          rcases present with same | later
          · exact same ▸ evaluate_bound world contracts enums heap allowed heapBound item (fun d p => introduced d (Or.inl p))
              value head
          · exact evaluateAll_bound world contracts enums heap allowed heapBound rest (fun d p => introduced d (Or.inr p))
              others tail found later

  theorem evaluateFields_bound (world : MembershipWorld Declaration Identity Source)
      (contracts : ContractDecisions world) (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) (allowed : Declaration → Prop)
      (heapBound : ∀ cell stored, heap cell = some stored → Bound allowed stored) :
      ∀ (fields : List (String × Form Declaration)), (∀ declaration ∈ introductionsOfFields fields, allowed declaration) →
        ∀ values, evaluateFields world contracts enums heap fields = some values → ∀ field ∈ values, Bound allowed field.2
    | [], _, values, evaluated => by
      simp only [evaluateFields, Option.some.injEq] at evaluated
      subst evaluated
      simp
    | (name, item) :: rest, introduced, values, evaluated => by
      simp only [introductionsOfFields, List.mem_append] at introduced
      simp only [evaluateFields] at evaluated
      cases head : evaluate world contracts enums heap item with
      | none => simp [head] at evaluated
      | some value =>
        cases tail : evaluateFields world contracts enums heap rest with
        | none => simp [head, tail] at evaluated
        | some others =>
          simp only [head, tail, Option.some.injEq] at evaluated
          subst evaluated
          intro found present
          simp only [List.mem_cons] at present
          rcases present with same | later
          · subst same
            exact evaluate_bound world contracts enums heap allowed heapBound item (fun d p => introduced d (Or.inl p)) value
              head
          · exact evaluateFields_bound world contracts enums heap allowed heapBound rest (fun d p => introduced d (Or.inr p))
              others tail found later
end

omit [DecidableEq Declaration] in
theorem nominal_member_carries_declaration (world : MembershipWorld Declaration Identity Source)
    {declaration : Declaration} {fields : RecordType Declaration} {candidate : Value Declaration Identity Source}
    (member : Membership.value world (.nominal declaration fields) candidate) : declaration ∈ nominals candidate := by
  cases candidate with
  | nominal actual contents =>
    simp only [Membership.value] at member
    simp [nominals, member.1]
  | _ => simp [Membership.value] at member

omit [DecidableEq Declaration] in
theorem nominal_member_unique (world : MembershipWorld Declaration Identity Source)
    {first second : Declaration} {firstFields secondFields : RecordType Declaration}
    {candidate : Value Declaration Identity Source}
    (firstMember : Membership.value world (.nominal first firstFields) candidate)
    (secondMember : Membership.value world (.nominal second secondFields) candidate) : first = second := by
  cases candidate with
  | nominal actual contents =>
    simp only [Membership.value] at firstMember secondMember
    exact firstMember.1.symm.trans secondMember.1
  | _ => simp [Membership.value] at firstMember

theorem construct_introduces (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (enums : WireEnums Declaration) (heap : Heap Declaration Identity Source) (declaration : Declaration) (schema : RecordType Declaration)
    (fields : List (String × Form Declaration)) (value : Value Declaration Identity Source)
    (evaluated : evaluate world contracts enums heap (.construct declaration schema fields) = some value) :
    Membership.value world (.nominal declaration schema) value := by
  simp only [evaluate] at evaluated
  cases all : evaluateFields world contracts enums heap fields with
  | none => simp [all] at evaluated
  | some values =>
    letI := Membership.decideTerm world contracts schema values
    by_cases member : Membership.row world schema values
    · simp only [all, member, ite_true, Option.some.injEq] at evaluated
      subst evaluated
      simp only [Membership.value, Membership.recordFields, Value.eraseNominal]
      exact ⟨by trivial, member⟩
    · simp [all, member] at evaluated

/-- Nominal introduction provenance over every Core world and every
value-forming expression: scalar, list and record literals, constructor
calls, `.require`, store reads, field reads and spreads. Every nominal
identity in a produced value is one the store already carried or one that a
constructor or `.require` target of the expression names, so a record literal,
a spread or a value of another nominal type never acquires a nominal type. A
member of a nominal type carries that declaration, so a nominal member
produced by an expression was introduced by it or read from the store. A
constructor introduces exactly its declaration and checks its schema, and two
declarations with the same name or shape stay distinct types. Raw membership
alone admits a forged candidate; this judgment is what excludes it. -/
theorem nominal_provenance (world : MembershipWorld Declaration Identity Source) (contracts : ContractDecisions world)
    (enums : WireEnums Declaration) :
    (∀ (heap : Heap Declaration Identity Source) (allowed : Declaration → Prop) (form : Form Declaration) value,
      (∀ cell stored, heap cell = some stored → Bound allowed stored) →
      (∀ declaration ∈ introductions form, allowed declaration) →
      evaluate world contracts enums heap form = some value → Bound allowed value) ∧
    (∀ declaration (fields : RecordType Declaration) (candidate : Value Declaration Identity Source),
      Membership.value world (.nominal declaration fields) candidate → declaration ∈ nominals candidate) ∧
    (∀ (heap : Heap Declaration Identity Source) (stored : Declaration → Prop) (form : Form Declaration) value
      declaration (fields : RecordType Declaration),
      (∀ cell found, heap cell = some found → Bound stored found) →
      evaluate world contracts enums heap form = some value →
      Membership.value world (.nominal declaration fields) value →
        stored declaration ∨ declaration ∈ introductions form) ∧
    (∀ (heap : Heap Declaration Identity Source) declaration schema fields value,
      evaluate world contracts enums heap (.construct declaration schema fields) = some value →
        Membership.value world (.nominal declaration schema) value) ∧
    (∀ (first second : Declaration) (firstFields secondFields : RecordType Declaration) candidate,
      Membership.value world (.nominal first firstFields) candidate →
      Membership.value world (.nominal second secondFields) candidate → first = second) := by
  refine ⟨fun heap allowed form value heapBound introduced evaluated =>
      evaluate_bound world contracts enums heap allowed heapBound form introduced value evaluated,
    fun _ _ _ member => nominal_member_carries_declaration world member, ?_,
    construct_introduces world contracts enums,
    fun _ _ _ _ _ firstMember secondMember => nominal_member_unique world firstMember secondMember⟩
  intro heap stored form value declaration fields heapBound evaluated member
  exact evaluate_bound world contracts enums heap (fun found => stored found ∨ found ∈ introductions form)
    (fun cell found present d inside => Or.inl (heapBound cell found present d inside)) form
    (fun d inside => Or.inr inside) value evaluated declaration (nominal_member_carries_declaration world member)

namespace Witness

abbrev ModuleDeclaration := String × String
abbrev ModuleValue := Value ModuleDeclaration Unit Unit

def moduleWorld : MembershipWorld ModuleDeclaration Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => False
  callable := fun _ _ => False
  scheme := fun _ _ => False
  generic := fun _ _ => False

def moduleContracts : ContractDecisions moduleWorld where
  stream := fun _ _ => isFalse (fun ⟨_, held, _⟩ => held)
  callable := fun _ _ => isFalse (fun ⟨_, held, _⟩ => held)
  scheme := fun _ _ => instDecidableFalse
  generic := fun _ _ => instDecidableFalse

def emptyHeap : Heap ModuleDeclaration Unit Unit := fun _ => none

def north : ModuleDeclaration := ("north", "Package")
def south : ModuleDeclaration := ("south", "Package")
def schema : RecordType ModuleDeclaration := .rowField "name" (.primitive .string) .rowEmpty

def nameForm : Form ModuleDeclaration := .scalar (.string "xsh")
def literal : Form ModuleDeclaration := .record [("name", nameForm)]
def built : Form ModuleDeclaration := .construct north schema [("name", nameForm)]

theorem forged_candidate_is_a_member_but_has_no_introduction :
    Membership.value moduleWorld (.nominal north .rowEmpty) (.nominal north (.record []) : ModuleValue) ∧
      ∀ (form : Form ModuleDeclaration) value, introductions form = [] →
        evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap form = some value →
          ¬ Membership.value moduleWorld (.nominal north .rowEmpty) value := by
  refine ⟨by simp [Membership.value, Membership.row, Membership.recordFields, Value.eraseNominal], ?_⟩
  intro form value none evaluated member
  rcases (nominal_provenance moduleWorld moduleContracts WireEnums.empty).2.2.1 emptyHeap (fun _ => False) form value north .rowEmpty
    (fun _ _ present => by simp [emptyHeap] at present) evaluated member with impossible | introduced
  · exact impossible
  · simp [none] at introduced

theorem record_literal_never_has_the_nominal_type :
    evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap literal = some (.record [("name", .scalar (.string "xsh"))]) ∧
      ¬ Membership.value moduleWorld (.nominal north schema)
        (.record [("name", .scalar (.string "xsh"))] : ModuleValue) := by
  refine ⟨rfl, ?_⟩
  simp [Membership.value]

theorem same_name_and_shape_in_another_module_is_another_type :
    ∃ value, evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap built = some value ∧
      Membership.value moduleWorld (.nominal north schema) value ∧
      ¬ Membership.value moduleWorld (.nominal south schema) value := by
  have row : Membership.row moduleWorld schema [("name", .scalar (.string "xsh"))] := by
    simp [schema, Membership.row, Membership.value, Membership.primitive, Value.eraseNominal]
  have evaluated : evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap built =
      some (.nominal north (.record [("name", .scalar (.string "xsh"))])) := by
    simp only [built, evaluate, evaluateFields, nameForm]
    letI := Membership.decideTerm moduleWorld moduleContracts schema [("name", .scalar (.string "xsh"))]
    simp [row]
  refine ⟨_, evaluated, construct_introduces moduleWorld moduleContracts WireEnums.empty emptyHeap north schema _ _ evaluated, ?_⟩
  intro member
  have := nominal_member_unique moduleWorld member
    (construct_introduces moduleWorld moduleContracts WireEnums.empty emptyHeap north schema _ _ evaluated)
  simp [north, south] at this

def importResolvedByName (declaration : ModuleDeclaration) : ModuleDeclaration := ("south", declaration.2)

def misresolvedConstruct (form : Form ModuleDeclaration) : Option ModuleValue :=
  match form with
  | .construct declaration _ _ =>
    (evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap form).map fun value =>
      match value with
      | .nominal _ contents => .nominal (importResolvedByName declaration) contents
      | other => other
  | other => evaluate moduleWorld moduleContracts WireEnums.empty emptyHeap other

theorem wrong_import_provenance_is_detected :
    ∃ value, misresolvedConstruct built = some value ∧ south ∈ nominals value ∧ south ∉ introductions built := by
  obtain ⟨value, evaluated, member, _⟩ := same_name_and_shape_in_another_module_is_another_type
  cases value with
  | nominal declaration contents =>
    refine ⟨.nominal south contents, ?_, by simp [nominals], by simp [built, introductions, introductionsOfFields,
      nameForm, north, south]⟩
    unfold misresolvedConstruct
    simp only [built] at evaluated ⊢
    rw [evaluated]
    simp [importResolvedByName, north, south]
  | _ => simp [Membership.value] at member

end Witness

end Xsh.Validation.Core.Nominal
