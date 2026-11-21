import Xsh.Validation.Core.Union
import Xsh.Validation.Core.WellFormed

namespace Xsh.Validation.Core.Require

open Xsh.Core
open Xsh.Validation.Core
open Xsh.Validation.Core.WellFormed

variable {Declaration Identity Source : Type}

structure Boundary (Declaration Identity Source : Type) where
  world : MembershipWorld Declaration Identity Source
  contracts : ContractDecisions world
  enums : WireEnums Declaration

inductive FieldPolicy where
  | keep
  | exact
  | stripped
  deriving DecidableEq

inductive PathStep where
  | field (name : String)
  | index (position : Nat)
  | key (key : MapKey)
  deriving DecidableEq

inductive ModuleViolation where
  | missing (name : String)
  | mismatched (name : String)
  | unexpected (name : String)
  deriving DecidableEq

inductive Reason where
  | mismatch
  | missingField (name : String)
  | unexpectedField (name : String)
  | unknownWire (text : String)
  | keyMismatch (key : MapKey)
  | setElement (position : Nat)
  | repeatedElement (position : Nat)
  | noAlternative
  | module (violations : List ModuleViolation)
  deriving DecidableEq

structure Failure where
  path : List PathStep
  reason : Reason
  deriving DecidableEq

abbrev Field (Declaration Identity Source : Type) := String × Value Declaration Identity Source
abbrev Entry (Declaration Identity Source : Type) := MapKey × Value Declaration Identity Source
abbrev Outcome (Declaration Identity Source : Type) := Except Failure (Value Declaration Identity Source)

def listItems (candidate : Value Declaration Identity Source) : Option (List (Value Declaration Identity Source)) :=
  match candidate.eraseNominal with
  | .list values => some values
  | .environmentPaths paths => some (paths.map fun path => .scalar (.path path))
  | _ => none

def keyOf (candidate : Value Declaration Identity Source) : Option MapKey :=
  match candidate.eraseNominal with
  | .scalar (.string value) => some (.string value)
  | .scalar (.integer value) => some (.integer value)
  | .scalar (.boolean value) => some (.boolean value)
  | .scalar (.bytes value) => some (.bytes value)
  | .scalar (.path value) => some (.path value)
  | .scalar (.duration value) => some (.duration value)
  | _ => none

def fieldsOf (boundary : Boundary Declaration Identity Source)
    (candidate : Value Declaration Identity Source) : Option (List (Field Declaration Identity Source)) :=
  match candidate.eraseNominal with
  | .record fields => some fields
  | .filesystemEntry reference => some (boundary.world.entryFields reference)
  | _ => none

def replaceField (name : String) (replacement : Value Declaration Identity Source) :
    List (Field Declaration Identity Source) → List (Field Declaration Identity Source)
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

def publish (policy : FieldPolicy) (candidate : Value Declaration Identity Source)
    (fields : List (Field Declaration Identity Source)) : Value Declaration Identity Source :=
  match policy, candidate.eraseNominal with
  | .stripped, _ => .record fields
  | _, .record _ => rewrap candidate (.record fields)
  | _, _ => .record fields

def recordEntry (field : Field Declaration Identity Source) : Entry Declaration Identity Source :=
  (.string field.1, field.2)

def undeclared (names : List String) (fields : List (Field Declaration Identity Source)) : Option String :=
  (fields.find? fun field => !names.contains field.1).map Prod.fst

def keyAdmits (type : ValueType Declaration) (key : MapKey) : Bool :=
  @decide (Membership.key type key) (Membership.decideKey type key)

section Decisions

variable [DecidableEq Declaration]

def checkMember (boundary : Boundary Declaration Identity Source) (path : List PathStep) (type : ValueType Declaration)
    (candidate : Value Declaration Identity Source) : Outcome Declaration Identity Source :=
  if Membership.check boundary.world boundary.contracts type candidate then .ok candidate else .error ⟨path, .mismatch⟩

def convertItems (step : Nat → Value Declaration Identity Source → Outcome Declaration Identity Source) :
    Nat → List (Value Declaration Identity Source) → Except Failure (List (Value Declaration Identity Source))
  | _, [] => .ok []
  | position, item :: rest =>
    match step position item with
    | .error failure => .error failure
    | .ok converted =>
      match convertItems step (position + 1) rest with
      | .error failure => .error failure
      | .ok others => .ok (converted :: others)

def convertEntries (step : MapKey → Value Declaration Identity Source → Outcome Declaration Identity Source) :
    List (Entry Declaration Identity Source) → Except Failure (List (Entry Declaration Identity Source))
  | [] => .ok []
  | (key, value) :: rest =>
    match step key value with
    | .error failure => .error failure
    | .ok converted =>
      match convertEntries step rest with
      | .error failure => .error failure
      | .ok others => .ok ((key, converted) :: others)

def setKeys (element : ValueType Declaration) (path : List PathStep) :
    Nat → List MapKey → List (Value Declaration Identity Source) → Except Failure (List MapKey)
  | _, _, [] => .ok []
  | position, seen, item :: rest =>
    match keyOf item with
    | none => .error ⟨path, .setElement position⟩
    | some key =>
      if keyAdmits element key then
        if key ∈ seen then .error ⟨path, .repeatedElement position⟩
        else
          match setKeys element path (position + 1) (key :: seen) rest with
          | .error failure => .error failure
          | .ok others => .ok (key :: others)
      else .error ⟨path, .setElement position⟩

def recordBody (policy : FieldPolicy) (path : List PathStep) (names : List String)
    (keep project : List (Field Declaration Identity Source) → Except Failure (List (Field Declaration Identity Source)))
    (fields : List (Field Declaration Identity Source)) : Except Failure (List (Field Declaration Identity Source)) :=
  match policy with
  | .keep => keep fields
  | .stripped => project fields
  | .exact =>
    match undeclared names fields with
    | some name => .error ⟨path, .unexpectedField name⟩
    | none => keep fields

def wire (declaration : Declaration) (mapping : List (String × String)) (path : List PathStep)
    (candidate : Value Declaration Identity Source) : Outcome Declaration Identity Source :=
  match candidate.eraseNominal with
  | .tagged actual variant fields =>
    if actual = declaration ∧ variant ∈ mapping.map Prod.fst ∧ fields.isEmpty = true then .ok candidate
    else .error ⟨path, .mismatch⟩
  | .scalar (.string text) =>
    match WireEnums.variantOf mapping text with
    | some variant => .ok (.tagged declaration variant [])
    | none => .error ⟨path, .unknownWire text⟩
  | _ => .error ⟨path, .mismatch⟩

def entryViolations (name : String) (presence : ExportPresence) (admitted : Value Declaration Identity Source → Bool)
    (actual : List (Field Declaration Identity Source)) : List ModuleViolation :=
  match actual.lookup name with
  | none =>
    match presence with
    | .required => [.missing name]
    | .optional => []
  | some found => if admitted found then [] else [.mismatched name]

def exportViolations (boundary : Boundary Declaration Identity Source) :
    ExportType Declaration → List (Field Declaration Identity Source) → List ModuleViolation
  | .variable id, _ => nomatch id
  | .exportsEmpty, _ => []
  | .exportValue name presence type rest, actual =>
    entryViolations name presence
        (fun found => !Membership.isCallable found && Membership.check boundary.world boundary.contracts type found)
        actual ++
      exportViolations boundary rest actual
  | .exportCallable name presence signature rest, actual =>
    entryViolations name presence
        (fun found => @decide _ (Membership.decideCallable boundary.world boundary.contracts signature found)) actual ++
      exportViolations boundary rest actual
  | .exportGenericCallable name presence kind rest, actual =>
    entryViolations name presence
        (fun found => @decide _ (Membership.decideGeneric boundary.world boundary.contracts kind found)) actual ++
      exportViolations boundary rest actual

def unexpectedExports (closure : ExportClosure) (exports : ExportType Declaration)
    (actual : List (Field Declaration Identity Source)) : List ModuleViolation :=
  match closure with
  | .open => []
  | .exact => (actual.filter fun field => !(Membership.exportNames exports).contains field.1).map
      fun field => .unexpected field.1

def moduleViolations (boundary : Boundary Declaration Identity Source) (closure : ExportClosure)
    (exports : ExportType Declaration) (actual : List (Field Declaration Identity Source)) : List ModuleViolation :=
  exportViolations boundary exports actual ++ unexpectedExports closure exports actual

mutual
  def convert (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) (path : List PathStep) :
      ValueType Declaration → Value Declaration Identity Source → Outcome Declaration Identity Source
    | .variable id, _ => nomatch id
    | .list element, candidate =>
      match listItems candidate with
      | some values =>
        (convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item) 0
          values).map .list
      | none => .error ⟨path, .mismatch⟩
    | .nonEmpty element, candidate =>
      match listItems candidate with
      | some (first :: rest) =>
        (convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item) 0
          (first :: rest)).map .list
      | _ => .error ⟨path, .mismatch⟩
    | .map keyType element, candidate =>
      match candidate.eraseNominal with
      | .map present =>
        (convertEntries (fun key value =>
          if keyAdmits keyType key then convert boundary policy (path ++ [.key key]) element value
          else .error ⟨path, .keyMismatch key⟩) present).map .map
      | .record fields =>
        if keyType = .primitive .string then
          (convertEntries (fun key value => convert boundary policy (path ++ [.key key]) element value)
            (fields.map recordEntry)).map .map
        else .error ⟨path, .mismatch⟩
      | _ => .error ⟨path, .mismatch⟩
    | .set element, candidate =>
      match candidate.eraseNominal with
      | .set _ => checkMember boundary path (.set element) candidate
      | _ =>
        match listItems candidate with
        | some values => (setKeys element path 0 [] values).map .set
        | none => .error ⟨path, .mismatch⟩
    | .record row, candidate =>
      match fieldsOf boundary candidate with
      | some fields =>
        (recordBody policy path (rowNames row) (convertRow boundary policy path row) (projectRow boundary policy path row)
          fields).map (publish policy candidate)
      | none => .error ⟨path, .mismatch⟩
    | .nominal declaration row, candidate =>
      match fieldsOf boundary candidate with
      | some fields =>
        (recordBody policy path (rowNames row) (convertRow boundary policy path row) (projectRow boundary policy path row)
          fields).map fun converted => .nominal declaration (.record converted)
      | none => .error ⟨path, .mismatch⟩
    | .optional element, candidate =>
      if Membership.check boundary.world boundary.contracts (.primitive .null) candidate then .ok candidate
      else convert boundary policy path element candidate
    | .union first second rest, candidate =>
      match convert boundary policy path first candidate with
      | .ok converted => .ok converted
      | .error _ =>
        match convert boundary policy path second candidate with
        | .ok converted => .ok converted
        | .error _ =>
          match convertMembers boundary policy path rest candidate with
          | some converted => .ok converted
          | none => .error ⟨path, .noAlternative⟩
    | .tagged declaration, candidate =>
      match boundary.enums.strings declaration with
      | none => checkMember boundary path (.tagged declaration) candidate
      | some mapping => wire declaration mapping path candidate
    | .module closure exports, candidate =>
      match candidate.eraseNominal with
      | .module _ actual =>
        match moduleViolations boundary closure exports actual with
        | [] => .ok candidate
        | violation :: others => .error ⟨path, .module (violation :: others)⟩
      | _ => .error ⟨path, .mismatch⟩
    | .primitive type, candidate => checkMember boundary path (.primitive type) candidate
    | .stream element, candidate => checkMember boundary path (.stream element) candidate
    | .result success failure, candidate => checkMember boundary path (.result success failure) candidate
    | .functionClass kind, candidate => checkMember boundary path (.functionClass kind) candidate
    | .callable signature, candidate => checkMember boundary path (.callable signature) candidate
    | .schemeCallable declaration, candidate => checkMember boundary path (.schemeCallable declaration) candidate
    | .errorFamily declaration, candidate => checkMember boundary path (.errorFamily declaration) candidate
    | .errorVariant family variant, candidate => checkMember boundary path (.errorVariant family variant) candidate
    | .errorFacet facet, candidate => checkMember boundary path (.errorFacet facet) candidate
    | .relativePath, candidate => checkMember boundary path .relativePath candidate
    | .boundedInteger bounds, candidate => checkMember boundary path (.boundedInteger bounds) candidate

  def convertRow (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) (path : List PathStep) :
      RecordType Declaration → List (Field Declaration Identity Source) →
        Except Failure (List (Field Declaration Identity Source))
    | .variable id, _ => nomatch id
    | .rowEmpty, actual => .ok actual
    | .rowField name type rest, actual =>
      match actual.lookup name with
      | none => .error ⟨path, .missingField name⟩
      | some found =>
        match convert boundary policy (path ++ [.field name]) type found with
        | .error failure => .error failure
        | .ok converted => convertRow boundary policy path rest (replaceField name converted actual)

  def projectRow (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) (path : List PathStep) :
      RecordType Declaration → List (Field Declaration Identity Source) →
        Except Failure (List (Field Declaration Identity Source))
    | .variable id, _ => nomatch id
    | .rowEmpty, _ => .ok []
    | .rowField name type rest, actual =>
      match actual.lookup name with
      | none => .error ⟨path, .missingField name⟩
      | some found =>
        match convert boundary policy (path ++ [.field name]) type found with
        | .error failure => .error failure
        | .ok converted =>
          match projectRow boundary policy path rest actual with
          | .error failure => .error failure
          | .ok others => .ok ((name, converted) :: others)

  def convertMembers (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) (path : List PathStep) :
      UnionTail Declaration → Value Declaration Identity Source → Option (Value Declaration Identity Source)
    | .variable id, _ => nomatch id
    | .membersEmpty, _ => none
    | .member type rest, candidate =>
      match convert boundary policy path type candidate with
      | .ok converted => some converted
      | .error _ => convertMembers boundary policy path rest candidate
end

end Decisions

def Pointwise {α β : Type} (relation : α → β → Prop) : List α → List β → Prop
  | [], [] => True
  | left :: lefts, right :: rights => relation left right ∧ Pointwise relation lefts rights
  | _, _ => False

def Frame (names : List String) :
    List String → List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop
  | _, [], [] => True
  | seen, field :: fields, result :: results =>
    result.1 = field.1 ∧ ((field.1 ∈ names ∧ field.1 ∉ seen) ∨ result.2 = field.2) ∧
      Frame names (field.1 :: seen) fields results
  | _, _, _ => False

def RecordSpec (policy : FieldPolicy) (names : List String)
    (keep project : List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop)
    (fields results : List (Field Declaration Identity Source)) : Prop :=
  match policy with
  | .keep => keep fields results
  | .stripped => project fields results
  | .exact => (∀ field ∈ fields, field.1 ∈ names) ∧ keep fields results

def WireSpec (declaration : Declaration) (mapping : List (String × String))
    (candidate result : Value Declaration Identity Source) : Prop :=
  (∃ variant fields, candidate.eraseNominal = .tagged declaration variant fields ∧ fields = [] ∧
      variant ∈ mapping.map Prod.fst ∧ result = candidate) ∨
    (∃ text variant, candidate.eraseNominal = .scalar (.string text) ∧ (variant, text) ∈ mapping ∧
      result = .tagged declaration variant [])

mutual
  def Converts (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      ValueType Declaration → Value Declaration Identity Source → Value Declaration Identity Source → Prop
    | .variable id, _, _ => nomatch id
    | .list element, candidate, result =>
      ∃ values results, listItems candidate = some values ∧ result = .list results ∧
        Pointwise (Converts boundary policy element) values results
    | .nonEmpty element, candidate, result =>
      ∃ values results, listItems candidate = some values ∧ values ≠ [] ∧ result = .list results ∧
        Pointwise (Converts boundary policy element) values results
    | .map keyType element, candidate, result =>
      ∃ results, result = .map results ∧
        ((∃ present, candidate.eraseNominal = .map present ∧
            Pointwise (fun entry converted => Membership.key keyType entry.1 ∧ converted.1 = entry.1 ∧
              Converts boundary policy element entry.2 converted.2) present results) ∨
          (keyType = .primitive .string ∧ ∃ fields, candidate.eraseNominal = .record fields ∧
            Pointwise (fun field converted => converted.1 = .string field.1 ∧
              Converts boundary policy element field.2 converted.2) fields results))
    | .set element, candidate, result =>
      (Membership.value boundary.world (.set element) candidate ∧ result = candidate) ∨
        (∃ values keys, listItems candidate = some values ∧ values.map keyOf = keys.map some ∧
          (∀ key ∈ keys, Membership.key element key) ∧ keys.Nodup ∧ result = .set keys)
    | .record row, candidate, result =>
      (∃ fields results, fieldsOf boundary candidate = some fields ∧ result = publish policy candidate results ∧
        RecordSpec policy (rowNames row)
          (fun fields results => RowDeclared boundary policy row fields results ∧ Frame (rowNames row) [] fields results)
          (RowProject boundary policy row) fields results)
    | .nominal declaration row, candidate, result =>
      (∃ fields results, fieldsOf boundary candidate = some fields ∧
        result = .nominal declaration (.record results) ∧
        RecordSpec policy (rowNames row)
          (fun fields results => RowDeclared boundary policy row fields results ∧ Frame (rowNames row) [] fields results)
          (RowProject boundary policy row) fields results)
    | .optional element, candidate, result =>
      (Membership.primitive .null candidate ∧ result = candidate) ∨
        (¬ Membership.primitive .null candidate ∧ Converts boundary policy element candidate result)
    | .union first second rest, candidate, result =>
      Converts boundary policy first candidate result ∨
        ((∀ other, ¬ Converts boundary policy first candidate other) ∧
          (Converts boundary policy second candidate result ∨
            ((∀ other, ¬ Converts boundary policy second candidate other) ∧
              FirstMember boundary policy rest candidate result)))
    | .tagged declaration, candidate, result =>
      match boundary.enums.strings declaration with
      | none => Membership.value boundary.world (.tagged declaration) candidate ∧ result = candidate
      | some mapping => WireSpec declaration mapping candidate result
    | .module closure exports, candidate, result =>
      Membership.value boundary.world (.module closure exports) candidate ∧ result = candidate
    | .primitive type, candidate, result =>
      Membership.value boundary.world (.primitive type) candidate ∧ result = candidate
    | .stream element, candidate, result =>
      Membership.value boundary.world (.stream element) candidate ∧ result = candidate
    | .result success failure, candidate, result =>
      Membership.value boundary.world (.result success failure) candidate ∧ result = candidate
    | .functionClass kind, candidate, result =>
      Membership.value boundary.world (.functionClass kind) candidate ∧ result = candidate
    | .callable signature, candidate, result =>
      Membership.value boundary.world (.callable signature) candidate ∧ result = candidate
    | .schemeCallable declaration, candidate, result =>
      Membership.value boundary.world (.schemeCallable declaration) candidate ∧ result = candidate
    | .errorFamily declaration, candidate, result =>
      Membership.value boundary.world (.errorFamily declaration) candidate ∧ result = candidate
    | .errorVariant family variant, candidate, result =>
      Membership.value boundary.world (.errorVariant family variant) candidate ∧ result = candidate
    | .errorFacet facet, candidate, result =>
      Membership.value boundary.world (.errorFacet facet) candidate ∧ result = candidate
    | .relativePath, candidate, result =>
      Membership.value boundary.world .relativePath candidate ∧ result = candidate
    | .boundedInteger bounds, candidate, result =>
      Membership.value boundary.world (.boundedInteger bounds) candidate ∧ result = candidate

  def RowDeclared (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      RecordType Declaration → List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop
    | .variable id, _, _ => nomatch id
    | .rowEmpty, _, _ => True
    | .rowField name type rest, fields, results =>
      (∃ found converted, fields.lookup name = some found ∧ results.lookup name = some converted ∧
        Converts boundary policy type found converted) ∧ RowDeclared boundary policy rest fields results

  def RowProject (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      RecordType Declaration → List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop
    | .variable id, _, _ => nomatch id
    | .rowEmpty, _, results => results = []
    | .rowField name type rest, fields, results =>
      ∃ found converted others, fields.lookup name = some found ∧ Converts boundary policy type found converted ∧
        results = (name, converted) :: others ∧ RowProject boundary policy rest fields others

  def FirstMember (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      UnionTail Declaration → Value Declaration Identity Source → Value Declaration Identity Source → Prop
    | .variable id, _, _ => nomatch id
    | .membersEmpty, _, _ => False
    | .member type rest, candidate, result =>
      Converts boundary policy type candidate result ∨
        ((∀ other, ¬ Converts boundary policy type candidate other) ∧ FirstMember boundary policy rest candidate result)
end

section Lemmas

theorem eraseNominal_wrap (declarations : List Declaration) (candidate : Value Declaration Identity Source) :
    (wrap declarations candidate).eraseNominal = candidate.eraseNominal := by
  induction declarations with
  | nil => rfl
  | cons head rest ih => simp [wrap, Value.eraseNominal, ih]

theorem eraseNominal_rewrap (original rebuilt : Value Declaration Identity Source) :
    (rewrap original rebuilt).eraseNominal = rebuilt.eraseNominal :=
  eraseNominal_wrap _ _

theorem wrap_peel (candidate : Value Declaration Identity Source) :
    wrap (peel candidate).1 (peel candidate).2 = candidate := by
  cases candidate with
  | nominal declaration contents => simp [peel, wrap, wrap_peel contents]
  | _ => rfl

theorem peel_eraseNominal (candidate : Value Declaration Identity Source) :
    (peel candidate).2 = candidate.eraseNominal := by
  cases candidate with
  | nominal declaration contents => simp [peel, Value.eraseNominal, peel_eraseNominal contents]
  | _ => rfl

theorem rewrap_erased (candidate : Value Declaration Identity Source) :
    rewrap candidate candidate.eraseNominal = candidate := by
  unfold rewrap
  rw [← peel_eraseNominal, wrap_peel]

theorem eraseNominal_publish (policy : FieldPolicy) (candidate : Value Declaration Identity Source)
    (fields : List (Field Declaration Identity Source)) :
    (publish policy candidate fields).eraseNominal = .record fields := by
  unfold publish
  split <;> simp [eraseNominal_rewrap, Value.eraseNominal]

theorem lookup_replaceField_same (name : String) (replacement : Value Declaration Identity Source) :
    ∀ {fields : List (Field Declaration Identity Source)} {found : Value Declaration Identity Source},
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

theorem lookup_replaceField_other (name other : String) (replacement : Value Declaration Identity Source)
    (different : other ≠ name) :
    ∀ fields : List (Field Declaration Identity Source),
      (replaceField name replacement fields).lookup other = fields.lookup other
  | [] => rfl
  | (key, value) :: rest => by
    by_cases same : key = name
    · subst same
      have differentNames : (other == key) = false := by simpa using different
      simp [replaceField, List.lookup, differentNames]
    · simp only [replaceField, same, ite_false, List.lookup]
      split <;> simp_all [lookup_replaceField_other name other replacement different rest]

theorem lookup_mem {α : Type} : ∀ {fields : List (String × α)} {name : String} {found : α},
    fields.lookup name = some found → (name, found) ∈ fields
  | [], _, _, present => by simp at present
  | (other, value) :: rest, name, found, present => by
    by_cases same : name = other
    · subst same
      simp [List.lookup] at present
      simp [present]
    · have differentNames : (name == other) = false := by simpa using same
      simp only [List.lookup, differentNames] at present
      exact List.mem_cons_of_mem _ (lookup_mem present)

theorem lookup_none_of_not_mem {α : Type} : ∀ {fields : List (String × α)} {name : String},
    fields.lookup name = none → name ∉ fields.map Prod.fst
  | [], _, _ => by simp
  | (other, value) :: rest, name, absent => by
    by_cases same : name = other
    · subst same
      simp [List.lookup] at absent
    · have differentNames : (name == other) = false := by simpa using same
      simp only [List.lookup, differentNames] at absent
      have := lookup_none_of_not_mem absent
      simp_all

theorem undeclared_none {names : List String} {fields : List (Field Declaration Identity Source)} :
    undeclared names fields = none ↔ ∀ field ∈ fields, field.1 ∈ names := by
  unfold undeclared
  cases hit : fields.find? (fun field => !names.contains field.1) with
  | none =>
    have := List.find?_eq_none.mp hit
    simp only [Option.map_none, true_iff]
    intro field present
    simpa using this field present
  | some field =>
    have present := List.mem_of_find?_eq_some hit
    have rejected := List.find?_some hit
    simp only [Option.map_some, reduceCtorEq, false_iff]
    intro all
    have listed := all field present
    simp [listed] at rejected

theorem undeclared_some {names : List String} {fields : List (Field Declaration Identity Source)} {name : String}
    (found : undeclared names fields = some name) : name ∈ fields.map Prod.fst ∧ name ∉ names := by
  unfold undeclared at found
  cases hit : fields.find? (fun field => !names.contains field.1) with
  | none => rw [hit] at found; simp at found
  | some field =>
    rw [hit] at found
    simp only [Option.map_some, Option.some.injEq] at found
    subst found
    have present := List.mem_of_find?_eq_some hit
    have rejected := List.find?_some hit
    exact ⟨List.mem_map_of_mem present, by simpa using rejected⟩

theorem keyAdmits_iff (type : ValueType Declaration) (key : MapKey) :
    keyAdmits type key = true ↔ Membership.key type key := by
  unfold keyAdmits
  letI := Membership.decideKey type key
  exact decide_eq_true_iff

theorem pointwise_length {α β : Type} {relation : α → β → Prop} :
    ∀ {lefts : List α} {rights : List β}, Pointwise relation lefts rights → lefts.length = rights.length
  | [], [], _ => rfl
  | _ :: lefts, _ :: rights, related => by
    simp only [Pointwise] at related
    simp [pointwise_length related.2]
  | [], _ :: _, related | _ :: _, [], related => by simp [Pointwise] at related

theorem pointwise_mem {α β : Type} {relation : α → β → Prop} :
    ∀ {lefts : List α} {rights : List β}, Pointwise relation lefts rights →
      ∀ right ∈ rights, ∃ left ∈ lefts, relation left right
  | [], [], _, _, present => by simp at present
  | left :: lefts, right' :: rights, related, right, present => by
    simp only [Pointwise] at related
    simp only [List.mem_cons] at present
    rcases present with same | later
    · exact ⟨left, by simp, same ▸ related.1⟩
    · obtain ⟨found, foundPresent, foundRelated⟩ := pointwise_mem related.2 right later
      exact ⟨found, List.mem_cons_of_mem _ foundPresent, foundRelated⟩
  | [], _ :: _, related, _, _ | _ :: _, [], related, _, _ => by simp [Pointwise] at related

theorem items_sound {step : Nat → Value Declaration Identity Source → Outcome Declaration Identity Source}
    {relation : Value Declaration Identity Source → Value Declaration Identity Source → Prop}
    (sound : ∀ position item result, step position item = .ok result → relation item result) :
    ∀ {position : Nat} {values results : List (Value Declaration Identity Source)},
      convertItems step position values = .ok results → Pointwise relation values results
  | _, [], results, converted => by
    simp only [convertItems, Except.ok.injEq] at converted
    subst converted
    simp [Pointwise]
  | position, item :: rest, results, converted => by
    simp only [convertItems] at converted
    split at converted
    · simp at converted
    · rename_i head headConverted
      split at converted
      · simp at converted
      · rename_i others othersConverted
        simp only [Except.ok.injEq] at converted
        subst converted
        exact ⟨sound _ _ _ headConverted, items_sound sound othersConverted⟩

theorem items_complete {step : Nat → Value Declaration Identity Source → Outcome Declaration Identity Source}
    {relation : Value Declaration Identity Source → Value Declaration Identity Source → Prop}
    (complete : ∀ position item result, relation item result → step position item = .ok result) :
    ∀ {position : Nat} {values results : List (Value Declaration Identity Source)},
      Pointwise relation values results → convertItems step position values = .ok results
  | _, [], [], _ => rfl
  | position, item :: rest, result :: results, related => by
    simp only [Pointwise] at related
    simp [convertItems, complete position item result related.1, items_complete complete related.2]
  | _, [], _ :: _, related | _, _ :: _, [], related => by simp [Pointwise] at related

theorem items_error {step : Nat → Value Declaration Identity Source → Outcome Declaration Identity Source} :
    ∀ {position : Nat} {values : List (Value Declaration Identity Source)} {failure : Failure},
      convertItems step position values = .error failure →
        ∃ offset item, values[offset]? = some item ∧ step (position + offset) item = .error failure
  | _, [], _, converted => by simp [convertItems] at converted
  | position, item :: rest, failure, converted => by
    simp only [convertItems] at converted
    split at converted
    · rename_i headFailure headFailed
      simp only [Except.error.injEq] at converted
      subst converted
      exact ⟨0, item, rfl, by simpa using headFailed⟩
    · split at converted
      · rename_i tailFailure tailFailed
        simp only [Except.error.injEq] at converted
        subst converted
        obtain ⟨offset, found, present, failed⟩ := items_error tailFailed
        exact ⟨offset + 1, found, by simpa using present, by simpa [Nat.add_assoc, Nat.add_comm 1 offset] using failed⟩
      · simp at converted

theorem entries_sound {step : MapKey → Value Declaration Identity Source → Outcome Declaration Identity Source}
    {relation : Entry Declaration Identity Source → Entry Declaration Identity Source → Prop}
    (sound : ∀ key value result, step key value = .ok result → relation (key, value) (key, result)) :
    ∀ {entries results : List (Entry Declaration Identity Source)},
      convertEntries step entries = .ok results → Pointwise relation entries results
  | [], results, converted => by
    simp only [convertEntries, Except.ok.injEq] at converted
    subst converted
    simp [Pointwise]
  | (key, value) :: rest, results, converted => by
    simp only [convertEntries] at converted
    split at converted
    · simp at converted
    · rename_i head headConverted
      split at converted
      · simp at converted
      · rename_i others othersConverted
        simp only [Except.ok.injEq] at converted
        subst converted
        exact ⟨sound _ _ _ headConverted, entries_sound sound othersConverted⟩

theorem entries_complete {step : MapKey → Value Declaration Identity Source → Outcome Declaration Identity Source}
    {relation : Entry Declaration Identity Source → Entry Declaration Identity Source → Prop}
    (complete : ∀ entry result, relation entry result → result.1 = entry.1 ∧ step entry.1 entry.2 = .ok result.2) :
    ∀ {entries results : List (Entry Declaration Identity Source)},
      Pointwise relation entries results → convertEntries step entries = .ok results
  | [], [], _ => rfl
  | (key, value) :: rest, result :: results, related => by
    simp only [Pointwise] at related
    obtain ⟨sameKey, converted⟩ := complete _ _ related.1
    obtain ⟨resultKey, resultValue⟩ := result
    simp only at sameKey converted
    subst sameKey
    simp [convertEntries, converted, entries_complete complete related.2]
  | [], _ :: _, related | _ :: _, [], related => by simp [Pointwise] at related

theorem entries_error {step : MapKey → Value Declaration Identity Source → Outcome Declaration Identity Source} :
    ∀ {entries : List (Entry Declaration Identity Source)} {failure : Failure},
      convertEntries step entries = .error failure → ∃ key value, (key, value) ∈ entries ∧ step key value = .error failure
  | [], _, converted => by simp [convertEntries] at converted
  | (key, value) :: rest, failure, converted => by
    simp only [convertEntries] at converted
    split at converted
    · rename_i headFailure headFailed
      simp only [Except.error.injEq] at converted
      subst converted
      exact ⟨key, value, by simp, headFailed⟩
    · split at converted
      · rename_i tailFailure tailFailed
        simp only [Except.error.injEq] at converted
        subst converted
        obtain ⟨found, foundValue, present, failed⟩ := entries_error tailFailed
        exact ⟨found, foundValue, List.mem_cons_of_mem _ present, failed⟩
      · simp at converted

theorem frame_refl (names : List String) :
    ∀ (seen : List String) (fields : List (Field Declaration Identity Source)), Frame names seen fields fields
  | _, [] => by simp [Frame]
  | seen, field :: rest => by simp [Frame, frame_refl names (field.1 :: seen) rest]

theorem frame_replace (name : String) (replacement : Value Declaration Identity Source) :
    ∀ {seen : List String} {fields : List (Field Declaration Identity Source)}, name ∉ seen →
      Frame [name] seen fields (replaceField name replacement fields)
  | _, [], _ => by simp [Frame, replaceField]
  | seen, (other, value) :: rest, absent => by
    by_cases same : other = name
    · subst same
      simp [replaceField, Frame, absent, frame_refl]
    · have tail := frame_replace name replacement (seen := other :: seen) (fields := rest)
        (by simp [Ne.symm same, absent])
      simp [replaceField, same, Frame, tail]

theorem frame_compose (first second : List String) :
    ∀ {seen : List String} {fields middle results : List (Field Declaration Identity Source)},
      Frame first seen fields middle → Frame second seen middle results → Frame (first ++ second) seen fields results
  | _, [], [], [], _, _ => by simp [Frame]
  | seen, field :: fields, step :: middle, result :: results, firstFrame, secondFrame => by
    simp only [Frame] at firstFrame secondFrame ⊢
    obtain ⟨stepName, stepValue, firstRest⟩ := firstFrame
    obtain ⟨resultName, resultValue, secondRest⟩ := secondFrame
    rw [stepName] at resultName resultValue secondRest
    refine ⟨resultName, ?_, frame_compose first second firstRest secondRest⟩
    rcases stepValue with declared | kept
    · exact Or.inl ⟨List.mem_append_left _ declared.1, declared.2⟩
    · rcases resultValue with declared | same
      · exact Or.inl ⟨List.mem_append_right _ declared.1, declared.2⟩
      · exact Or.inr (same.trans kept)
  | _, [], [], _ :: _, _, secondFrame => by simp [Frame] at secondFrame
  | _, [], _ :: _, _, firstFrame, _ | _, _ :: _, [], _, firstFrame, _ => by simp [Frame] at firstFrame
  | _, _ :: _, _ :: _, [], _, secondFrame => by simp [Frame] at secondFrame

theorem frame_lookup_undeclared {names : List String} {name : String} (absent : name ∉ names) :
    ∀ {seen : List String} {fields results : List (Field Declaration Identity Source)},
      Frame names seen fields results → results.lookup name = fields.lookup name
  | _, [], [], _ => rfl
  | seen, (other, value) :: fields, (resultName, resultValue) :: results, framed => by
    simp only [Frame] at framed
    obtain ⟨sameName, kept, rest⟩ := framed
    subst sameName
    by_cases same : name = resultName
    · subst same
      have : resultValue = value := by
        rcases kept with declared | equal
        · exact absurd declared.1 absent
        · exact equal
      simp [List.lookup, this]
    · have differentNames : (name == resultName) = false := by simpa using same
      simp only [List.lookup, differentNames]
      exact frame_lookup_undeclared absent rest
  | _, [], _ :: _, framed | _, _ :: _, [], framed => by simp [Frame] at framed

theorem frame_empty :
    ∀ {seen : List String} {fields results : List (Field Declaration Identity Source)},
      Frame [] seen fields results → results = fields
  | _, [], [], _ => rfl
  | seen, (other, value) :: fields, (resultName, resultValue) :: results, framed => by
    simp only [Frame, List.not_mem_nil, false_and, false_or] at framed
    obtain ⟨sameName, kept, rest⟩ := framed
    rw [sameName, kept, frame_empty rest]
  | _, [], _ :: _, framed | _, _ :: _, [], framed => by simp [Frame] at framed

theorem frame_forget {name : String} {names : List String} :
    ∀ {seen : List String} {fields results : List (Field Declaration Identity Source)}, name ∈ seen →
      Frame (name :: names) seen fields results → Frame names seen fields results
  | _, [], [], _, _ => by simp [Frame]
  | seen, field :: fields, result :: results, seenName, framed => by
    simp only [Frame, List.mem_cons] at framed ⊢
    obtain ⟨sameName, kept, rest⟩ := framed
    refine ⟨sameName, ?_, frame_forget (List.mem_cons_of_mem _ seenName) rest⟩
    rcases kept with ⟨declared | listed, unseen⟩ | equal
    · subst declared
      exact absurd seenName unseen
    · exact Or.inl ⟨listed, unseen⟩
    · exact Or.inr equal
  | _, [], _ :: _, _, framed | _, _ :: _, [], _, framed => by simp [Frame] at framed

theorem frame_unreplace {name : String} {names : List String} (absent : name ∉ names)
    {replacement : Value Declaration Identity Source} :
    ∀ {seen : List String} {fields results : List (Field Declaration Identity Source)}, name ∉ seen →
      Frame (name :: names) seen fields results → results.lookup name = some replacement →
        Frame names seen (replaceField name replacement fields) results
  | _, [], [], _, _, _ => by simp [Frame, replaceField]
  | seen, (other, value) :: fields, (resultName, resultValue) :: results, unseen, framed, found => by
    simp only [Frame] at framed
    obtain ⟨sameName, kept, rest⟩ := framed
    subst sameName
    by_cases same : resultName = name
    · subst same
      simp [List.lookup] at found
      subst found
      have tail := frame_forget (name := resultName) (names := names) (seen := resultName :: seen) (by simp) rest
      simp [replaceField, Frame, tail]
    · have differentNames : (name == resultName) = false := by simpa using Ne.symm same
      simp only [List.lookup, differentNames] at found
      have tail := frame_unreplace absent (seen := resultName :: seen) (by simp [Ne.symm same, unseen]) rest found
      have head : (resultName ∈ names ∧ resultName ∉ seen) ∨ resultValue = value := by
        rcases kept with ⟨declared, unseenOther⟩ | equal
        · simp only [List.mem_cons] at declared
          rcases declared with named | listed
          · exact absurd named same
          · exact Or.inl ⟨listed, unseenOther⟩
        · exact Or.inr equal
      rcases head with declared | equal
      · simp [replaceField, same, Frame, tail, declared]
      · simp [replaceField, same, Frame, tail, equal]
  | _, [], _ :: _, _, framed, _ | _, _ :: _, [], _, framed, _ => by simp [Frame] at framed

end Lemmas

section Exactness

theorem pointwise_map_left {α β γ : Type} {relation : β → γ → Prop} {transform : α → β} :
    ∀ {lefts : List α} {rights : List γ},
      Pointwise relation (lefts.map transform) rights ↔ Pointwise (fun left right => relation (transform left) right) lefts rights
  | [], [] => by simp [Pointwise]
  | _ :: lefts, _ :: rights => by simp [Pointwise, pointwise_map_left (lefts := lefts) (rights := rights)]
  | [], _ :: _ | _ :: _, [] => by simp [Pointwise]

theorem rowDeclared_congr [DecidableEq Declaration] (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
    ∀ (row : RecordType Declaration) {fields other results : List (Field Declaration Identity Source)},
      (∀ name ∈ rowNames row, other.lookup name = fields.lookup name) →
        RowDeclared boundary policy row other results → RowDeclared boundary policy row fields results
  | .variable id, _, _, _, _, _ => nomatch id
  | .rowEmpty, _, _, _, _, _ => by simp [RowDeclared]
  | .rowField name type rest, fields, other, results, same, declared => by
    simp only [RowDeclared] at declared ⊢
    obtain ⟨⟨found, converted, present, result, related⟩, others⟩ := declared
    refine ⟨⟨found, converted, ?_, result, related⟩,
      rowDeclared_congr boundary policy rest (fun listed member => same listed (by simp [rowNames, member])) others⟩
    rw [← same name (by simp [rowNames]), present]

theorem setKeys_sound [DecidableEq Declaration] {element : ValueType Declaration} {path : List PathStep} :
    ∀ {position : Nat} {seen : List MapKey} {values : List (Value Declaration Identity Source)} {keys : List MapKey},
      setKeys element path position seen values = .ok keys →
        values.map keyOf = keys.map some ∧ (∀ key ∈ keys, Membership.key element key) ∧ keys.Nodup ∧
          ∀ key ∈ keys, key ∉ seen
  | _, _, [], keys, converted => by
    simp only [setKeys, Except.ok.injEq] at converted
    subst converted
    simp
  | position, seen, item :: rest, keys, converted => by
    simp only [setKeys] at converted
    split at converted
    · simp at converted
    · rename_i key keyed
      by_cases admitted : keyAdmits element key = true
      · rw [ite_eq_left admitted] at converted
        by_cases repeated : key ∈ seen
        · rw [ite_eq_left repeated] at converted
          simp at converted
        · rw [ite_eq_right repeated] at converted
          split at converted
          · simp at converted
          · rename_i others othersConverted
            simp only [Except.ok.injEq] at converted
            subst converted
            obtain ⟨mapped, admittedAll, distinct, fresh⟩ := setKeys_sound othersConverted
            refine ⟨by simp [keyed, mapped], ?_, ?_, ?_⟩
            · intro found present
              simp only [List.mem_cons] at present
              rcases present with same | later
              · exact same ▸ (keyAdmits_iff element key).mp admitted
              · exact admittedAll found later
            · exact List.nodup_cons.mpr ⟨fun present => fresh key present (by simp), distinct⟩
            · intro found present
              simp only [List.mem_cons] at present
              rcases present with same | later
              · exact same ▸ repeated
              · exact fun inSeen => fresh found later (List.mem_cons_of_mem _ inSeen)
      · rw [ite_eq_right admitted] at converted
        simp at converted

theorem setKeys_complete [DecidableEq Declaration] {element : ValueType Declaration} {path : List PathStep} :
    ∀ {position : Nat} {seen : List MapKey} {values : List (Value Declaration Identity Source)} {keys : List MapKey},
      values.map keyOf = keys.map some → (∀ key ∈ keys, Membership.key element key) → keys.Nodup →
        (∀ key ∈ keys, key ∉ seen) → setKeys element path position seen values = .ok keys
  | _, _, [], [], _, _, _, _ => rfl
  | position, seen, item :: rest, key :: keys, mapped, admitted, distinct, fresh => by
    simp only [List.map_cons, List.cons.injEq] at mapped
    have admittedKey : keyAdmits element key = true := (keyAdmits_iff element key).mpr (admitted key (by simp))
    have unseen : key ∉ seen := fresh key (by simp)
    have tail := setKeys_complete (position := position + 1) (seen := key :: seen) (path := path) mapped.2
      (fun found present => admitted found (List.mem_cons_of_mem _ present)) (List.nodup_cons.mp distinct).2
      (by
        intro found present inSeen
        simp only [List.mem_cons] at inSeen
        rcases inSeen with same | earlier
        · exact (List.nodup_cons.mp distinct).1 (same ▸ present)
        · exact fresh found (List.mem_cons_of_mem _ present) earlier)
    simp [setKeys, mapped.1, admittedKey, unseen, tail]
  | _, _, [], _ :: _, mapped, _, _, _ | _, _, _ :: _, [], mapped, _, _, _ => by simp at mapped

theorem recordBody_sound {policy : FieldPolicy} {path : List PathStep} {names : List String}
    {keep project : List (Field Declaration Identity Source) → Except Failure (List (Field Declaration Identity Source))}
    {keepSpec projectSpec : List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop}
    (keepSound : ∀ fields results, keep fields = .ok results → keepSpec fields results)
    (projectSound : ∀ fields results, project fields = .ok results → projectSpec fields results)
    {fields results : List (Field Declaration Identity Source)}
    (converted : recordBody policy path names keep project fields = .ok results) :
    RecordSpec policy names keepSpec projectSpec fields results := by
  cases policy with
  | keep => exact keepSound _ _ converted
  | stripped => exact projectSound _ _ converted
  | exact =>
    simp only [recordBody] at converted
    split at converted
    · simp at converted
    · rename_i declared
      exact ⟨undeclared_none.mp declared, keepSound _ _ converted⟩

theorem recordBody_complete {policy : FieldPolicy} {path : List PathStep} {names : List String}
    {keep project : List (Field Declaration Identity Source) → Except Failure (List (Field Declaration Identity Source))}
    {keepSpec projectSpec : List (Field Declaration Identity Source) → List (Field Declaration Identity Source) → Prop}
    (keepComplete : ∀ fields results, keepSpec fields results → keep fields = .ok results)
    (projectComplete : ∀ fields results, projectSpec fields results → project fields = .ok results)
    {fields results : List (Field Declaration Identity Source)}
    (specified : RecordSpec policy names keepSpec projectSpec fields results) :
    recordBody policy path names keep project fields = .ok results := by
  cases policy with
  | keep => exact keepComplete _ _ specified
  | stripped => exact projectComplete _ _ specified
  | exact =>
    obtain ⟨declared, kept⟩ := specified
    simp only [recordBody, undeclared_none.mpr declared]
    exact keepComplete _ _ kept

section Decisions

variable [DecidableEq Declaration]

theorem checkMember_ok {boundary : Boundary Declaration Identity Source} {path : List PathStep}
    {type : ValueType Declaration} {candidate result : Value Declaration Identity Source} :
    checkMember boundary path type candidate = .ok result ↔
      Membership.value boundary.world type candidate ∧ result = candidate := by
  unfold checkMember
  by_cases member : Membership.check boundary.world boundary.contracts type candidate = true
  · rw [ite_eq_left member]
    have := (Membership.check_iff boundary.world boundary.contracts type candidate).mp member
    simp [this, eq_comm]
  · rw [ite_eq_right member]
    have : ¬ Membership.value boundary.world type candidate :=
      fun present => member ((Membership.check_iff boundary.world boundary.contracts type candidate).mpr present)
    simp [this]

theorem checkMember_error {boundary : Boundary Declaration Identity Source} {path : List PathStep}
    {type : ValueType Declaration} {candidate : Value Declaration Identity Source} {failure : Failure}
    (failed : checkMember boundary path type candidate = .error failure) :
    failure = ⟨path, .mismatch⟩ ∧ ¬ Membership.value boundary.world type candidate := by
  unfold checkMember at failed
  by_cases member : Membership.check boundary.world boundary.contracts type candidate = true
  · rw [ite_eq_left member] at failed
    simp at failed
  · rw [ite_eq_right member] at failed
    simp only [Except.error.injEq] at failed
    exact ⟨failed.symm,
      fun present => member ((Membership.check_iff boundary.world boundary.contracts type candidate).mpr present)⟩

theorem null_check_iff (boundary : Boundary Declaration Identity Source) (candidate : Value Declaration Identity Source) :
    Membership.check boundary.world boundary.contracts (.primitive .null) candidate = true ↔
      Membership.primitive .null candidate := by
  rw [Membership.check_iff]
  simp [Membership.value]

theorem wire_exact {enums : WireEnums Declaration} (wires : WireWellFormed enums) {declaration : Declaration}
    {mapping : List (String × String)} (declared : enums.strings declaration = some mapping) (path : List PathStep)
    (candidate result : Value Declaration Identity Source) :
    wire declaration mapping path candidate = .ok result ↔ WireSpec declaration mapping candidate result := by
  unfold wire WireSpec
  constructor
  · intro converted
    split at converted
    · rename_i actual variant fields erased
      split at converted
      · rename_i accepted
        obtain ⟨sameDeclaration, listed, empty⟩ := accepted
        subst sameDeclaration
        simp only [Except.ok.injEq] at converted
        exact Or.inl ⟨variant, fields, erased, List.isEmpty_iff.mp empty, listed, converted.symm⟩
      · simp at converted
    · rename_i text erased
      split at converted
      · rename_i variant found
        simp only [Except.ok.injEq] at converted
        exact Or.inr ⟨text, variant, erased, WireEnums.variantOf_mem found, converted.symm⟩
      · simp at converted
    · simp at converted
  · rintro (⟨variant, fields, erased, empty, listed, rfl⟩ | ⟨text, variant, erased, present, rfl⟩)
    · subst empty
      simp [erased, listed]
    · cases found : WireEnums.variantOf mapping text with
      | none => exact absurd present (WireEnums.variantOf_none found variant)
      | some other =>
        have same := wire_variant_unique wires declared (WireEnums.variantOf_mem found) present
        subst same
        simp [erased, found]

omit [DecidableEq Declaration] in
theorem entryViolations_nil (name : String) (presence : ExportPresence)
    (admitted : Value Declaration Identity Source → Bool) (actual : List (Field Declaration Identity Source)) :
    entryViolations name presence admitted actual = [] ↔
      match actual.lookup name with
      | some found => admitted found = true
      | none => Membership.absentAllowed presence := by
  unfold entryViolations
  cases actual.lookup name with
  | none => cases presence <;> simp [Membership.absentAllowed]
  | some found => by_cases accepted : admitted found = true <;> simp [accepted]

theorem exportViolations_nil (boundary : Boundary Declaration Identity Source) :
    ∀ (exports : ExportType Declaration) (actual : List (Field Declaration Identity Source)),
      exportViolations boundary exports actual = [] ↔ Membership.exports boundary.world exports actual
  | .variable id, _ => nomatch id
  | .exportsEmpty, _ => by simp [exportViolations, Membership.exports]
  | .exportValue name presence type rest, actual => by
    simp only [exportViolations, List.append_eq_nil_iff, entryViolations_nil, Membership.exports,
      exportViolations_nil boundary rest actual]
    cases actual.lookup name <;> simp [Membership.check_iff]
  | .exportCallable name presence signature rest, actual => by
    simp only [exportViolations, List.append_eq_nil_iff, entryViolations_nil, Membership.exports,
      exportViolations_nil boundary rest actual]
    cases actual.lookup name with
    | none => simp
    | some found =>
      letI := Membership.decideCallable boundary.world boundary.contracts signature found
      simp
  | .exportGenericCallable name presence kind rest, actual => by
    simp only [exportViolations, List.append_eq_nil_iff, entryViolations_nil, Membership.exports,
      exportViolations_nil boundary rest actual]
    cases actual.lookup name with
    | none => simp
    | some found =>
      letI := Membership.decideGeneric boundary.world boundary.contracts kind found
      simp

omit [DecidableEq Declaration] in
theorem unexpectedExports_nil (closure : ExportClosure) (exports : ExportType Declaration)
    (actual : List (Field Declaration Identity Source)) :
    unexpectedExports closure exports actual = [] ↔
      (closure = .exact → ∀ field ∈ actual, field.1 ∈ Membership.exportNames exports) := by
  cases closure with
  | «open» => simp [unexpectedExports]
  | exact =>
    simp [unexpectedExports, List.filter_eq_nil_iff]

theorem moduleViolations_nil (boundary : Boundary Declaration Identity Source) (closure : ExportClosure)
    (exports : ExportType Declaration) (reference : Reference Identity .module)
    (actual : List (Field Declaration Identity Source)) {candidate : Value Declaration Identity Source}
    (erased : candidate.eraseNominal = .module reference actual) :
    moduleViolations boundary closure exports actual = [] ↔
      Membership.value boundary.world (.module closure exports) candidate := by
  simp only [moduleViolations, List.append_eq_nil_iff, exportViolations_nil, unexpectedExports_nil,
    Membership.value, erased]

mutual
  theorem convert_exact (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (type : ValueType Declaration), WellFormed type → ∀ (path : List PathStep) candidate result,
        convert boundary policy path type candidate = .ok result ↔ Converts boundary policy type candidate result
    | .variable id, _, _, _, _ => nomatch id
    | .list element, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      have sound := fun position item converted (ok : convert boundary policy (path ++ [.index position]) element item =
          .ok converted) => (convert_exact boundary wires policy element wellFormed _ item converted).mp ok
      have complete := fun position item converted (related : Converts boundary policy element item converted) =>
        (convert_exact boundary wires policy element wellFormed (path ++ [.index position]) item converted).mpr related
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i values listed
          cases items : convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item)
              0 values with
          | error failure => simp [items, Except.map] at converted
          | ok results =>
            simp only [items, Except.map, Except.ok.injEq] at converted
            exact ⟨values, results, listed, converted.symm, items_sound sound items⟩
        · simp at converted
      · rintro ⟨values, results, listed, rfl, related⟩
        simp only [listed]
        rw [items_complete complete related]
        rfl
    | .nonEmpty element, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      have sound := fun position item converted (ok : convert boundary policy (path ++ [.index position]) element item =
          .ok converted) => (convert_exact boundary wires policy element wellFormed _ item converted).mp ok
      have complete := fun position item converted (related : Converts boundary policy element item converted) =>
        (convert_exact boundary wires policy element wellFormed (path ++ [.index position]) item converted).mpr related
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i first rest listed
          cases items : convertItems (fun position item => convert boundary policy (path ++ [.index position]) element item)
              0 (first :: rest) with
          | error failure => simp [items, Except.map] at converted
          | ok results =>
            simp only [items, Except.map, Except.ok.injEq] at converted
            exact ⟨first :: rest, results, listed, by simp, converted.symm, items_sound sound items⟩
        · simp at converted
      · rintro ⟨values, results, listed, nonempty, rfl, related⟩
        cases values with
        | nil => exact absurd rfl nonempty
        | cons first rest =>
          simp only [listed]
          rw [items_complete complete related]
          rfl
    | .map keyType element, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i present erased
          cases entries : convertEntries (fun key value =>
              if keyAdmits keyType key then convert boundary policy (path ++ [.key key]) element value
              else .error ⟨path, .keyMismatch key⟩) present with
          | error failure => simp [entries, Except.map] at converted
          | ok results =>
            simp only [entries, Except.map, Except.ok.injEq] at converted
            refine ⟨results, converted.symm, Or.inl ⟨present, erased, entries_sound ?_ entries⟩⟩
            intro key value converted' stepped
            by_cases admitted : keyAdmits keyType key = true
            · rw [ite_eq_left admitted] at stepped
              exact ⟨(keyAdmits_iff keyType key).mp admitted, rfl,
                (convert_exact boundary wires policy element wellFormed.2 _ value converted').mp stepped⟩
            · rw [ite_eq_right admitted] at stepped
              simp at stepped
        · rename_i fields erased
          split at converted
          · rename_i stringKeys
            cases entries : convertEntries (fun key value => convert boundary policy (path ++ [.key key]) element value)
                (fields.map recordEntry) with
            | error failure => simp [entries, Except.map] at converted
            | ok results =>
              simp only [entries, Except.map, Except.ok.injEq] at converted
              refine ⟨results, converted.symm, Or.inr ⟨stringKeys, fields, erased, ?_⟩⟩
              have related := entries_sound
                (relation := fun entry converted => converted.1 = entry.1 ∧
                  Converts boundary policy element entry.2 converted.2)
                (fun key value converted' stepped =>
                  ⟨rfl, (convert_exact boundary wires policy element wellFormed.2 _ value converted').mp stepped⟩) entries
              exact pointwise_map_left.mp related
          · simp at converted
        · simp at converted
      · rintro ⟨results, rfl, ⟨present, erased, related⟩ | ⟨stringKeys, fields, erased, related⟩⟩
        · simp only [erased]
          rw [entries_complete (fun entry converted relatedEntry => ⟨relatedEntry.2.1, by
            rw [ite_eq_left ((keyAdmits_iff keyType entry.1).mpr relatedEntry.1)]
            exact (convert_exact boundary wires policy element wellFormed.2 _ _ _).mpr relatedEntry.2.2⟩) related]
          rfl
        · simp only [erased, stringKeys, ite_true]
          rw [entries_complete (relation := fun entry converted => converted.1 = entry.1 ∧
              Converts boundary policy element entry.2 converted.2)
            (fun entry converted relatedEntry => ⟨relatedEntry.1,
              (convert_exact boundary wires policy element wellFormed.2 _ _ _).mpr relatedEntry.2⟩)
            (pointwise_map_left.mpr related)]
          rfl
    | .set element, _, path, candidate, result => by
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · exact Or.inl (checkMember_ok.mp converted)
        · split at converted
          · rename_i values listed
            cases keys : setKeys element path 0 [] values with
            | error failure => simp [keys, Except.map] at converted
            | ok found =>
              simp only [keys, Except.map, Except.ok.injEq] at converted
              obtain ⟨mapped, admitted, distinct, _⟩ := setKeys_sound keys
              exact Or.inr ⟨values, found, listed, mapped, admitted, distinct, converted.symm⟩
          · simp at converted
      · rintro (⟨member, same⟩ | ⟨values, keys, listed, mapped, admitted, distinct, rfl⟩)
        · rw [same]
          have isSet : ∃ present, candidate.eraseNominal = .set present := by
            simp only [Membership.value] at member
            split at member
            · exact ⟨_, by assumption⟩
            · exact absurd member id
          obtain ⟨present, erased⟩ := isSet
          simp only [erased]
          exact checkMember_ok.mpr ⟨member, rfl⟩
        · have notSet : ∀ present, candidate.eraseNominal ≠ .set present := by
            intro present erased
            simp [listItems, erased] at listed
          split
          · rename_i present erased
            exact absurd erased (notSet present)
          · simp only [listed]
            rw [setKeys_complete (path := path) (position := 0) mapped admitted distinct (by simp)]
            rfl
    | .record row, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      have keepSound := fun fields results (ok : convertRow boundary policy path row fields = .ok results) =>
        (convertRow_exact boundary wires policy row wellFormed path fields results).mp ok
      have keepComplete := fun fields results related =>
        (convertRow_exact boundary wires policy row wellFormed path fields results).mpr related
      have projectSound := fun fields results (ok : projectRow boundary policy path row fields = .ok results) =>
        (projectRow_exact boundary wires policy row wellFormed path fields results).mp ok
      have projectComplete := fun fields results related =>
        (projectRow_exact boundary wires policy row wellFormed path fields results).mpr related
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i fields fielded
          cases body : recordBody policy path (rowNames row) (convertRow boundary policy path row)
              (projectRow boundary policy path row) fields with
          | error failure => simp [body, Except.map] at converted
          | ok results =>
            simp only [body, Except.map, Except.ok.injEq] at converted
            exact ⟨fields, results, fielded, converted.symm, recordBody_sound keepSound projectSound body⟩
        · simp at converted
      · rintro ⟨fields, results, fielded, rfl, specified⟩
        simp only [fielded]
        rw [recordBody_complete keepComplete projectComplete specified]
        rfl
    | .nominal declaration row, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      have keepSound := fun fields results (ok : convertRow boundary policy path row fields = .ok results) =>
        (convertRow_exact boundary wires policy row wellFormed path fields results).mp ok
      have keepComplete := fun fields results related =>
        (convertRow_exact boundary wires policy row wellFormed path fields results).mpr related
      have projectSound := fun fields results (ok : projectRow boundary policy path row fields = .ok results) =>
        (projectRow_exact boundary wires policy row wellFormed path fields results).mp ok
      have projectComplete := fun fields results related =>
        (projectRow_exact boundary wires policy row wellFormed path fields results).mpr related
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i fields fielded
          cases body : recordBody policy path (rowNames row) (convertRow boundary policy path row)
              (projectRow boundary policy path row) fields with
          | error failure => simp [body, Except.map] at converted
          | ok results =>
            simp only [body, Except.map, Except.ok.injEq] at converted
            exact ⟨fields, results, fielded, converted.symm, recordBody_sound keepSound projectSound body⟩
        · simp at converted
      · rintro ⟨fields, results, fielded, rfl, specified⟩
        simp only [fielded]
        rw [recordBody_complete keepComplete projectComplete specified]
        rfl
    | .optional element, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      simp only [convert, Converts]
      by_cases null : Membership.check boundary.world boundary.contracts (.primitive .null) candidate = true
      · rw [ite_eq_left null]
        have isNull := (null_check_iff boundary candidate).mp null
        simp [isNull, eq_comm]
      · rw [ite_eq_right null]
        have notNull : ¬ Membership.primitive .null candidate := fun isNull => null ((null_check_iff boundary candidate).mpr isNull)
        simp only [notNull, false_and, not_false_eq_true, true_and, false_or]
        exact convert_exact boundary wires policy element wellFormed path candidate result
    | .union first second rest, wellFormed, path, candidate, result => by
      simp only [WellFormed] at wellFormed
      obtain ⟨firstFormed, secondFormed, restFormed⟩ := wellFormed
      have firstExact := convert_exact boundary wires policy first firstFormed path candidate
      have secondExact := convert_exact boundary wires policy second secondFormed path candidate
      have restExact := convertMembers_exact boundary wires policy rest restFormed path candidate
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i accepted firstOk
          simp only [Except.ok.injEq] at converted
          subst converted
          exact Or.inl ((firstExact accepted).mp firstOk)
        · rename_i firstFailure firstFailed
          have firstNone : ∀ other, ¬ Converts boundary policy first candidate other := fun other related => by
            rw [(firstExact other).mpr related] at firstFailed
            simp at firstFailed
          split at converted
          · rename_i accepted secondOk
            simp only [Except.ok.injEq] at converted
            subst converted
            exact Or.inr ⟨firstNone, Or.inl ((secondExact accepted).mp secondOk)⟩
          · rename_i secondFailure secondFailed
            have secondNone : ∀ other, ¬ Converts boundary policy second candidate other := fun other related => by
              rw [(secondExact other).mpr related] at secondFailed
              simp at secondFailed
            split at converted
            · rename_i accepted restSome
              simp only [Except.ok.injEq] at converted
              subst converted
              exact Or.inr ⟨firstNone, Or.inr ⟨secondNone, (restExact accepted).mp restSome⟩⟩
            · simp at converted
      · rintro (related | ⟨firstNone, related | ⟨secondNone, related⟩⟩)
        · simp [(firstExact result).mpr related]
        · have firstFailed : ∃ failure, convert boundary policy path first candidate = .error failure := by
            cases firstConverted : convert boundary policy path first candidate with
            | error failure => exact ⟨failure, rfl⟩
            | ok other => exact absurd ((firstExact other).mp firstConverted) (firstNone other)
          obtain ⟨failure, firstFailed⟩ := firstFailed
          simp [firstFailed, (secondExact result).mpr related]
        · have firstFailed : ∃ failure, convert boundary policy path first candidate = .error failure := by
            cases firstConverted : convert boundary policy path first candidate with
            | error failure => exact ⟨failure, rfl⟩
            | ok other => exact absurd ((firstExact other).mp firstConverted) (firstNone other)
          have secondFailed : ∃ failure, convert boundary policy path second candidate = .error failure := by
            cases secondConverted : convert boundary policy path second candidate with
            | error failure => exact ⟨failure, rfl⟩
            | ok other => exact absurd ((secondExact other).mp secondConverted) (secondNone other)
          obtain ⟨failure, firstFailed⟩ := firstFailed
          obtain ⟨failure', secondFailed⟩ := secondFailed
          simp [firstFailed, secondFailed, (restExact result).mpr related]
    | .tagged declaration, _, path, candidate, result => by
      simp only [convert, Converts]
      cases declared : boundary.enums.strings declaration with
      | none => exact checkMember_ok
      | some mapping => exact wire_exact wires declared path candidate result
    | .module closure exports, _, path, candidate, result => by
      simp only [convert, Converts]
      constructor
      · intro converted
        split at converted
        · rename_i reference actual erased
          split at converted
          · rename_i none'
            simp only [Except.ok.injEq] at converted
            exact ⟨(moduleViolations_nil boundary closure exports reference actual erased).mp none', converted.symm⟩
          · simp at converted
        · simp at converted
      · rintro ⟨member, same⟩
        rw [same]
        have isModule : ∃ reference actual, candidate.eraseNominal = .module reference actual := by
          simp only [Membership.value] at member
          split at member
          · exact ⟨_, _, by assumption⟩
          · exact absurd member id
        obtain ⟨reference, actual, erased⟩ := isModule
        simp only [erased, (moduleViolations_nil boundary closure exports reference actual erased).mpr member]
    | .primitive type, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .stream element, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .result success failure, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .functionClass kind, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .callable signature, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .schemeCallable declaration, _, path, candidate, result => by
      simp only [convert, Converts]; exact checkMember_ok
    | .errorFamily declaration, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .errorVariant family variant, _, path, candidate, result => by
      simp only [convert, Converts]; exact checkMember_ok
    | .errorFacet facet, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .relativePath, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok
    | .boundedInteger bounds, _, path, candidate, result => by simp only [convert, Converts]; exact checkMember_ok

  theorem convertRow_exact (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ (path : List PathStep) fields results,
        convertRow boundary policy path row fields = .ok results ↔
          RowDeclared boundary policy row fields results ∧ Frame (rowNames row) [] fields results
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, path, fields, results => by
      simp only [convertRow, RowDeclared, rowNames, true_and, Except.ok.injEq]
      constructor
      · rintro rfl
        exact frame_refl [] [] fields
      · intro framed
        exact (frame_empty framed).symm
    | .rowField name type rest, wellFormed, path, fields, results => by
      simp only [RowWellFormed] at wellFormed
      obtain ⟨fresh, typeFormed, restFormed⟩ := wellFormed
      have different : ∀ other ∈ rowNames rest, other ≠ name := fun other listed same => fresh (same ▸ listed)
      constructor
      · intro converted
        simp only [convertRow] at converted
        split at converted
        · simp at converted
        · rename_i found present
          split at converted
          · simp at converted
          · rename_i replacement replaced
            obtain ⟨declared, framed⟩ :=
              (convertRow_exact boundary wires policy rest restFormed path _ results).mp converted
            simp only [RowDeclared, rowNames]
            refine ⟨⟨⟨found, replacement, present, ?_,
              (convert_exact boundary wires policy type typeFormed _ found replacement).mp replaced⟩, ?_⟩, ?_⟩
            · rw [frame_lookup_undeclared fresh framed]
              exact lookup_replaceField_same name replacement present
            · exact rowDeclared_congr boundary policy rest
                (fun other listed => lookup_replaceField_other name other replacement (different other listed) fields)
                declared
            · exact frame_compose [name] (rowNames rest) (frame_replace name replacement (by simp)) framed
      · rintro ⟨declaredAll, framed⟩
        simp only [RowDeclared] at declaredAll
        simp only [rowNames] at framed
        obtain ⟨⟨found, replacement, present, resultPresent, related⟩, declared⟩ := declaredAll
        simp only [convertRow, present,
          (convert_exact boundary wires policy type typeFormed (path ++ [.field name]) found replacement).mpr related]
        refine (convertRow_exact boundary wires policy rest restFormed path _ results).mpr ⟨?_, ?_⟩
        · exact rowDeclared_congr boundary policy rest
            (fun other listed => (lookup_replaceField_other name other replacement (different other listed) fields).symm)
            declared
        · exact frame_unreplace fresh (by simp) framed resultPresent

  theorem projectRow_exact (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ (path : List PathStep) fields results,
        projectRow boundary policy path row fields = .ok results ↔ RowProject boundary policy row fields results
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, path, fields, results => by
      simp only [projectRow, RowProject, Except.ok.injEq]
      exact eq_comm
    | .rowField name type rest, wellFormed, path, fields, results => by
      simp only [RowWellFormed] at wellFormed
      obtain ⟨_, typeFormed, restFormed⟩ := wellFormed
      constructor
      · intro converted
        simp only [projectRow] at converted
        split at converted
        · simp at converted
        · rename_i found present
          split at converted
          · simp at converted
          · rename_i replacement replaced
            split at converted
            · simp at converted
            · rename_i others projected
              simp only [Except.ok.injEq] at converted
              simp only [RowProject]
              exact ⟨found, replacement, others, present,
                (convert_exact boundary wires policy type typeFormed _ found replacement).mp replaced, converted.symm,
                (projectRow_exact boundary wires policy rest restFormed path fields others).mp projected⟩
      · intro projectedAll
        simp only [RowProject] at projectedAll
        obtain ⟨found, replacement, others, present, related, rfl, projected⟩ := projectedAll
        simp [projectRow, present,
          (convert_exact boundary wires policy type typeFormed (path ++ [.field name]) found replacement).mpr related,
          (projectRow_exact boundary wires policy rest restFormed path fields others).mpr projected]

  theorem convertMembers_exact (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (rest : UnionTail Declaration), MembersWellFormed rest → ∀ (path : List PathStep) candidate result,
        convertMembers boundary policy path rest candidate = some result ↔ FirstMember boundary policy rest candidate result
    | .variable id, _, _, _, _ => nomatch id
    | .membersEmpty, _, path, candidate, result => by simp [convertMembers, FirstMember]
    | .member type rest, wellFormed, path, candidate, result => by
      simp only [MembersWellFormed] at wellFormed
      have typeExact := convert_exact boundary wires policy type wellFormed.1 path candidate
      have restExact := convertMembers_exact boundary wires policy rest wellFormed.2 path candidate
      simp only [convertMembers, FirstMember]
      constructor
      · intro converted
        split at converted
        · rename_i accepted typeOk
          simp only [Option.some.injEq] at converted
          subst converted
          exact Or.inl ((typeExact accepted).mp typeOk)
        · rename_i failure typeFailed
          refine Or.inr ⟨fun other related => ?_, (restExact result).mp converted⟩
          rw [(typeExact other).mpr related] at typeFailed
          simp at typeFailed
      · rintro (related | ⟨typeNone, related⟩)
        · simp [(typeExact result).mpr related]
        · cases typeConverted : convert boundary policy path type candidate with
          | error failure => simpa [typeConverted] using (restExact result).mpr related
          | ok other => exact absurd ((typeExact other).mp typeConverted) (typeNone other)
end

end Decisions

end Exactness

section Publication

theorem recordFields_publish (world : MembershipWorld Declaration Identity Source) (policy : FieldPolicy)
    (candidate : Value Declaration Identity Source) (fields : List (Field Declaration Identity Source)) :
    Membership.recordFields world (publish policy candidate fields) = some fields := by
  simp [Membership.recordFields, eraseNominal_publish]

theorem row_cons_irrelevant (world : MembershipWorld Declaration Identity Source) (name : String)
    (value : Value Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (Field Declaration Identity Source)), name ∉ rowNames row →
      (Membership.row world row ((name, value) :: fields) ↔ Membership.row world row fields)
  | .variable id, _, _ => nomatch id
  | .rowEmpty, _, _ => by simp [Membership.row]
  | .rowField other type rest, fields, absent => by
    simp only [rowNames, List.mem_cons, not_or] at absent
    have differentNames : (other == name) = false := by simpa using Ne.symm absent.1
    simp only [Membership.row, List.lookup, differentNames, row_cons_irrelevant world name value rest fields absent.2]

theorem eraseNominal_eraseNominal (candidate : Value Declaration Identity Source) :
    candidate.eraseNominal.eraseNominal = candidate.eraseNominal :=
  Value.eraseNominal_idempotent candidate

mutual
  theorem converts_member (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      ∀ (type : ValueType Declaration), WellFormed type → ∀ (candidate result : Value Declaration Identity Source),
        Converts boundary policy type candidate result → Membership.value boundary.world type result
    | .variable id, _, _, _, _ => nomatch id
    | .list element, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      obtain ⟨values, results, _, rfl, related⟩ := converted
      simp only [Membership.value, Value.eraseNominal]
      intro item present
      obtain ⟨source, _, itemConverted⟩ := pointwise_mem related item present
      exact converts_member boundary policy element wellFormed source item itemConverted
    | .nonEmpty element, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      obtain ⟨values, results, _, nonempty, rfl, related⟩ := converted
      simp only [Membership.value, Value.eraseNominal]
      refine ⟨fun empty => nonempty (List.eq_nil_of_length_eq_zero (by simp [pointwise_length related, empty])), ?_⟩
      intro item present
      obtain ⟨source, _, itemConverted⟩ := pointwise_mem related item present
      exact converts_member boundary policy element wellFormed source item itemConverted
    | .map keyType element, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      obtain ⟨results, rfl, ⟨present, _, related⟩ | ⟨stringKeys, fields, _, related⟩⟩ := converted
      · simp only [Membership.value, Value.eraseNominal]
        intro entry listed
        obtain ⟨source, _, admitted, sameKey, entryConverted⟩ := pointwise_mem related entry listed
        exact ⟨sameKey ▸ admitted, converts_member boundary policy element wellFormed.2 _ _ entryConverted⟩
      · subst stringKeys
        simp only [Membership.value, Value.eraseNominal]
        intro entry listed
        obtain ⟨source, _, sameKey, entryConverted⟩ := pointwise_mem related entry listed
        refine ⟨?_, converts_member boundary policy element wellFormed.2 _ _ entryConverted⟩
        rw [sameKey]
        simp [Membership.key, Membership.primitive, MapKey.scalar, Value.eraseNominal]
    | .set element, _, candidate, result, converted => by
      simp only [Converts] at converted
      rcases converted with ⟨member, rfl⟩ | ⟨values, keys, _, _, admitted, _, rfl⟩
      · exact member
      · simpa [Membership.value, Value.eraseNominal] using admitted
    | .record row, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      obtain ⟨fields, results, _, rfl, specified⟩ := converted
      simp only [Membership.value, recordFields_publish]
      cases policy with
      | keep => exact rowDeclared_member boundary .keep row wellFormed fields results specified.1
      | exact => exact rowDeclared_member boundary .exact row wellFormed fields results specified.2.1
      | stripped => exact rowProject_member boundary .stripped row wellFormed fields results specified
    | .nominal declaration row, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      obtain ⟨fields, results, _, rfl, specified⟩ := converted
      simp only [Membership.value, Membership.recordFields, Value.eraseNominal, true_and]
      cases policy with
      | keep => exact rowDeclared_member boundary .keep row wellFormed fields results specified.1
      | exact => exact rowDeclared_member boundary .exact row wellFormed fields results specified.2.1
      | stripped => exact rowProject_member boundary .stripped row wellFormed fields results specified
    | .optional element, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      simp only [Membership.value]
      rcases converted with ⟨null, rfl⟩ | ⟨_, inner⟩
      · exact Or.inl null
      · exact Or.inr (converts_member boundary policy element wellFormed candidate result inner)
    | .union first second rest, wellFormed, candidate, result, converted => by
      simp only [WellFormed] at wellFormed
      simp only [Converts] at converted
      simp only [Membership.value]
      rcases converted with first' | ⟨_, second' | ⟨_, rest'⟩⟩
      · exact Or.inl (converts_member boundary policy first wellFormed.1 candidate result first')
      · exact Or.inr (Or.inl (converts_member boundary policy second wellFormed.2.1 candidate result second'))
      · exact Or.inr (Or.inr (firstMember_member boundary policy rest wellFormed.2.2 candidate result rest'))
    | .tagged declaration, _, candidate, result, converted => by
      simp only [Converts] at converted
      cases declared : boundary.enums.strings declaration with
      | none =>
        simp only [declared] at converted
        obtain ⟨member, rfl⟩ := converted
        exact member
      | some mapping =>
        simp only [declared, WireSpec] at converted
        rcases converted with ⟨variant, fields, erased, _, _, rfl⟩ | ⟨text, variant, _, _, rfl⟩
        · simp [Membership.value, erased]
        · simp [Membership.value, Value.eraseNominal]
    | .module closure exports, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .primitive type, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .stream element, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .result success failure, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .functionClass kind, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .callable signature, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .schemeCallable declaration, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .errorFamily declaration, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .errorVariant family variant, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .errorFacet facet, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .relativePath, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member
    | .boundedInteger bounds, _, candidate, result, converted => by
      simp only [Converts] at converted; obtain ⟨member, rfl⟩ := converted; exact member

  theorem rowDeclared_member (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ fields results,
        RowDeclared boundary policy row fields results → Membership.row boundary.world row results
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, _ => by simp [Membership.row]
    | .rowField name type rest, wellFormed, fields, results, declared => by
      simp only [RowWellFormed] at wellFormed
      simp only [RowDeclared] at declared
      obtain ⟨⟨found, converted, _, resultPresent, related⟩, others⟩ := declared
      simp only [Membership.row, resultPresent]
      exact ⟨converts_member boundary policy type wellFormed.2.1 found converted related,
        rowDeclared_member boundary policy rest wellFormed.2.2 fields results others⟩

  theorem rowProject_member (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ fields results,
        RowProject boundary policy row fields results → Membership.row boundary.world row results
    | .variable id, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, _ => by simp [Membership.row]
    | .rowField name type rest, wellFormed, fields, results, projected => by
      simp only [RowWellFormed] at wellFormed
      simp only [RowProject] at projected
      obtain ⟨found, converted, others, _, related, rfl, rest'⟩ := projected
      simp only [Membership.row, List.lookup, beq_self_eq_true]
      exact ⟨converts_member boundary policy type wellFormed.2.1 found converted related,
        (row_cons_irrelevant boundary.world name converted rest others wellFormed.1).mpr
          (rowProject_member boundary policy rest wellFormed.2.2 fields others rest')⟩

  theorem firstMember_member (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
      ∀ (rest : UnionTail Declaration), MembersWellFormed rest → ∀ (candidate result : Value Declaration Identity Source),
        FirstMember boundary policy rest candidate result → Membership.members boundary.world rest result
    | .variable id, _, _, _, _ => nomatch id
    | .membersEmpty, _, _, _, converted => by simp [FirstMember] at converted
    | .member type rest, wellFormed, candidate, result, converted => by
      simp only [MembersWellFormed] at wellFormed
      simp only [FirstMember] at converted
      simp only [Membership.members]
      rcases converted with here | ⟨_, later⟩
      · exact Or.inl (converts_member boundary policy type wellFormed.1 candidate result here)
      · exact Or.inr (firstMember_member boundary policy rest wellFormed.2 candidate result later)
end

end Publication

section Failures

def stripOptional : ValueType Declaration → ValueType Declaration
  | .optional element => stripOptional element
  | type => type

def childSlot : ValueType Declaration → PathStep → Option (ValueType Declaration)
  | .list element, .index _ => some element
  | .nonEmpty element, .index _ => some element
  | .map _ element, .key _ => some element
  | .record row, .field name => rowLookup row name
  | .nominal _ row, .field name => rowLookup row name
  | _, _ => none

def SlotAt : ValueType Declaration → List PathStep → ValueType Declaration → Prop
  | type, [], slot => slot = stripOptional type
  | type, step :: rest, slot => ∃ child, childSlot (stripOptional type) step = some child ∧ SlotAt child rest slot

def At (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
    Value Declaration Identity Source → List PathStep → Value Declaration Identity Source → Prop
  | candidate, [], sub => sub = candidate
  | candidate, .field name :: rest, sub =>
    ∃ fields found, fieldsOf boundary candidate = some fields ∧ fields.lookup name = some found ∧
      At boundary policy found rest sub
  | candidate, .index position :: rest, sub =>
    ∃ values found, listItems candidate = some values ∧ values[position]? = some found ∧ At boundary policy found rest sub
  | candidate, .key key :: rest, sub =>
    ∃ found, ((∃ entries, candidate.eraseNominal = .map entries ∧ (key, found) ∈ entries) ∨
        (∃ fields name, candidate.eraseNominal = .record fields ∧ key = .string name ∧ (name, found) ∈ fields)) ∧
      At boundary policy found rest sub

def declaredNames : ValueType Declaration → List String
  | .record row => rowNames row
  | .nominal _ row => rowNames row
  | _ => []

def Admits (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy) :
    ValueType Declaration → Value Declaration Identity Source → Prop
  | .list _, candidate => listItems candidate ≠ none
  | .nonEmpty _, candidate => ∃ first rest, listItems candidate = some (first :: rest)
  | .map keyType _, candidate =>
    (∃ entries, candidate.eraseNominal = .map entries) ∨
      (keyType = .primitive .string ∧ ∃ fields, candidate.eraseNominal = .record fields)
  | .set element, candidate => Membership.value boundary.world (.set element) candidate ∨ listItems candidate ≠ none
  | .record row, candidate =>
    fieldsOf boundary candidate ≠ none
  | .nominal _ row, candidate =>
    fieldsOf boundary candidate ≠ none
  | .optional _, _ => True
  | .union _ _ _, _ => True
  | .tagged declaration, candidate =>
    match boundary.enums.strings declaration with
    | none => Membership.value boundary.world (.tagged declaration) candidate
    | some mapping =>
      (∃ variant fields, candidate.eraseNominal = .tagged declaration variant fields ∧ fields = [] ∧
          variant ∈ mapping.map Prod.fst) ∨
        ∃ text, candidate.eraseNominal = .scalar (.string text)
  | .module _ _, candidate => ∃ reference actual, candidate.eraseNominal = .module reference actual
  | type, candidate => Membership.value boundary.world type candidate

def Local [DecidableEq Declaration] (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
    (slot : ValueType Declaration) (sub : Value Declaration Identity Source) : Reason → Prop
  | .mismatch => ¬ Admits boundary policy slot sub
  | .missingField name =>
    name ∈ declaredNames slot ∧ ∃ fields, fieldsOf boundary sub = some fields ∧ fields.lookup name = none
  | .unexpectedField name =>
    policy = .exact ∧ ∃ fields, fieldsOf boundary sub = some fields ∧ name ∈ fields.map Prod.fst ∧
      name ∉ declaredNames slot
  | .unknownWire text =>
    ∃ declaration mapping, slot = .tagged declaration ∧ boundary.enums.strings declaration = some mapping ∧
      sub.eraseNominal = .scalar (.string text) ∧ ∀ variant, (variant, text) ∉ mapping
  | .keyMismatch key =>
    ∃ keyType element entries, slot = .map keyType element ∧ sub.eraseNominal = .map entries ∧
      key ∈ entries.map Prod.fst ∧ ¬ Membership.key keyType key
  | .setElement position =>
    ∃ element values item, slot = .set element ∧ listItems sub = some values ∧ values[position]? = some item ∧
      ∀ key, keyOf item = some key → ¬ Membership.key element key
  | .repeatedElement position =>
    ∃ element values item key, slot = .set element ∧ listItems sub = some values ∧ values[position]? = some item ∧
      keyOf item = some key ∧ ∃ earlier < position, ∃ other, values[earlier]? = some other ∧ keyOf other = some key
  | .noAlternative =>
    ∃ members, unionMembers slot = some members ∧ ∀ member ∈ members, ∀ result, ¬ Converts boundary policy member sub result
  | .module violations =>
    ∃ closure exports reference actual, slot = .module closure exports ∧ sub.eraseNominal = .module reference actual ∧
      violations = moduleViolations boundary closure exports actual ∧ violations ≠ []

def Located [DecidableEq Declaration] (boundary : Boundary Declaration Identity Source) (policy : FieldPolicy)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) (base : List PathStep)
    (failure : Failure) : Prop :=
  ∃ suffix sub slot, failure.path = base ++ suffix ∧ At boundary policy candidate suffix sub ∧ SlotAt type suffix slot ∧
    Local boundary policy slot sub failure.reason

variable [DecidableEq Declaration]

theorem map_error {α β ε : Type} {transform : α → β} {outcome : Except ε α} {failure : ε}
    (failed : outcome.map transform = .error failure) : outcome = .error failure := by
  cases outcome <;> simp_all [Except.map]

theorem located_here {boundary : Boundary Declaration Identity Source} {policy : FieldPolicy}
    {type : ValueType Declaration} {candidate : Value Declaration Identity Source} {path : List PathStep}
    {reason : Reason} (local' : Local boundary policy (stripOptional type) candidate reason) :
    Located boundary policy type candidate path ⟨path, reason⟩ :=
  ⟨[], candidate, stripOptional type, by simp, rfl, rfl, local'⟩

theorem located_step {boundary : Boundary Declaration Identity Source} {policy : FieldPolicy}
    {type child : ValueType Declaration} {candidate inner : Value Declaration Identity Source}
    {path : List PathStep} {step : PathStep} {failure : Failure}
    (slot : childSlot (stripOptional type) step = some child)
    (reach : ∀ suffix sub, At boundary policy inner suffix sub → At boundary policy candidate (step :: suffix) sub)
    (located : Located boundary policy child inner (path ++ [step]) failure) :
    Located boundary policy type candidate path failure := by
  obtain ⟨suffix, sub, found, failurePath, reached, slotted, local'⟩ := located
  exact ⟨step :: suffix, sub, found, by simp [failurePath], reach suffix sub reached, ⟨child, slot, slotted⟩, local'⟩

theorem located_optional {boundary : Boundary Declaration Identity Source} {policy : FieldPolicy}
    {element : ValueType Declaration} {candidate : Value Declaration Identity Source} {path : List PathStep}
    {failure : Failure} (located : Located boundary policy element candidate path failure) :
    Located boundary policy (.optional element) candidate path failure := by
  obtain ⟨suffix, sub, found, failurePath, reached, slotted, local'⟩ := located
  refine ⟨suffix, sub, found, failurePath, reached, ?_, local'⟩
  cases suffix <;> simpa [SlotAt, stripOptional] using slotted

omit [DecidableEq Declaration] in
theorem members_wellFormed : ∀ {rest : UnionTail Declaration}, MembersWellFormed rest →
    ∀ member ∈ tailMembers rest, WellFormed member
  | .variable id, _ => nomatch id
  | .membersEmpty, _ => by simp [tailMembers]
  | .member type rest, wellFormed => by
    simp only [MembersWellFormed] at wellFormed
    intro member present
    simp only [tailMembers, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ wellFormed.1
    · exact members_wellFormed wellFormed.2 member later

theorem convertMembers_none {boundary : Boundary Declaration Identity Source} {policy : FieldPolicy}
    {path : List PathStep} {candidate : Value Declaration Identity Source} :
    ∀ {rest : UnionTail Declaration}, convertMembers boundary policy path rest candidate = none →
      ∀ member ∈ tailMembers rest, ∃ failure, convert boundary policy path member candidate = .error failure
  | .variable id, _ => nomatch id
  | .membersEmpty, _ => by simp [tailMembers]
  | .member type rest, none' => by
    simp only [convertMembers] at none'
    intro member present
    simp only [tailMembers, List.mem_cons] at present
    split at none'
    · simp at none'
    · rename_i failure failed
      rcases present with same | later
      · exact ⟨failure, same ▸ failed⟩
      · exact convertMembers_none none' member later

theorem no_conversion (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
    (policy : FieldPolicy) {type : ValueType Declaration} (wellFormed : WellFormed type) {path : List PathStep}
    {candidate : Value Declaration Identity Source} {failure : Failure}
    (failed : convert boundary policy path type candidate = .error failure) :
    ∀ result, ¬ Converts boundary policy type candidate result := fun result related => by
  rw [(convert_exact boundary wires policy type wellFormed path candidate result).mpr related] at failed
  simp at failed

omit [DecidableEq Declaration] in
theorem setKeys_error {element : ValueType Declaration} {path : List PathStep} :
    ∀ {position : Nat} {seen : List MapKey} {values : List (Value Declaration Identity Source)} {failure : Failure},
      setKeys element path position seen values = .error failure →
        ∃ offset item, values[offset]? = some item ∧
          ((failure = ⟨path, .setElement (position + offset)⟩ ∧
              ∀ key, keyOf item = some key → ¬ Membership.key element key) ∨
            (failure = ⟨path, .repeatedElement (position + offset)⟩ ∧ ∃ key, keyOf item = some key ∧
              (key ∈ seen ∨ ∃ earlier < offset, ∃ other, values[earlier]? = some other ∧ keyOf other = some key)))
  | _, _, [], _, failed => by simp [setKeys] at failed
  | position, seen, item :: rest, failure, failed => by
    simp only [setKeys] at failed
    split at failed
    · rename_i unkeyed
      simp only [Except.error.injEq] at failed
      exact ⟨0, item, rfl, Or.inl ⟨by simp [failed], fun key keyed => by simp [unkeyed] at keyed⟩⟩
    · rename_i key keyed
      by_cases admitted : keyAdmits element key = true
      · rw [ite_eq_left admitted] at failed
        by_cases repeated : key ∈ seen
        · rw [ite_eq_left repeated] at failed
          simp only [Except.error.injEq] at failed
          exact ⟨0, item, rfl, Or.inr ⟨by simp [failed], key, keyed, Or.inl repeated⟩⟩
        · rw [ite_eq_right repeated] at failed
          split at failed
          · rename_i tailFailure tailFailed
            simp only [Except.error.injEq] at failed
            subst failed
            obtain ⟨offset, found, present, reported⟩ := setKeys_error tailFailed
            refine ⟨offset + 1, found, by simpa using present, ?_⟩
            rcases reported with ⟨same, rejected⟩ | ⟨same, other, otherKeyed, earlier⟩
            · exact Or.inl ⟨by simp [same, Nat.add_assoc, Nat.add_comm 1 offset], rejected⟩
            · refine Or.inr ⟨by simp [same, Nat.add_assoc, Nat.add_comm 1 offset], other, otherKeyed, ?_⟩
              rcases earlier with inSeen | ⟨index, before, previous, previousPresent, previousKeyed⟩
              · simp only [List.mem_cons] at inSeen
                rcases inSeen with first | older
                · exact Or.inr ⟨0, by omega, item, rfl, first ▸ keyed⟩
                · exact Or.inl older
              · exact Or.inr ⟨index + 1, by omega, previous, by simpa using previousPresent, previousKeyed⟩
          · simp at failed
      · rw [ite_eq_right admitted] at failed
        simp only [Except.error.injEq] at failed
        refine ⟨0, item, rfl, Or.inl ⟨by simp [failed], fun found foundKeyed member => ?_⟩⟩
        rw [keyed] at foundKeyed
        simp only [Option.some.injEq] at foundKeyed
        subst foundKeyed
        exact admitted ((keyAdmits_iff element key).mpr member)

omit [DecidableEq Declaration] in
theorem recordBody_error {policy : FieldPolicy} {path : List PathStep} {names : List String}
    {keep project : List (Field Declaration Identity Source) → Except Failure (List (Field Declaration Identity Source))}
    {fields : List (Field Declaration Identity Source)} {failure : Failure}
    (failed : recordBody policy path names keep project fields = .error failure) :
    (policy = .exact ∧ ∃ name, undeclared names fields = some name ∧ failure = ⟨path, .unexpectedField name⟩) ∨
      (policy ≠ .stripped ∧ keep fields = .error failure) ∨ (policy = .stripped ∧ project fields = .error failure) := by
  cases policy with
  | keep => exact Or.inr (Or.inl ⟨by simp, failed⟩)
  | stripped => exact Or.inr (Or.inr ⟨rfl, failed⟩)
  | exact =>
    simp only [recordBody] at failed
    split at failed
    · rename_i name found
      simp only [Except.error.injEq] at failed
      exact Or.inl ⟨rfl, name, found, failed.symm⟩
    · exact Or.inr (Or.inl ⟨by simp, failed⟩)

omit [DecidableEq Declaration] in
theorem listItems_none_of_set {candidate : Value Declaration Identity Source} {keys : List MapKey}
    (erased : candidate.eraseNominal = .set keys) : listItems candidate = none := by
  simp [listItems, erased]

end Failures

section Location

variable [DecidableEq Declaration]

omit [DecidableEq Declaration] in
theorem set_member_shape {world : MembershipWorld Declaration Identity Source} {element : ValueType Declaration}
    {candidate : Value Declaration Identity Source} (member : Membership.value world (.set element) candidate) :
    ∃ keys, candidate.eraseNominal = .set keys := by
  simp only [Membership.value] at member
  split at member
  · exact ⟨_, by assumption⟩
  · exact member.elim

theorem record_located {boundary : Boundary Declaration Identity Source} {policy : FieldPolicy}
    {type : ValueType Declaration} {row : RecordType Declaration} {candidate : Value Declaration Identity Source}
    {path : List PathStep} {failure : Failure} {fields : List (Field Declaration Identity Source)}
    (rowSlot : ∀ name, childSlot (stripOptional type) (.field name) = rowLookup row name)
    (declared : declaredNames (stripOptional type) = rowNames row)
    (fielded : fieldsOf boundary candidate = some fields)
    (reported : (∃ name ∈ rowNames row, fields.lookup name = none ∧ failure = ⟨path, .missingField name⟩) ∨
      (∃ name fieldType found, rowLookup row name = some fieldType ∧ fields.lookup name = some found ∧
        Located boundary policy fieldType found (path ++ [.field name]) failure)) :
    Located boundary policy type candidate path failure := by
  rcases reported with ⟨name, listed, absent, same⟩ | ⟨name, fieldType, found, slot, present, located⟩
  · subst same
    refine located_here ?_
    simp only [Local, declared]
    exact ⟨listed, fields, fielded, absent⟩
  · exact located_step (step := .field name) (by rw [rowSlot]; exact slot)
      (fun suffix sub reached => ⟨fields, found, fielded, present, reached⟩) located

mutual
  theorem convert_located (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (type : ValueType Declaration), WellFormed type → ∀ (path : List PathStep) candidate failure,
        convert boundary policy path type candidate = .error failure → Located boundary policy type candidate path failure
    | .variable id, _, _, _, _, _ => nomatch id
    | .list element, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      split at failed
      · rename_i values listed
        obtain ⟨offset, item, present, itemFailed⟩ := items_error (map_error failed)
        simp only [Nat.zero_add] at itemFailed
        exact located_step (child := element) (step := .index offset) (by simp [stripOptional, childSlot])
          (fun suffix sub reached => ⟨values, item, listed, present, reached⟩)
          (convert_located boundary wires policy element wellFormed _ item failure itemFailed)
      · rename_i unlisted
        simp only [Except.error.injEq] at failed
        subst failed
        exact located_here (by simp [stripOptional, Local, Admits, unlisted])
    | .nonEmpty element, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      split at failed
      · rename_i first rest listed
        obtain ⟨offset, item, present, itemFailed⟩ := items_error (map_error failed)
        simp only [Nat.zero_add] at itemFailed
        exact located_step (child := element) (step := .index offset) (by simp [stripOptional, childSlot])
          (fun suffix sub reached => ⟨first :: rest, item, listed, present, reached⟩)
          (convert_located boundary wires policy element wellFormed _ item failure itemFailed)
      · rename_i notListed
        simp only [Except.error.injEq] at failed
        subst failed
        refine located_here ?_
        simp only [stripOptional, Local, Admits]
        rintro ⟨first, rest, listed⟩
        exact notListed first rest listed
    | .map keyType element, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      split at failed
      · rename_i present erased
        obtain ⟨key, value, listed, stepFailed⟩ := entries_error (map_error failed)
        by_cases admitted : keyAdmits keyType key = true
        · rw [ite_eq_left admitted] at stepFailed
          exact located_step (child := element) (step := .key key) (by simp [stripOptional, childSlot])
            (fun suffix sub reached => ⟨value, Or.inl ⟨present, erased, listed⟩, reached⟩)
            (convert_located boundary wires policy element wellFormed.2 _ value failure stepFailed)
        · rw [ite_eq_right admitted] at stepFailed
          simp only [Except.error.injEq] at stepFailed
          subst stepFailed
          refine located_here ?_
          simp only [stripOptional, Local]
          exact ⟨keyType, element, present, rfl, erased, List.mem_map_of_mem (f := Prod.fst) listed,
            fun member => admitted ((keyAdmits_iff _ _).mpr member)⟩
      · rename_i fields erased
        split at failed
        · rename_i stringKeys
          obtain ⟨key, value, listed, stepFailed⟩ := entries_error (map_error failed)
          obtain ⟨field, fieldPresent, same⟩ := List.mem_map.mp listed
          simp only [recordEntry, Prod.mk.injEq] at same
          obtain ⟨keySame, valueSame⟩ := same
          subst keySame
          subst valueSame
          exact located_step (child := element) (step := .key (.string field.1)) (by simp [stripOptional, childSlot])
            (fun suffix sub reached => ⟨field.2, Or.inr ⟨fields, field.1, erased, rfl, fieldPresent⟩, reached⟩)
            (convert_located boundary wires policy element wellFormed.2 _ _ failure stepFailed)
        · rename_i notString
          simp only [Except.error.injEq] at failed
          subst failed
          refine located_here ?_
          simp only [stripOptional, Local, Admits]
          rintro (⟨entries, impossible⟩ | ⟨isString, _⟩)
          · rw [erased] at impossible
            cases impossible
          · exact notString isString
      · rename_i notMap notRecord
        simp only [Except.error.injEq] at failed
        subst failed
        refine located_here ?_
        simp only [stripOptional, Local, Admits]
        rintro (⟨entries, erased⟩ | ⟨_, fields, erased⟩)
        · exact notMap entries erased
        · exact notRecord fields erased
    | .set element, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      split at failed
      · rename_i keys erased
        obtain ⟨same, rejected⟩ := checkMember_error failed
        subst same
        refine located_here ?_
        simp only [stripOptional, Local, Admits, listItems_none_of_set erased]
        simpa using rejected
      · rename_i notSet
        split at failed
        · rename_i values listed
          obtain ⟨offset, item, present, reported⟩ := setKeys_error (map_error failed)
          rcases reported with ⟨same, rejected⟩ | ⟨same, key, keyed, earlier⟩
          · subst same
            refine located_here ?_
            simp only [stripOptional, Local]
            exact ⟨element, values, item, rfl, listed, by simpa using present, rejected⟩
          · subst same
            refine located_here ?_
            simp only [stripOptional, Local]
            rcases earlier with impossible | earlier
            · simp at impossible
            · exact ⟨element, values, item, key, rfl, listed, by simpa using present, keyed, by simpa using earlier⟩
        · rename_i unlisted
          simp only [Except.error.injEq] at failed
          subst failed
          refine located_here ?_
          simp only [stripOptional, Local, Admits, unlisted]
          intro admitted
          obtain ⟨keys, erased⟩ := set_member_shape (by simpa using admitted)
          exact notSet keys erased
    | .record row, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      split at failed
      · rename_i fields fielded
        rcases recordBody_error (map_error failed) with ⟨exact', name, found, same⟩ | ⟨_, keepFailed⟩ |
            ⟨_, projectFailed⟩
        · subst same
          obtain ⟨present, absent⟩ := undeclared_some found
          refine located_here ?_
          simp only [stripOptional, Local, declaredNames]
          exact ⟨exact', fields, fielded, present, absent⟩
        · exact record_located (fun _ => by simp [stripOptional, childSlot]) (by simp [stripOptional, declaredNames])
            fielded (convertRow_located boundary wires policy row wellFormed path fields failure keepFailed)
        · exact record_located (fun _ => by simp [stripOptional, childSlot]) (by simp [stripOptional, declaredNames])
            fielded (projectRow_located boundary wires policy row wellFormed path fields failure projectFailed)
      · rename_i unfielded
        simp only [Except.error.injEq] at failed
        subst failed
        refine located_here ?_
        simp only [stripOptional, Local, Admits, unfielded]
        exact fun impossible => impossible rfl
    | .nominal declaration row, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      split at failed
      · rename_i fields fielded
        rcases recordBody_error (map_error failed) with ⟨exact', name, found, same⟩ | ⟨_, keepFailed⟩ |
            ⟨_, projectFailed⟩
        · subst same
          obtain ⟨present, absent⟩ := undeclared_some found
          refine located_here ?_
          simp only [stripOptional, Local, declaredNames]
          exact ⟨exact', fields, fielded, present, absent⟩
        · exact record_located (fun _ => by simp [stripOptional, childSlot]) (by simp [stripOptional, declaredNames])
            fielded (convertRow_located boundary wires policy row wellFormed path fields failure keepFailed)
        · exact record_located (fun _ => by simp [stripOptional, childSlot]) (by simp [stripOptional, declaredNames])
            fielded (projectRow_located boundary wires policy row wellFormed path fields failure projectFailed)
      · rename_i unfielded
        simp only [Except.error.injEq] at failed
        subst failed
        refine located_here ?_
        simp only [stripOptional, Local, Admits, unfielded]
        exact fun impossible => impossible rfl
    | .optional element, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      simp only [convert] at failed
      by_cases null : Membership.check boundary.world boundary.contracts (.primitive .null) candidate = true
      · rw [ite_eq_left null] at failed
        simp at failed
      · rw [ite_eq_right null] at failed
        exact located_optional (convert_located boundary wires policy element wellFormed path candidate failure failed)
    | .union first second rest, wellFormed, path, candidate, failure, failed => by
      simp only [WellFormed] at wellFormed
      obtain ⟨firstFormed, secondFormed, restFormed⟩ := wellFormed
      simp only [convert] at failed
      split at failed
      · simp at failed
      · rename_i firstFailure firstFailed
        split at failed
        · simp at failed
        · rename_i secondFailure secondFailed
          split at failed
          · simp at failed
          · rename_i restNone
            simp only [Except.error.injEq] at failed
            subst failed
            refine located_here ?_
            simp only [stripOptional, Local, unionMembers]
            refine ⟨_, rfl, ?_⟩
            intro member present
            simp only [List.mem_cons] at present
            rcases present with same | same | later
            · subst same
              exact no_conversion boundary wires policy firstFormed firstFailed
            · subst same
              exact no_conversion boundary wires policy secondFormed secondFailed
            · obtain ⟨_, memberFailed⟩ := convertMembers_none restNone member later
              exact no_conversion boundary wires policy (members_wellFormed restFormed member later) memberFailed
    | .tagged declaration, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      cases declared : boundary.enums.strings declaration with
      | none =>
        simp only [declared] at failed
        obtain ⟨same, rejected⟩ := checkMember_error failed
        subst same
        exact located_here (by simp [stripOptional, Local, Admits, declared, rejected])
      | some mapping =>
        simp only [declared, wire] at failed
        split at failed
        · rename_i actual variant fields erased
          split at failed
          · simp at failed
          · rename_i rejected
            simp only [Except.error.injEq] at failed
            subst failed
            refine located_here ?_
            simp only [stripOptional, Local, Admits, declared]
            rintro (⟨variant', fields', same, empty, listed⟩ | ⟨text, impossible⟩)
            · rw [erased] at same
              cases same
              exact rejected ⟨rfl, listed, by simp [empty]⟩
            · rw [erased] at impossible
              cases impossible
        · rename_i text erased
          split at failed
          · simp at failed
          · rename_i unknown
            simp only [Except.error.injEq] at failed
            subst failed
            refine located_here ?_
            simp only [stripOptional, Local]
            exact ⟨declaration, mapping, rfl, declared, erased, WireEnums.variantOf_none unknown⟩
        · rename_i notTagged notString
          simp only [Except.error.injEq] at failed
          subst failed
          refine located_here ?_
          simp only [stripOptional, Local, Admits, declared]
          rintro (⟨variant, fields, erased, _⟩ | ⟨text, erased⟩)
          · exact notTagged _ _ _ erased
          · exact notString text erased
    | .module closure exports, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      split at failed
      · rename_i reference actual erased
        split at failed
        · simp at failed
        · rename_i violation others violations
          simp only [Except.error.injEq] at failed
          subst failed
          refine located_here ?_
          simp only [stripOptional, Local]
          exact ⟨closure, exports, reference, actual, rfl, erased, violations.symm, by simp⟩
      · rename_i notModule
        simp only [Except.error.injEq] at failed
        subst failed
        refine located_here ?_
        simp only [stripOptional, Local, Admits]
        rintro ⟨reference, actual, erased⟩
        exact notModule reference actual erased
    | .primitive type, _, path, candidate, failure, failed
    | .stream type, _, path, candidate, failure, failed
    | .functionClass type, _, path, candidate, failure, failed
    | .callable type, _, path, candidate, failure, failed
    | .schemeCallable type, _, path, candidate, failure, failed
    | .errorFamily type, _, path, candidate, failure, failed
    | .errorFacet type, _, path, candidate, failure, failed
    | .boundedInteger type, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      obtain ⟨same, rejected⟩ := checkMember_error failed
      subst same
      exact located_here (by simpa [stripOptional, Local, Admits] using rejected)
    | .result success failure', _, path, candidate, failure, failed => by
      simp only [convert] at failed
      obtain ⟨same, rejected⟩ := checkMember_error failed
      subst same
      exact located_here (by simpa [stripOptional, Local, Admits] using rejected)
    | .errorVariant family variant, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      obtain ⟨same, rejected⟩ := checkMember_error failed
      subst same
      exact located_here (by simpa [stripOptional, Local, Admits] using rejected)
    | .relativePath, _, path, candidate, failure, failed => by
      simp only [convert] at failed
      obtain ⟨same, rejected⟩ := checkMember_error failed
      subst same
      exact located_here (by simpa [stripOptional, Local, Admits] using rejected)

  theorem convertRow_located (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ (path : List PathStep) fields failure,
        convertRow boundary policy path row fields = .error failure →
          (∃ name ∈ rowNames row, fields.lookup name = none ∧ failure = ⟨path, .missingField name⟩) ∨
            (∃ name type found, rowLookup row name = some type ∧ fields.lookup name = some found ∧
              Located boundary policy type found (path ++ [.field name]) failure)
    | .variable id, _, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, _, failed => by simp [convertRow] at failed
    | .rowField name type rest, wellFormed, path, fields, failure, failed => by
      simp only [RowWellFormed] at wellFormed
      obtain ⟨fresh, typeFormed, restFormed⟩ := wellFormed
      simp only [convertRow] at failed
      split at failed
      · rename_i absent
        simp only [Except.error.injEq] at failed
        exact Or.inl ⟨name, by simp [rowNames], absent, failed.symm⟩
      · rename_i found present
        split at failed
        · rename_i fieldFailure fieldFailed
          simp only [Except.error.injEq] at failed
          subst failed
          exact Or.inr ⟨name, type, found, by simp [rowLookup], present,
            convert_located boundary wires policy type typeFormed _ found _ fieldFailed⟩
        · rename_i replacement replaced
          rcases convertRow_located boundary wires policy rest restFormed path _ failure failed with
            ⟨other, listed, absent, same⟩ | ⟨other, otherType, otherFound, slot, otherPresent, located⟩
          · have different : other ≠ name := fun same' => fresh (same' ▸ listed)
            rw [lookup_replaceField_other name other replacement different] at absent
            exact Or.inl ⟨other, by simp [rowNames, listed], absent, same⟩
          · have different : other ≠ name := fun same' => fresh (same' ▸ rowLookup_mem_names slot)
            rw [lookup_replaceField_other name other replacement different] at otherPresent
            exact Or.inr ⟨other, otherType, otherFound, by simp [rowLookup, Ne.symm different, slot], otherPresent,
              located⟩

  theorem projectRow_located (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
      (policy : FieldPolicy) :
      ∀ (row : RecordType Declaration), RowWellFormed row → ∀ (path : List PathStep) fields failure,
        projectRow boundary policy path row fields = .error failure →
          (∃ name ∈ rowNames row, fields.lookup name = none ∧ failure = ⟨path, .missingField name⟩) ∨
            (∃ name type found, rowLookup row name = some type ∧ fields.lookup name = some found ∧
              Located boundary policy type found (path ++ [.field name]) failure)
    | .variable id, _, _, _, _, _ => nomatch id
    | .rowEmpty, _, _, _, _, failed => by simp [projectRow] at failed
    | .rowField name type rest, wellFormed, path, fields, failure, failed => by
      simp only [RowWellFormed] at wellFormed
      obtain ⟨fresh, typeFormed, restFormed⟩ := wellFormed
      simp only [projectRow] at failed
      split at failed
      · rename_i absent
        simp only [Except.error.injEq] at failed
        exact Or.inl ⟨name, by simp [rowNames], absent, failed.symm⟩
      · rename_i found present
        split at failed
        · rename_i fieldFailure fieldFailed
          simp only [Except.error.injEq] at failed
          subst failed
          exact Or.inr ⟨name, type, found, by simp [rowLookup], present,
            convert_located boundary wires policy type typeFormed _ found _ fieldFailed⟩
        · split at failed
          · rename_i restFailure restFailed
            simp only [Except.error.injEq] at failed
            subst failed
            rcases projectRow_located boundary wires policy rest restFormed path fields _ restFailed with
              ⟨other, listed, absent, same⟩ | ⟨other, otherType, otherFound, slot, otherPresent, located⟩
            · exact Or.inl ⟨other, by simp [rowNames, listed], absent, same⟩
            · have different : other ≠ name := fun same' => fresh (same' ▸ rowLookup_mem_names slot)
              exact Or.inr ⟨other, otherType, otherFound, by simp [rowLookup, Ne.symm different, slot], otherPresent,
                located⟩
          · simp at failed
end

end Location

section Report

inductive ExportEntry (Declaration : Type) where
  | value (type : ValueType Declaration)
  | callable (signature : SignatureType Declaration)
  | generic (kind : CallableKind)

def ExportEntry.Admits (world : MembershipWorld Declaration Identity Source) :
    ExportEntry Declaration → Value Declaration Identity Source → Prop
  | .value type, found => Membership.isCallable found = false ∧ Membership.value world type found
  | .callable signature, found => Membership.callable world signature found
  | .generic kind, found => Membership.generic world kind found

def exportEntries : ExportType Declaration → List (String × ExportPresence × ExportEntry Declaration)
  | .exportValue name presence type rest => (name, presence, .value type) :: exportEntries rest
  | .exportCallable name presence signature rest => (name, presence, .callable signature) :: exportEntries rest
  | .exportGenericCallable name presence kind rest => (name, presence, .generic kind) :: exportEntries rest
  | _ => []

def hasMissing : List ModuleViolation → Bool
  | [] => false
  | .missing _ :: _ => true
  | _ :: rest => hasMissing rest

def hasMismatched : List ModuleViolation → Bool
  | [] => false
  | .mismatched _ :: _ => true
  | _ :: rest => hasMismatched rest

def hasUnexpected : List ModuleViolation → Bool
  | [] => false
  | .unexpected _ :: _ => true
  | _ :: rest => hasUnexpected rest

def facetsOf : Reason → List String
  | .module violations =>
    (if hasMissing violations then ["MissingExport"] else []) ++
      (if hasMismatched violations then ["MismatchedExport"] else []) ++
        (if hasUnexpected violations then ["UnexpectedExport"] else [])
  | _ => []

def hexDigit (digit : Nat) : Char :=
  if digit < 10 then Char.ofNat (48 + digit) else Char.ofNat (87 + digit)

def hexBytes (bytes : List UInt8) : String :=
  String.ofList (bytes.flatMap fun byte => [hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)])

def renderKey : MapKey → String
  | .string text => "\"" ++ text ++ "\""
  | .integer value => toString value.val
  | .boolean value => toString value
  | .duration value => toString value.val ++ "ms"
  | .bytes value => hexBytes value
  | .path value => hexBytes value.bytes

def renderStep (rendered : String) : PathStep → String
  | .field name => if rendered = "$" then name else rendered ++ "." ++ name
  | .index position => rendered ++ "[" ++ toString position ++ "]"
  | .key key => rendered ++ "[" ++ renderKey key ++ "]"

def renderPath (path : List PathStep) : String :=
  path.foldl renderStep "$"

def reportedPath (failure : Failure) : List PathStep :=
  match failure.reason with
  | .unexpectedField name => failure.path ++ [.field name]
  | _ => failure.path

def schemaError (family : Declaration) (failure : Failure) : ErrorData Declaration Identity Source where
  family := family
  variant := "schema"
  kind := "schema"
  message := "schema check failed at " ++ renderPath (reportedPath failure)
  payload := []
  facets := facetsOf failure.reason
  span := none
  contexts := []
  cause := none
  abort := none

variable [DecidableEq Declaration]

def requireValue (boundary : Boundary Declaration Identity Source) (family : Declaration) (policy : FieldPolicy)
    (type : ValueType Declaration) (candidate : Value Declaration Identity Source) : Value Declaration Identity Source :=
  match convert boundary policy [] type candidate with
  | .ok converted => .ok converted
  | .error failure => .err (.error (schemaError family failure))

omit [DecidableEq Declaration] in
theorem entryViolations_members (name : String) (presence : ExportPresence)
    (admitted : Value Declaration Identity Source → Bool) (actual : List (Field Declaration Identity Source))
    (violation : ModuleViolation) :
    violation ∈ entryViolations name presence admitted actual ↔
      (violation = .missing name ∧ presence = .required ∧ actual.lookup name = none) ∨
        (violation = .mismatched name ∧ ∃ found, actual.lookup name = some found ∧ admitted found = false) := by
  unfold entryViolations
  cases actual.lookup name with
  | none => cases presence <;> simp
  | some found => by_cases accepted : admitted found = true <;> simp [accepted]

theorem exportViolations_members (boundary : Boundary Declaration Identity Source) :
    ∀ (exports : ExportType Declaration) (actual : List (Field Declaration Identity Source)) (violation : ModuleViolation),
      violation ∈ exportViolations boundary exports actual ↔
        ∃ entry ∈ exportEntries exports,
          (violation = .missing entry.1 ∧ entry.2.1 = .required ∧ actual.lookup entry.1 = none) ∨
            (violation = .mismatched entry.1 ∧ ∃ found, actual.lookup entry.1 = some found ∧
              ¬ entry.2.2.Admits boundary.world found)
  | .variable id, _, _ => nomatch id
  | .exportsEmpty, _, _ => by simp [exportViolations, exportEntries]
  | .exportValue name presence type rest, actual, violation => by
    simp only [exportViolations, List.mem_append, entryViolations_members, exportEntries, List.mem_cons,
      exists_eq_or_imp, exportViolations_members boundary rest actual violation, ExportEntry.Admits]
    have decided : ∀ found,
        (!Membership.isCallable found && Membership.check boundary.world boundary.contracts type found) = false ↔
          ¬ (Membership.isCallable found = false ∧ Membership.value boundary.world type found) := fun found => by
      rw [← Membership.check_iff boundary.world boundary.contracts]
      cases Membership.isCallable found <;> simp
    simp only [decided]
  | .exportCallable name presence signature rest, actual, violation => by
    simp only [exportViolations, List.mem_append, entryViolations_members, exportEntries, List.mem_cons,
      exists_eq_or_imp, exportViolations_members boundary rest actual violation, ExportEntry.Admits]
    have decided : ∀ found, @decide _ (Membership.decideCallable boundary.world boundary.contracts signature found) = false ↔
        ¬ Membership.callable boundary.world signature found := fun found => by
      letI := Membership.decideCallable boundary.world boundary.contracts signature found
      simp
    simp only [decided]
  | .exportGenericCallable name presence kind rest, actual, violation => by
    simp only [exportViolations, List.mem_append, entryViolations_members, exportEntries, List.mem_cons,
      exists_eq_or_imp, exportViolations_members boundary rest actual violation, ExportEntry.Admits]
    have decided : ∀ found, @decide _ (Membership.decideGeneric boundary.world boundary.contracts kind found) = false ↔
        ¬ Membership.generic boundary.world kind found := fun found => by
      letI := Membership.decideGeneric boundary.world boundary.contracts kind found
      simp
    simp only [decided]

/-- A failed module contract reports every violation: each required export
that is absent, each listed export whose value or callable does not fit, and,
for an exact contract, each export the contract does not list. -/
theorem module_report (boundary : Boundary Declaration Identity Source) (closure : ExportClosure)
    (exports : ExportType Declaration) (actual : List (Field Declaration Identity Source)) (name : String) :
    (.missing name ∈ moduleViolations boundary closure exports actual ↔
      ∃ entry ∈ exportEntries exports, entry.1 = name ∧ entry.2.1 = .required ∧ actual.lookup name = none) ∧
    (.mismatched name ∈ moduleViolations boundary closure exports actual ↔
      ∃ entry ∈ exportEntries exports, entry.1 = name ∧ ∃ found, actual.lookup name = some found ∧
        ¬ entry.2.2.Admits boundary.world found) ∧
    (.unexpected name ∈ moduleViolations boundary closure exports actual ↔
      closure = .exact ∧ name ∈ actual.map Prod.fst ∧ name ∉ Membership.exportNames exports) := by
  simp only [moduleViolations, List.mem_append, exportViolations_members]
  refine ⟨?_, ?_, ?_⟩
  · have noUnexpected : ModuleViolation.missing name ∉ unexpectedExports closure exports actual := by
      cases closure <;> simp [unexpectedExports]
    simp only [noUnexpected, or_false]
    constructor
    · rintro ⟨entry, present, ⟨same, required, absent⟩ | ⟨impossible, _⟩⟩
      · simp only [ModuleViolation.missing.injEq] at same
        subst same
        exact ⟨entry, present, rfl, required, absent⟩
      · cases impossible
    · rintro ⟨entry, present, rfl, required, absent⟩
      exact ⟨entry, present, Or.inl ⟨rfl, required, absent⟩⟩
  · have noUnexpected : ModuleViolation.mismatched name ∉ unexpectedExports closure exports actual := by
      cases closure <;> simp [unexpectedExports]
    simp only [noUnexpected, or_false]
    constructor
    · rintro ⟨entry, present, ⟨impossible, _⟩ | ⟨same, found, located, rejected⟩⟩
      · cases impossible
      · simp only [ModuleViolation.mismatched.injEq] at same
        subst same
        exact ⟨entry, present, rfl, found, located, rejected⟩
    · rintro ⟨entry, present, rfl, found, located, rejected⟩
      exact ⟨entry, present, Or.inr ⟨rfl, found, located, rejected⟩⟩
  · have noListed : ¬ ∃ entry ∈ exportEntries exports,
        (ModuleViolation.unexpected name = .missing entry.1 ∧ entry.2.1 = .required ∧ actual.lookup entry.1 = none) ∨
          (ModuleViolation.unexpected name = .mismatched entry.1 ∧ ∃ found, actual.lookup entry.1 = some found ∧
            ¬ entry.2.2.Admits boundary.world found) := by
      rintro ⟨_, _, ⟨impossible, _⟩ | ⟨impossible, _⟩⟩ <;> cases impossible
    simp only [noListed, false_or]
    cases closure with
    | «open» => simp [unexpectedExports]
    | exact =>
      simp only [unexpectedExports, List.mem_map, List.mem_filter, ModuleViolation.unexpected.injEq, true_and]
      constructor
      · rintro ⟨field, ⟨present, unlisted⟩, rfl⟩
        exact ⟨⟨field, present, rfl⟩, by simpa using unlisted⟩
      · rintro ⟨⟨field, present, rfl⟩, unlisted⟩
        exact ⟨field, ⟨present, by simpa using unlisted⟩, rfl⟩

theorem hasMissing_iff : ∀ violations : List ModuleViolation,
    hasMissing violations = true ↔ ∃ name, .missing name ∈ violations
  | [] => by simp [hasMissing]
  | .missing name :: rest => by simp [hasMissing]
  | .mismatched other :: rest => by simp [hasMissing, hasMissing_iff rest]
  | .unexpected other :: rest => by simp [hasMissing, hasMissing_iff rest]

theorem hasMismatched_iff : ∀ violations : List ModuleViolation,
    hasMismatched violations = true ↔ ∃ name, .mismatched name ∈ violations
  | [] => by simp [hasMismatched]
  | .mismatched name :: rest => by simp [hasMismatched]
  | .missing other :: rest => by simp [hasMismatched, hasMismatched_iff rest]
  | .unexpected other :: rest => by simp [hasMismatched, hasMismatched_iff rest]

theorem hasUnexpected_iff : ∀ violations : List ModuleViolation,
    hasUnexpected violations = true ↔ ∃ name, .unexpected name ∈ violations
  | [] => by simp [hasUnexpected]
  | .unexpected name :: rest => by simp [hasUnexpected]
  | .missing other :: rest => by simp [hasUnexpected, hasUnexpected_iff rest]
  | .mismatched other :: rest => by simp [hasUnexpected, hasUnexpected_iff rest]

theorem facets_report (violations : List ModuleViolation) :
    ("MissingExport" ∈ facetsOf (.module violations) ↔ ∃ name, .missing name ∈ violations) ∧
    ("MismatchedExport" ∈ facetsOf (.module violations) ↔ ∃ name, .mismatched name ∈ violations) ∧
    ("UnexpectedExport" ∈ facetsOf (.module violations) ↔ ∃ name, .unexpected name ∈ violations) := by
  rw [← hasMissing_iff, ← hasMismatched_iff, ← hasUnexpected_iff]
  simp only [facetsOf]
  generalize hasMissing violations = first
  generalize hasMismatched violations = second
  generalize hasUnexpected violations = third
  cases first <;> cases second <;> cases third <;> simp

end Report

section Theorem

variable [DecidableEq Declaration]

/-- `.require(T)`, `.require_exact(T)` and `.require_stripped(T)` over every
Core type, value, world and well-formed wire declaration set, on a target
whose record schemas and module contracts list each name once. The published
`Ok` value is exactly the SPEC conversion `Converts` of the candidate: Str-backed
enum slots take the variant of their wire string, a record in a `Map[Str, V]`
slot becomes a map of its fields, a set slot accepts a list of distinct keys,
unions take their first converting alternative, and the policy keeps, rejects
or strips undeclared record fields through declared fields, lists, map values,
optional slots, union alternatives and validated bases. The conversion is a
function and publishes a member of the target. Otherwise `.require` returns a
recoverable `schema` error whose structured failure has no conversion and
names the value path and local reason: a mismatched slot, a missing or
unexpected field, an unknown wire string, a rejected map key, a rejected or
repeated set element, a union none of whose alternatives converts, or the
complete violation list of a module contract, whose facets name each
violation kind. -/
theorem recursive_require (boundary : Boundary Declaration Identity Source) (wires : WireWellFormed boundary.enums)
    (family : Declaration) (policy : FieldPolicy) (type : ValueType Declaration) (wellFormed : WellFormed type)
    (candidate : Value Declaration Identity Source) :
    (∀ result, requireValue boundary family policy type candidate = .ok result ↔
      Converts boundary policy type candidate result) ∧
    (∀ result, Converts boundary policy type candidate result → Membership.value boundary.world type result) ∧
    (∀ first second, Converts boundary policy type candidate first → Converts boundary policy type candidate second →
      first = second) ∧
    (∀ error, requireValue boundary family policy type candidate = .err error ↔
      ∃ failure, convert boundary policy [] type candidate = .error failure ∧
        error = .error (schemaError family failure)) ∧
    (∀ failure, convert boundary policy [] type candidate = .error failure →
      (∀ result, ¬ Converts boundary policy type candidate result) ∧
        Located boundary policy type candidate [] failure) ∧
    (∀ closure exports (actual : List (Field Declaration Identity Source)) name,
      (.missing name ∈ moduleViolations boundary closure exports actual ↔
        ∃ entry ∈ exportEntries exports, entry.1 = name ∧ entry.2.1 = .required ∧ actual.lookup name = none) ∧
      (.mismatched name ∈ moduleViolations boundary closure exports actual ↔
        ∃ entry ∈ exportEntries exports, entry.1 = name ∧ ∃ found, actual.lookup name = some found ∧
          ¬ entry.2.2.Admits boundary.world found) ∧
      (.unexpected name ∈ moduleViolations boundary closure exports actual ↔
        closure = .exact ∧ name ∈ actual.map Prod.fst ∧ name ∉ Membership.exportNames exports)) ∧
    (∀ violations : List ModuleViolation,
      ("MissingExport" ∈ facetsOf (.module violations) ↔ ∃ name, .missing name ∈ violations) ∧
      ("MismatchedExport" ∈ facetsOf (.module violations) ↔ ∃ name, .mismatched name ∈ violations) ∧
      ("UnexpectedExport" ∈ facetsOf (.module violations) ↔ ∃ name, .unexpected name ∈ violations)) := by
  have exact := convert_exact boundary wires policy type wellFormed [] candidate
  refine ⟨?_, converts_member boundary policy type wellFormed candidate, ?_, ?_, ?_,
    module_report boundary, facets_report⟩
  · intro result
    rw [← exact result]
    unfold requireValue
    split <;> simp_all
  · intro first second firstConverts secondConverts
    have firstOk := (exact first).mpr firstConverts
    rw [(exact second).mpr secondConverts] at firstOk
    simpa using firstOk.symm
  · intro error
    unfold requireValue
    split
    · rename_i converted convertedOk
      simp [convertedOk]
    · rename_i failure failed
      simp only [Value.err.injEq, failed, Except.error.injEq]
      constructor
      · rintro rfl
        exact ⟨failure, rfl, rfl⟩
      · rintro ⟨_, rfl, rfl⟩
        rfl
  · intro failure failed
    exact ⟨no_conversion boundary wires policy wellFormed failed,
      convert_located boundary wires policy type wellFormed [] candidate failure failed⟩

end Theorem

namespace Witness

abbrev WitnessValue := Value Nat Unit Unit
abbrev WitnessType := ValueType Nat

def world : MembershipWorld Nat Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => True
  callable := fun _ _ => True
  scheme := fun _ _ => False
  generic := fun _ _ => False

def contracts : ContractDecisions world where
  stream := fun _ type => isTrue ⟨type, trivial, .refl _⟩
  callable := fun _ type => isTrue ⟨type, trivial, .refl _⟩
  scheme := fun _ _ => instDecidableFalse
  generic := fun _ _ => instDecidableFalse

def level : Nat := 0
def levelWire : List (String × String) := [("Warn", "warn"), ("Error", "error")]
def enums : WireEnums Nat := ⟨fun declaration => if declaration = level then some levelWire else none⟩
def boundary : Boundary Nat Unit Unit := ⟨world, contracts, enums⟩
def unconverted : Boundary Nat Unit Unit := ⟨world, contracts, WireEnums.empty⟩

theorem wires : WireWellFormed enums := by
  intro declaration mapping declared
  simp only [enums] at declared
  split at declared
  · simp only [Option.some.injEq] at declared
    subst declared
    decide
  · simp at declared

def text (value : String) : WitnessValue := .scalar (.string value)
def one : WitnessValue := .scalar (.integer ⟨1, by decide⟩)
def warn : WitnessValue := .tagged level "Warn" []

def settings : WitnessType := .record (.rowField "level" (.tagged level) .rowEmpty)
def warnSettings : WitnessValue := .record [("level", text "warn")]
def fatalSettings : WitnessValue := .record [("level", text "fatal")]

def stringMap : WitnessType := .map (.primitive .string) (.primitive .string)
def table : WitnessValue := .record [("a", text "x")]

def config : WitnessType := .record (.rowField "name" (.primitive .string) .rowEmpty)
def raw : WitnessValue := .record [("name", text "worker"), ("future", one)]

def unionSlot : WitnessType := .record (.rowField "x" (.union (.primitive .integer) (.primitive .boolean) .membersEmpty) .rowEmpty)
def textFirst : WitnessType := .union (.primitive .string) (.tagged level) .membersEmpty
def enumFirst : WitnessType := .union (.tagged level) (.primitive .string) .membersEmpty
def repeated : WitnessValue := .list [text "a", text "a"]
def distinctNames : WitnessValue := .list [text "a", text "b"]
def contract : WitnessType :=
  .module .exact (.exportValue "a" .required (.primitive .string) (.exportValue "b" .required (.primitive .integer) .exportsEmpty))
def plugin : WitnessValue := .module ⟨()⟩ [("a", one), ("c", text "x")]
def anyContract : WitnessType := .module .open (.exportValue "f" .required (.primitive .any) .exportsEmpty)
def withCallable : WitnessValue := .module ⟨()⟩ [("f", .callable ⟨()⟩ .pure)]

def entryWorld : MembershipWorld Nat Unit Unit where
  entryFields := fun _ => [("level", .scalar (.string "warn"))]
  stream := fun _ _ => True
  callable := fun _ _ => True
  scheme := fun _ _ => False
  generic := fun _ _ => False

def entryContracts : ContractDecisions entryWorld where
  stream := fun _ type => isTrue ⟨type, trivial, .refl _⟩
  callable := fun _ type => isTrue ⟨type, trivial, .refl _⟩
  scheme := fun _ _ => instDecidableFalse
  generic := fun _ _ => instDecidableFalse

def entryBoundary : Boundary Nat Unit Unit := ⟨entryWorld, entryContracts, enums⟩
def entry : WitnessValue := .filesystemEntry ⟨()⟩

def duplicated : WitnessType := .record (.rowField "level" (.tagged level) (.rowField "level" (.primitive .string) .rowEmpty))

macro "evaluate" : tactic => `(tactic| simp [convert, convertRow, projectRow, recordBody, fieldsOf, listItems,
  Value.eraseNominal, publish, rewrap, peel, wrap, replaceField, List.lookup, wire, WireEnums.variantOf, enums,
  levelWire, level, boundary, unconverted, WireEnums.empty, settings, warnSettings, fatalSettings, text, warn, one, raw,
  config, stringMap, table, Except.map, checkMember, Membership.check_iff, Membership.value, Membership.primitive,
  convertItems, convertEntries, recordEntry, undeclared, rowNames, setKeys, keyOf, keyAdmits_iff, Membership.key,
  MapKey.scalar, convertMembers, moduleViolations, exportViolations, entryViolations, unexpectedExports,
  Membership.exportNames, Membership.isCallable, Membership.recordFields, Membership.row, decide_eq_true_eq, contract, plugin, anyContract, withCallable, duplicated, unionSlot, textFirst, enumFirst, repeated, distinctNames])

theorem wire_string_converts :
    convert boundary .keep [] settings warnSettings = .ok (.record [("level", warn)]) := by
  evaluate

theorem unknown_wire_string_reports_its_field :
    convert boundary .keep [] settings fatalSettings = .error ⟨[.field "level"], .unknownWire "fatal"⟩ := by
  evaluate

theorem list_item_failure_reports_index_and_field :
    convert boundary .keep [] (.list settings) (.list [warnSettings, fatalSettings]) =
      .error ⟨[.index 1, .field "level"], .unknownWire "fatal"⟩ := by
  evaluate

theorem record_in_string_map_publishes_a_map :
    convert boundary .keep [] stringMap table = .ok (.map [(.string "a", text "x")]) := by
  evaluate

theorem field_policies_follow_the_spec_example :
    convert boundary .keep [] config raw = .ok raw ∧
      convert boundary .stripped [] config raw = .ok (.record [("name", text "worker")]) ∧
      convert boundary .exact [] config raw = .error ⟨[], .unexpectedField "future"⟩ := by
  refine ⟨?_, ?_, ?_⟩ <;> evaluate

theorem union_failure_reports_the_union_path :
    convert boundary .keep [] unionSlot (.record [("x", text "s")]) = .error ⟨[.field "x"], .noAlternative⟩ := by
  evaluate

theorem union_order_decides_wire_conversion :
    convert boundary .keep [] textFirst (text "warn") = .ok (text "warn") ∧
      convert boundary .keep [] enumFirst (text "warn") = .ok warn := by
  refine ⟨?_, ?_⟩ <;> evaluate

theorem set_admits_distinct_lists_and_reports_repeats :
    convert boundary .keep [] (.set (.primitive .string)) distinctNames = .ok (.set [.string "a", .string "b"]) ∧
      convert boundary .keep [] (.set (.primitive .string)) repeated = .error ⟨[], .repeatedElement 1⟩ := by
  refine ⟨?_, ?_⟩ <;> evaluate

theorem module_contract_reports_every_violation :
    convert boundary .keep [] contract plugin =
        .error ⟨[], .module [.mismatched "a", .missing "b", .unexpected "c"]⟩ ∧
      facetsOf (.module [.mismatched "a", .missing "b", .unexpected "c"]) =
        ["MissingExport", "MismatchedExport", "UnexpectedExport"] := by
  refine ⟨?_, by decide⟩
  evaluate

theorem settings_wellFormed : WellFormed settings := by
  simp [settings, WellFormed, RowWellFormed, rowNames]

theorem stringMap_wellFormed : WellFormed stringMap := by
  simp [stringMap, WellFormed]

theorem check_without_conversion_is_detected :
    checkMember boundary [] settings warnSettings = .error ⟨[], .mismatch⟩ ∧
      Converts boundary .keep settings warnSettings (.record [("level", warn)]) := by
  refine ⟨by evaluate, ?_⟩
  exact (convert_exact boundary wires .keep settings settings_wellFormed [] _ _).mp wire_string_converts

theorem skipped_enum_conversion_is_detected :
    convert unconverted .keep [] settings warnSettings = .error ⟨[.field "level"], .mismatch⟩ ∧
      Converts boundary .keep settings warnSettings (.record [("level", warn)]) := by
  refine ⟨by evaluate, ?_⟩
  exact (convert_exact boundary wires .keep settings settings_wellFormed [] _ _).mp wire_string_converts

theorem map_published_as_record_is_detected :
    Membership.value world stringMap table ∧ ¬ Converts boundary .keep stringMap table table := by
  refine ⟨by simp [stringMap, table, text, Membership.value, Membership.primitive, Value.eraseNominal], ?_⟩
  intro related
  have published := (convert_exact boundary wires .keep stringMap stringMap_wellFormed [] table table).mpr related
  rw [record_in_string_map_publishes_a_map] at published
  simp [table] at published

theorem dropped_field_path_is_detected :
    Located boundary .keep settings fatalSettings [] ⟨[.field "level"], .unknownWire "fatal"⟩ ∧
      ¬ Located boundary .keep settings fatalSettings [] ⟨[], .unknownWire "fatal"⟩ := by
  refine ⟨((recursive_require boundary wires level .keep settings settings_wellFormed fatalSettings).2.2.2.2.1 _
    unknown_wire_string_reports_its_field).2, ?_⟩
  rintro ⟨suffix, sub, slot, samePath, reached, slotted, local'⟩
  simp only [List.nil_append] at samePath
  subst samePath
  simp only [At] at reached
  simp only [SlotAt] at slotted
  subst reached
  subst slotted
  simp [Local, stripOptional, settings] at local'

theorem duplicate_fields_break_publication :
    ¬ WellFormed duplicated ∧
      convert boundary .stripped [] duplicated warnSettings = .ok (.record [("level", warn), ("level", text "warn")]) ∧
      ¬ Membership.value world duplicated (.record [("level", warn), ("level", text "warn")]) := by
  refine ⟨by simp [duplicated, WellFormed, RowWellFormed, rowNames], by evaluate, ?_⟩
  simp [duplicated, warn, Membership.value, Membership.row, Membership.recordFields, Membership.primitive,
    Value.eraseNominal, List.lookup]

theorem callable_for_value_export_is_mismatched :
    convert boundary .keep [] anyContract withCallable = .error ⟨[], .module [.mismatched "f"]⟩ := by
  evaluate

theorem entry_converts_under_every_policy :
    convert entryBoundary .keep [] settings entry = .ok (.record [("level", warn)]) ∧
      convert entryBoundary .exact [] settings entry = .ok (.record [("level", warn)]) ∧
      convert entryBoundary .stripped [] settings entry = .ok (.record [("level", warn)]) := by
  refine ⟨?_, ?_, ?_⟩ <;>
    simp [convert, entry, fieldsOf, Value.eraseNominal, entryBoundary, entryWorld, settings, recordBody,
      convertRow, projectRow, undeclared, rowNames, replaceField, List.lookup, wire, WireEnums.variantOf, enums,
      levelWire, level, Except.map, publish, warn]

theorem unexpected_field_message_names_the_field :
    (schemaError (Identity := Unit) (Source := Unit) level ⟨[], .unexpectedField "future"⟩).message =
      "schema check failed at future" := by
  decide

end Witness

end Xsh.Validation.Core.Require
