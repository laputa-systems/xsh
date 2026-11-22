import Xsh.Proofs.Core.Hoisting
import Xsh.Proofs.Core.Resolution

namespace Xsh.Proofs.Core.Patterns

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

variable {Declaration Identity Source : Type}

section Names

/-- The names a text pattern's holes bind, in order. -/
def textNames : List Xsh.Syntax.TextPatternPart → List Xsh.Syntax.RawName
  | [] => []
  | .text _ :: rest => textNames rest
  | .hole name _ :: rest => name.toList ++ textNames rest

mutual
  /-- The names a pattern binds, read from its syntax (SPEC 6.8); an
  alternation's are its first alternative's. -/
  def patternNames : Xsh.Syntax.Pattern → List Xsh.Syntax.RawName
    | .wildcard => []
    | .binding name => [name]
    | .group inner => patternNames inner
    | .alias inner name => patternNames inner ++ [name]
    | .alternation members => alternativeNames members
    | .constructor _ none => []
    | .constructor _ (some inner) => patternNames inner
    | .errorVariant _ _ fields => fieldPatternNames fields
    | .facet _ => []
    | .text parts => textNames parts
    | .testName name _ => [name]
    | .typeBind name _ => name.toList
    | .literal _ => []
    | .record fields _ => fieldPatternNames fields
    | .list elements none => itemNames elements
    | .list elements (some tail) => itemNames elements ++ patternNames tail
    | .tuple elements => itemNames elements
    | .textHole _ _ => []

  def fieldPatternNames : List (Xsh.Syntax.RawName × Xsh.Syntax.Pattern) → List Xsh.Syntax.RawName
    | [] => []
    | (_, pattern) :: rest => patternNames pattern ++ fieldPatternNames rest

  def itemNames : List Xsh.Syntax.Pattern → List Xsh.Syntax.RawName
    | [] => []
    | pattern :: rest => patternNames pattern ++ itemNames rest

  def alternativeNames : List Xsh.Syntax.Pattern → List Xsh.Syntax.RawName
    | [] => []
    | member :: _ => patternNames member
end

mutual
  /-- Every alternation inside a pattern binds the same names in each
  alternative (SPEC 6.8: "Alternatives must bind the same names with the same
  types"), up to order. -/
  def Agrees : Xsh.Syntax.Pattern → Prop
    | .group inner => Agrees inner
    | .alias inner _ => Agrees inner
    | .alternation members => MembersAgree (alternativeNames members) members
    | .constructor _ (some inner) => Agrees inner
    | .errorVariant _ _ fields => FieldsAgree fields
    | .record fields _ => FieldsAgree fields
    | .list elements none => ItemsAgree elements
    | .list elements (some tail) => ItemsAgree elements ∧ Agrees tail
    | .tuple elements => ItemsAgree elements
    | .wildcard | .binding _ | .constructor _ none | .facet _ | .text _ | .testName _ _ | .typeBind _ _ | .literal _
      | .textHole _ _ => True

  def FieldsAgree : List (Xsh.Syntax.RawName × Xsh.Syntax.Pattern) → Prop
    | [] => True
    | (_, pattern) :: rest => Agrees pattern ∧ FieldsAgree rest

  def ItemsAgree : List Xsh.Syntax.Pattern → Prop
    | [] => True
    | pattern :: rest => Agrees pattern ∧ ItemsAgree rest

  def MembersAgree (names : List Xsh.Syntax.RawName) : List Xsh.Syntax.Pattern → Prop
    | [] => True
    | member :: rest => (patternNames member).Perm names ∧ Agrees member ∧ MembersAgree names rest
end

theorem both_matched {first : Match Declaration Identity Source} {next : Unit → Match Declaration Identity Source}
    {captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)}
    (matched : first.both next = .matched captures) :
    ∃ head tail, first = .matched head ∧ next () = .matched tail ∧ captures = head ++ tail := by
  cases first with
  | matched head =>
    simp only [Match.both] at matched
    cases rest : next () with
    | matched tail =>
      rw [rest] at matched
      simp only [Match.matched.injEq] at matched
      exact ⟨head, tail, rfl, rfl, matched.symm⟩
    | _ => rw [rest] at matched; cases matched
  | _ => simp [Match.both] at matched

theorem holeValue_single (spelling : Xsh.Syntax.RawName → String) (spec : Option Xsh.Syntax.RawName)
    (taken : List Char) (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source))
    (matched : holeValue spelling spec taken = .matched captures) : ∃ key value, captures = [(key, value)] := by
  unfold holeValue integerHole at matched
  split at matched <;> (try split at matched) <;> first
    | (simp only [Match.matched.injEq] at matched; subst matched; exact ⟨_, _, rfl⟩)
    | cases matched

theorem holeCapture_names (spelling : Xsh.Syntax.RawName → String) (name spec : Option Xsh.Syntax.RawName)
    (taken : List Char) (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source))
    (matched : holeCapture spelling name spec taken = .matched captures) : captures.map Prod.fst = name.toList := by
  unfold holeCapture at matched
  cases produced : holeValue spelling spec taken with
  | matched found =>
    obtain ⟨key, value, single⟩ := holeValue_single spelling spec taken found produced
    subst single
    rw [produced] at matched
    cases name <;> (simp at matched; subst matched; simp)
  | failed => rw [produced] at matched; cases matched
  | unsupported => rw [produced] at matched; cases matched

theorem matchText_names (spelling : Xsh.Syntax.RawName → String) :
    ∀ (parts : List Xsh.Syntax.TextPatternPart) (characters : List Char)
      (captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)),
      matchText spelling parts characters = .matched captures → captures.map Prod.fst = textNames parts
  | [], [], captures, matched => by
    simp only [matchText, Match.matched.injEq] at matched
    subst matched
    rfl
  | [], _ :: _, _, matched => by simp [matchText] at matched
  | .text literal :: rest, characters, captures, matched => by
    simp only [matchText] at matched
    split at matched
    · exact matchText_names spelling rest _ captures matched
    · cases matched
  | .hole name spec :: rest, characters, captures, matched => by
    cases rest with
    | nil =>
      simp only [matchText] at matched
      simpa [textNames] using holeCapture_names spelling name spec characters captures matched
    | cons next more =>
      cases next with
      | hole _ _ => simp [matchText] at matched
      | text literal =>
        cases more with
        | nil =>
          simp only [matchText] at matched
          split at matched
          · simpa [textNames] using holeCapture_names spelling name spec _ captures matched
          · cases matched
        | cons after others =>
          simp only [matchText] at matched
          split at matched
          · cases matched
          · obtain ⟨head, tail, first, later, joined⟩ := both_matched matched
            subst joined
            rw [List.map_append, holeCapture_names spelling name spec _ head first,
              matchText_names spelling (.text literal :: after :: others) _ tail later]
            rfl

end Names

section Matching

theorem matched_nil {captures : List (Xsh.Syntax.RawName × Value Declaration Identity Source)}
    (matched : (Match.matched [] : Match Declaration Identity Source) = .matched captures) :
    (captures.map Prod.fst).Perm [] := by
  simp only [Match.matched.injEq] at matched
  subst matched
  exact List.Perm.refl _

set_option hygiene false in
local macro "close_names" : tactic => `(tactic| repeat' (first
  | (cases matched; done)
  | exact matched_nil matched
  | exact matchPattern_names context test _ _ captures (by first | exact agrees | trivial | simpa [Agrees] using agrees) matched
  | exact matchItems_names context test _ _ captures (by first | exact agrees | trivial | simpa [Agrees] using agrees) matched
  | (simp only [Option.some.injEq, Xsh.Syntax.Pattern.tuple.injEq] at *; subst_vars)
  | (exfalso; simp_all; done)
  | split at matched))

mutual
  theorem matchPattern_names (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      ∀ (pattern : Xsh.Syntax.Pattern) (value : Value Declaration Identity Source) captures, Agrees pattern →
        matchPattern context test pattern value = .matched captures →
          (captures.map Prod.fst).Perm (patternNames pattern)
    | .wildcard, _, _, _, matched => by
      simp only [matchPattern] at matched
      simpa [patternNames] using matched_nil matched
    | .binding name, value, captures, _, matched => by
      simp only [matchPattern, Match.matched.injEq] at matched
      subst matched
      simp [patternNames]
    | .group inner, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simpa [patternNames] using matchPattern_names context test inner value captures agrees matched
    | .alias inner name, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      obtain ⟨head, tail, first, later, joined⟩ := both_matched matched
      simp only [Match.matched.injEq] at later
      subst later
      subst joined
      rw [List.map_append]
      simp only [patternNames]
      exact (matchPattern_names context test inner value head agrees first).append (List.Perm.refl _)
    | .alternation members, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simpa [patternNames] using matchAlternatives_names context test members value captures _ agrees matched
    | .constructor name none, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      close_names
    | .constructor name (some inner), value, captures, agrees, matched => by
      cases inner <;> (rw [matchPattern] at matched; simp only [patternNames]; close_names)
    | .errorVariant family variant fields, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · split at matched
        · exact matchFields_names context test fields _ captures agrees matched
        · cases matched
      · cases matched
    | .facet name, value, captures, _, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · split at matched
        · exact matched_nil matched
        · cases matched
      · cases matched
    | .text parts, value, captures, _, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · exact List.Perm.of_eq (matchText_names context.spelling parts _ captures matched)
      · cases matched
    | .testName name type, value, captures, _, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · simp only [Match.matched.injEq] at matched
        subst matched
        simp
      · cases matched
      · cases matched
    | .typeBind name type, value, captures, _, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · simp only [Match.matched.injEq] at matched
        subst matched
        cases name <;> simp
      · cases matched
      · cases matched
    | .literal written, value, captures, _, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · split at matched
        · split at matched
          · exact matched_nil matched
          · cases matched
          · cases matched
        · cases matched
      · cases matched
    | .record fields _, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · exact matchFields_names context test fields _ captures agrees matched
      · cases matched
    | .list elements none, value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · split at matched
        · exact matchItems_names context test elements _ captures agrees matched
        · cases matched
      · cases matched
    | .list elements (some tail), value, captures, agrees, matched => by
      simp only [matchPattern] at matched
      simp only [patternNames]
      split at matched
      · split at matched
        · obtain ⟨head, rest, first, later, joined⟩ := both_matched matched
          subst joined
          rw [List.map_append]
          exact (matchItems_names context test elements _ head agrees.1 first).append
            (matchPattern_names context test tail _ rest agrees.2 later)
        · cases matched
      · cases matched
    | .tuple _, _, _, _, matched => by simp [matchPattern] at matched
    | .textHole _ _, _, _, _, matched => by simp [matchPattern] at matched

  theorem matchFields_names (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      ∀ (fields : List (Xsh.Syntax.RawName × Xsh.Syntax.Pattern)) (actual : List (String × Value Declaration Identity Source))
        captures, FieldsAgree fields → matchFields context test fields actual = .matched captures →
          (captures.map Prod.fst).Perm (fieldPatternNames fields)
    | [], _, _, _, matched => by
      simp only [matchFields] at matched
      simpa [fieldPatternNames] using matched_nil matched
    | (label, pattern) :: rest, actual, captures, agrees, matched => by
      simp only [matchFields] at matched
      split at matched
      · cases matched
      · obtain ⟨head, tail, first, later, joined⟩ := both_matched matched
        subst joined
        rw [List.map_append]
        exact (matchPattern_names context test pattern _ head agrees.1 first).append
          (matchFields_names context test rest actual tail agrees.2 later)

  theorem matchItems_names (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      ∀ (patterns : List Xsh.Syntax.Pattern) (items : List (Value Declaration Identity Source)) captures,
        ItemsAgree patterns → matchItems context test patterns items = .matched captures →
          (captures.map Prod.fst).Perm (itemNames patterns)
    | [], _, _, _, matched => by
      simp only [matchItems] at matched
      simpa [itemNames] using matched_nil matched
    | pattern :: rest, item :: items, captures, agrees, matched => by
      simp only [matchItems] at matched
      obtain ⟨head, tail, first, later, joined⟩ := both_matched matched
      subst joined
      rw [List.map_append]
      exact (matchPattern_names context test pattern item head agrees.1 first).append
        (matchItems_names context test rest items tail agrees.2 later)
    | _ :: _, [], _, _, matched => by simp [matchItems] at matched

  theorem matchAlternatives_names (context : Context Declaration Identity Source)
      (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
      ∀ (members : List Xsh.Syntax.Pattern) (value : Value Declaration Identity Source) captures names,
        MembersAgree names members → matchAlternatives context test members value = .matched captures →
          (captures.map Prod.fst).Perm names
    | [], _, _, _, _, matched => by simp [matchAlternatives] at matched
    | member :: rest, value, captures, names, agrees, matched => by
      simp only [matchAlternatives] at matched
      split at matched
      · rename_i found produced
        simp only [Match.matched.injEq] at matched
        subst matched
        exact (matchPattern_names context test member value found agrees.2.1 produced).trans agrees.1
      · exact matchAlternatives_names context test rest value captures names agrees.2.2 matched
      · cases matched
end

end Matching

section Roots

open Xsh.Proofs.Core.Resolution

theorem staticBind_eq (scope : List Xsh.Syntax.RawName) :
    ∀ names : List Xsh.Syntax.RawName, staticBind scope names = names.reverse ++ scope
  | [] => rfl
  | name :: rest => by
    show staticBind (name :: scope) rest = _
    rw [staticBind_eq (name :: scope) rest]
    simp

theorem represents_perm {names : LexicalEnvironment} {top top' : List Xsh.Syntax.RawName} {outer : StaticScopes}
    (carried : Represents names (top :: outer)) (perm : top.Perm top') : Represents names (top' :: outer) := by
  refine ⟨by simpa using carried.1, fun name => ?_⟩
  rw [carried.2 name]
  simp only [staticResolve, perm.mem_iff]

variable [DecidableEq Declaration]

omit [DecidableEq Declaration] in
/-- Capture names are a static function of the pattern (SPEC 6.8): a
successful match binds exactly `patternNames pattern`, up to order, for every
pattern form, alternation included when its alternatives bind the same names
("Alternatives must bind the same names with the same types"). -/
theorem pattern_names (context : Context Declaration Identity Source) (test : Xsh.Syntax.TypeExpr → Value Declaration Identity Source → Option Bool) :
    ∀ pattern (value : Value Declaration Identity Source) captures, Agrees pattern →
      matchPattern context test pattern value = .matched captures →
        (captures.map Prod.fst).Perm (patternNames pattern) :=
  matchPattern_names context test

/-- The resolution pass in pattern captures and comprehension targets. Under
the canonical resolution, the captures a match arm, `if let`, `while let`, a
`??` handler or a `guard let` else block binds in its fresh scope declare
their names in a new innermost static scope, and so does a comprehension's
or `for` loop's target; a bound body that completes normally leaves a lexical
environment representing the pass's scopes after its statements, starting
from the captures' scope. The captures of a successful match of a pattern
whose alternatives agree declare `patternNames pattern`, a static function
of the pattern, in that scope (`pattern_names`), so the pass needs no name
from the match. -/
theorem capture_resolution {world : World Declaration Identity Source} (machine : Machine world) :
    ∀ base types, machine.resolution = canonicalResolution base types →
      (∀ (frame next : Frame world) captures scopes, Represents frame.environment.names scopes →
        bindCaptures machine frame.enter captures = .normal next →
          Represents next.environment.names (staticBind [] (captures.map Prod.fst) :: scopes)) ∧
      (∀ (frame next : Frame world) target item scopes, Represents frame.environment.names scopes →
        bindTarget machine frame.enter .immutable target item = .normal next →
          Represents next.environment.names (staticBind [] (targetNames target) :: scopes)) ∧
      (∀ fuel (frame bound last : Frame world) captures body scopes, Represents frame.environment.names scopes →
        bindCaptures machine frame.enter captures = .normal bound →
          runStatements machine fuel bound body = .normal last →
            Represents last.environment.names
              (staticDeclareAll (staticBind [] (captures.map Prod.fst) :: scopes) body)) ∧
      (∀ (frame next : Frame world) pattern value captures scopes, Represents frame.environment.names scopes →
        Agrees pattern → matchPattern machine.context machine.typeTest pattern value = .matched captures →
          bindCaptures machine frame.enter captures = .normal next →
            Represents next.environment.names (staticBind [] (patternNames pattern) :: scopes)) ∧
      (∀ fuel (frame bound last : Frame world) pattern value captures body scopes,
        Represents frame.environment.names scopes →
        Agrees pattern → matchPattern machine.context machine.typeTest pattern value = .matched captures →
          bindCaptures machine frame.enter captures = .normal bound →
            runStatements machine fuel bound body = .normal last →
              Represents last.environment.names
                (staticDeclareAll (staticBind [] (patternNames pattern) :: scopes) body)) := by
  intro base types resolved
  have static : ∀ (frame next : Frame world) pattern value captures scopes,
      Represents frame.environment.names scopes →
      Agrees pattern → matchPattern machine.context machine.typeTest pattern value = .matched captures →
        bindCaptures machine frame.enter captures = .normal next →
          Represents next.environment.names (staticBind [] (patternNames pattern) :: scopes) := by
    intro frame next pattern value captures scopes carried agrees matched bound
    have names := pattern_names machine.context machine.typeTest pattern value captures agrees matched
    refine represents_perm (bindCaptures_resolves machine base types resolved frame next captures scopes carried bound) ?_
    rw [staticBind_eq, staticBind_eq]
    simpa using (List.reverse_perm _).trans (names.trans (List.reverse_perm _).symm)
  refine ⟨fun frame next captures scopes carried bound =>
      bindCaptures_resolves machine base types resolved frame next captures scopes carried bound,
    fun frame next target item scopes carried bound =>
      enteredTarget_resolves machine base types resolved frame next target item scopes carried bound,
    fun fuel frame bound last captures body scopes carried entered ran =>
      boundBody_resolves machine base types resolved fuel frame bound last captures body scopes carried entered ran,
    static, ?_⟩
  intro fuel frame bound last pattern value captures body scopes carried agrees matched entered ran
  exact statements_resolve machine base types resolved fuel bound body last _ scopes
    (static frame bound pattern value captures scopes carried agrees matched entered) ran

end Roots

namespace Witness

open Xsh.Proofs.Core.Control.Witness

/-- `[a, b] | [b, a, _]`: the verification binary gives `12` for `[1, 2]`
and `21` for `[1, 2, 7]` with `a * 10 + b`, and rejects `[a, b] | [b, c, _]`
(check.pattern-alternative-binding). -/
def swapped : Xsh.Syntax.Pattern :=
  .alternation [.list [.binding aName, .binding bName] none, .list [.binding bName, .binding aName, .wildcard] none]

def one : Value Unit Unit Unit := .scalar (.boolean true)
def two : Value Unit Unit Unit := .scalar (.boolean false)

theorem swapped_agrees : Agrees swapped := by
  simp only [swapped, Agrees, MembersAgree, alternativeNames, patternNames, itemNames]
  refine ⟨by simp, by simp [Agrees, ItemsAgree], ?_, by simp [Agrees, ItemsAgree], trivial⟩
  simpa using List.Perm.swap aName bName []

/-- The second alternative binds `b` then `a`, the first alternative's names
in another order, and `f"{k}={v}"` and `[a, ..b]` bind their names from the
syntax. -/
theorem alternative_names :
    matchPattern machine.context machine.typeTest swapped (.list [one, two, one]) =
        .matched [(bName, one), (aName, two)] ∧
      patternNames swapped = [aName, bName] ∧
      patternNames (.text [.hole (some kName) none, .text "=", .hole (some vName) none]) = [kName, vName] ∧
      patternNames (.list [.binding aName] (some (.binding bName))) = [aName, bName] := by
  refine ⟨rfl, ?_⟩
  decide

/-- Two mutants are rejected: names read as all alternatives' names
together (`[a, b, b, a]`), and an ordered reading of `pattern_names`, since
the match above binds `[b, a]`. -/
def allAlternativeNames : Xsh.Syntax.Pattern → List Xsh.Syntax.RawName
  | .alternation members => members.flatMap patternNames
  | other => patternNames other

theorem names_mutants_rejected :
    ¬ ([bName, aName].Perm (allAlternativeNames swapped)) ∧ [bName, aName] ≠ patternNames swapped := by
  refine ⟨fun perm => ?_, by decide⟩
  have := perm.length_eq
  simp [allAlternativeNames, swapped, patternNames, itemNames] at this

/-- The bits a hole's capture holds, if it matched one `Float`. -/
def floatCapture : Match Unit Unit Unit → Option Nat
  | .matched [(_, .scalar (.float bits))] => some bits.val
  | _ => none

def isFailed : Match Unit Unit Unit → Bool
  | .failed => true
  | _ => false

/-- An `f` hole reads `1.5e3` as `1500.0`, as Rust's text pattern does, and
fails on `nan`, which a Float hole refuses. -/
theorem float_hole_reads :
    floatCapture (holeValue (fun _ => "f") (some ⟨⟨0, by decide⟩⟩) "1.5e3".toList) = some 4654311885213007872 ∧
      isFailed (holeValue (fun _ => "f") (some ⟨⟨0, by decide⟩⟩) "nan".toList) = true := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Patterns
