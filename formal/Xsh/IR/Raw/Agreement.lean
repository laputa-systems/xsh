import Xsh.IR.Raw.Structure
import Xsh.IR.Raw.Spec

/-!
The fuel-indexed checker agrees with the independent specification `Spec.WellFormed`, and the
entry budget `budget` is enough for every schema whose recursive shapes contain no pattern,
stage, text pattern or recursion inside a block (`Regular`, which `rawSchema` satisfies).
-/

namespace Xsh.IR.Raw

open Xsh.Foundation
open Spec

theorem idIn_iff (count word : Nat) : IdIn count word ↔ OneBased word count := by
  unfold IdIn OneBased absent irNone
  omega

theorem cell_eq (row : Row) (column : Nat) : cell row column = rowWord row column := rfl

/-! ### Word windows -/

/-- The words of the table from `position` up to `limit`. -/
def window (doc : Document) (position limit : Nat) : List Nat :=
  ((doc.extra.drop position).take (limit - position)).map (·.val)

theorem window_cons {doc : Document} {position limit word : Nat} {rest : List Nat}
    (within : limit ≤ doc.extra.length) :
    window doc position limit = word :: rest ↔
      readWord doc position limit = some word ∧ rest = window doc (position + 1) limit := by
  unfold window readWord wordAt
  by_cases inside : position < limit
  · have inTable : position < doc.extra.length := Nat.lt_of_lt_of_le inside within
    rw [List.drop_eq_getElem_cons inTable]
    obtain ⟨remaining, remainingEq⟩ : ∃ remaining, limit - position = remaining + 1 := ⟨limit - position - 1, by omega⟩
    have restCount : limit - (position + 1) = remaining := by omega
    rw [remainingEq, List.take_succ_cons, restCount]
    simp only [List.map_cons, List.cons.injEq, inside, if_true, List.getElem?_eq_getElem inTable, Option.map_some,
      Option.some.injEq]
    constructor
    · rintro ⟨rfl, rfl⟩; exact ⟨rfl, rfl⟩
    · rintro ⟨rfl, rfl⟩; exact ⟨rfl, rfl⟩
  · have empty : limit - position = 0 := by omega
    simp [empty, inside]

theorem window_nil {doc : Document} {position limit : Nat} (within : limit ≤ doc.extra.length) :
    window doc position limit = [] ↔ limit ≤ position := by
  unfold window
  simp only [List.map_eq_nil_iff, List.take_eq_nil_iff, List.drop_eq_nil_iff]
  omega

theorem slice_window {doc : Document} {start count : Nat} {words : List Nat} :
    Slice doc start count words ↔ start + count ≤ doc.extra.length ∧ words = window doc start (start + count) := by
  unfold Slice Within window
  rw [Nat.add_sub_cancel_left]

/-! ### Tags -/

theorem nodup_map_injective {α β : Type} (f : α → β) :
    ∀ {items : List α}, (items.map f).Nodup → ∀ {first second}, first ∈ items → second ∈ items →
      f first = f second → first = second
  | [], _, _, _, member, _, _ => by cases member
  | item :: rest, nodup, first, second, firstMember, secondMember, same => by
      rw [List.map_cons, List.nodup_cons] at nodup
      obtain ⟨fresh, restNodup⟩ := nodup
      rcases List.mem_cons.mp firstMember with rfl | firstRest <;>
        rcases List.mem_cons.mp secondMember with rfl | secondRest
      · rfl
      · exact absurd (same ▸ List.mem_map_of_mem secondRest) fresh
      · exact absurd (same.symm ▸ List.mem_map_of_mem firstRest) fresh
      · exact nodup_map_injective f restNodup firstRest secondRest same

theorem names_unique {table : List TagSchema} (distinct : (table.map (·.ordinal)).Nodup) {tag : TagWord}
    {first second : TagSchema} (named : Names table tag first) (other : Names table tag second) : first = second :=
  nodup_map_injective (·.ordinal) distinct named.1 other.1 (named.2.1.trans other.2.1.symm)

theorem lookupTag_iff {table : List TagSchema} (distinct : (table.map (·.ordinal)).Nodup) (tag : TagWord)
    (entry : TagSchema) : lookupTag table tag = some entry ↔ Names table tag entry := by
  unfold lookupTag
  constructor
  · intro found
    split at found
    · next candidate candidateEq =>
        by_cases named : candidate.name = tag.1
        · rw [if_pos named] at found
          cases found
          have member := List.mem_of_find?_eq_some candidateEq
          have ordinal := List.find?_some candidateEq
          exact ⟨member, by simpa using ordinal, named⟩
        · rw [if_neg named] at found
          cases found
    · cases found
  · intro named
    obtain ⟨member, ordinal, name⟩ := named
    cases candidateEq : table.find? (fun candidate => candidate.ordinal == tag.2.val) with
    | none =>
        have := List.find?_eq_none.mp candidateEq entry member
        simp [ordinal] at this
    | some candidate =>
        have candidateMember := List.mem_of_find?_eq_some candidateEq
        have candidateOrdinal : candidate.ordinal = tag.2.val := by simpa using List.find?_some candidateEq
        have same : candidate = entry :=
          nodup_map_injective (·.ordinal) distinct candidateMember member (candidateOrdinal.trans ordinal.symm)
        subst same
        simp [name]

theorem mapM_some_iff {α β : Type} (f : α → Option β) :
    ∀ (items : List α) (results : List β),
      items.mapM f = some results ↔ results.length = items.length ∧
        ∀ (index : Nat) (item : α), items[index]? = some item → ∃ result, results[index]? = some result ∧ f item = some result
  | [], results => by
      constructor
      · intro same
        cases same
        exact ⟨rfl, fun _ _ found => by cases found⟩
      · rintro ⟨lengths, _⟩
        cases results with
        | nil => rfl
        | cons _ _ => cases lengths
  | item :: items, results => by
      rw [List.mapM_cons]
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq]
      constructor
      · rintro ⟨first, firstEq, rest, restEq, rfl⟩
        obtain ⟨lengths, each⟩ := (mapM_some_iff f items rest).mp restEq
        refine ⟨by simp [lengths], ?_⟩
        intro index found foundEq
        cases index with
        | zero =>
            cases foundEq
            exact ⟨first, rfl, firstEq⟩
        | succ index =>
            obtain ⟨result, resultEq, mapped⟩ := each index found (by simpa using foundEq)
            exact ⟨result, by simpa using resultEq, mapped⟩
      · rintro ⟨lengths, each⟩
        cases results with
        | nil => cases lengths
        | cons first rest =>
            obtain ⟨first', firstEq, firstMapped⟩ := each 0 item rfl
            cases firstEq
            refine ⟨first, firstMapped, rest, (mapM_some_iff f items rest).mpr ⟨by simpa using lengths, ?_⟩, rfl⟩
            intro index found foundEq
            obtain ⟨result, resultEq, mapped⟩ := each (index + 1) found (by simpa using foundEq)
            exact ⟨result, by simpa using resultEq, mapped⟩

theorem resolveTags_names {table : List TagSchema} (distinct : (table.map (·.ordinal)).Nodup)
    {tags : List TagWord} {entries : List TagSchema} (resolved : resolveTags table tags = some entries) :
    ∀ (index : Nat) (entry : TagSchema), entries[index]? = some entry ↔ ∃ tag, tags[index]? = some tag ∧ Names table tag entry := by
  obtain ⟨lengths, each⟩ := (mapM_some_iff _ _ _).mp resolved
  intro index entry
  constructor
  · intro found
    have inside : index < tags.length := by
      rw [← lengths]; exact (List.getElem?_eq_some_iff.mp found).1
    obtain ⟨result, resultEq, mapped⟩ := each index tags[index] (List.getElem?_eq_getElem inside)
    rw [found] at resultEq
    cases resultEq
    exact ⟨tags[index], List.getElem?_eq_getElem inside, (lookupTag_iff distinct _ _).mp mapped⟩
  · rintro ⟨tag, tagEq, named⟩
    obtain ⟨result, resultEq, mapped⟩ := each index tag tagEq
    rw [resultEq, names_unique distinct ((lookupTag_iff distinct _ _).mp mapped) named]

theorem resolveTags_some_iff {table : List TagSchema} (distinct : (table.map (·.ordinal)).Nodup)
    (tags : List TagWord) : (∃ entries, resolveTags table tags = some entries) ↔ ∀ tag ∈ tags, ∃ entry, Names table tag entry := by
  constructor
  · rintro ⟨entries, resolved⟩ tag member
    obtain ⟨index, inside, rfl⟩ := List.getElem_of_mem member
    obtain ⟨_, each⟩ := (mapM_some_iff _ _ _).mp resolved
    obtain ⟨result, _, mapped⟩ := each index tags[index] (List.getElem?_eq_getElem inside)
    exact ⟨result, (lookupTag_iff distinct _ _).mp mapped⟩
  · intro every
    induction tags with
    | nil => exact ⟨[], rfl⟩
    | cons tag rest inductionHypothesis =>
        obtain ⟨entry, named⟩ := every tag (by simp)
        obtain ⟨entries, resolved⟩ := inductionHypothesis fun other member => every other (by simp [member])
        refine ⟨entry :: entries, ?_⟩
        show (tag :: rest).mapM (lookupTag table) = some (entry :: entries)
        rw [List.mapM_cons]
        change ((lookupTag table tag).bind fun first => (rest.mapM (lookupTag table)).bind fun more => some (first :: more)) = _
        rw [(lookupTag_iff distinct _ _).mpr named]
        change (rest.mapM (lookupTag table)).bind (fun more => some (entry :: more)) = _
        rw [show rest.mapM (lookupTag table) = some entries from resolved]
        rfl

/-! ### Schemas the budget suffices for -/

mutual
/-- The shape takes no recursion, pattern, stage or text pattern, so no entry budget. -/
def inertShape : Shape → Bool
  | .word role => role != .pattern && role != .stage
  | .seq items => inertShapes items
  | .choice _ variants => inertShapes variants
  | .many item => inertShape item
  | .block _ item => inertShape item
  | .recur _ => false
  | .textPattern => false
def inertShapes : List Shape → Bool
  | [] => true
  | item :: rest => inertShape item && inertShapes rest
end

mutual
/-- The shape may recur but takes no pattern, stage or text pattern, and its blocks are inert. -/
def flatShape : Shape → Bool
  | .word role => role != .pattern && role != .stage
  | .seq items => flatShapes items
  | .choice _ variants => flatShapes variants
  | .many item => flatShape item
  | .block _ item => inertShape item
  | .recur _ => true
  | .textPattern => false
def flatShapes : List Shape → Bool
  | [] => true
  | item :: rest => flatShape item && flatShapes rest
end

/-- Distinct ordinals in every tag table, distinct recursive shape names, and flat recursive
shapes: no recursive shape reaches a pattern, a stage, a text pattern, or recursion inside a block. -/
structure Regular (schema : Schema) : Prop where
  instructions : (schema.instructions.map (·.ordinal)).Nodup
  patterns : (schema.patterns.map (·.ordinal)).Nodup
  stages : (schema.stages.map (·.ordinal)).Nodup
  values : (schema.values.map (·.ordinal)).Nodup
  drivers : (schema.drivers.map (·.ordinal)).Nodup
  names : (schema.recursive.map (·.name)).Nodup
  flat : ∀ shape ∈ schema.recursive, flatShapes shape.variants = true

/-- Each resolved table holds the entry its document tag names. -/
structure Agrees (env : Env) : Prop where
  instructions : ∀ (index : Nat) (entry : TagSchema), env.instructions[index]? = some entry ↔
    ∃ tag, env.doc.tags[index]? = some tag ∧ Names env.schema.instructions tag entry
  patterns : ∀ (index : Nat) (entry : TagSchema), env.patterns[index]? = some entry ↔
    ∃ tag, env.doc.patterns[index]? = some tag ∧ Names env.schema.patterns tag entry
  stages : ∀ (index : Nat) (entry : TagSchema), env.stages[index]? = some entry ↔
    ∃ tag, env.doc.stages[index]? = some tag ∧ Names env.schema.stages tag entry
  values : ∀ (index : Nat) (entry : TagSchema), env.values[index]? = some entry ↔
    ∃ tag, env.doc.values[index]? = some tag ∧ Names env.schema.values tag entry
  drivers : ∀ (index : Nat) (entry : TagSchema), env.drivers[index]? = some entry ↔
    ∃ tag, env.doc.driverStepTags[index]? = some tag ∧ Names env.schema.drivers tag entry
  names : (env.schema.recursive.map (·.name)).Nodup

theorem resolve_fields {schema : Schema} {doc : Document} {env : Env} (resolved : Env.resolve? schema doc = some env) :
    env.schema = schema ∧ env.doc = doc ∧ resolveTags schema.instructions doc.tags = some env.instructions ∧
      resolveTags schema.patterns doc.patterns = some env.patterns ∧
      resolveTags schema.stages doc.stages = some env.stages ∧
      resolveTags schema.values doc.values = some env.values ∧
      resolveTags schema.drivers doc.driverStepTags = some env.drivers := by
  unfold Env.resolve? at resolved
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at resolved
  obtain ⟨instructions, instructionsEq, patterns, patternsEq, stages, stagesEq, values, valuesEq, drivers, driversEq,
    rfl⟩ := resolved
  exact ⟨rfl, rfl, instructionsEq, patternsEq, stagesEq, valuesEq, driversEq⟩

theorem resolve_agrees {schema : Schema} {doc : Document} {env : Env} (regular : Regular schema)
    (resolved : Env.resolve? schema doc = some env) : Agrees env := by
  obtain ⟨schemaEq, docEq, instructions, patterns, stages, values, drivers⟩ := resolve_fields resolved
  subst schemaEq docEq
  exact ⟨resolveTags_names regular.instructions instructions, resolveTags_names regular.patterns patterns,
    resolveTags_names regular.stages stages, resolveTags_names regular.values values,
    resolveTags_names regular.drivers drivers, regular.names⟩

/-! ### Tables -/

theorem utf8Valid_complete : ∀ {bytes : List Nat}, Utf8 bytes → utf8Valid bytes = true
  | _, .nil => rfl
  | _, .ascii below rest => by
      unfold utf8Valid
      rw [if_pos below]
      exact utf8Valid_complete rest
  | _, .two low high second rest => by
      simp only [Continues] at second
      unfold utf8Valid
      rw [if_neg (by omega), if_pos (by simp; omega)]
      simp [continuation, second.1, second.2, utf8Valid_complete rest]
  | _, .threeE0 low high third rest => by
      simp only [Continues] at third
      simp [utf8Valid, continuation, low, high, third.1, third.2, utf8Valid_complete rest]
  | _, .three low high notED second third rest => by
      simp only [Continues] at second third
      unfold utf8Valid
      rw [if_neg (by omega), if_neg (by simp; omega), if_pos (by simp; omega)]
      simp only
      rw [if_neg (by omega), if_neg notED]
      simp [continuation, second.1, second.2, third.1, third.2, utf8Valid_complete rest]
  | _, .threeED low high third rest => by
      simp only [Continues] at third
      simp [utf8Valid, continuation, low, high, third.1, third.2, utf8Valid_complete rest]
  | _, .fourF0 low high third fourth rest => by
      simp only [Continues] at third fourth
      simp [utf8Valid, continuation, low, high, third.1, third.2, fourth.1, fourth.2, utf8Valid_complete rest]
  | _, .four low high second third fourth rest => by
      simp only [Continues] at second third fourth
      unfold utf8Valid
      rw [if_neg (by omega), if_neg (by simp; omega), if_neg (by simp; omega), if_pos (by simp; omega)]
      simp only
      rw [if_neg (by omega), if_neg (by omega)]
      simp [continuation, second.1, second.2, third.1, third.2, fourth.1, fourth.2, utf8Valid_complete rest]
  | _, .fourF4 low high third fourth rest => by
      simp only [Continues] at third fourth
      simp [utf8Valid, continuation, low, high, third.1, third.2, fourth.1, fourth.2, utf8Valid_complete rest]

theorem utf8Valid_sound : ∀ (fuel : Nat) (bytes : List Nat), bytes.length ≤ fuel → utf8Valid bytes = true → Utf8 bytes
  | _, [], _, _ => .nil
  | 0, _ :: _, short, _ => by simp at short
  | fuel + 1, byte :: rest, short, valid => by
      simp only [List.length_cons] at short
      unfold utf8Valid at valid
      split at valid
      · next ascii => exact .ascii ascii (utf8Valid_sound fuel rest (by omega) valid)
      · next notAscii =>
        split at valid
        · next two =>
          split at valid
          · next second rest' =>
            simp only [Bool.and_eq_true, continuation, decide_eq_true_eq] at valid two
            simp only [List.length_cons] at short
            exact .two two.1 two.2 valid.1 (utf8Valid_sound fuel rest' (by omega) valid.2)
          · cases valid
        · next notTwo =>
          split at valid
          · next three =>
            split at valid
            · next second third rest' =>
              simp only [Bool.and_eq_true, continuation, decide_eq_true_eq] at valid three
              simp only [List.length_cons] at short
              have tail := utf8Valid_sound fuel rest' (by omega) valid.2
              split at valid
              · next isE0 =>
                subst isE0
                simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                exact .threeE0 valid.1.1.1 valid.1.1.2 valid.1.2 tail
              · next notE0 =>
                split at valid
                · next isED =>
                  subst isED
                  simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                  exact .threeED valid.1.1.1 valid.1.1.2 valid.1.2 tail
                · next notED =>
                  simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                  exact .three (by omega) three.2 notED valid.1.1 valid.1.2 tail
            · cases valid
          · next notThree =>
            split at valid
            · next four =>
              split at valid
              · next second third fourth rest' =>
                simp only [Bool.and_eq_true, continuation, decide_eq_true_eq] at valid four
                simp only [List.length_cons] at short
                have tail := utf8Valid_sound fuel rest' (by omega) valid.2
                split at valid
                · next isF0 =>
                  subst isF0
                  simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                  exact .fourF0 valid.1.1.1.1 valid.1.1.1.2 valid.1.1.2 valid.1.2 tail
                · next notF0 =>
                  split at valid
                  · next isF4 =>
                    subst isF4
                    simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                    exact .fourF4 valid.1.1.1.1 valid.1.1.1.2 valid.1.1.2 valid.1.2 tail
                  · next notF4 =>
                    simp only [Bool.and_eq_true, decide_eq_true_eq] at valid
                    exact .four (by omega) (by omega) valid.1.1.1 valid.1.1.2 valid.1.2 tail
              · cases valid
            · cases valid

theorem utf8Valid_iff (bytes : List Nat) : utf8Valid bytes = true ↔ Utf8 bytes :=
  ⟨utf8Valid_sound bytes.length bytes (Nat.le_refl _), utf8Valid_complete⟩

theorem nodup_map_iff {α β : Type} (f : α → β) (injective : ∀ first second, f first = f second → first = second) :
    ∀ items : List α, (items.map f).Nodup ↔ items.Nodup
  | [] => by simp
  | item :: rest => by
      rw [List.map_cons, List.nodup_cons, List.nodup_cons, nodup_map_iff f injective rest]
      constructor
      · rintro ⟨fresh, nodup⟩
        exact ⟨fun member => fresh (List.mem_map_of_mem member), nodup⟩
      · rintro ⟨fresh, nodup⟩
        refine ⟨fun member => ?_, nodup⟩
        obtain ⟨other, otherMember, same⟩ := List.mem_map.mp member
        exact fresh (injective _ _ same ▸ otherMember)

theorem scalars_iff (schema : Schema) (doc : Document) :
    ScalarsWF doc ∧ TransfersWF schema doc ↔ Scalars schema doc := by
  unfold ScalarsWF TransfersWF Scalars
  rw [nodup_map_iff String.toList (fun _ _ same => String.toList_inj.mp same)]
  constructor
  · rintro ⟨⟨symbols, source, flags, checked, root, sources⟩, transfers⟩
    exact ⟨symbols, source, flags, checked, transfers, root, sources⟩
  · rintro ⟨symbols, source, flags, checked, transfers, root, sources⟩
    exact ⟨⟨symbols, source, flags, checked, root, sources⟩, transfers⟩

theorem pools_iff (schema : Schema) (doc : Document) : PoolsWF schema doc ↔ Pools schema doc := by
  unfold PoolsWF Pools slice
  simp only [utf8Valid_iff]
  rfl

/-- The owner the checker numbers `index`: functions first, then driver steps. -/
def ownerAt (doc : Document) (index : Nat) : Owner :=
  if index < doc.functions.length then .function index else .step (index - doc.functions.length)

/-- The checker's number for an owner. -/
def ownerIndex (doc : Document) : Owner → Nat
  | .function index => index
  | .step index => doc.functions.length + index

theorem ownerIndex_ownerAt (doc : Document) {index : Nat} : ownerIndex doc (ownerAt doc index) = index := by
  unfold ownerAt
  by_cases function : index < doc.functions.length
  · simp [function, ownerIndex]
  · simp only [function, if_false, ownerIndex]; omega

theorem owners_eq (doc : Document) :
    owners doc = (List.range (doc.functions.length + doc.driverSteps.length)).map (ownerAt doc) := by
  unfold owners
  rw [List.range_add, List.map_append, List.map_map]
  congr 1
  · apply List.map_congr_left
    intro index member
    simp [ownerAt, List.mem_range.mp member]
  · apply List.map_congr_left
    intro index _
    simp [ownerAt]

theorem tail_append_getD (items : List Nat) (last index : Nat) (inside : index < items.length) :
    ((items.tail ++ [last])[index]?).getD 0 = (items[index + 1]?).getD last := by
  by_cases inner : index + 1 < items.length
  · rw [List.getElem?_append_left (by simp; omega), List.getElem?_tail, List.getElem?_eq_getElem inner]
    rfl
  · have edge : index + 1 = items.length := by omega
    have first : index - (items.length - 1) = 0 := by omega
    rw [List.getElem?_append_right (by simp; omega), List.getElem?_eq_none (show items.length ≤ index + 1 by omega)]
    simp [first]

theorem starts_get {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length)
    {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) :
    (ownerStarts doc)[index]? = some ((ownerAt doc index).first doc) := by
  unfold ownerStarts ownerAt
  by_cases function : index < doc.functions.length
  · have inStarts : index < doc.functionInstructionStarts.length := by omega
    rw [if_pos function, List.getElem?_append_left (by simpa using inStarts), List.getElem?_map,
      List.getElem?_eq_getElem inStarts]
    simp [Owner.first, List.getElem?_eq_getElem inStarts]
  · have inSteps : index - doc.functions.length < doc.driverSteps.length := by omega
    rw [if_neg function, List.getElem?_append_right (by simp; omega)]
    simp only [List.length_map, lengths, List.getElem?_map, List.getElem?_eq_getElem inSteps, Option.map_some,
      Owner.first, cell_eq, Option.getD_some]

theorem ends_get {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length)
    {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) :
    ((ownerEnds doc)[index]?).getD 0 = (ownerAt doc index).bound doc := by
  have startsLength : (ownerStarts doc).length = doc.functions.length + doc.driverSteps.length := by
    simp [ownerStarts, lengths]
  unfold ownerEnds
  rw [tail_append_getD _ _ _ (by omega)]
  by_cases next : index + 1 < doc.functions.length + doc.driverSteps.length
  · rw [starts_get lengths next]
    simp only [Option.getD_some]
    unfold ownerAt
    by_cases function : index < doc.functions.length
    · rw [if_pos function]
      by_cases nextFunction : index + 1 < doc.functions.length
      · rw [if_pos nextFunction]
        simp [Owner.first, Owner.bound, List.getElem?_eq_getElem (show index + 1 < doc.functionInstructionStarts.length by omega)]
      · have edge : index + 1 = doc.functions.length := by omega
        rw [if_neg nextFunction]
        simp only [Owner.first, Owner.bound, edge, Nat.sub_self,
          List.getElem?_eq_none (show doc.functionInstructionStarts.length ≤ doc.functions.length by omega)]
        rw [List.getElem?_eq_getElem (show 0 < doc.driverSteps.length by omega)]
        rfl
    · have nextOut : ¬ index + 1 < doc.functions.length := by omega
      rw [if_neg function, if_neg nextOut]
      have shift : index + 1 - doc.functions.length = index - doc.functions.length + 1 := by omega
      simp only [Owner.first, Owner.bound, shift]
      rw [List.getElem?_eq_getElem (show index - doc.functions.length + 1 < doc.driverSteps.length by omega)]
      rfl
  · have edge : index + 1 = (ownerStarts doc).length := by omega
    rw [List.getElem?_eq_none (show (ownerStarts doc).length ≤ index + 1 by omega)]
    simp only [Option.getD_none]
    unfold ownerAt
    by_cases function : index < doc.functions.length
    · have noSteps : doc.driverSteps.length = 0 := by omega
      rw [if_pos function]
      simp only [Owner.bound, List.getElem?_eq_none (show doc.functionInstructionStarts.length ≤ index + 1 by omega),
        List.getElem?_eq_none (show doc.driverSteps.length ≤ 0 by omega)]
      rfl
    · rw [if_neg function]
      simp only [Owner.bound, List.getElem?_eq_none (show doc.driverSteps.length ≤ index - doc.functions.length + 1 by omega)]
      rfl

theorem slots_get {doc : Document} {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) :
    ((ownerSlots doc)[index]?).getD 0 = (ownerAt doc index).slots doc := by
  unfold ownerSlots ownerAt
  by_cases function : index < doc.functions.length
  · rw [if_pos function, List.getElem?_append_left (by simpa using function), List.getElem?_map]
    rfl
  · rw [if_neg function, List.getElem?_append_right (by simp; omega)]
    simp only [List.length_map, List.getElem?_map]
    rfl

theorem ownerSegment_eq {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length)
    {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) :
    ownerSegment doc index = ((ownerAt doc index).first doc, (ownerAt doc index).bound doc, (ownerAt doc index).slots doc) := by
  unfold ownerSegment
  rw [starts_get lengths inside, ends_get lengths inside, slots_get inside]
  rfl

theorem layout_iff {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length) :
    SegmentsWF doc ↔ Layout doc := by
  have startsLength : (ownerStarts doc).length = doc.functions.length + doc.driverSteps.length := by
    simp [ownerStarts, lengths]
  have startsEmpty : ownerStarts doc = [] ↔ owners doc = [] := by
    rw [← List.length_eq_zero_iff, ← List.length_eq_zero_iff, startsLength, owners_eq, List.length_map,
      List.length_range]
  have firstStart : ∀ owner, (owners doc).head? = some owner →
      ((ownerStarts doc).head? = some 0 ↔ owner.first doc = 0) := by
    intro owner headEq
    rw [owners_eq] at headEq
    have nonempty : 0 < doc.functions.length + doc.driverSteps.length := by
      cases count : doc.functions.length + doc.driverSteps.length with
      | zero => rw [count] at headEq; simp at headEq
      | succ _ => omega
    rw [List.head?_eq_getElem?, List.getElem?_map, List.getElem?_range nonempty] at headEq
    cases headEq
    have segment := ownerSegment_eq lengths nonempty
    unfold ownerSegment at segment
    rw [List.head?_eq_getElem?]
    have startEq := congrArg Prod.fst segment
    simp only at startEq
    rw [List.getElem?_eq_getElem (by omega)] at startEq ⊢
    simp only [Option.getD_some] at startEq
    rw [← startEq]
    simp
  unfold SegmentsWF Layout
  constructor
  · rintro ⟨empty, head, ordered⟩
    refine ⟨fun none => ?_, fun owner headEq => ?_, fun owner member => ?_⟩
    · have := empty (startsEmpty.mpr none)
      exact List.length_eq_zero_iff.mp this
    · have nonempty : ownerStarts doc ≠ [] := by
        intro none
        rw [startsEmpty.mp none] at headEq
        cases headEq
      exact (firstStart owner headEq).mp (head nonempty)
    · rw [owners_eq, List.mem_map] at member
      obtain ⟨index, indexMember, rfl⟩ := member
      have inside := List.mem_range.mp indexMember
      have := ordered index (by omega)
      rw [ownerSegment_eq lengths inside] at this
      exact this
  · rintro ⟨empty, head, ordered⟩
    refine ⟨fun none => ?_, fun nonempty => ?_, fun index inside => ?_⟩
    · rw [empty (startsEmpty.mp none)]; rfl
    · have ownersNonempty : owners doc ≠ [] := fun none => nonempty (startsEmpty.mpr none)
      obtain ⟨owner, headEq⟩ := Option.ne_none_iff_exists'.mp (mt List.head?_eq_none_iff.mp ownersNonempty)
      exact (firstStart owner headEq).mpr (head owner headEq)
    · rw [startsLength] at inside
      rw [ownerSegment_eq lengths inside]
      exact ordered _ (by rw [owners_eq]; exact List.mem_map_of_mem (List.mem_range.mpr inside))

/-! ### Frames as checker scopes -/

/-- The checker scope that carries a specification frame. -/
def scopeOf (doc : Document) (frame : Frame) : Scope :=
  { segStart := (frame.owner.map (Owner.first doc)).getD 0, segEnd := (frame.owner.map (Owner.bound doc)).getD 0,
    user := frame.user, slots := frame.slots, patternBound := frame.patterns, stageBound := frame.stages,
    valueBound := frame.values, owner := (frame.owner.map Owner.word).getD irNone, importer := frame.importer }

theorem operandOk_scopeOf {env : Env} (agrees : Agrees env) (frame : Frame) (kind : RowKind) (word : Nat) :
    OperandOk env (scopeOf env.doc frame) kind word ↔ Operand env.schema env.doc frame kind word := by
  have kinds : (env.instructions[word]?).map (·.kind) = some kind ↔ KindIs env.schema env.doc word kind := by
    unfold KindIs
    constructor
    · intro mapped
      obtain ⟨entry, entryEq, kindEq⟩ := Option.map_eq_some_iff.mp mapped
      obtain ⟨tag, tagEq, named⟩ := (agrees.instructions word entry).mp entryEq
      exact ⟨tag, entry, tagEq, named, kindEq⟩
    · rintro ⟨tag, entry, tagEq, named, kindEq⟩
      exact Option.map_eq_some_iff.mpr ⟨entry, (agrees.instructions word entry).mpr ⟨tag, tagEq, named⟩, kindEq⟩
  unfold OperandOk Operand Precedes scopeOf
  rw [kinds]
  cases frame.owner with
  | none => simp
  | some owner =>
      cases frame.user with
      | none => simp [and_assoc]
      | some user => simp [and_assoc]

theorem imports_scopeOf (doc : Document) (frame : Frame) (word : Nat) :
    Xsh.IR.Raw.Imports doc (scopeOf doc frame) word ↔ Spec.Imports doc frame word := by
  unfold Xsh.IR.Raw.Imports Spec.Imports scopeOf programEnd
  cases frame.importer with
  | none => simp
  | some step =>
      simp only [Option.some.injEq, exists_eq_left', ← idIn_iff]
      constructor
      · rintro ⟨identity, bounded⟩
        have inside : word - 1 < doc.driverPrograms.length := by unfold IdIn at identity; omega
        refine ⟨identity, doc.driverPrograms[word - 1], List.getElem?_eq_getElem inside, ?_⟩
        simpa [List.getElem?_eq_getElem inside, cell_eq] using bounded
      · rintro ⟨identity, row, rowEq, bounded⟩
        exact ⟨identity, by simpa [rowEq, cell_eq] using bounded⟩

theorem roleOk_scopeOf {env : Env} (agrees : Agrees env) (frame : Frame) (role : Role) (word : Nat) :
    RoleOk env (scopeOf env.doc frame) role word ↔ WordRule env.schema env.doc frame role word := by
  cases role <;> simp only [RoleOk, WordRule] <;> first
    | exact operandOk_scopeOf agrees frame _ word
    | exact imports_scopeOf _ frame word
    | simp [idIn_iff, scopeOf, poolSize, poolCount, absent, irNone]

theorem wordClaims_taken (role : Role) (word : Nat) : wordClaims role word = taken role word := by
  cases role <;> rfl

theorem blockRange_eq_some {doc : Document} {owner word : Nat} {statements : Bool} {start length : Nat} :
    blockRange doc owner word statements = some (start, length) ↔ ∃ row, 1 ≤ word ∧ doc.blocks[word - 1]? = some row ∧
      rowWord row 4 % 2 = (if statements then 1 else 0) ∧ (rowWord row 3 = irNone ∨ rowWord row 3 = owner) ∧
      rowWord row 0 + rowWord row 1 ≤ doc.extra.length ∧ start = rowWord row 0 ∧ length = rowWord row 1 := by
  unfold blockRange
  by_cases positive : 1 ≤ word
  · rw [if_pos positive]
    cases rowEq : doc.blocks[word - 1]? with
    | none => simp
    | some row =>
        simp only
        by_cases holds : rowWord row 4 % 2 = (if statements then 1 else 0) ∧
            (rowWord row 3 = irNone ∨ rowWord row 3 = owner) ∧ rowWord row 0 + rowWord row 1 ≤ doc.extra.length
        · rw [if_pos holds]
          constructor
          · intro same
            cases same
            exact ⟨row, positive, rfl, holds.1, holds.2.1, holds.2.2, rfl, rfl⟩
          · rintro ⟨row', _, same, _, _, _, rfl, rfl⟩
            cases same
            rfl
        · rw [if_neg holds]
          constructor
          · intro impossible; cases impossible
          · rintro ⟨row', _, same, flag, owned, within, _, _⟩
            cases same
            exact absurd ⟨flag, owned, within⟩ holds
  · rw [if_neg positive]
    constructor
    · intro impossible; cases impossible
    · rintro ⟨_, inside, _⟩; exact absurd inside positive

theorem blockRange_within {doc : Document} {owner word : Nat} {statements : Bool} {start length : Nat}
    (ranged : blockRange doc owner word statements = some (start, length)) : start + length ≤ doc.extra.length := by
  obtain ⟨_, _, _, _, _, within, rfl, rfl⟩ := blockRange_eq_some.mp ranged
  exact within

theorem blockOf_iff (doc : Document) (frame : Frame) (word : Nat) (statements : Bool) (words : List Nat) :
    BlockOf doc frame word statements words ↔ ∃ start length,
      blockRange doc (scopeOf doc frame).owner word statements = some (start, length) ∧
        words = window doc start (start + length) := by
  have owners : ∀ row, (cell row 3 = absent ∨ frame.owner.map Owner.word = some (cell row 3)) ↔
      (rowWord row 3 = irNone ∨ rowWord row 3 = (scopeOf doc frame).owner) := by
    intro row
    unfold scopeOf
    cases frame.owner with
    | none => simp [cell_eq, absent, irNone]
    | some owner => simp [cell_eq, absent, irNone, eq_comm]
  unfold BlockOf
  constructor
  · rintro ⟨row, positive, rowEq, flag, owned, sliced⟩
    obtain ⟨within, wordsEq⟩ := slice_window.mp sliced
    exact ⟨cell row 0, cell row 1, blockRange_eq_some.mpr ⟨row, positive, rowEq, flag, (owners row).mp owned, within,
      rfl, rfl⟩, wordsEq⟩
  · rintro ⟨start, length, ranged, wordsEq⟩
    obtain ⟨row, positive, rowEq, flag, owned, within, rfl, rfl⟩ := blockRange_eq_some.mp ranged
    exact ⟨row, positive, rowEq, flag, (owners row).mpr owned, slice_window.mpr ⟨within, wordsEq⟩⟩

theorem payloadRange_within {table : List Row} {index extra start length : Nat}
    (ranged : payloadRange table index extra = some (start, length)) : start + length ≤ extra := by
  unfold payloadRange at ranged
  split at ranged
  · split at ranged
    · next holds => cases ranged; exact holds
    · cases ranged
  · cases ranged

theorem payloadOf_iff (doc : Document) (table : List Row) (index : Nat) (words : List Nat) :
    PayloadOf doc table index words ↔ ∃ start length,
      payloadRange table index doc.extra.length = some (start, length) ∧ words = window doc start (start + length) := by
  unfold PayloadOf payloadRange
  constructor
  · rintro ⟨row, rowEq, sliced⟩
    obtain ⟨within, wordsEq⟩ := slice_window.mp sliced
    refine ⟨cell row 0, cell row 1, ?_, wordsEq⟩
    rw [rowEq]
    simp only
    rw [if_pos (show rowWord row 0 + rowWord row 1 ≤ doc.extra.length from within)]
    rfl
  · rintro ⟨start, length, ranged, wordsEq⟩
    cases rowEq : table[index]? with
    | none => rw [rowEq] at ranged; cases ranged
    | some row =>
        rw [rowEq] at ranged
        simp only at ranged
        by_cases within : rowWord row 0 + rowWord row 1 ≤ doc.extra.length
        · rw [if_pos within] at ranged
          cases ranged
          exact ⟨row, rfl, slice_window.mpr ⟨within, wordsEq⟩⟩
        · rw [if_neg within] at ranged
          cases ranged

theorem lookupRecursive_iff {schema : Schema} (names : (schema.recursive.map (·.name)).Nodup) (name : String)
    (shape : RecursiveShape) : lookupRecursive schema name = some shape ↔ shape ∈ schema.recursive ∧ shape.name = name := by
  unfold lookupRecursive
  constructor
  · intro found
    exact ⟨List.mem_of_find?_eq_some found, by simpa using List.find?_some found⟩
  · rintro ⟨member, named⟩
    cases foundEq : schema.recursive.find? (·.name == name) with
    | none =>
        have := List.find?_eq_none.mp foundEq shape member
        simp [named] at this
    | some found =>
        have foundMember := List.mem_of_find?_eq_some foundEq
        have foundName : found.name = name := by simpa using List.find?_some foundEq
        rw [nodup_map_injective (·.name) names foundMember member (foundName.trans named.symm)]

/-! ### Checker fits are specification parses -/

theorem wordsFit_sound {env : Env} {scope : Scope} {role : Role} {limit : Nat} (within : limit ≤ env.doc.extra.length) :
    ∀ {count position stop : Nat} {claims : List Claim}, WordsFit env scope role limit count position stop claims →
      ∃ words : List Nat, words.length = count ∧ (∀ word ∈ words, RoleOk env scope role word) ∧
        window env.doc position limit = words ++ window env.doc stop limit ∧ claims = words.flatMap (wordClaims role)
  | _, _, _, _, .zero => ⟨[], rfl, by simp, rfl, rfl⟩
  | _, _, _, _, .succ readEq ok rest => by
      obtain ⟨words, lengthEq, oks, windowEq, claimsEq⟩ := wordsFit_sound within rest
      refine ⟨_ :: words, by simp [lengthEq], ?_, ?_, by rw [claimsEq, List.flatMap_cons]⟩
      · intro word member
        rcases List.mem_cons.mp member with rfl | inner
        · exact ok
        · exact oks word inner
      · rw [(window_cons within).mpr ⟨readEq, rfl⟩, windowEq]
        rfl

theorem unclaimed_words (role : Role) (unclaimed : ∀ word, wordClaims role word = []) :
    ∀ words : List Nat, words.flatMap (wordClaims role) = []
  | [] => rfl
  | word :: words => by simp [unclaimed word, unclaimed_words role unclaimed words]

section Sound
variable {env : Env} (agrees : Agrees env) {entry : EntrySpec}
  (entered : ∀ frame shape position limit stop claims, entry (scopeOf env.doc frame) shape position limit stop claims →
    limit ≤ env.doc.extra.length →
      Parses env.schema env.doc frame shape (window env.doc position limit) (window env.doc stop limit) claims)
include agrees entered

theorem patternRef_sound {frame : Frame} {position limit : Nat} {claims : List Claim}
    (reference : PatternRef env entry (scopeOf env.doc frame) position limit claims) (within : limit ≤ env.doc.extra.length) :
    Parses env.schema env.doc frame (.word .pattern) (window env.doc position limit) (window env.doc (position + 1) limit)
      claims := by
  obtain ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, rfl⟩ := reference
  obtain ⟨tagWord, tagWordEq, named⟩ := (agrees.patterns word tag).mp tagEq
  have innerWithin := payloadRange_within rangeEq
  have parsed := entered { frame with patterns := word } (.seq tag.fields) start (start + length) (start + length) inner
    fits innerWithin
  rw [(window_nil innerWithin).mpr (Nat.le_refl _)] at parsed
  cases parsed with
  | seq all =>
      rw [(window_cons within).mpr ⟨readEq, rfl⟩]
      exact .pattern bound tagWordEq named ((payloadOf_iff _ _ _ _).mpr ⟨start, length, rangeEq, rfl⟩) all

theorem stageRef_sound {frame : Frame} {position limit : Nat} {claims : List Claim}
    (reference : StageRef env entry (scopeOf env.doc frame) position limit claims) (within : limit ≤ env.doc.extra.length) :
    Parses env.schema env.doc frame (.word .stage) (window env.doc position limit) (window env.doc (position + 1) limit)
      claims := by
  obtain ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, rfl⟩ := reference
  obtain ⟨tagWord, tagWordEq, named⟩ := (agrees.stages word tag).mp tagEq
  have innerWithin := payloadRange_within rangeEq
  have parsed := entered { frame with stages := word } (.seq tag.fields) start (start + length) (start + length) inner
    fits innerWithin
  rw [(window_nil innerWithin).mpr (Nat.le_refl _)] at parsed
  cases parsed with
  | seq all =>
      rw [(window_cons within).mpr ⟨readEq, rfl⟩]
      exact .stage bound tagWordEq named ((payloadOf_iff _ _ _ _).mpr ⟨start, length, rangeEq, rfl⟩) all

theorem patternRefs_sound {frame : Frame} {limit : Nat} (within : limit ≤ env.doc.extra.length) :
    ∀ {count position stop : Nat} {claims : List Claim},
      PatternRefs env entry (scopeOf env.doc frame) limit count position stop claims →
      ParsesTimes env.schema env.doc frame (.word .pattern) count (window env.doc position limit)
        (window env.doc stop limit) claims
  | _, _, _, _, .zero => .zero
  | _, _, _, _, .succ first rest =>
      .succ (patternRef_sound agrees entered first within) (patternRefs_sound within rest)

section Fixed
variable {frame : Frame}

mutual
theorem fits_sound : ∀ {shape : Shape} {position limit stop : Nat} {claims : List Claim},
    Fits env entry (scopeOf env.doc frame) shape position limit stop claims → limit ≤ env.doc.extra.length →
      Parses env.schema env.doc frame shape (window env.doc position limit) (window env.doc stop limit) claims
  | _, _, _, _, _, .word notPattern notStage readEq ok, within => by
      rw [(window_cons within).mpr ⟨readEq, rfl⟩, wordClaims_taken]
      exact .word notPattern notStage ((roleOk_scopeOf agrees frame _ _).mp ok)
  | _, _, _, _, _, .pattern reference, within => patternRef_sound agrees entered reference within
  | _, _, _, _, _, .stage reference, within => stageRef_sound agrees entered reference within
  | _, _, _, _, _, .seq items, within => .seq (fitsList_sound items within)
  | _, _, _, _, _, .choice readEq chosen, within => by
      obtain ⟨position, variant, tagAt, first, variantAt, parsed⟩ := fitsChoice_sound chosen within
      rw [(window_cons within).mpr ⟨readEq, rfl⟩]
      exact .choice tagAt first variantAt parsed
  | _, _, _, _, _, .many readEq rest, within => by
      rw [(window_cons within).mpr ⟨readEq, rfl⟩]
      exact .many (fitsMany_sound rest within)
  | _, _, _, _, _, .block readEq rangeEq inner, within => by
      have innerWithin := blockRange_within rangeEq
      have parsed := fits_sound inner innerWithin
      rw [(window_nil innerWithin).mpr (Nat.le_refl _)] at parsed
      rw [(window_cons within).mpr ⟨readEq, rfl⟩]
      exact .block ((blockOf_iff _ _ _ _ _).mpr ⟨_, _, rangeEq, rfl⟩) parsed
  | _, _, _, _, _, .recur lookupEq enteredFit, within => by
      obtain ⟨member, named⟩ := (lookupRecursive_iff agrees.names _ _).mp lookupEq
      exact .recur member named (entered _ _ _ _ _ _ enteredFit within)
  | _, _, _, _, _, .textPattern readEq rangeEq countEq holes tagsFit textsFit, within => by
      have innerWithin := blockRange_within rangeEq
      have blockWords := (window_cons innerWithin).mpr ⟨countEq, rfl⟩
      have holesParsed := patternRefs_sound agrees entered innerWithin holes
      rw [(window_nil innerWithin).mpr (Nat.le_refl _)] at holesParsed
      obtain ⟨kinds, kindsLength, kindsOk, kindsWindow, tagsEq⟩ := wordsFit_sound within tagsFit
      obtain ⟨texts, textsLength, textsOk, textsWindow, textsEq⟩ := wordsFit_sound within textsFit
      rw [(window_cons within).mpr ⟨readEq, rfl⟩, kindsWindow, textsWindow, tagsEq, textsEq,
        unclaimed_words .textHole (fun _ => rfl), unclaimed_words .stringId (fun _ => rfl), List.append_nil,
        List.append_nil]
      refine .text ((blockOf_iff _ _ _ _ _).mpr ⟨_, _, rangeEq, blockWords.symm⟩) holesParsed kindsLength ?_ textsLength ?_
      · exact fun kind member => kindsOk kind member
      · exact fun text member => (idIn_iff _ _).mpr (textsOk text member)
theorem fitsList_sound : ∀ {items : List Shape} {position limit stop : Nat} {claims : List Claim},
    FitsList env entry (scopeOf env.doc frame) items position limit stop claims → limit ≤ env.doc.extra.length →
      ParsesAll env.schema env.doc frame items (window env.doc position limit) (window env.doc stop limit) claims
  | _, _, _, _, _, .nil, _ => .nil
  | _, _, _, _, _, .cons first rest, within => .cons (fits_sound first within) (fitsList_sound rest within)
theorem fitsChoice_sound : ∀ {tag : Nat} {tags : List Nat} {variants : List Shape}
    {position limit stop : Nat} {claims : List Claim},
    FitsChoice env entry (scopeOf env.doc frame) tag tags variants position limit stop claims →
      limit ≤ env.doc.extra.length →
      ∃ (index : Nat) (variant : Shape), tags[index]? = some tag ∧ (∀ earlier, earlier < index → tags[earlier]? ≠ some tag) ∧
        variants[index]? = some variant ∧
        Parses env.schema env.doc frame variant (window env.doc position limit) (window env.doc stop limit) claims
  | _, _, _, _, _, _, _, .here fits, within =>
      ⟨0, _, rfl, fun _ below => absurd below (Nat.not_lt_zero _), rfl, fits_sound fits within⟩
  | _, _, _, _, _, _, _, .there different rest, within => by
      obtain ⟨index, variant, tagAt, first, variantAt, parsed⟩ := fitsChoice_sound rest within
      refine ⟨index + 1, variant, by simpa using tagAt, ?_, by simpa using variantAt, parsed⟩
      intro earlier below
      cases earlier with
      | zero => simpa using different
      | succ earlier => simpa using first earlier (by omega)
theorem fitsMany_sound : ∀ {item : Shape} {limit count position stop : Nat} {claims : List Claim},
    FitsMany env entry (scopeOf env.doc frame) item limit count position stop claims → limit ≤ env.doc.extra.length →
      ParsesTimes env.schema env.doc frame item count (window env.doc position limit) (window env.doc stop limit) claims
  | _, _, _, _, _, _, .zero, _ => .zero
  | _, _, _, _, _, _, .succ first rest, within => .succ (fits_sound first within) (fitsMany_sound rest within)
end

end Fixed

end Sound

theorem entryAt_sound {env : Env} (agrees : Agrees env) : ∀ (fuel : Nat) (frame : Frame) (shape : Shape)
    (position limit stop : Nat) (claims : List Claim),
    EntryAt env fuel (scopeOf env.doc frame) shape position limit stop claims → limit ≤ env.doc.extra.length →
      Parses env.schema env.doc frame shape (window env.doc position limit) (window env.doc stop limit) claims
  | 0, _, _, _, _, _, _, impossible, _ => impossible.elim
  | fuel + 1, _, _, _, _, _, _, fits, within => fits_sound agrees (entryAt_sound agrees fuel) fits within

/-! ### Specification parses are checker fits within the budget -/

theorem readWord_below {doc : Document} {position limit word : Nat} (read : readWord doc position limit = some word) :
    position < limit := by
  unfold readWord at read
  by_cases below : position < limit
  · exact below
  · rw [if_neg below] at read; cases read

theorem window_length {doc : Document} {position limit : Nat} (within : limit ≤ doc.extra.length) :
    (window doc position limit).length = limit - position := by
  unfold window
  simp only [List.length_map, List.length_take, List.length_drop]
  omega

theorem window_split {doc : Document} {position limit : Nat} {words rest : List Nat} (within : limit ≤ doc.extra.length)
    (split : window doc position limit = words ++ rest) : rest.length ≤ limit := by
  have lengths := congrArg List.length split
  rw [window_length within, List.length_append] at lengths
  omega

theorem fitsChoice_of {env : Env} {entry : EntrySpec} {scope : Scope} {tag : Nat} {variant : Shape}
    {position limit stop : Nat} {claims : List Claim} (fits : Fits env entry scope variant position limit stop claims) :
    ∀ {index : Nat} {tags : List Nat} {variants : List Shape}, tags[index]? = some tag →
      (∀ earlier, earlier < index → tags[earlier]? ≠ some tag) → variants[index]? = some variant →
      FitsChoice env entry scope tag tags variants position limit stop claims
  | 0, tags, variants, tagAt, _, variantAt => by
      cases tags with
      | nil => cases tagAt
      | cons first tags =>
          cases variants with
          | nil => cases variantAt
          | cons other variants =>
              simp only [List.getElem?_cons_zero, Option.some.injEq] at tagAt variantAt
              subst tagAt variantAt
              exact .here fits
  | index + 1, tags, variants, tagAt, first, variantAt => by
      cases tags with
      | nil => cases tagAt
      | cons candidate tags =>
          cases variants with
          | nil => cases variantAt
          | cons other variants =>
              have different : candidate ≠ tag := by
                intro same
                exact first 0 (Nat.zero_lt_succ _) (by simp [same])
              exact .there different (fitsChoice_of fits (by simpa using tagAt)
                (fun earlier below => by simpa using first (earlier + 1) (by omega)) (by simpa using variantAt))

theorem inertShapes_get : ∀ {shapes : List Shape} {index : Nat} {shape : Shape}, inertShapes shapes = true →
    shapes[index]? = some shape → inertShape shape = true
  | [], _, _, _, found => by cases found
  | first :: rest, 0, shape, inert, found => by
      simp only [List.getElem?_cons_zero, Option.some.injEq] at found
      subst found
      simp only [inertShapes, Bool.and_eq_true] at inert
      exact inert.1
  | first :: rest, index + 1, shape, inert, found => by
      simp only [inertShapes, Bool.and_eq_true] at inert
      exact inertShapes_get inert.2 (by simpa using found)

theorem flatShapes_get : ∀ {shapes : List Shape} {index : Nat} {shape : Shape}, flatShapes shapes = true →
    shapes[index]? = some shape → flatShape shape = true
  | [], _, _, _, found => by cases found
  | first :: rest, 0, shape, flat, found => by
      simp only [List.getElem?_cons_zero, Option.some.injEq] at found
      subst found
      simp only [flatShapes, Bool.and_eq_true] at flat
      exact flat.1
  | first :: rest, index + 1, shape, flat, found => by
      simp only [flatShapes, Bool.and_eq_true] at flat
      exact flatShapes_get flat.2 (by simpa using found)

theorem wordsFit_complete {env : Env} {scope : Scope} {role : Role} {limit : Nat} (within : limit ≤ env.doc.extra.length)
    (unclaimed : ∀ word, wordClaims role word = []) :
    ∀ (words : List Nat) {position : Nat} {rest : List Nat}, (∀ word ∈ words, RoleOk env scope role word) →
      window env.doc position limit = words ++ rest → position ≤ limit →
      WordsFit env scope role limit words.length position (position + words.length) [] ∧
        rest = window env.doc (position + words.length) limit ∧ position + words.length ≤ limit
  | [], position, rest, _, split, below => ⟨.zero, by simpa using split.symm, by simpa using below⟩
  | word :: words, position, rest, oks, split, _ => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp split
      have below := readWord_below readEq
      obtain ⟨fits, restWindow, bound⟩ := wordsFit_complete within unclaimed words
        (fun other member => oks other (List.mem_cons_of_mem _ member)) restEq.symm (by omega)
      have step := WordsFit.succ readEq (oks word (List.mem_cons_self ..)) fits
      rw [unclaimed, List.nil_append] at step
      refine ⟨?_, ?_, ?_⟩
      · simpa [Nat.add_assoc, Nat.add_comm 1] using step
      · simpa [Nat.add_assoc, Nat.add_comm 1] using restWindow
      · simp only [List.length_cons]; omega

mutual
theorem parses_shorter {schema : Schema} {doc : Document} : ∀ {frame : Frame} {shape : Shape} {words rest : List Nat}
    {claims : List Claim}, Parses schema doc frame shape words rest claims → rest.length ≤ words.length
  | _, _, _, _, _, .word _ _ _ => by simp
  | _, _, _, _, _, .pattern _ _ _ _ _ => by simp
  | _, _, _, _, _, .stage _ _ _ _ _ => by simp
  | _, _, _, _, _, .seq items => parsesAll_shorter items
  | _, _, _, _, _, .choice _ _ _ parsed => by have := parses_shorter parsed; simp; omega
  | _, _, _, _, _, .many times => by have := parsesTimes_shorter times; simp; omega
  | _, _, _, _, _, .block _ _ => by simp
  | _, _, _, _, _, .recur _ _ parsed => parses_shorter parsed
  | _, _, _, _, _, .text _ _ _ _ _ _ => by simp; omega
theorem parsesAll_shorter {schema : Schema} {doc : Document} : ∀ {frame : Frame} {items : List Shape}
    {words rest : List Nat} {claims : List Claim}, ParsesAll schema doc frame items words rest claims →
      rest.length ≤ words.length
  | _, _, _, _, _, .nil => Nat.le_refl _
  | _, _, _, _, _, .cons first more => Nat.le_trans (parsesAll_shorter more) (parses_shorter first)
theorem parsesTimes_shorter {schema : Schema} {doc : Document} : ∀ {frame : Frame} {item : Shape} {count : Nat}
    {words rest : List Nat} {claims : List Claim}, ParsesTimes schema doc frame item count words rest claims →
      rest.length ≤ words.length
  | _, _, _, _, _, _, .zero => Nat.le_refl _
  | _, _, _, _, _, _, .succ first more => Nat.le_trans (parsesTimes_shorter more) (parses_shorter first)
end

section Complete
variable {env : Env} (agrees : Agrees env)
include agrees

mutual
theorem inert_complete {entry : EntrySpec} : ∀ {frame : Frame} {shape : Shape} {words rest : List Nat}
    {claims : List Claim}, Parses env.schema env.doc frame shape words rest claims → inertShape shape = true →
    ∀ {position limit : Nat}, words = window env.doc position limit → position ≤ limit → limit ≤ env.doc.extra.length →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        Fits env entry (scopeOf env.doc frame) shape position limit stop claims
  | frame, _, _, _, _, .word notPattern notStage ok, _, position, limit, wordsEq, _, within => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      refine ⟨position + 1, readWord_below readEq, restEq, ?_⟩
      rw [← wordClaims_taken]
      exact .word notPattern notStage readEq ((roleOk_scopeOf agrees frame _ _).mpr ok)
  | _, _, _, _, _, .pattern _ _ _ _ _, inert, _, _, _, _, _ => by simp [inertShape] at inert
  | _, _, _, _, _, .stage _ _ _ _ _, inert, _, _, _, _, _ => by simp [inertShape] at inert
  | _, _, _, _, _, .seq items, inert, _, _, wordsEq, below, within => by
      obtain ⟨stop, bound, restEq, fits⟩ := inertAll_complete items inert wordsEq below within
      exact ⟨stop, bound, restEq, .seq fits⟩
  | _, _, _, _, _, .choice tagAt first variantAt parsed, inert, position, limit, wordsEq, _, within => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨stop, bound, restWindow, fits⟩ := inert_complete parsed (inertShapes_get inert variantAt) restEq
        (readWord_below readEq) within
      exact ⟨stop, bound, restWindow, .choice readEq (fitsChoice_of fits tagAt first variantAt)⟩
  | _, _, _, _, _, .many times, inert, position, limit, wordsEq, _, within => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨stop, bound, restWindow, fits⟩ := inertTimes_complete times inert restEq (readWord_below readEq) within
      exact ⟨stop, bound, restWindow, .many readEq fits⟩
  | frame, _, _, _, _, .block blockOf inner, inert, position, limit, wordsEq, _, within => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, innerEq⟩ := (blockOf_iff _ _ _ _ _).mp blockOf
      have innerWithin := blockRange_within rangeEq
      obtain ⟨stop, bound, emptyEq, fits⟩ := inert_complete inner inert innerEq (Nat.le_add_right _ _) innerWithin
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      exact ⟨position + 1, readWord_below readEq, restEq, .block readEq rangeEq fits⟩
  | _, _, _, _, _, .recur _ _ _, inert, _, _, _, _, _ => by simp [inertShape] at inert
  | _, _, _, _, _, .text _ _ _ _ _ _, inert, _, _, _, _, _ => by simp [inertShape] at inert
theorem inertAll_complete {entry : EntrySpec} : ∀ {frame : Frame} {items : List Shape} {words rest : List Nat}
    {claims : List Claim}, ParsesAll env.schema env.doc frame items words rest claims → inertShapes items = true →
    ∀ {position limit : Nat}, words = window env.doc position limit → position ≤ limit → limit ≤ env.doc.extra.length →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsList env entry (scopeOf env.doc frame) items position limit stop claims
  | _, _, _, _, _, .nil, _, position, _, wordsEq, below, _ => ⟨position, below, wordsEq, .nil⟩
  | _, _, _, _, _, .cons first more, inert, _, _, wordsEq, below, within => by
      simp only [inertShapes, Bool.and_eq_true] at inert
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := inert_complete first inert.1 wordsEq below within
      obtain ⟨stop, bound, restEq, moreFits⟩ := inertAll_complete more inert.2 middleEq middleBound within
      exact ⟨stop, bound, restEq, .cons firstFits moreFits⟩
theorem inertTimes_complete {entry : EntrySpec} : ∀ {frame : Frame} {item : Shape} {count : Nat} {words rest : List Nat}
    {claims : List Claim}, ParsesTimes env.schema env.doc frame item count words rest claims → inertShape item = true →
    ∀ {position limit : Nat}, words = window env.doc position limit → position ≤ limit → limit ≤ env.doc.extra.length →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsMany env entry (scopeOf env.doc frame) item limit count position stop claims
  | _, _, _, _, _, _, .zero, _, position, _, wordsEq, below, _ => ⟨position, below, wordsEq, .zero⟩
  | _, _, _, _, _, _, .succ first more, inert, _, _, wordsEq, below, within => by
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := inert_complete first inert wordsEq below within
      obtain ⟨stop, bound, restEq, moreFits⟩ := inertTimes_complete more inert middleEq middleBound within
      exact ⟨stop, bound, restEq, .succ firstFits moreFits⟩
end

variable (regular : Regular env.schema)
include regular

mutual
theorem flat_complete : ∀ {frame : Frame} {shape : Shape} {words rest : List Nat} {claims : List Claim},
    Parses env.schema env.doc frame shape words rest claims → flatShape shape = true →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → words.length ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        Fits env (EntryAt env fuel) (scopeOf env.doc frame) shape position limit stop claims
  | frame, _, _, _, _, .word notPattern notStage ok, _, _, position, limit, wordsEq, _, within, _ => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      refine ⟨position + 1, readWord_below readEq, restEq, ?_⟩
      rw [← wordClaims_taken]
      exact .word notPattern notStage readEq ((roleOk_scopeOf agrees frame _ _).mpr ok)
  | _, _, _, _, _, .pattern _ _ _ _ _, flat, _, _, _, _, _, _, _ => by simp [flatShape] at flat
  | _, _, _, _, _, .stage _ _ _ _ _, flat, _, _, _, _, _, _, _ => by simp [flatShape] at flat
  | _, _, _, _, _, .seq items, flat, _, _, _, wordsEq, below, within, short => by
      obtain ⟨stop, bound, restEq, fits⟩ := flatAll_complete items flat wordsEq below within short
      exact ⟨stop, bound, restEq, .seq fits⟩
  | _, _, _, _, _, .choice tagAt first variantAt parsed, flat, fuel, position, limit, wordsEq, _, within, short => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      simp only [List.length_cons] at short
      obtain ⟨stop, bound, restWindow, fits⟩ := flat_complete (fuel := fuel) parsed (flatShapes_get flat variantAt) restEq
        (readWord_below readEq) within (by omega)
      exact ⟨stop, bound, restWindow, .choice readEq (fitsChoice_of fits tagAt first variantAt)⟩
  | _, _, _, _, _, .many times, flat, fuel, position, limit, wordsEq, _, within, short => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      simp only [List.length_cons] at short
      obtain ⟨stop, bound, restWindow, fits⟩ := flatTimes_complete (fuel := fuel) times flat restEq
        (readWord_below readEq) within (by omega)
      exact ⟨stop, bound, restWindow, .many readEq fits⟩
  | _, _, _, _, _, .block blockOf inner, flat, _, position, limit, wordsEq, _, within, _ => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, innerEq⟩ := (blockOf_iff _ _ _ _ _).mp blockOf
      have innerWithin := blockRange_within rangeEq
      obtain ⟨stop, bound, emptyEq, fits⟩ := inert_complete agrees inner flat innerEq (Nat.le_add_right _ _) innerWithin
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      exact ⟨position + 1, readWord_below readEq, restEq, .block readEq rangeEq fits⟩
  | _, _, _, _, _, .recur member named (.choice tagAt first variantAt parsed), _, fuel, position, limit, wordsEq, _,
      within, short => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      simp only [List.length_cons] at short
      obtain ⟨remaining, rfl⟩ : ∃ remaining, fuel = remaining + 1 := ⟨fuel - 1, by omega⟩
      obtain ⟨stop, bound, restWindow, fits⟩ := flat_complete (fuel := remaining) parsed
        (flatShapes_get (regular.flat _ member) variantAt) restEq (readWord_below readEq) within (by omega)
      have chosen : Fits env (EntryAt env remaining) (scopeOf env.doc _) (.choice _ _) position limit stop _ :=
        .choice readEq (fitsChoice_of fits tagAt first variantAt)
      exact ⟨stop, bound, restWindow, .recur ((lookupRecursive_iff agrees.names _ _).mpr ⟨member, named⟩) chosen⟩
  | _, _, _, _, _, .text _ _ _ _ _ _, flat, _, _, _, _, _, _, _ => by simp [flatShape] at flat
theorem flatAll_complete : ∀ {frame : Frame} {items : List Shape} {words rest : List Nat} {claims : List Claim},
    ParsesAll env.schema env.doc frame items words rest claims → flatShapes items = true →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → words.length ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsList env (EntryAt env fuel) (scopeOf env.doc frame) items position limit stop claims
  | _, _, _, _, _, .nil, _, _, position, _, wordsEq, below, _, _ => ⟨position, below, wordsEq, .nil⟩
  | _, _, _, _, _, .cons first more, flat, fuel, position, limit, wordsEq, below, within, short => by
      simp only [flatShapes, Bool.and_eq_true] at flat
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := flat_complete first flat.1 wordsEq below within short
      obtain ⟨stop, bound, restEq, moreFits⟩ := flatAll_complete (fuel := fuel) more flat.2 middleEq middleBound within
        (Nat.le_trans (parses_shorter first) short)
      exact ⟨stop, bound, restEq, .cons firstFits moreFits⟩
theorem flatTimes_complete : ∀ {frame : Frame} {item : Shape} {count : Nat} {words rest : List Nat}
    {claims : List Claim}, ParsesTimes env.schema env.doc frame item count words rest claims → flatShape item = true →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → words.length ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsMany env (EntryAt env fuel) (scopeOf env.doc frame) item limit count position stop claims
  | _, _, _, _, _, _, .zero, _, _, position, _, wordsEq, below, _, _ => ⟨position, below, wordsEq, .zero⟩
  | _, _, _, _, _, _, .succ first more, flat, fuel, position, limit, wordsEq, below, within, short => by
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := flat_complete first flat wordsEq below within short
      obtain ⟨stop, bound, restEq, moreFits⟩ := flatTimes_complete (fuel := fuel) more flat middleEq middleBound within
        (Nat.le_trans (parses_shorter first) short)
      exact ⟨stop, bound, restEq, .succ firstFits moreFits⟩
end

end Complete

/-- The entry fuel a payload in this frame needs: one level per pattern and stage below the
ceilings, and one per word for recursion. -/
def need (doc : Document) (frame : Frame) : Nat := (frame.patterns + frame.stages + 1) * (doc.extra.length + 1)

theorem need_pattern (doc : Document) (frame : Frame) {word : Nat} (below : word < frame.patterns) :
    need doc { frame with patterns := word } + (doc.extra.length + 1) ≤ need doc frame := by
  unfold need
  rw [← Nat.succ_mul]
  exact Nat.mul_le_mul_right _ (by simp only; omega)

theorem need_stage (doc : Document) (frame : Frame) {word : Nat} (below : word < frame.stages) :
    need doc { frame with stages := word } + (doc.extra.length + 1) ≤ need doc frame := by
  unfold need
  rw [← Nat.succ_mul]
  exact Nat.mul_le_mul_right _ (by simp only; omega)

theorem need_word (doc : Document) (frame : Frame) : doc.extra.length + 1 ≤ need doc frame := by
  unfold need
  have := Nat.mul_le_mul_right (doc.extra.length + 1) (Nat.succ_le_succ (Nat.zero_le (frame.patterns + frame.stages)))
  simpa using this

section Complete
variable {env : Env} (agrees : Agrees env) (regular : Regular env.schema)
include agrees regular

mutual
theorem parses_complete : ∀ {frame : Frame} {shape : Shape} {words rest : List Nat} {claims : List Claim},
    Parses env.schema env.doc frame shape words rest claims →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → need env.doc frame ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        Fits env (EntryAt env fuel) (scopeOf env.doc frame) shape position limit stop claims
  | frame, _, _, _, _, .word notPattern notStage ok, _, position, limit, wordsEq, _, within, _ => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      refine ⟨position + 1, readWord_below readEq, restEq, ?_⟩
      rw [← wordClaims_taken]
      exact .word notPattern notStage readEq ((roleOk_scopeOf agrees frame _ _).mpr ok)
  | frame, _, _, _, _, .pattern bound tagEq named payload fields, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, fieldsEq⟩ := (payloadOf_iff _ _ _ _).mp payload
      have innerWithin := payloadRange_within rangeEq
      have lowered := need_pattern env.doc frame bound
      obtain ⟨remaining, rfl⟩ : ∃ remaining, fuel = remaining + 1 := ⟨fuel - 1, by omega⟩
      obtain ⟨stop, _, emptyEq, fits⟩ := parsesAll_complete (fuel := remaining) fields fieldsEq (Nat.le_add_right _ _)
        innerWithin (by omega)
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      have entered : EntryAt env (remaining + 1) (scopeOf env.doc { frame with patterns := _ }) (.seq _) start
          (start + length) (start + length) _ := Fits.seq fits
      exact ⟨position + 1, readWord_below readEq, restEq,
        .pattern ⟨_, _, start, length, _, readEq, bound, (agrees.patterns _ _).mpr ⟨_, tagEq, named⟩, rangeEq, entered,
          rfl⟩⟩
  | frame, _, _, _, _, .stage bound tagEq named payload fields, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, fieldsEq⟩ := (payloadOf_iff _ _ _ _).mp payload
      have innerWithin := payloadRange_within rangeEq
      have lowered := need_stage env.doc frame bound
      obtain ⟨remaining, rfl⟩ : ∃ remaining, fuel = remaining + 1 := ⟨fuel - 1, by omega⟩
      obtain ⟨stop, _, emptyEq, fits⟩ := parsesAll_complete (fuel := remaining) fields fieldsEq (Nat.le_add_right _ _)
        innerWithin (by omega)
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      have entered : EntryAt env (remaining + 1) (scopeOf env.doc { frame with stages := _ }) (.seq _) start
          (start + length) (start + length) _ := Fits.seq fits
      exact ⟨position + 1, readWord_below readEq, restEq,
        .stage ⟨_, _, start, length, _, readEq, bound, (agrees.stages _ _).mpr ⟨_, tagEq, named⟩, rangeEq, entered,
          rfl⟩⟩
  | _, _, _, _, _, .seq items, fuel, _, _, wordsEq, below, within, enough => by
      obtain ⟨stop, bound, restEq, fits⟩ := parsesAll_complete (fuel := fuel) items wordsEq below within enough
      exact ⟨stop, bound, restEq, .seq fits⟩
  | _, _, _, _, _, .choice tagAt first variantAt parsed, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨stop, bound, restWindow, fits⟩ := parses_complete (fuel := fuel) parsed restEq (readWord_below readEq)
        within enough
      exact ⟨stop, bound, restWindow, .choice readEq (fitsChoice_of fits tagAt first variantAt)⟩
  | _, _, _, _, _, .many times, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨stop, bound, restWindow, fits⟩ := parsesTimes_complete (fuel := fuel) times restEq (readWord_below readEq)
        within enough
      exact ⟨stop, bound, restWindow, .many readEq fits⟩
  | _, _, _, _, _, .block blockOf inner, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, innerEq⟩ := (blockOf_iff _ _ _ _ _).mp blockOf
      have innerWithin := blockRange_within rangeEq
      obtain ⟨stop, bound, emptyEq, fits⟩ := parses_complete (fuel := fuel) inner innerEq (Nat.le_add_right _ _)
        innerWithin enough
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      exact ⟨position + 1, readWord_below readEq, restEq, .block readEq rangeEq fits⟩
  | frame, _, _, _, _, .recur member named (.choice tagAt first variantAt parsed), fuel, position, limit, wordsEq, _,
      within, enough => by
      obtain ⟨readEq, restEq⟩ := (window_cons within).mp wordsEq.symm
      have enoughWord := need_word env.doc frame
      obtain ⟨remaining, rfl⟩ : ∃ remaining, fuel = remaining + 1 := ⟨fuel - 1, by omega⟩
      have short : (window env.doc (position + 1) limit).length ≤ remaining := by
        rw [window_length within]
        omega
      obtain ⟨stop, bound, restWindow, fits⟩ := flat_complete agrees regular (fuel := remaining) parsed
        (flatShapes_get (regular.flat _ member) variantAt) restEq (readWord_below readEq) within
        (by rw [restEq]; exact short)
      have chosen : Fits env (EntryAt env remaining) (scopeOf env.doc frame) (.choice _ _) position limit stop _ :=
        .choice readEq (fitsChoice_of fits tagAt first variantAt)
      exact ⟨stop, bound, restWindow, .recur ((lookupRecursive_iff agrees.names _ _).mpr ⟨member, named⟩) chosen⟩
  | frame, _, _, _, _, @Parses.text _ _ _ _ count _ rest _ kinds texts blockOf holes kindsLength kindsBound textsLength
      textsIds, fuel, position, limit, wordsEq, _, within, enough => by
      obtain ⟨readEq, tailEq⟩ := (window_cons within).mp wordsEq.symm
      obtain ⟨start, length, rangeEq, blockEq⟩ := (blockOf_iff _ _ _ _ _).mp blockOf
      have innerWithin := blockRange_within rangeEq
      obtain ⟨countEq, holesEq⟩ := (window_cons innerWithin).mp blockEq.symm
      obtain ⟨stop, _, emptyEq, refs⟩ := holes_complete (fuel := fuel) holes rfl holesEq (readWord_below countEq)
        innerWithin enough
      have stopEq : stop = start + length := by
        have := (window_nil innerWithin).mp emptyEq.symm
        omega
      subst stopEq
      obtain ⟨kindsFit, textsRest, kindsBelow⟩ := wordsFit_complete (scope := scopeOf env.doc frame) (role := .textHole)
        within (fun _ => rfl) kinds (fun kind member => kindsBound kind member) tailEq.symm (readWord_below readEq)
      rw [kindsLength] at kindsFit textsRest kindsBelow
      obtain ⟨textsFit, restWindow, textsBelow⟩ := wordsFit_complete (scope := scopeOf env.doc frame) (role := .stringId)
        within (fun _ => rfl) texts (fun text member => (idIn_iff _ _).mp (textsIds text member)) textsRest.symm
        kindsBelow
      rw [textsLength] at textsFit restWindow textsBelow
      have fits := Fits.textPattern readEq rangeEq countEq refs kindsFit textsFit
      rw [List.append_nil, List.append_nil] at fits
      exact ⟨_, textsBelow, restWindow, fits⟩
theorem parsesAll_complete : ∀ {frame : Frame} {items : List Shape} {words rest : List Nat} {claims : List Claim},
    ParsesAll env.schema env.doc frame items words rest claims →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → need env.doc frame ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsList env (EntryAt env fuel) (scopeOf env.doc frame) items position limit stop claims
  | _, _, _, _, _, .nil, _, position, _, wordsEq, below, _, _ => ⟨position, below, wordsEq, .nil⟩
  | _, _, _, _, _, .cons first more, fuel, _, _, wordsEq, below, within, enough => by
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := parses_complete (fuel := fuel) first wordsEq below within enough
      obtain ⟨stop, bound, restEq, moreFits⟩ := parsesAll_complete (fuel := fuel) more middleEq middleBound within enough
      exact ⟨stop, bound, restEq, .cons firstFits moreFits⟩
theorem parsesTimes_complete : ∀ {frame : Frame} {item : Shape} {count : Nat} {words rest : List Nat}
    {claims : List Claim}, ParsesTimes env.schema env.doc frame item count words rest claims →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → need env.doc frame ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        FitsMany env (EntryAt env fuel) (scopeOf env.doc frame) item limit count position stop claims
  | _, _, _, _, _, _, .zero, _, position, _, wordsEq, below, _, _ => ⟨position, below, wordsEq, .zero⟩
  | _, _, _, _, _, _, .succ first more, fuel, _, _, wordsEq, below, within, enough => by
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := parses_complete (fuel := fuel) first wordsEq below within enough
      obtain ⟨stop, bound, restEq, moreFits⟩ := parsesTimes_complete (fuel := fuel) more middleEq middleBound within
        enough
      exact ⟨stop, bound, restEq, .succ firstFits moreFits⟩
theorem holes_complete : ∀ {frame : Frame} {item : Shape} {count : Nat} {words rest : List Nat} {claims : List Claim},
    ParsesTimes env.schema env.doc frame item count words rest claims → item = .word .pattern →
    ∀ {fuel position limit : Nat}, words = window env.doc position limit → position ≤ limit →
      limit ≤ env.doc.extra.length → need env.doc frame ≤ fuel →
      ∃ stop, stop ≤ limit ∧ rest = window env.doc stop limit ∧
        PatternRefs env (EntryAt env fuel) (scopeOf env.doc frame) limit count position stop claims
  | _, _, _, _, _, _, .zero, _, _, position, _, wordsEq, below, _, _ => ⟨position, below, wordsEq, .zero⟩
  | _, _, _, _, _, _, .succ first more, isPattern, fuel, _, _, wordsEq, below, within, enough => by
      obtain ⟨middle, middleBound, middleEq, firstFits⟩ := parses_complete (fuel := fuel) first wordsEq below within enough
      obtain ⟨stop, bound, restEq, moreRefs⟩ := holes_complete (fuel := fuel) more isPattern middleEq middleBound within
        enough
      subst isPattern
      cases firstFits with
      | pattern reference => exact ⟨stop, bound, restEq, .succ reference moreRefs⟩
      | word notPattern _ _ _ => exact absurd rfl notPattern
end

end Complete

/-! ### Jobs are specification sites -/

theorem ownerWord_eq (doc : Document) (index : Nat) : ownerWord doc index = (ownerAt doc index).word := by
  unfold ownerWord ownerAt
  by_cases function : index < doc.functions.length
  · simp [function, Owner.word]
  · simp [function, Owner.word, driverBit, driverOwnerBit]

theorem scopeFor_eq {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length)
    {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) (user : Option Nat) :
    scopeFor doc index user =
      scopeOf doc ⟨some (ownerAt doc index), user, (ownerAt doc index).slots doc, doc.patterns.length,
        doc.stages.length, doc.values.length, none⟩ := by
  unfold scopeFor
  rw [ownerSegment_eq lengths inside, ownerWord_eq]
  rfl

theorem ownerAt_function (doc : Document) {index : Nat} (inside : index < doc.functions.length) :
    ownerAt doc index = .function index := by simp [ownerAt, inside]

theorem ownerAt_step (doc : Document) (index : Nat) : ownerAt doc (doc.functions.length + index) = .step index := by
  simp [ownerAt]

theorem ownerRows_eq {doc : Document} (lengths : doc.functionInstructionStarts.length = doc.functions.length)
    {index : Nat} (inside : index < doc.functions.length + doc.driverSteps.length) :
    ownerRows doc index = (ownerAt doc index).instructions doc := by
  unfold ownerRows Owner.instructions
  rw [ownerSegment_eq lengths inside]

theorem budget_need (doc : Document) (frame : Frame) (patterns : frame.patterns ≤ doc.patterns.length)
    (stages : frame.stages ≤ doc.stages.length) : need doc frame ≤ budget doc :=
  Nat.mul_le_mul_right _ (by omega)

section Sites
variable {env : Env} (agrees : Agrees env) (regular : Regular env.schema)
include agrees regular

theorem payload_iff {frame : Frame} (bounded : need env.doc frame ≤ budget env.doc) {fields : List Shape}
    {start length : Nat} {claims : List Claim} (within : start + length ≤ env.doc.extra.length) :
    entryFit (fitFuel env (budget env.doc + 1)) (scopeOf env.doc frame) fields start length = some claims ↔
      ParsesAll env.schema env.doc frame fields (window env.doc start (start + length)) [] claims := by
  rw [entryFit_eq_some (fitFuel_iff env (budget env.doc + 1))]
  unfold EntryFits
  constructor
  · intro fits
    have parsed := fits_sound agrees (entryAt_sound agrees (budget env.doc)) fits within
    rw [(window_nil within).mpr (Nat.le_refl _)] at parsed
    cases parsed with
    | seq all => exact all
  · intro all
    obtain ⟨stop, _, emptyEq, fits⟩ := parsesAll_complete agrees regular (fuel := budget env.doc) all rfl
      (Nat.le_add_right _ _) within bounded
    have stopEq : stop = start + length := by
      have := (window_nil within).mp emptyEq.symm
      omega
    subst stopEq
    exact Fits.seq fits

theorem runJob_eq_some {job : Job} {claims : List Claim} :
    runJob env job = some claims ↔ ∃ inner, entryFit (fitFuel env (budget env.doc + 1)) job.scope job.fields job.start
      job.length = some inner ∧ claims = job.head ++ inner := by
  unfold runJob
  rw [Option.map_eq_some_iff]
  constructor
  · rintro ⟨inner, fitted, rfl⟩; exact ⟨inner, fitted, rfl⟩
  · rintro ⟨inner, fitted, rfl⟩; exact ⟨inner, fitted, rfl⟩

theorem instructionSite_iff (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
    {index row : Nat} (inside : index < env.doc.functions.length + env.doc.driverSteps.length) {claims : List Claim} :
    (instructionJob env index row).bind (runJob env) = some claims ↔
      ∃ tag entry words, env.doc.tags[row]? = some tag ∧ Names env.schema.instructions tag entry ∧
        PayloadOf env.doc env.doc.data row words ∧
        ParsesAll env.schema env.doc (instructionFrame env.doc (ownerAt env.doc index) row) entry.fields words [] claims := by
  have bounded : need env.doc (instructionFrame env.doc (ownerAt env.doc index) row) ≤ budget env.doc :=
    budget_need _ _ (Nat.le_refl _) (Nat.le_refl _)
  unfold instructionJob
  constructor
  · intro ran
    obtain ⟨job, jobEq, runEq⟩ := Option.bind_eq_some_iff.mp ran
    obtain ⟨entry, entryEq, rest⟩ := Option.bind_eq_some_iff.mp jobEq
    obtain ⟨⟨start, length⟩, rangeEq, pureEq⟩ := Option.bind_eq_some_iff.mp rest
    cases pureEq
    obtain ⟨tag, tagEq, named⟩ := (agrees.instructions row entry).mp entryEq
    obtain ⟨inner, fitted, rfl⟩ := (runJob_eq_some agrees regular).mp runEq
    rw [scopeFor_eq lengths inside] at fitted
    have within := payloadRange_within rangeEq
    refine ⟨tag, entry, _, tagEq, named, (payloadOf_iff _ _ _ _).mpr ⟨start, length, rangeEq, rfl⟩, ?_⟩
    exact (payload_iff agrees regular bounded within).mp fitted
  · rintro ⟨tag, entry, words, tagEq, named, payload, parsed⟩
    obtain ⟨start, length, rangeEq, rfl⟩ := (payloadOf_iff _ _ _ _).mp payload
    have within := payloadRange_within rangeEq
    have entryEq := (agrees.instructions row entry).mpr ⟨tag, tagEq, named⟩
    have fitted : entryFit (fitFuel env (budget env.doc + 1)) (scopeFor env.doc index (some row)) entry.fields start
        length = some claims := by
      rw [scopeFor_eq lengths inside]
      exact (payload_iff agrees regular bounded within).mpr parsed
    have jobEq : (do
        let tag ← env.instructions[row]?
        let (payload, length) ← payloadRange env.doc.data row env.doc.extra.length
        pure (⟨scopeFor env.doc index (some row), tag.fields, payload, length, []⟩ : Job)) =
        some ⟨scopeFor env.doc index (some row), entry.fields, start, length, []⟩ := by
      rw [entryEq]
      show (payloadRange env.doc.data row env.doc.extra.length).bind _ = _
      rw [rangeEq]
      rfl
    rw [jobEq]
    exact (runJob_eq_some agrees regular).mpr ⟨claims, fitted, rfl⟩

theorem stepSite_iff (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
    {step : Nat} (inside : step < env.doc.driverSteps.length) {claims : List Claim} :
    (driverJob env step).bind (runJob env) = some claims ↔
      ∃ tag entry words, env.doc.driverStepTags[step]? = some tag ∧ Names env.schema.drivers tag entry ∧
        PayloadOf env.doc env.doc.driverSteps step words ∧
        ParsesAll env.schema env.doc (stepFrame env.doc step) entry.fields words [] claims := by
  have bounded : need env.doc (stepFrame env.doc step) ≤ budget env.doc :=
    budget_need _ _ (Nat.le_refl _) (Nat.le_refl _)
  have scopeEq : { scopeFor env.doc (env.doc.functions.length + step) none with importer := some step } =
      scopeOf env.doc (stepFrame env.doc step) := by
    rw [scopeFor_eq lengths (by omega), ownerAt_step]
    rfl
  unfold driverJob
  constructor
  · intro ran
    obtain ⟨job, jobEq, runEq⟩ := Option.bind_eq_some_iff.mp ran
    obtain ⟨entry, entryEq, rest⟩ := Option.bind_eq_some_iff.mp jobEq
    obtain ⟨⟨start, length⟩, rangeEq, pureEq⟩ := Option.bind_eq_some_iff.mp rest
    cases pureEq
    obtain ⟨tag, tagEq, named⟩ := (agrees.drivers step entry).mp entryEq
    obtain ⟨inner, fitted, rfl⟩ := (runJob_eq_some agrees regular).mp runEq
    have fitted' : entryFit _ (scopeOf env.doc (stepFrame env.doc step)) _ _ _ = _ := scopeEq ▸ fitted
    have within := payloadRange_within rangeEq
    refine ⟨tag, entry, _, tagEq, named, (payloadOf_iff _ _ _ _).mpr ⟨start, length, rangeEq, rfl⟩, ?_⟩
    exact (payload_iff agrees regular bounded within).mp fitted'
  · rintro ⟨tag, entry, words, tagEq, named, payload, parsed⟩
    obtain ⟨start, length, rangeEq, rfl⟩ := (payloadOf_iff _ _ _ _).mp payload
    have within := payloadRange_within rangeEq
    have entryEq := (agrees.drivers step entry).mpr ⟨tag, tagEq, named⟩
    have fitted : entryFit (fitFuel env (budget env.doc + 1))
        { scopeFor env.doc (env.doc.functions.length + step) none with importer := some step } entry.fields start
        length = some claims := by
      rw [scopeEq]
      exact (payload_iff agrees regular bounded within).mpr parsed
    have jobEq : (do
        let tag ← env.drivers[step]?
        let (payload, length) ← payloadRange env.doc.driverSteps step env.doc.extra.length
        let scope := { scopeFor env.doc (env.doc.functions.length + step) none with importer := some step }
        pure (⟨scope, tag.fields, payload, length, []⟩ : Job)) =
        some ⟨{ scopeFor env.doc (env.doc.functions.length + step) none with importer := some step }, entry.fields,
          start, length, []⟩ := by
      rw [entryEq]
      show (payloadRange env.doc.driverSteps step env.doc.extra.length).bind _ = _
      rw [rangeEq]
      rfl
    rw [jobEq]
    exact (runJob_eq_some agrees regular).mpr ⟨claims, fitted, rfl⟩

theorem valueSite_iff {value : Nat} {claims : List Claim} :
    (valueJob env value).bind (runJob env) = some claims ↔
      ∃ tag entry words, env.doc.values[value]? = some tag ∧ Names env.schema.values tag entry ∧
        PayloadOf env.doc env.doc.valueData value words ∧
        ParsesAll env.schema env.doc (valueFrame value) entry.fields words [] claims := by
  have bounded : need env.doc (valueFrame value) ≤ budget env.doc :=
    budget_need _ _ (Nat.zero_le _) (Nat.zero_le _)
  unfold valueJob
  constructor
  · intro ran
    obtain ⟨job, jobEq, runEq⟩ := Option.bind_eq_some_iff.mp ran
    obtain ⟨entry, entryEq, rest⟩ := Option.bind_eq_some_iff.mp jobEq
    obtain ⟨⟨start, length⟩, rangeEq, pureEq⟩ := Option.bind_eq_some_iff.mp rest
    cases pureEq
    obtain ⟨tag, tagEq, named⟩ := (agrees.values value entry).mp entryEq
    obtain ⟨inner, fitted, rfl⟩ := (runJob_eq_some agrees regular).mp runEq
    have within := payloadRange_within rangeEq
    refine ⟨tag, entry, _, tagEq, named, (payloadOf_iff _ _ _ _).mpr ⟨start, length, rangeEq, rfl⟩, ?_⟩
    exact (payload_iff agrees regular bounded within).mp fitted
  · rintro ⟨tag, entry, words, tagEq, named, payload, parsed⟩
    obtain ⟨start, length, rangeEq, rfl⟩ := (payloadOf_iff _ _ _ _).mp payload
    have within := payloadRange_within rangeEq
    have entryEq := (agrees.values value entry).mpr ⟨tag, tagEq, named⟩
    have fitted := (payload_iff agrees regular bounded within).mpr parsed
    have jobEq : (do
        let tag ← env.values[value]?
        let (payload, length) ← payloadRange env.doc.valueData value env.doc.extra.length
        pure (⟨valueScope value, tag.fields, payload, length, []⟩ : Job)) =
        some ⟨valueScope value, entry.fields, start, length, []⟩ := by
      rw [entryEq]
      show (payloadRange env.doc.valueData value env.doc.extra.length).bind _ = _
      rw [rangeEq]
      rfl
    rw [jobEq]
    exact (runJob_eq_some agrees regular).mpr ⟨claims, fitted, rfl⟩

theorem bodySite_iff (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
    {function : Nat} (inside : function < env.doc.functions.length) {claims : List Claim} :
    (bodyJob env function).bind (runJob env) = some claims ↔
      ∃ row, env.doc.functions[function]? = some row ∧
        Parses env.schema env.doc (bodyFrame env.doc function) (.block true (.many (.word .stmt))) [cell row 7] [] claims := by
  have bounded : need env.doc (bodyFrame env.doc function) ≤ budget env.doc :=
    budget_need _ _ (Nat.le_refl _) (Nat.le_refl _)
  have scopeEq : scopeFor env.doc function none = scopeOf env.doc (bodyFrame env.doc function) := by
    rw [scopeFor_eq lengths (by omega), ownerAt_function _ inside]
    rfl
  have rowEq := List.getElem?_eq_getElem inside
  have bodyEq : bodyBlock env.doc function = cell env.doc.functions[function] 7 := by
    unfold bodyBlock; rw [rowEq]; rfl
  have ownerEq : (scopeOf env.doc (bodyFrame env.doc function)).owner = function + 1 := rfl
  unfold bodyJob
  rw [bodyEq]
  constructor
  · intro ran
    obtain ⟨job, jobEq, runEq⟩ := Option.bind_eq_some_iff.mp ran
    obtain ⟨⟨start, length⟩, rangeEq, pureEq⟩ := Option.bind_eq_some_iff.mp jobEq
    cases pureEq
    obtain ⟨inner, fitted, rfl⟩ := (runJob_eq_some agrees regular).mp runEq
    rw [scopeEq] at fitted
    have within := blockRange_within rangeEq
    have all := (payload_iff agrees regular bounded within).mp fitted
    cases all with
    | cons first rest =>
        cases rest
        refine ⟨_, rowEq, ?_⟩
        rw [List.append_nil]
        exact .block ((blockOf_iff _ _ _ _ _).mpr ⟨start, length, ownerEq ▸ rangeEq, rfl⟩) first
  · rintro ⟨row, sameRow, parsed⟩
    rw [rowEq] at sameRow
    cases sameRow
    cases parsed with
    | block blockOf inner =>
        obtain ⟨start, length, rangeEq, rfl⟩ := (blockOf_iff _ _ _ _ _).mp blockOf
        have within := blockRange_within rangeEq
        rw [ownerEq] at rangeEq
        rw [rangeEq]
        refine Option.bind_eq_some_iff.mpr ⟨_, rfl, (runJob_eq_some agrees regular).mpr ⟨_, ?_, rfl⟩⟩
        show entryFit _ (scopeFor env.doc function none) _ _ _ = _
        rw [scopeEq]
        have all : ParsesAll env.schema env.doc (bodyFrame env.doc function) [.many (.word .stmt)]
            (window env.doc start (start + length)) [] (_ ++ []) := .cons inner .nil
        rw [List.append_nil] at all
        exact (payload_iff agrees regular bounded within).mpr all

end Sites

theorem mapM_eq_map_getD {α β : Type} (f : α → Option β) (fallback : β) :
    ∀ {items : List α} {results : List β}, items.mapM f = some results →
      results = items.map (fun item => (f item).getD fallback) ∧ ∀ item ∈ items, ∃ result, f item = some result
  | [], results, mapped => by
      cases mapped
      exact ⟨rfl, fun _ member => by cases member⟩
  | item :: items, results, mapped => by
      rw [List.mapM_cons] at mapped
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at mapped
      obtain ⟨first, firstEq, rest, restEq, rfl⟩ := mapped
      obtain ⟨restMap, restSome⟩ := mapM_eq_map_getD f fallback restEq
      refine ⟨by simp [firstEq, restMap], ?_⟩
      intro other member
      rcases List.mem_cons.mp member with rfl | inner
      · exact ⟨first, firstEq⟩
      · exact restSome other inner

theorem mapM_of_some {α β : Type} (f : α → Option β) (fallback : β) :
    ∀ {items : List α}, (∀ item ∈ items, ∃ result, f item = some result) →
      items.mapM f = some (items.map fun item => (f item).getD fallback)
  | [], _ => rfl
  | item :: items, every => by
      obtain ⟨first, firstEq⟩ := every item (List.mem_cons_self ..)
      rw [List.mapM_cons, firstEq, mapM_of_some f fallback (fun other member => every other (List.mem_cons_of_mem _ member))]
      simp [firstEq]

/-- A job value for positions where every site already produced one. -/
def unusedJob : Job := ⟨valueScope 0, [], 0, 0, []⟩

theorem getD_run {job : Option Job} {run : Job → Option (List Claim)} {claims : List Claim}
    (ran : job.bind run = some claims) : run (job.getD unusedJob) = some claims := by
  cases job with
  | none => cases ran
  | some job => exact ran

theorem rangeClaims_eq (rows : List Row) (column : Nat) (claim : Nat → Claim) :
    Xsh.IR.Raw.rangeClaims rows column claim = Spec.rangeClaims rows column claim := by
  unfold Xsh.IR.Raw.rangeClaims Spec.rangeClaims
  congr 1
  funext index
  cases rows[index]? <;> rfl

theorem tableClaims_eq (doc : Document) : Xsh.IR.Raw.tableClaims doc = Spec.tableClaims doc := by
  unfold Xsh.IR.Raw.tableClaims Spec.tableClaims
  simp only [rangeClaims_eq]

theorem claimed_iff (doc : Document) (claims : List Claim) : ClaimsWF doc claims ↔ Claimed doc claims := by
  have blocks : (∀ block, block < doc.blocks.length → rowWord (doc.blocks.getD block []) 3 ≠ irNone →
      Claim.block block ∈ claims) ↔
      (∀ block row, doc.blocks[block]? = some row → cell row 3 ≠ absent → Claim.block block ∈ claims) := by
    constructor
    · intro every block row rowEq owned
      have inside := (List.getElem?_eq_some_iff.mp rowEq).1
      refine every block inside ?_
      rw [List.getD_eq_getElem?_getD, rowEq]
      exact owned
    · intro every block inside owned
      rw [List.getD_eq_getElem?_getD, List.getElem?_eq_getElem inside] at owned
      exact every block _ (List.getElem?_eq_getElem inside) owned
  unfold ClaimsWF Claimed
  rw [blocks]
  constructor
  · rintro ⟨nodup, patterns, stages, instructions, owned, programs, rest⟩
    exact ⟨nodup, instructions, owned, patterns, stages, programs, rest⟩
  · rintro ⟨nodup, instructions, owned, patterns, stages, programs, rest⟩
    exact ⟨nodup, patterns, stages, instructions, owned, programs, rest⟩

/-- Each site's checker run produced the assignment's claims. -/
structure Ran (env : Env) (assignment : Assignment) : Prop where
  instructions : ∀ index, index < env.doc.functions.length + env.doc.driverSteps.length →
    ∀ row ∈ ownerRows env.doc index,
      (instructionJob env index row).bind (runJob env) = some (assignment.instruction (ownerAt env.doc index) row)
  steps : ∀ step, step < env.doc.driverSteps.length → (driverJob env step).bind (runJob env) = some (assignment.step step)
  bodies : ∀ function, function < env.doc.functions.length →
    (bodyJob env function).bind (runJob env) = some (assignment.body function)
  values : ∀ value, value < env.doc.values.length → (valueJob env value).bind (runJob env) = some (assignment.value value)

theorem bind_some_job {job : Option Job} {run : Job → Option (List Claim)} {claims : List Claim}
    (ran : job.bind run = some claims) : ∃ result, job = some result := by
  cases job with
  | none => cases ran
  | some job => exact ⟨job, rfl⟩

theorem flatMap_congr {α β : Type} {items : List α} {first second : α → List β}
    (same : ∀ item ∈ items, first item = second item) : items.flatMap first = items.flatMap second := by
  unfold List.flatMap
  rw [List.map_congr_left same]

theorem ran_jobs {env : Env} (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
    {assignment : Assignment} (ran : Ran env assignment) :
    ∃ jobs claims, documentJobs env = some jobs ∧ jobs.mapM (runJob env) = some claims ∧
      claims.flatten ++ Xsh.IR.Raw.tableClaims env.doc = assignment.all env.doc := by
  have ownersLength : (ownerStarts env.doc).length = env.doc.functions.length + env.doc.driverSteps.length := by
    simp [ownerStarts, lengths]
  let instructionJobsOf := fun index =>
    (ownerRows env.doc index).map fun row => (instructionJob env index row).getD unusedJob
  have instructionsEq : (List.range (ownerStarts env.doc).length).mapM (instructionJobs env) =
      some ((List.range (ownerStarts env.doc).length).map instructionJobsOf) := by
    have := mapM_of_some (instructionJobs env) [] (items := List.range (ownerStarts env.doc).length) (by
      intro index member
      have inside : index < _ := ownersLength ▸ List.mem_range.mp member
      exact ⟨_, mapM_of_some _ unusedJob fun row rowMember =>
        bind_some_job (ran.instructions index inside row rowMember)⟩)
    rw [this]
    congr 1
    apply List.map_congr_left
    intro index member
    have inside : index < _ := ownersLength ▸ List.mem_range.mp member
    unfold instructionJobs
    rw [mapM_of_some _ unusedJob fun row rowMember => bind_some_job (ran.instructions index inside row rowMember)]
    rfl
  have stepsEq := mapM_of_some (driverJob env) unusedJob (items := List.range env.doc.driverSteps.length)
    fun step member => bind_some_job (ran.steps step (List.mem_range.mp member))
  have bodiesEq := mapM_of_some (bodyJob env) unusedJob (items := List.range env.doc.functions.length)
    fun function member => bind_some_job (ran.bodies function (List.mem_range.mp member))
  have valuesEq := mapM_of_some (valueJob env) unusedJob (items := List.range env.doc.values.length)
    fun value member => bind_some_job (ran.values value (List.mem_range.mp member))
  let jobs := ((List.range (ownerStarts env.doc).length).map instructionJobsOf).flatten ++
    (List.range env.doc.driverSteps.length).map (fun step => (driverJob env step).getD unusedJob) ++
    (List.range env.doc.functions.length).map (fun function => (bodyJob env function).getD unusedJob) ++
    (List.range env.doc.values.length).map (fun value => (valueJob env value).getD unusedJob)
  have jobsEq : documentJobs env = some jobs := by
    unfold documentJobs
    rw [instructionsEq, stepsEq, bodiesEq, valuesEq]
    rfl
  have runs : ∀ job ∈ jobs, ∃ claims, runJob env job = some claims := by
    intro job member
    simp only [jobs, List.mem_append, List.mem_flatten, List.mem_map, List.mem_range] at member
    rcases member with ((⟨list, ⟨index, inside, rfl⟩, rowMember⟩ | ⟨step, inside, rfl⟩) | ⟨function, inside, rfl⟩) |
      ⟨value, inside, rfl⟩
    · obtain ⟨row, rowIn, rfl⟩ := List.mem_map.mp rowMember
      exact ⟨_, getD_run (ran.instructions index (ownersLength ▸ inside) row rowIn)⟩
    · exact ⟨_, getD_run (ran.steps step inside)⟩
    · exact ⟨_, getD_run (ran.bodies function inside)⟩
    · exact ⟨_, getD_run (ran.values value inside)⟩
  refine ⟨jobs, _, jobsEq, mapM_of_some (runJob env) [] runs, ?_⟩
  rw [tableClaims_eq]
  unfold Assignment.all
  congr 1
  have instructionPart : ((((List.range (ownerStarts env.doc).length).map instructionJobsOf).flatten).map
      (fun job => (runJob env job).getD [])).flatten =
      (owners env.doc).flatMap (fun owner => (owner.instructions env.doc).flatMap (assignment.instruction owner)) := by
    rw [owners_eq, List.flatMap_map, List.map_flatten, List.flatten_flatten, List.map_map, ownersLength, List.map_map]
    unfold List.flatMap
    congr 1
    apply List.map_congr_left
    intro index member
    have inside := List.mem_range.mp member
    simp only [Function.comp, instructionJobsOf, List.map_map]
    rw [← ownerRows_eq lengths inside]
    congr 1
    apply List.map_congr_left
    intro row rowMember
    simp only [Function.comp, getD_run (ran.instructions index inside row rowMember), Option.getD_some]
  have pointwise : ∀ {site : Nat → Option Job} {claims : Nat → List Claim} {count : Nat},
      (∀ index, index < count → (site index).bind (runJob env) = some (claims index)) →
      (((List.range count).map fun index => (site index).getD unusedJob).map
        (fun job => (runJob env job).getD [])).flatten = (List.range count).flatMap claims := by
    intro site claims count every
    rw [List.map_map]
    unfold List.flatMap
    congr 1
    apply List.map_congr_left
    intro index member
    simp only [Function.comp, getD_run (every index (List.mem_range.mp member)), Option.getD_some]
  simp only [jobs, List.map_append, List.flatten_append]
  rw [instructionPart, pointwise ran.steps, pointwise ran.bodies, pointwise ran.values]

/-- The claims the checker's run of each site produced. -/
def assignmentOf (env : Env) : Assignment :=
  { instruction := fun owner row => ((instructionJob env (ownerIndex env.doc owner) row).bind (runJob env)).getD []
    step := fun step => ((driverJob env step).bind (runJob env)).getD []
    body := fun function => ((bodyJob env function).bind (runJob env)).getD []
    value := fun value => ((valueJob env value).bind (runJob env)).getD [] }

theorem site_ran {job : Option Job} {jobs : List Job} {claims : List (List Claim)}
    (claimsEq : jobs.mapM (runJob env) = some claims) (member : ∀ result, job = some result → result ∈ jobs)
    (present : ∃ result, job = some result) : job.bind (runJob env) = some ((job.bind (runJob env)).getD []) := by
  obtain ⟨result, rfl⟩ := present
  obtain ⟨_, runs⟩ := mapM_eq_map_getD (runJob env) [] claimsEq
  obtain ⟨claimed, claimedEq⟩ := runs result (member result rfl)
  simp [claimedEq]

theorem jobs_ran {env : Env} (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
    {jobs : List Job} {claims : List (List Claim)} (jobsEq : documentJobs env = some jobs)
    (claimsEq : jobs.mapM (runJob env) = some claims) : Ran env (assignmentOf env) := by
  have ownersLength : (ownerStarts env.doc).length = env.doc.functions.length + env.doc.driverSteps.length := by
    simp [ownerStarts, lengths]
  unfold documentJobs at jobsEq
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at jobsEq
  obtain ⟨instructions, instructionsEq, steps, stepsEq, bodies, bodiesEq, values, valuesEq, rfl⟩ := jobsEq
  obtain ⟨instructionsMap, instructionsSome⟩ := mapM_eq_map_getD _ [] instructionsEq
  obtain ⟨stepsMap, stepsSome⟩ := mapM_eq_map_getD _ unusedJob stepsEq
  obtain ⟨bodiesMap, bodiesSome⟩ := mapM_eq_map_getD _ unusedJob bodiesEq
  obtain ⟨valuesMap, valuesSome⟩ := mapM_eq_map_getD _ unusedJob valuesEq
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro index inside row rowMember
    have indexMember : index ∈ List.range (ownerStarts env.doc).length := List.mem_range.mpr (ownersLength ▸ inside)
    obtain ⟨rowJobs, rowJobsEq⟩ := instructionsSome index indexMember
    obtain ⟨rowsMap, rowsSome⟩ := mapM_eq_map_getD _ unusedJob rowJobsEq
    have ran := site_ran claimsEq (job := instructionJob env index row) (fun result resultEq => by
      simp only [List.mem_append, List.mem_flatten]
      refine Or.inl (Or.inl (Or.inl ⟨rowJobs, ?_, ?_⟩))
      · rw [instructionsMap]
        exact List.mem_map.mpr ⟨index, indexMember, by simp [rowJobsEq]⟩
      · rw [rowsMap]
        exact List.mem_map.mpr ⟨row, rowMember, by simp [resultEq]⟩) (rowsSome row rowMember)
    simp only [assignmentOf, ownerIndex_ownerAt]
    exact ran
  · intro step inside
    exact site_ran claimsEq (fun result resultEq => by
      simp only [List.mem_append]
      refine Or.inl (Or.inl (Or.inr ?_))
      rw [stepsMap]
      exact List.mem_map.mpr ⟨step, List.mem_range.mpr inside, by simp [resultEq]⟩)
      (stepsSome step (List.mem_range.mpr inside))
  · intro function inside
    exact site_ran claimsEq (fun result resultEq => by
      simp only [List.mem_append]
      refine Or.inl (Or.inr ?_)
      rw [bodiesMap]
      exact List.mem_map.mpr ⟨function, List.mem_range.mpr inside, by simp [resultEq]⟩)
      (bodiesSome function (List.mem_range.mpr inside))
  · intro value inside
    exact site_ran claimsEq (fun result resultEq => by
      simp only [List.mem_append]
      refine Or.inr ?_
      rw [valuesMap]
      exact List.mem_map.mpr ⟨value, List.mem_range.mpr inside, by simp [resultEq]⟩)
      (valuesSome value (List.mem_range.mpr inside))

section Main
variable {env : Env} (agrees : Agrees env) (regular : Regular env.schema)
  (lengths : env.doc.functionInstructionStarts.length = env.doc.functions.length)
include agrees regular lengths

theorem ran_payloads {assignment : Assignment} (ran : Ran env assignment) :
    Payloads env.schema env.doc assignment := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro owner ownerMember row rowMember
    rw [owners_eq, List.mem_map] at ownerMember
    obtain ⟨index, indexMember, rfl⟩ := ownerMember
    have inside := List.mem_range.mp indexMember
    rw [← ownerRows_eq lengths inside] at rowMember
    exact (instructionSite_iff agrees regular lengths inside).mp (ran.instructions index inside row rowMember)
  · intro step inside
    exact (stepSite_iff agrees regular lengths inside).mp (ran.steps step inside)
  · intro function inside
    exact (bodySite_iff agrees regular lengths inside).mp (ran.bodies function inside)
  · intro value _
    exact (valueSite_iff agrees regular).mp (ran.values value ‹_›)

theorem payloads_ran {assignment : Assignment} (payloads : Payloads env.schema env.doc assignment) :
    Ran env assignment := by
  obtain ⟨instructions, steps, bodies, values⟩ := payloads
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro index inside row rowMember
    have ownerMember : ownerAt env.doc index ∈ owners env.doc := by
      rw [owners_eq]
      exact List.mem_map_of_mem (List.mem_range.mpr inside)
    rw [ownerRows_eq lengths inside] at rowMember
    exact (instructionSite_iff agrees regular lengths inside).mpr (instructions _ ownerMember row rowMember)
  · intro step inside
    exact (stepSite_iff agrees regular lengths inside).mpr (steps step inside)
  · intro function inside
    exact (bodySite_iff agrees regular lengths inside).mpr (bodies function inside)
  · intro value inside
    exact (valueSite_iff agrees regular).mpr (values value inside)

end Main

theorem resolve_of_tags {schema : Schema} (regular : Regular schema) {doc : Document} (tags : Tags schema doc) :
    ∃ env, Env.resolve? schema doc = some env := by
  obtain ⟨instructions, instructionsEq⟩ := (resolveTags_some_iff regular.instructions doc.tags).mpr tags.1
  obtain ⟨patterns, patternsEq⟩ := (resolveTags_some_iff regular.patterns doc.patterns).mpr tags.2.1
  obtain ⟨stages, stagesEq⟩ := (resolveTags_some_iff regular.stages doc.stages).mpr tags.2.2.1
  obtain ⟨values, valuesEq⟩ := (resolveTags_some_iff regular.values doc.values).mpr tags.2.2.2.1
  obtain ⟨drivers, driversEq⟩ := (resolveTags_some_iff regular.drivers doc.driverStepTags).mpr tags.2.2.2.2
  refine ⟨⟨schema, doc, instructions, patterns, stages, values, drivers⟩, ?_⟩
  unfold Env.resolve?
  rw [instructionsEq, patternsEq, stagesEq, valuesEq, driversEq]
  rfl

theorem tags_of_resolve {schema : Schema} (regular : Regular schema) {doc : Document} {env : Env}
    (resolved : Env.resolve? schema doc = some env) : Tags schema doc := by
  obtain ⟨_, _, instructions, patterns, stages, values, drivers⟩ := resolve_fields resolved
  exact ⟨(resolveTags_some_iff regular.instructions _).mp ⟨_, instructions⟩,
    (resolveTags_some_iff regular.patterns _).mp ⟨_, patterns⟩, (resolveTags_some_iff regular.stages _).mp ⟨_, stages⟩,
    (resolveTags_some_iff regular.values _).mp ⟨_, values⟩, (resolveTags_some_iff regular.drivers _).mp ⟨_, drivers⟩⟩

/-- The structural checker accepts exactly the documents that satisfy the independent specification,
for every regular schema. -/
theorem structuralWF_iff_wellFormed {schema : Schema} (regular : Regular schema) (doc : Document) :
    StructuralWF schema doc ↔ WellFormed schema doc := by
  constructor
  · rintro ⟨shaped, env, resolved, tables, jobs, jobsEq, claims, fitted, claimsWF⟩
    have agrees := resolve_agrees regular resolved
    have tags := tags_of_resolve regular resolved
    obtain ⟨schemaEq, docEq, _⟩ := resolve_fields resolved
    subst schemaEq docEq
    obtain ⟨columns, scalars, transfers, segments, rows, pools⟩ := tables
    have ran := jobs_ran columns.2.2.2.2.2.2.2.1 jobsEq ((mapM_runJob_iff jobs claims).mpr fitted)
    obtain ⟨jobs', claims', jobsEq', claimsEq', allEq⟩ := ran_jobs columns.2.2.2.2.2.2.2.1 ran
    rw [jobsEq] at jobsEq'
    cases jobsEq'
    rw [(mapM_runJob_iff jobs claims).mpr fitted] at claimsEq'
    cases claimsEq'
    exact ⟨shaped, columns, tags, (scalars_iff _ _).mp ⟨scalars, transfers⟩,
      (layout_iff columns.2.2.2.2.2.2.2.1).mp segments, rows, (pools_iff _ _).mp pools,
      ⟨assignmentOf env, ran_payloads agrees regular columns.2.2.2.2.2.2.2.1 ran,
        allEq ▸ (claimed_iff _ _).mp claimsWF⟩⟩
  · rintro ⟨shaped, columns, tags, scalars, layout, rows, pools, assignment, payloads, claimed⟩
    obtain ⟨env, resolved⟩ := resolve_of_tags regular tags
    have agrees := resolve_agrees regular resolved
    obtain ⟨schemaEq, docEq, _⟩ := resolve_fields resolved
    subst schemaEq docEq
    have lengths := columns.2.2.2.2.2.2.2.1
    obtain ⟨scalarsWF, transfers⟩ := (scalars_iff _ _).mpr scalars
    have ran := payloads_ran agrees regular lengths payloads
    obtain ⟨jobs, claims, jobsEq, claimsEq, allEq⟩ := ran_jobs lengths ran
    exact ⟨shaped, env, resolved, ⟨columns, scalarsWF, transfers, (layout_iff lengths).mpr layout, rows,
      (pools_iff _ _).mpr pools⟩, jobs, jobsEq, claims, (mapM_runJob_iff jobs claims).mp claimsEq,
      (claimed_iff _ _).mpr (allEq ▸ claimed)⟩

theorem verify_iff_wellFormed {schema : Schema} (regular : Regular schema) (doc : Document) :
    verify schema doc = true ↔ WellFormed schema doc :=
  (verify_iff schema doc).trans (structuralWF_iff_wellFormed regular doc)

theorem rawSchema_regular : Regular rawSchema where
  instructions := (distinct_iff _).mp (by decide +kernel)
  patterns := (distinct_iff _).mp (by decide +kernel)
  stages := (distinct_iff _).mp (by decide +kernel)
  values := (distinct_iff _).mp (by decide +kernel)
  drivers := (distinct_iff _).mp (by decide +kernel)
  names := (distinct_iff _).mp (by decide +kernel)
  flat := by decide +kernel

/-- The structural checker over the shipped schema accepts exactly the specification's documents. -/
theorem verify_rawSchema_iff (doc : Document) : verify rawSchema doc = true ↔ WellFormed rawSchema doc :=
  verify_iff_wellFormed rawSchema_regular doc

/-- The code list decodes to a document that satisfies the specification over the shipped schema. -/
def AcceptedDocument (codes : Xsh.Conformance.Transport.ExchangeCodes) : Prop :=
  ∃ doc, decodeDocumentCodes codes = some doc ∧ WellFormed rawSchema doc

theorem verifyCodes_iff (codes : Xsh.Conformance.Transport.ExchangeCodes) : verifyCodes codes = true ↔ AcceptedDocument codes := by
  unfold verifyCodes AcceptedDocument
  cases decoded : decodeDocumentCodes codes with
  | none =>
      simp only [Bool.false_eq_true, false_iff]
      rintro ⟨doc, impossible, _⟩
      cases impossible
  | some doc =>
      rw [verify_rawSchema_iff]
      constructor
      · intro accepted
        exact ⟨doc, rfl, accepted⟩
      · rintro ⟨doc', same, accepted⟩
        cases same
        exact accepted

/-- For a regular schema, a payload inside the word table that fits with any fuel fits within the
document's budget: a derivation at some fuel is a specification parse, and every parse fits at the budget. -/
theorem budget_suffices {schema : Schema} (regular : Regular schema) {doc : Document} {env : Env}
    (resolved : Env.resolve? schema doc = some env) {frame : Frame} (patterns : frame.patterns ≤ doc.patterns.length)
    (stages : frame.stages ≤ doc.stages.length) {fuel : Nat} {fields : List Shape} {start length : Nat}
    {claims : List Claim} (within : start + length ≤ doc.extra.length)
    (fitted : entryFit (fitFuel env fuel) (scopeOf doc frame) fields start length = some claims) :
    entryFit (fitFuel env (budget doc + 1)) (scopeOf doc frame) fields start length = some claims := by
  have agrees := resolve_agrees regular resolved
  obtain ⟨schemaEq, docEq, _⟩ := resolve_fields resolved
  subst schemaEq docEq
  have fits := (entryFit_eq_some (fitFuel_iff env fuel)).mp fitted
  unfold EntryFits at fits
  have parsed := entryAt_sound agrees fuel frame _ _ _ _ _ fits within
  rw [(window_nil within).mpr (Nat.le_refl _)] at parsed
  cases parsed with
  | seq all => exact (payload_iff agrees regular (budget_need _ _ patterns stages) within).mpr all

/-- With the shipped schema, the entry budget is enough for every payload that fits at all. -/
theorem rawSchema_budget_suffices {doc : Document} {env : Env} (resolved : Env.resolve? rawSchema doc = some env)
    {frame : Frame} (patterns : frame.patterns ≤ doc.patterns.length) (stages : frame.stages ≤ doc.stages.length)
    {fuel : Nat} {fields : List Shape} {start length : Nat} {claims : List Claim}
    (within : start + length ≤ doc.extra.length)
    (fitted : entryFit (fitFuel env fuel) (scopeOf doc frame) fields start length = some claims) :
    entryFit (fitFuel env (budget doc + 1)) (scopeOf doc frame) fields start length = some claims :=
  budget_suffices rawSchema_regular resolved patterns stages within fitted

end Xsh.IR.Raw
