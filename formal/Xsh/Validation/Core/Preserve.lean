import Xsh.Validation.Core.Paths
import Xsh.Validation.Core.Validated
import Xsh.Proofs.Core.Env

namespace Xsh.Validation.Core.Preserve

open Xsh.Core
open Xsh.Validation.RelPath
open Xsh.Validation.Core.Paths

theorem slash_ne_nul : slashByte ≠ nulByte := by decide

theorem components_ne_nil (bytes : PathBytes) : components bytes ≠ [] := List.splitOn_ne_nil _ _

theorem components_no_slash : ∀ (bytes : PathBytes), ∀ part ∈ components bytes, slashByte ∉ part
  | [] => by simp [components]
  | byte :: rest => by
    intro part present
    simp only [components, List.splitOn_cons_eq_ite_modifyHead] at present
    have ih := components_no_slash rest
    simp only [components] at ih
    by_cases isSlash : byte = slashByte
    · simp only [isSlash, beq_self_eq_true, ite_true, List.mem_cons] at present
      rcases present with same | later
      · simp [same]
      · exact ih part later
    · have different : (byte == slashByte) = false := by simpa using isSlash
      simp only [different, Bool.false_eq_true, ite_false] at present
      obtain ⟨first, others, split⟩ : ∃ first others, rest.splitOn slashByte = first :: others := by
        cases h : rest.splitOn slashByte with
        | nil => exact absurd h (List.splitOn_ne_nil _ _)
        | cons first others => exact ⟨first, others, rfl⟩
      rw [split] at present ih
      simp only [List.modifyHead_cons, List.mem_cons] at present
      rcases present with same | later
      · subst same
        simp only [List.mem_cons, not_or]
        exact ⟨Ne.symm isSlash, ih first (by simp)⟩
      · exact ih part (by simp [later])

theorem mem_join {byte : UInt8} : ∀ {parts : List Component}, byte ∈ join parts →
    byte = slashByte ∨ ∃ part ∈ parts, byte ∈ part
  | [], present => by simp [join] at present
  | [part], present => by simp only [join, List.intercalate_singleton] at present; exact Or.inr ⟨part, by simp, present⟩
  | part :: next :: rest, present => by
    simp only [join, List.intercalate_cons_cons, List.mem_append, List.mem_singleton] at present
    rcases present with (here | slash) | later
    · exact Or.inr ⟨part, by simp, here⟩
    · exact Or.inl slash
    · rcases mem_join (parts := next :: rest) later with slash | ⟨found, foundPresent, inside⟩
      · exact Or.inl slash
      · exact Or.inr ⟨found, List.mem_cons_of_mem _ foundPresent, inside⟩

theorem mem_join_of_mem {byte : UInt8} {part : Component} : ∀ {parts : List Component}, part ∈ parts → byte ∈ part →
    byte ∈ join parts
  | [], present, _ => by simp at present
  | [only], present, inside => by simp only [List.mem_singleton] at present; subst present; simpa [join] using inside
  | first :: next :: rest, present, inside => by
    simp only [join, List.intercalate_cons_cons, List.mem_append]
    simp only [List.mem_cons] at present
    rcases present with same | later
    · subst same
      exact Or.inl (Or.inl inside)
    · exact Or.inr (mem_join_of_mem (parts := next :: rest) (by simpa using later) inside)

theorem component_bytes_in_path {bytes : PathBytes} {part : Component} {byte : UInt8}
    (present : part ∈ components bytes) (inside : byte ∈ part) : byte ∈ bytes := by
  have joined := mem_join_of_mem present inside
  simpa [join, components] using joined

theorem first_component_of_relative {bytes : PathBytes} (relative : isRelPath bytes) :
    ∃ byte first others, components bytes = (byte :: first) :: others := by
  obtain ⟨_, scanned⟩ := relative
  simp only [scanPath] at scanned
  cases bytes with
  | nil => simp at scanned
  | cons byte rest =>
    have notSlash : byte ≠ slashByte := by
      intro same
      subst same
      simp at scanned
    have different : (byte == slashByte) = false := by simpa using notSlash
    obtain ⟨first, others, split⟩ : ∃ first others, rest.splitOn slashByte = first :: others := by
      cases h : rest.splitOn slashByte with
      | nil => exact absurd h (List.splitOn_ne_nil _ _)
      | cons first others => exact ⟨first, others, rfl⟩
    refine ⟨byte, first, others, ?_⟩
    simp [components, List.splitOn_cons_eq_ite_modifyHead, different, split]

theorem scan_append (first second : List Component) (depth : Nat) :
    scanComponents (first ++ second) depth = (scanComponents first depth).bind (scanComponents second) := by
  induction first generalizing depth with
  | nil => simp [scanComponents]
  | cons part rest ih =>
    simp only [List.cons_append, scanComponents]
    cases advanceComponent depth part <;> simp [ih]

theorem scan_prefix {first second : List Component} {depth : Nat}
    (scanned : scanComponents (first ++ second) depth ≠ none) : scanComponents first depth ≠ none := by
  rw [scan_append] at scanned
  intro missing
  rw [missing] at scanned
  exact scanned rfl

theorem advance_shift {depth result : Nat} {part : Component} (advanced : advanceComponent depth part = some result)
    (extra : Nat) : advanceComponent (depth + extra) part = some (result + extra) := by
  unfold advanceComponent at advanced ⊢
  by_cases trivialPart : (decide (part = []) || decide (part = [dotByte])) = true
  · rw [if_pos trivialPart] at advanced ⊢
    simp only [Option.some.injEq] at advanced ⊢
    omega
  · rw [if_neg trivialPart] at advanced ⊢
    by_cases parentPart : part = [dotByte, dotByte]
    · rw [if_pos parentPart] at advanced ⊢
      by_cases zero : depth = 0
      · rw [if_pos zero] at advanced
        cases advanced
      · rw [if_neg zero] at advanced
        have shifted : depth + extra ≠ 0 := by omega
        rw [if_neg shifted]
        simp only [Option.some.injEq] at advanced ⊢
        omega
    · rw [if_neg parentPart] at advanced ⊢
      simp only [Option.some.injEq] at advanced ⊢
      omega

theorem scan_shift : ∀ {parts : List Component} {depth result : Nat},
    scanComponents parts depth = some result → ∀ extra, scanComponents parts (depth + extra) = some (result + extra)
  | [], depth, result, scanned, extra => by
    simp only [scanComponents, Option.some.injEq] at scanned
    subst scanned
    rfl
  | part :: rest, depth, result, scanned, extra => by
    simp only [scanComponents] at scanned ⊢
    cases advanced : advanceComponent depth part with
    | none => simp [advanced] at scanned
    | some next =>
      simp only [advanced] at scanned
      rw [advance_shift advanced extra]
      exact scan_shift scanned extra

def Named (part : Component) : Prop := isTrivial part = false ∧ part ≠ [dotByte, dotByte]

theorem scan_names : ∀ {parts : List Component}, (∀ part ∈ parts, Named part) →
    ∀ depth, scanComponents parts depth = some (depth + parts.length)
  | [], _, depth => rfl
  | part :: rest, named, depth => by
    obtain ⟨notTrivial, notParent⟩ := named part (by simp)
    have notEmpty : part ≠ [] := by intro same; subst same; simp [isTrivial] at notTrivial
    have notDot : part ≠ [dotByte] := by intro same; subst same; simp [isTrivial] at notTrivial
    simp only [scanComponents, ordinary_component_increases_depth depth part notEmpty notDot notParent]
    rw [scan_names (fun other present => named other (List.mem_cons_of_mem _ present)) (depth + 1)]
    simp only [List.length_cons, Option.some.injEq]
    omega

theorem relative_join {parts : List Component} {byte : UInt8} {first : Component} {others : List Component}
    (shape : parts = (byte :: first) :: others) (noSlash : ∀ part ∈ parts, slashByte ∉ part)
    (noNul : ∀ part ∈ parts, nulByte ∉ part) (scanned : scanComponents parts 0 ≠ none) : isRelPath (join parts) := by
  have notEmpty : parts ≠ [] := by rw [shape]; simp
  refine ⟨?_, ?_⟩
  · intro present
    rcases mem_join present with isSlash | ⟨part, partPresent, inside⟩
    · exact slash_ne_nul isSlash.symm
    · exact noNul part partPresent inside
  · have headNotSlash : byte ≠ slashByte := by
      intro same
      exact noSlash (byte :: first) (by rw [shape]; simp) (by simp [same])
    have joined : join parts = byte :: join (first :: others) := by
      rw [shape]
      simp [join]
    simp only [scanPath, joined, List.isEmpty_cons, List.head?_cons, Option.some.injEq, headNotSlash,
      Bool.false_or, decide_false, Bool.false_eq_true, ite_false]
    rw [← joined]
    simp only [join]
    rw [List.splitOn_intercalate slashByte noSlash notEmpty]
    exact scanned

theorem scan_read : ∀ (parts : List Component) (depth : Nat),
    scanComponents (readComponents parts) depth = scanComponents parts depth
  | [], _ => rfl
  | part :: rest, depth => by
    by_cases trivialPart : isTrivial part = true
    · have advanced : advanceComponent depth part = some depth := by
        unfold advanceComponent
        rw [if_pos (by simpa [isTrivial] using trivialPart)]
      simp only [readComponents, List.filter_cons, trivialPart, Bool.not_true, Bool.false_eq_true, ite_false]
      simp only [scanComponents, advanced]
      exact scan_read rest depth
    · simp only [readComponents, List.filter_cons, trivialPart, Bool.not_false, ite_true, scanComponents]
      cases advanceComponent depth part with
      | none => rfl
      | some next => exact scan_read rest next

theorem parent_parts (parts : List Component) :
    ∃ suffix, parentComponents parts ++ suffix = readComponents parts := by
  unfold parentComponents
  cases read : readComponents parts with
  | nil => exact ⟨[], rfl⟩
  | cons first rest => exact ⟨[(first :: rest).getLast (by simp)], List.dropLast_concat_getLast _⟩

theorem parent_member {parts : List Component} {part : Component} (present : part ∈ parentComponents parts) :
    part ∈ parts ∧ isTrivial part = false := by
  obtain ⟨suffix, split⟩ := parent_parts parts
  have inRead : part ∈ readComponents parts := split ▸ List.mem_append_left _ present
  simp only [readComponents, List.mem_filter, Bool.not_eq_true'] at inRead
  exact inRead

theorem parent_relative {bytes : PathBytes} (relative : isRelPath bytes) : isRelPath (parent bytes) := by
  unfold parent
  cases kept : parentComponents (components bytes) with
  | nil => exact (by decide : isRelPath [dotByte])
  | cons head rest =>
    show isRelPath (join (head :: rest))
    have member : ∀ part ∈ head :: rest, part ∈ components bytes ∧ isTrivial part = false := fun part present =>
      parent_member (kept ▸ present)
    obtain ⟨byte, first, shape⟩ : ∃ byte first, head = byte :: first := by
      cases head with
      | nil => have := (member [] (by simp)).2; simp [isTrivial] at this
      | cons byte first => exact ⟨byte, first, rfl⟩
    subst shape
    refine relative_join rfl (fun part present => components_no_slash bytes part (member part present).1)
      (fun part present inside => relative.1 (component_bytes_in_path (member part present).1 inside)) ?_
    have scanned := relative.2
    simp only [scanPath] at scanned
    split at scanned
    · exact absurd rfl scanned
    · change scanComponents (components bytes) 0 ≠ none at scanned
      rw [← scan_read] at scanned
      obtain ⟨suffix, split⟩ := parent_parts (components bytes)
      rw [kept] at split
      rw [← split] at scanned
      exact scan_prefix scanned

theorem normalizeStack_named : ∀ {stack rest : List Component}, (∀ part ∈ stack, Named part) →
    ∀ part ∈ normalizeStack stack rest, Named part ∧ (part ∈ stack ∨ part ∈ rest)
  | stack, [], named => fun part present => ⟨named part present, Or.inl present⟩
  | stack, component :: rest, named => by
    intro part present
    simp only [normalizeStack] at present
    split at present
    · obtain ⟨partNamed, origin⟩ := normalizeStack_named named part present
      exact ⟨partNamed, by rcases origin with here | later <;> simp_all⟩
    · split at present
      · have tailNamed : ∀ other ∈ stack.tail, Named other := fun other inside =>
          named other (List.mem_of_mem_tail inside)
        obtain ⟨partNamed, origin⟩ := normalizeStack_named tailNamed part present
        exact ⟨partNamed, by rcases origin with here | later
                             · exact Or.inl (List.mem_of_mem_tail here)
                             · simp [later]⟩
      · rename_i trivial parentPart
        have componentNamed : Named component := ⟨by simpa using trivial, parentPart⟩
        have consNamed : ∀ other ∈ component :: stack, Named other := fun other inside => by
          simp only [List.mem_cons] at inside
          rcases inside with same | later
          · exact same ▸ componentNamed
          · exact named other later
        obtain ⟨partNamed, origin⟩ := normalizeStack_named consNamed part present
        exact ⟨partNamed, by
          simp only [List.mem_cons] at origin ⊢
          rcases origin with (same | later) | later
          · exact Or.inr (Or.inl same)
          · exact Or.inl later
          · exact Or.inr (Or.inr later)⟩

theorem normalize_relative {bytes : PathBytes} (noNul : isPathBytes bytes) : isRelPath (normalize bytes) := by
  unfold normalize
  have origin := normalizeStack_named (stack := []) (rest := components bytes) (by simp)
  cases parts : (normalizeStack [] (components bytes)).reverse with
  | nil => exact (by decide : isRelPath [dotByte])
  | cons head rest =>
    show isRelPath (join (head :: rest))
    have member : ∀ part ∈ head :: rest, Named part ∧ part ∈ components bytes := by
      intro part present
      rw [← parts, List.mem_reverse] at present
      obtain ⟨named, source⟩ := origin part present
      simpa using And.intro named source
    have headNamed := (member head (by simp)).1
    obtain ⟨byte, first, headShape⟩ : ∃ byte first, head = byte :: first := by
      cases head with
      | nil => simp [Named, isTrivial] at headNamed
      | cons byte first => exact ⟨byte, first, rfl⟩
    subst headShape
    refine relative_join rfl (fun part present => components_no_slash bytes part (member part present).2)
      (fun part present inside => noNul (component_bytes_in_path (member part present).2 inside)) ?_
    rw [scan_names (fun part present => (member part present).1) 0]
    simp

theorem parent_no_nul {bytes : PathBytes} (noNul : isPathBytes bytes) : isPathBytes (parent bytes) := by
  unfold parent
  cases kept : parentComponents (components bytes) with
  | nil => exact (by unfold isPathBytes; decide : isPathBytes [dotByte])
  | cons head rest =>
    show nulByte ∉ join (head :: rest)
    intro present
    rcases mem_join present with isSlash | ⟨part, partPresent, inside⟩
    · exact slash_ne_nul isSlash.symm
    · exact noNul (component_bytes_in_path (parent_member (kept ▸ partPresent)).1 inside)

theorem normalize_no_nul {bytes : PathBytes} (noNul : isPathBytes bytes) : isPathBytes (normalize bytes) :=
  (normalize_relative noNul).1

theorem native_no_nul (path : Xsh.Values.NativePath) : isPathBytes path.bytes := by
  intro present
  exact path.noNul nulByte present rfl

theorem no_nul_native {bytes : PathBytes} (noNul : isPathBytes bytes) : ∀ byte, byte ∈ bytes → byte ≠ 0 := by
  intro byte present same
  subst same
  exact noNul present

def parentPath (path : Xsh.Values.NativePath) : Xsh.Values.NativePath :=
  ⟨parent path.bytes, no_nul_native (parent_no_nul (native_no_nul path))⟩

def normalizePath (path : Xsh.Values.NativePath) : Xsh.Values.NativePath :=
  ⟨normalize path.bytes, no_nul_native (normalize_no_nul (native_no_nul path))⟩

variable {Declaration Identity Source : Type}

def listItems (candidate : Value Declaration Identity Source) : Option (List (Value Declaration Identity Source)) :=
  match candidate.eraseNominal with
  | .list items => some items
  | .environmentPaths paths => some (paths.map fun path => .scalar (.path path))
  | _ => none

def push (candidate item : Value Declaration Identity Source) : Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list (items ++ [item])

def concatenate (left right : Value Declaration Identity Source) : Option (Value Declaration Identity Source) :=
  match listItems left, listItems right with
  | some first, some second => some (.list (first ++ second))
  | _, _ => none

def comprehend (candidate : Value Declaration Identity Source)
    (body : Value Declaration Identity Source → Value Declaration Identity Source) :
    Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list (items.map body)

def filterItems (candidate : Value Declaration Identity Source) (keep : Value Declaration Identity Source → Bool) :
    Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list (items.filter keep)

def replaceAt (candidate : Value Declaration Identity Source) (index : Nat) (item : Value Declaration Identity Source) :
    Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list (items.set index item)

def sliceItems (candidate : Value Declaration Identity Source) (start stop : Nat) :
    Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list ((items.drop start).take (stop - start))

def comprehendMany (candidate : Value Declaration Identity Source)
    (body : Value Declaration Identity Source → List (Value Declaration Identity Source)) :
    Option (Value Declaration Identity Source) :=
  (listItems candidate).map fun items => .list (items.flatMap body)

def pathOf (candidate : Value Declaration Identity Source) : Option Xsh.Values.NativePath :=
  match candidate.eraseNominal with
  | .scalar (.path path) => some path
  | _ => none

def pathParent (candidate : Value Declaration Identity Source) : Option (Value Declaration Identity Source) :=
  (pathOf candidate).map fun path => .scalar (.path (parentPath path))

def pathNormalize (candidate : Value Declaration Identity Source) : Option (Value Declaration Identity Source) :=
  (pathOf candidate).map fun path => .scalar (.path (normalizePath path))

theorem list_member_iff (world : MembershipWorld Declaration Identity Source) (element : ValueType Declaration)
    (candidate : Value Declaration Identity Source) :
    Membership.value world (.list element) candidate ↔
      ∃ items, listItems candidate = some items ∧ ∀ item ∈ items, Membership.value world element item := by
  simp only [Membership.value, listItems]
  cases candidate.eraseNominal <;> simp

theorem nonEmpty_member_iff (world : MembershipWorld Declaration Identity Source) (element : ValueType Declaration)
    (candidate : Value Declaration Identity Source) :
    Membership.value world (.nonEmpty element) candidate ↔
      ∃ items, listItems candidate = some items ∧ items ≠ [] ∧ ∀ item ∈ items, Membership.value world element item := by
  simp only [Membership.value, listItems]
  cases candidate.eraseNominal <;> simp

theorem relative_member_iff (world : MembershipWorld Declaration Identity Source)
    (candidate : Value Declaration Identity Source) :
    Membership.value world .relativePath candidate ↔ ∃ path, pathOf candidate = some path ∧ isRelPath path.bytes := by
  simp only [Membership.value, pathOf]
  cases candidate.eraseNominal with
  | scalar scalar => cases scalar <;> simp
  | _ => simp

theorem list_value_items (items : List (Value Declaration Identity Source)) :
    listItems (.list items) = some items := rfl

/-- The preserving operations of every Core validated type. On a `NonEmpty`
receiver, pushing an element, extending with a list, concatenating with a
`NonEmpty` on either side, a single-clause comprehension without a filter, and
replacing an element keep the value a `NonEmpty`; filtering gives only the
base list. On a `RelPath`, `.parent()` and `.normalize()` give a `RelPath`.
Every case holds for each element type, world and receiver representation,
including environment path lists and values that carry nominal wrappers. -/
theorem preserving_operations (world : MembershipWorld Declaration Identity Source) :
    (∀ (element : ValueType Declaration) receiver item result,
      Membership.value world (.nonEmpty element) receiver → Membership.value world element item →
        push receiver item = some result → Membership.value world (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) left right result,
      Membership.value world (.nonEmpty element) left → Membership.value world (.list element) right →
        concatenate left right = some result → Membership.value world (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) left right result,
      Membership.value world (.list element) left → Membership.value world (.nonEmpty element) right →
        concatenate left right = some result → Membership.value world (.nonEmpty element) result) ∧
    (∀ (element produced : ValueType Declaration) receiver body result,
      Membership.value world (.nonEmpty element) receiver →
        (∀ item, Membership.value world element item → Membership.value world produced (body item)) →
        comprehend receiver body = some result → Membership.value world (.nonEmpty produced) result) ∧
    (∀ (element : ValueType Declaration) receiver index item result,
      Membership.value world (.nonEmpty element) receiver → Membership.value world element item →
        replaceAt receiver index item = some result → Membership.value world (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) receiver keep result,
      Membership.value world (.list element) receiver →
        filterItems receiver keep = some result → Membership.value world (.list element) result) ∧
    (∀ receiver result, Membership.value world .relativePath receiver →
      pathParent receiver = some result → Membership.value world .relativePath result) ∧
    (∀ receiver result, Membership.value world .relativePath receiver →
      pathNormalize receiver = some result → Membership.value world .relativePath result) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · intro element receiver item result receiverMember itemMember pushed
    obtain ⟨items, found, _, members⟩ := (nonEmpty_member_iff world element receiver).mp receiverMember
    simp only [push, found, Option.map_some, Option.some.injEq] at pushed
    subst pushed
    refine (nonEmpty_member_iff world element _).mpr ⟨_, list_value_items _, by simp, ?_⟩
    intro other present
    simp only [List.mem_append, List.mem_singleton] at present
    rcases present with early | last
    · exact members other early
    · exact last ▸ itemMember
  · intro element left right result leftMember rightMember joined
    obtain ⟨first, firstFound, firstNonempty, firstMembers⟩ := (nonEmpty_member_iff world element left).mp leftMember
    obtain ⟨second, secondFound, secondMembers⟩ := (list_member_iff world element right).mp rightMember
    simp only [concatenate, firstFound, secondFound, Option.some.injEq] at joined
    subst joined
    refine (nonEmpty_member_iff world element _).mpr ⟨_, list_value_items _, by simp [firstNonempty], ?_⟩
    intro other present
    simp only [List.mem_append] at present
    rcases present with early | late
    · exact firstMembers other early
    · exact secondMembers other late
  · intro element left right result leftMember rightMember joined
    obtain ⟨first, firstFound, firstMembers⟩ := (list_member_iff world element left).mp leftMember
    obtain ⟨second, secondFound, secondNonempty, secondMembers⟩ := (nonEmpty_member_iff world element right).mp rightMember
    simp only [concatenate, firstFound, secondFound, Option.some.injEq] at joined
    subst joined
    refine (nonEmpty_member_iff world element _).mpr ⟨_, list_value_items _, by simp [secondNonempty], ?_⟩
    intro other present
    simp only [List.mem_append] at present
    rcases present with early | late
    · exact firstMembers other early
    · exact secondMembers other late
  · intro element produced receiver body result receiverMember bodyTyped comprehended
    obtain ⟨items, found, nonempty, members⟩ := (nonEmpty_member_iff world element receiver).mp receiverMember
    simp only [comprehend, found, Option.map_some, Option.some.injEq] at comprehended
    subst comprehended
    refine (nonEmpty_member_iff world produced _).mpr ⟨_, list_value_items _, by simpa using nonempty, ?_⟩
    intro other present
    obtain ⟨source, sourcePresent, same⟩ := List.mem_map.mp present
    exact same ▸ bodyTyped source (members source sourcePresent)
  · intro element receiver index item result receiverMember itemMember replaced
    obtain ⟨items, found, nonempty, members⟩ := (nonEmpty_member_iff world element receiver).mp receiverMember
    simp only [replaceAt, found, Option.map_some, Option.some.injEq] at replaced
    subst replaced
    refine (nonEmpty_member_iff world element _).mpr ⟨_, list_value_items _, ?_, ?_⟩
    · intro empty
      have : (items.set index item).length = 0 := by rw [empty]; rfl
      simp only [List.length_set] at this
      exact nonempty (List.eq_nil_of_length_eq_zero this)
    · intro other present
      rcases List.mem_or_eq_of_mem_set present with kept | same
      · exact members other kept
      · exact same ▸ itemMember
  · intro element receiver keep result receiverMember filtered
    obtain ⟨items, found, members⟩ := (list_member_iff world element receiver).mp receiverMember
    simp only [filterItems, found, Option.map_some, Option.some.injEq] at filtered
    subst filtered
    exact (list_member_iff world element _).mpr ⟨_, list_value_items _,
      fun other present => members other (List.mem_filter.mp present).1⟩
  · intro receiver result receiverMember parented
    obtain ⟨path, found, relative⟩ := (relative_member_iff world receiver).mp receiverMember
    simp only [pathParent, found, Option.map_some, Option.some.injEq] at parented
    subst parented
    exact (relative_member_iff world _).mpr ⟨parentPath path, rfl, parent_relative relative⟩
  · intro receiver result receiverMember normalized
    obtain ⟨path, found, relative⟩ := (relative_member_iff world receiver).mp receiverMember
    simp only [pathNormalize, found, Option.map_some, Option.some.injEq] at normalized
    subst normalized
    exact (relative_member_iff world _).mpr ⟨normalizePath path, rfl, normalize_relative relative.1⟩


theorem base_results (world : MembershipWorld Declaration Identity Source) :
    (∀ (element : ValueType Declaration) receiver start stop result,
      Membership.value world (.list element) receiver → sliceItems receiver start stop = some result →
        Membership.value world (.list element) result) ∧
    (∀ (element produced : ValueType Declaration) receiver body result,
      Membership.value world (.list element) receiver →
        (∀ item, Membership.value world element item → ∀ output ∈ body item, Membership.value world produced output) →
        comprehendMany receiver body = some result → Membership.value world (.list produced) result) := by
  refine ⟨?_, ?_⟩
  · intro element receiver start stop result receiverMember sliced
    obtain ⟨items, found, members⟩ := (list_member_iff world element receiver).mp receiverMember
    simp only [sliceItems, found, Option.map_some, Option.some.injEq] at sliced
    subst sliced
    exact (list_member_iff world element _).mpr ⟨_, list_value_items _,
      fun other present => members other (List.mem_of_mem_drop (List.mem_of_mem_take present))⟩
  · intro element produced receiver body result receiverMember bodyTyped comprehended
    obtain ⟨items, found, members⟩ := (list_member_iff world element receiver).mp receiverMember
    simp only [comprehendMany, found, Option.map_some, Option.some.injEq] at comprehended
    subst comprehended
    refine (list_member_iff world produced _).mpr ⟨_, list_value_items _, fun other present => ?_⟩
    obtain ⟨source, sourcePresent, inside⟩ := List.mem_flatMap.mp present
    exact bodyTyped source (members source sourcePresent) other inside

theorem referenceValues_append (first second : List (Value Declaration Identity Source)) :
    referenceValues (first ++ second) = referenceValues first ++ referenceValues second := by
  induction first with
  | nil => simp [referenceValues]
  | cons head rest ih => simp [referenceValues, ih]

theorem listItems_references {candidate : Value Declaration Identity Source}
    {items : List (Value Declaration Identity Source)} (found : listItems candidate = some items) :
    ∀ reference ∈ referenceValues items, reference ∈ candidate.references := by
  intro reference present
  rw [← Value.references_eraseNominal]
  simp only [listItems] at found
  cases erased : candidate.eraseNominal <;> simp only [erased, reduceCtorEq, Option.some.injEq] at found
  case list values =>
    subst found
    simpa [Value.references] using present
  case environmentPaths paths =>
    subst found
    have empty : ∀ paths : List Xsh.Values.NativePath,
        referenceValues (paths.map fun path => (.scalar (.path path) : Value Declaration Identity Source)) = [] := by
      intro paths
      induction paths with
      | nil => simp [referenceValues]
      | cons head rest ih => simp [referenceValues, Value.references, ih]
    rw [empty] at present
    cases present

theorem extend_commits (world : Xsh.Semantics.CoreWorld.World Declaration Identity Source) [DecidableEq Declaration]
    (decisions : Xsh.Semantics.CoreWorld.Decisions world) (store : Xsh.Semantics.CoreWorld.Store world)
    (cell : Xsh.Semantics.Environments.CellId) (current : Xsh.Semantics.CoreWorld.Cell world)
    (element : ValueType Declaration) (other result : Value Declaration Identity Source)
    (present : store cell = some current) (mutableCell : current.mutability = .mutable)
    (validated : current.contents.typeTag = .nonEmpty element)
    (otherMember : Membership.value world.membership (.list element) other)
    (otherReferences : other.ReferencesValid world.references)
    (extended : concatenate current.contents.value other = some result) :
    ∃ next, Xsh.Semantics.CoreWorld.assign world decisions store cell result = .committed next ∧
      ∃ replaced, next cell = some replaced ∧ replaced.contents.value = result ∧
        Membership.value world.membership (.nonEmpty element) replaced.contents.value := by
  have currentMember : Membership.value world.membership (.nonEmpty element) current.contents.value :=
    validated ▸ current.contents.wellTyped.2
  have resultMember := (preserving_operations world.membership).2.1 element _ other result currentMember
    otherMember extended
  have resultReferences : result.ReferencesValid world.references := by
    obtain ⟨first, firstFound, _⟩ := (nonEmpty_member_iff _ _ _).mp currentMember
    obtain ⟨second, secondFound, _⟩ := (list_member_iff _ _ _).mp otherMember
    simp only [concatenate, firstFound, secondFound, Option.some.injEq] at extended
    subst extended
    intro reference inside
    simp only [Value.references, referenceValues_append, List.mem_append] at inside
    rcases inside with early | late
    · exact current.contents.wellTyped.1 reference (listItems_references firstFound reference early)
    · exact otherReferences reference (listItems_references secondFound reference late)
  obtain ⟨next, committed⟩ := (Xsh.Proofs.Core.Env.assign_committed_iff world decisions store cell result).mpr
    ⟨current, present, mutableCell, resultReferences, validated ▸ resultMember⟩
  obtain ⟨_, found, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
    Xsh.Proofs.Core.Env.assign_committed_effect world decisions store next cell result committed
  exact ⟨next, committed, replaced, replacedAt, replacedValue, replacedValue ▸ resultMember⟩

/-- Validated-type operations over every Core world, type and value. The
preserving operations keep `NonEmpty` and `RelPath`, `.parent()` reading the
path by its non-trivial components; filtering, slicing and comprehensions with
several clauses return only the base list type; and `.extend` or `+=` on a
`NonEmpty` variable is a store write that commits and keeps the variable a
`NonEmpty`. -/
theorem validated_operations (world : Xsh.Semantics.CoreWorld.World Declaration Identity Source)
    [DecidableEq Declaration] (decisions : Xsh.Semantics.CoreWorld.Decisions world) :
    ((∀ (element : ValueType Declaration) receiver item result,
      Membership.value world.membership (.nonEmpty element) receiver →
        Membership.value world.membership element item →
        push receiver item = some result → Membership.value world.membership (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) left right result,
      Membership.value world.membership (.nonEmpty element) left →
        Membership.value world.membership (.list element) right →
        concatenate left right = some result → Membership.value world.membership (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) left right result,
      Membership.value world.membership (.list element) left →
        Membership.value world.membership (.nonEmpty element) right →
        concatenate left right = some result → Membership.value world.membership (.nonEmpty element) result) ∧
    (∀ (element produced : ValueType Declaration) receiver body result,
      Membership.value world.membership (.nonEmpty element) receiver →
        (∀ item, Membership.value world.membership element item →
          Membership.value world.membership produced (body item)) →
        comprehend receiver body = some result → Membership.value world.membership (.nonEmpty produced) result) ∧
    (∀ (element : ValueType Declaration) receiver index item result,
      Membership.value world.membership (.nonEmpty element) receiver →
        Membership.value world.membership element item →
        replaceAt receiver index item = some result → Membership.value world.membership (.nonEmpty element) result) ∧
    (∀ (element : ValueType Declaration) receiver keep result,
      Membership.value world.membership (.list element) receiver →
        filterItems receiver keep = some result → Membership.value world.membership (.list element) result) ∧
    (∀ receiver result, Membership.value world.membership .relativePath receiver →
      pathParent receiver = some result → Membership.value world.membership .relativePath result) ∧
    (∀ receiver result, Membership.value world.membership .relativePath receiver →
      pathNormalize receiver = some result → Membership.value world.membership .relativePath result)) ∧
    ((∀ (element : ValueType Declaration) receiver start stop result,
      Membership.value world.membership (.list element) receiver → sliceItems receiver start stop = some result →
        Membership.value world.membership (.list element) result) ∧
    (∀ (element produced : ValueType Declaration) receiver body result,
      Membership.value world.membership (.list element) receiver →
        (∀ item, Membership.value world.membership element item →
          ∀ output ∈ body item, Membership.value world.membership produced output) →
        comprehendMany receiver body = some result → Membership.value world.membership (.list produced) result)) ∧
    (∀ store cell (current : Xsh.Semantics.CoreWorld.Cell world) (element : ValueType Declaration) other result,
      store cell = some current → current.mutability = .mutable → current.contents.typeTag = .nonEmpty element →
      Membership.value world.membership (.list element) other → other.ReferencesValid world.references →
      concatenate current.contents.value other = some result →
        ∃ next, Xsh.Semantics.CoreWorld.assign world decisions store cell result = .committed next ∧
          ∃ replaced, next cell = some replaced ∧ replaced.contents.value = result ∧
            Membership.value world.membership (.nonEmpty element) replaced.contents.value) :=
  ⟨preserving_operations world.membership, base_results world.membership,
    fun store cell current element other result present mutableCell validated otherMember otherReferences extended =>
      extend_commits world decisions store cell current element other result present mutableCell validated otherMember
        otherReferences extended⟩

namespace Witness

def words : Value Unit Unit Unit := .list [.scalar (.string "a")]

def openWorld : MembershipWorld Unit Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => True
  callable := fun _ _ => True
  scheme := fun _ _ => False
  generic := fun _ _ => False

theorem pushed_list_stays_nonempty_and_filtered_list_does_not :
    push words (.scalar (.string "b")) = some (.list [.scalar (.string "a"), .scalar (.string "b")]) ∧
      Membership.value openWorld (.nonEmpty (.primitive .string))
        (.list [.scalar (.string "a"), .scalar (.string "b")] : Value Unit Unit Unit) ∧
      filterItems words (fun _ => false) = some (.list []) ∧
      Membership.value openWorld (.list (.primitive .string)) (.list [] : Value Unit Unit Unit) ∧
      ¬ Membership.value openWorld (.nonEmpty (.primitive .string)) (.list [] : Value Unit Unit Unit) := by
  refine ⟨rfl, ?_, rfl, ?_, ?_⟩ <;>
    simp [Membership.value, Membership.primitive, Value.eraseNominal]

theorem relative_path_operations :
    parent (asciiBytes [97, 47, 98]) = asciiBytes [97] ∧
      parent (asciiBytes [97]) = asciiBytes [46] ∧
      parent (asciiBytes [97, 47, 47, 98]) = asciiBytes [97] ∧
      parent (asciiBytes [97, 47, 46, 47, 98]) = asciiBytes [97] ∧
      parent (asciiBytes [97, 47, 98, 47]) = asciiBytes [97] ∧
      parent (asciiBytes [97, 47, 46, 46]) = asciiBytes [97] ∧
      normalize (asciiBytes [97, 47, 46, 46, 47, 98]) = asciiBytes [98] ∧
      normalize (asciiBytes [97, 47, 46, 46]) = asciiBytes [46] ∧
      normalize (asciiBytes [97, 47, 46, 47, 47, 98, 47]) = asciiBytes [97, 47, 98] := by
  decide

def relativeTo (target origin : PathBytes) : PathBytes :=
  join (List.replicate (readComponents (components origin)).length [dotByte, dotByte] ++
    readComponents (components target))

def withExt (path extension : PathBytes) : PathBytes := path ++ dotByte :: extension

def port : Xsh.Validation.BoundedSpec :=
  ⟨.int, 1, 255, by decide, by unfold Xsh.Validation.IntegerBase.accepts; decide,
    by unfold Xsh.Validation.IntegerBase.accepts; decide⟩

theorem base_operations_do_not_preserve :
    sliceItems words 0 0 = some (.list []) ∧
      comprehendMany words (fun _ => []) = some (.list []) ∧
      ¬ Membership.value openWorld (.nonEmpty (.primitive .string)) (.list [] : Value Unit Unit Unit) ∧
      isRelPath (asciiBytes [97]) ∧ ¬ isRelPath (relativeTo (asciiBytes [97]) (asciiBytes [98])) ∧
      ¬ isRelPath (withExt (asciiBytes [97]) (asciiBytes [47, 46, 46, 47, 46, 46])) ∧
      (components (asciiBytes [97, 47, 46, 46])).contains (asciiBytes [46, 46]) ∧
      ¬ isRelPath (asciiBytes [46, 46]) ∧
      port.accepts 255 ∧ ¬ port.accepts (255 + 1) := by
  refine ⟨rfl, rfl, by simp [Membership.value, Value.eraseNominal], ?_⟩
  decide

def filterResultMutant (element : ValueType Unit) : ValueType Unit := .nonEmpty element

theorem refinement_retained_after_filtering_is_killed :
    Membership.value openWorld (.nonEmpty (.primitive .string)) words ∧
      filterItems words (fun _ => false) = some (.list []) ∧
      ¬ Membership.value openWorld (filterResultMutant (.primitive .string)) (.list [] : Value Unit Unit Unit) := by
  refine ⟨?_, rfl, ?_⟩ <;> simp [words, filterResultMutant, Membership.value, Membership.primitive, Value.eraseNominal]

def dropFirstComponent (bytes : PathBytes) : PathBytes := join (components bytes).tail

theorem suffix_instead_of_parent_is_detected :
    isRelPath (asciiBytes [97, 47, 46, 46, 47, 98]) ∧
      ¬ isRelPath (dropFirstComponent (asciiBytes [97, 47, 46, 46, 47, 98])) ∧
      isRelPath (parent (asciiBytes [97, 47, 46, 46, 47, 98])) := by
  decide

end Witness

end Xsh.Validation.Core.Preserve
