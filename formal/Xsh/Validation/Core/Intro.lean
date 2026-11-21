import Xsh.Validation.Core.Preserve
import Xsh.Validation.Core.Nominal

namespace Xsh.Validation.Core.Intro

open Xsh.Core
open Xsh.Validation.RelPath
open Xsh.Validation.Core
open Xsh.Validation.Core.Paths
open Xsh.Validation.Core.Preserve

variable {Declaration Identity Source : Type}

inductive ListItem (Declaration Identity Source : Type) where
  | element (value : Value Declaration Identity Source)
  | splice (values : List (Value Declaration Identity Source)) (nonEmpty : Bool)

def judgeListLiteral : List (ListItem Declaration Identity Source) → Bool
  | [] => false
  | .element _ :: _ => true
  | .splice _ nonEmpty :: rest => nonEmpty || judgeListLiteral rest

def listLiteralValues : List (ListItem Declaration Identity Source) → List (Value Declaration Identity Source)
  | [] => []
  | .element value :: rest => value :: listLiteralValues rest
  | .splice values _ :: rest => values ++ listLiteralValues rest

def ItemsTyped (world : MembershipWorld Declaration Identity Source) (element : ValueType Declaration) :
    List (ListItem Declaration Identity Source) → Prop
  | [] => True
  | .element value :: rest => Membership.value world element value ∧ ItemsTyped world element rest
  | .splice values nonEmpty :: rest =>
    (∀ value ∈ values, Membership.value world element value) ∧ (nonEmpty = true → values ≠ []) ∧
      ItemsTyped world element rest

theorem literal_values_typed (world : MembershipWorld Declaration Identity Source) (element : ValueType Declaration) :
    ∀ {items : List (ListItem Declaration Identity Source)}, ItemsTyped world element items →
      ∀ value ∈ listLiteralValues items, Membership.value world element value
  | [], _ => by simp [listLiteralValues]
  | .element head :: rest, typed => by
    intro value present
    simp only [listLiteralValues, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ typed.1
    · exact literal_values_typed world element typed.2 value later
  | .splice values _ :: rest, typed => by
    intro value present
    simp only [listLiteralValues, List.mem_append] at present
    rcases present with spliced | later
    · exact typed.1 value spliced
    · exact literal_values_typed world element typed.2.2 value later

theorem judged_literal_nonempty (world : MembershipWorld Declaration Identity Source) (element : ValueType Declaration) :
    ∀ {items : List (ListItem Declaration Identity Source)}, ItemsTyped world element items →
      judgeListLiteral items = true → listLiteralValues items ≠ []
  | [], _, judged => by simp [judgeListLiteral] at judged
  | .element _ :: _, _, _ => by simp [listLiteralValues]
  | .splice values nonEmpty :: rest, typed, judged => by
    simp only [judgeListLiteral, Bool.or_eq_true] at judged
    simp only [listLiteralValues, ne_eq, List.append_eq_nil_iff, not_and]
    rcases judged with spliced | later
    · intro empty
      exact absurd empty (typed.2.1 spliced)
    · intro _
      exact judged_literal_nonempty world element typed.2.2 later

theorem rejected_literal_is_only_plain_splices :
    ∀ {items : List (ListItem Declaration Identity Source)}, judgeListLiteral items = false →
      ∀ item ∈ items, ∃ values, item = .splice values false
  | [], _ => by simp
  | .element _ :: _, judged => by simp [judgeListLiteral] at judged
  | .splice values nonEmpty :: rest, judged => by
    simp only [judgeListLiteral, Bool.or_eq_false_iff] at judged
    intro item present
    simp only [List.mem_cons] at present
    rcases present with same | later
    · exact ⟨values, by rw [same, judged.1]⟩
    · exact rejected_literal_is_only_plain_splices judged.2 item later

theorem plain_splices_may_be_empty :
    ∀ items : List (ListItem Declaration Identity Source),
      listLiteralValues (items.map fun _ => (.splice [] false : ListItem Declaration Identity Source)) = []
  | [] => rfl
  | _ :: rest => by simp [listLiteralValues, plain_splices_may_be_empty rest]

inductive Segment where
  | text (component : Component)
  | slot (path : PathBytes)

def templateComponents : List Segment → List Component
  | [] => []
  | .text component :: rest => component :: templateComponents rest
  | .slot _ :: rest => [dotByte] :: templateComponents rest

def renderComponents : List Segment → List Component
  | [] => []
  | .text component :: rest => component :: renderComponents rest
  | .slot path :: rest => components path ++ renderComponents rest

def TextFree : List Segment → Prop
  | [] => True
  | .text component :: rest => slashByte ∉ component ∧ TextFree rest
  | .slot _ :: rest => TextFree rest

def SlotsRelative : List Segment → Prop
  | [] => True
  | .text _ :: rest => SlotsRelative rest
  | .slot path :: rest => isRelPath path ∧ SlotsRelative rest

def judgeFormat (segments : List Segment) : Prop :=
  TextFree segments ∧ isRelPath (join (templateComponents segments))

def render (segments : List Segment) : PathBytes := join (renderComponents segments)

instance decideTextFree : (segments : List Segment) → Decidable (TextFree segments)
  | [] => isTrue trivial
  | .text component :: rest => by
    unfold TextFree
    have := decideTextFree rest
    infer_instance
  | .slot _ :: rest => by
    unfold TextFree
    exact decideTextFree rest

instance decideSlotsRelative : (segments : List Segment) → Decidable (SlotsRelative segments)
  | [] => isTrue trivial
  | .text _ :: rest => by
    unfold SlotsRelative
    exact decideSlotsRelative rest
  | .slot path :: rest => by
    unfold SlotsRelative
    have := decideSlotsRelative rest
    infer_instance

instance (segments : List Segment) : Decidable (judgeFormat segments) := by
  unfold judgeFormat
  infer_instance

theorem template_no_slash : ∀ {segments : List Segment}, TextFree segments →
    ∀ part ∈ templateComponents segments, slashByte ∉ part
  | [], _ => by simp [templateComponents]
  | .text component :: rest, free => by
    intro part present
    simp only [templateComponents, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ free.1
    · exact template_no_slash free.2 part later
  | .slot _ :: rest, free => by
    intro part present
    simp only [templateComponents, List.mem_cons] at present
    rcases present with same | later
    · subst same; decide
    · exact template_no_slash (segments := rest) free part later

theorem render_no_slash : ∀ {segments : List Segment}, TextFree segments →
    ∀ part ∈ renderComponents segments, slashByte ∉ part
  | [], _ => by simp [renderComponents]
  | .text component :: rest, free => by
    intro part present
    simp only [renderComponents, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ free.1
    · exact render_no_slash free.2 part later
  | .slot path :: rest, free => by
    intro part present
    simp only [renderComponents, List.mem_append] at present
    rcases present with spliced | later
    · exact components_no_slash path part spliced
    · exact render_no_slash (segments := rest) free part later

theorem render_no_nul : ∀ {segments : List Segment}, SlotsRelative segments →
    (∀ part ∈ templateComponents segments, nulByte ∉ part) → ∀ part ∈ renderComponents segments, nulByte ∉ part
  | [], _, _ => by simp [renderComponents]
  | .text component :: rest, relative, template => by
    intro part present
    simp only [renderComponents, List.mem_cons] at present
    rcases present with same | later
    · exact same ▸ template component (by simp [templateComponents])
    · exact render_no_nul (segments := rest) relative
        (fun other inside => template other (by simp [templateComponents, inside])) part later
  | .slot path :: rest, relative, template => by
    intro part present
    simp only [renderComponents, List.mem_append] at present
    rcases present with spliced | later
    · intro inside
      exact relative.1.1 (component_bytes_in_path spliced inside)
    · exact render_no_nul relative.2
        (fun other inside => template other (by simp [templateComponents, inside])) part later

theorem render_scan : ∀ {segments : List Segment}, SlotsRelative segments →
    ∀ {depth shifted result : Nat}, depth ≤ shifted →
      scanComponents (templateComponents segments) depth = some result →
        ∃ rendered, scanComponents (renderComponents segments) shifted = some rendered ∧ result ≤ rendered
  | [], _, depth, shifted, result, le, scanned => by
    simp only [templateComponents, scanComponents, Option.some.injEq] at scanned
    subst scanned
    exact ⟨shifted, rfl, le⟩
  | .text component :: rest, relative, depth, shifted, result, le, scanned => by
    simp only [templateComponents, scanComponents] at scanned
    cases advanced : advanceComponent depth component with
    | none => simp [advanced] at scanned
    | some next =>
      simp only [advanced] at scanned
      have moved := advance_shift advanced (shifted - depth)
      rw [show depth + (shifted - depth) = shifted by omega] at moved
      simp only [renderComponents, scanComponents, moved]
      exact render_scan (segments := rest) relative (by omega) scanned
  | .slot path :: rest, relative, depth, shifted, result, le, scanned => by
    simp only [templateComponents, scanComponents, dot_component_preserves_depth] at scanned
    have pathScanned := relative.1.2
    simp only [scanPath] at pathScanned
    split at pathScanned
    · exact absurd rfl pathScanned
    · change scanComponents (components path) 0 ≠ none at pathScanned
      cases pathDepth : scanComponents (components path) 0 with
      | none => exact absurd pathDepth pathScanned
      | some extra =>
        have moved := scan_shift pathDepth shifted
        rw [Nat.zero_add] at moved
        simp only [renderComponents, scan_append, moved, Option.bind_some]
        exact render_scan relative.2 (by omega) scanned

theorem template_ne_nil {segments : List Segment} (judged : judgeFormat segments) :
    templateComponents segments ≠ [] := by
  intro empty
  have relative := judged.2
  rw [empty] at relative
  exact empty_path_is_rejected (by simpa [join] using relative)

theorem render_head {segments : List Segment} (relative : SlotsRelative segments) (judged : judgeFormat segments) :
    ∃ byte first others, renderComponents segments = (byte :: first) :: others := by
  have split : components (join (templateComponents segments)) = templateComponents segments :=
    List.splitOn_intercalate slashByte (template_no_slash judged.1) (template_ne_nil judged)
  obtain ⟨byte, first, others, shape⟩ := first_component_of_relative judged.2
  rw [split] at shape
  cases segments with
  | nil => simp [templateComponents] at shape
  | cons segment rest =>
    cases segment with
    | text component =>
      simp only [templateComponents, List.cons.injEq] at shape
      exact ⟨byte, first, renderComponents rest, by simp [renderComponents, shape.1]⟩
    | slot path =>
      obtain ⟨pathByte, pathFirst, pathOthers, pathShape⟩ := first_component_of_relative relative.1
      exact ⟨pathByte, pathFirst, pathOthers ++ renderComponents rest, by simp [renderComponents, pathShape]⟩

theorem format_literal_relative {segments : List Segment} (judged : judgeFormat segments)
    (relative : SlotsRelative segments) : isRelPath (render segments) := by
  obtain ⟨byte, first, others, shape⟩ := render_head relative judged
  have split : components (join (templateComponents segments)) = templateComponents segments :=
    List.splitOn_intercalate slashByte (template_no_slash judged.1) (template_ne_nil judged)
  have templateNoNul : ∀ part ∈ templateComponents segments, nulByte ∉ part := by
    intro part present inside
    exact judged.2.1 (by
      have joined := mem_join_of_mem present inside
      exact joined)
  refine relative_join shape (render_no_slash judged.1) (render_no_nul relative templateNoNul) ?_
  have scanned := judged.2.2
  simp only [scanPath] at scanned
  split at scanned
  · exact absurd rfl scanned
  · change scanComponents (components (join (templateComponents segments))) 0 ≠ none at scanned
    rw [split] at scanned
    cases templateDepth : scanComponents (templateComponents segments) 0 with
    | none => exact absurd templateDepth scanned
    | some depth =>
      obtain ⟨rendered, renderedScan, _⟩ := render_scan relative (Nat.le_refl 0) templateDepth
      rw [renderedScan]
      simp

theorem path_literal_iff (world : MembershipWorld Declaration Identity Source) (path : Xsh.Values.NativePath) :
    Membership.value world .relativePath (.scalar (.path path) : Value Declaration Identity Source) ↔
      isRelPath path.bytes := by
  simp [Membership.value, Value.eraseNominal]

theorem bounded_literal_iff (world : MembershipWorld Declaration Identity Source)
    (bounds : Xsh.Validation.BoundedSpec) (integer : Xsh.Values.IntValue) :
    Membership.value world (.boundedInteger bounds) (.scalar (.integer integer) : Value Declaration Identity Source) ↔
      bounds.accepts integer.val := by
  simp [Membership.value, Value.eraseNominal]

/-- The introduction routes of every Core validated type. A judged list
literal (one that writes an element or splices a `NonEmpty` list) of typed
items is a `NonEmpty`, and a literal the judgment rejects is only plain
splices, which may all be empty. A path literal is a `RelPath` exactly when its
bytes pass the rule, and a bounded-integer literal is a member exactly when its
interval accepts it. An `fp` literal whose text, with each `RelPath`
interpolation read as `.`, passes the rule renders to a `RelPath`. A
constructor introduces its nominal type and an accepted `.require` is a member
of its target. No other route exists: a view into a validated type is the
identity and no base views as its validated type. -/
theorem introduction_routes [DecidableEq Declaration] (world : MembershipWorld Declaration Identity Source)
    (contracts : ContractDecisions world) :
    (∀ (element : ValueType Declaration) (items : List (ListItem Declaration Identity Source)),
      ItemsTyped world element items → judgeListLiteral items = true →
        Membership.value world (.nonEmpty element) (.list (listLiteralValues items))) ∧
    (∀ items : List (ListItem Declaration Identity Source), judgeListLiteral items = false →
      (∀ item ∈ items, ∃ values, item = .splice values false) ∧
        listLiteralValues (items.map fun _ => (.splice [] false : ListItem Declaration Identity Source)) = []) ∧
    (∀ path : Xsh.Values.NativePath,
      Membership.value world .relativePath (.scalar (.path path) : Value Declaration Identity Source) ↔
        isRelPath path.bytes) ∧
    (∀ segments, judgeFormat segments → SlotsRelative segments → isRelPath (render segments)) ∧
    (∀ (bounds : Xsh.Validation.BoundedSpec) (integer : Xsh.Values.IntValue),
      Membership.value world (.boundedInteger bounds) (.scalar (.integer integer) : Value Declaration Identity Source) ↔
        bounds.accepts integer.val) ∧
    (∀ heap declaration schema fields value,
      Xsh.Validation.Core.Nominal.evaluate world contracts heap (.construct declaration schema fields) = some value →
        Membership.value world (.nominal declaration schema) value) ∧
    (∀ (type : ValueType Declaration) candidate converted, Xsh.Validation.Core.Require.Admissible type →
      Xsh.Validation.Core.Require.require world contracts type candidate = some converted →
        Membership.value world type converted) ∧
    (∀ source target : ValueType Declaration, BaseView source target → validation target ≠ none → source = target) ∧
    (∀ validated baseType : ValueType Declaration, base validated = some baseType → ¬ BaseView baseType validated) := by
  refine ⟨?_, fun items judged => ⟨rejected_literal_is_only_plain_splices judged, plain_splices_may_be_empty items⟩,
    path_literal_iff world, fun _ judged relative => format_literal_relative judged relative,
    bounded_literal_iff world, Xsh.Validation.Core.Nominal.construct_introduces world contracts,
    fun type candidate converted admissible required =>
      Xsh.Validation.Core.Require.require_sound world contracts type admissible candidate converted required,
    fun _ _ view validated => Xsh.Validation.Core.Widen.view_into_validated_is_identity view validated,
    fun _ _ hasBase => Xsh.Validation.Core.Widen.base_never_views_validated hasBase⟩
  intro element items typed judged
  simp only [Membership.value, Value.eraseNominal]
  exact ⟨judged_literal_nonempty world element typed judged, literal_values_typed world element typed⟩

namespace Witness

def dir : PathBytes := asciiBytes [97]
def name : PathBytes := asciiBytes [98, 47, 99]

theorem format_literal_joins_relative_paths :
    judgeFormat [.slot dir, .slot name] ∧ render [.slot dir, .slot name] = asciiBytes [97, 47, 98, 47, 99] ∧
      isRelPath (render [.slot dir, .slot name]) ∧
      ¬ judgeFormat [.slot dir, .text (asciiBytes [46, 46])] ∧
      ¬ judgeFormat [.text [], .slot dir] := by
  have judged : judgeFormat [.slot dir, .slot name] := by decide
  exact ⟨judged, by decide, format_literal_relative judged (by decide), by decide, by decide⟩

def plainSplice : ListItem Unit Unit Unit := .splice [] false

theorem plain_splice_literal_is_rejected_and_element_is_accepted :
    judgeListLiteral [plainSplice] = false ∧ listLiteralValues [plainSplice] = [] ∧
      judgeListLiteral [.element (.scalar (.string "x")), plainSplice] = true := by
  decide

def acceptsAnySplice : List (ListItem Unit Unit Unit) → Bool := fun items => !items.isEmpty

theorem accepting_plain_splices_is_detected :
    acceptsAnySplice [plainSplice] = true ∧
      ¬ Membership.value Xsh.Validation.Core.Preserve.Witness.openWorld (.nonEmpty (.primitive .string))
        (.list (listLiteralValues [plainSplice])) := by
  refine ⟨rfl, ?_⟩
  simp [plainSplice, listLiteralValues, Membership.value, Value.eraseNominal]

def port : Xsh.Validation.BoundedSpec :=
  ⟨.int, 1, 255, by decide, by unfold Xsh.Validation.IntegerBase.accepts; decide,
    by unfold Xsh.Validation.IntegerBase.accepts; decide⟩

theorem bounded_literal_edges :
    port.accepts 255 ∧ ¬ port.accepts 256 ∧ ¬ port.accepts 0 := by
  decide

end Witness

end Xsh.Validation.Core.Intro
