import Xsh.Conformance.Vertical.SourceClassifier

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## Tree spans

A source node owns the tokens it consumes. `TokenSpan` defines a token slice's
byte span as its first token's start through its last token's stop, so a node's
span *is* its own tokens' span by construction. The theorems below show that
span lies inside the source: both endpoints come from positioned tokens, which
`positioned_spans_bounded` confines to the source. Quotes are excluded from
`D`, so a domain node carries no string tokens and its span carries no string. -/

/-- The half-open byte span of a token slice: its first token's start through its last
token's stop. An empty slice has no span. -/
def TokenSpan : List (Token × Nat × Nat) → Option Span
  | [] => none
  | (_, start, stop) :: rest =>
      some ⟨start, (rest.getLast?.elim stop (fun last => last.2.2))⟩

theorem token_span_nil : TokenSpan [] = none := rfl

theorem token_span_cons (token : Token) (start : Nat) (stop : Nat)
    (rest : List (Token × Nat × Nat)) :
    TokenSpan ((token, start, stop) :: rest) =
      some ⟨start, (rest.getLast?.elim stop (fun last => last.2.2))⟩ := rfl

/-- The first token supplies the span's start and the last token its stop: a slice's span is
exactly the span of its own tokens. -/
theorem span_is_endpoints {tokens : List (Token × Nat × Nat)} :
    ∀ firstStart stop, TokenSpan tokens = some ⟨firstStart, stop⟩ →
      ∃ head tail, tokens = head :: tail ∧ head.2.1 = firstStart ∧
        (tail.getLast?.elim head.2.2 (fun last => last.2.2)) = stop := by
  cases tokens with
  | nil => intro _ _ found; rw [token_span_nil] at found; cases found
  | cons head rest =>
    rcases head with ⟨token, start, stop⟩
    intro foundStart foundStop found
    rw [token_span_cons] at found
    cases found
    exact ⟨(token, start, stop), rest, rfl, rfl, rfl⟩

/-- The last entry of a nonempty positioned token list, when present, is a member. -/
theorem get_last_mem {tokens : List (Token × Nat × Nat)} {last : Token × Nat × Nat}
    (found : tokens.getLast? = some last) : last ∈ tokens := by
  rw [List.getLast?_eq_some_iff] at found
  obtain ⟨front, same⟩ := found
  rw [same]
  simp

/-- A token slice's span lies inside the source: its start is at least the source offset and
its stop at most the source end. Both come from positioned tokens. -/
theorem token_span_within_source {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (positioned : PositionedLexed offset codes tokens)
    {span : Span} (found : TokenSpan tokens = some span) :
    offset ≤ span.start ∧ span.stop ≤ offset + codes.length := by
  cases tokens with
  | nil => rw [token_span_nil] at found; cases found
  | cons head rest =>
    rcases head with ⟨headToken, headStart, headStop⟩
    rw [token_span_cons] at found
    cases found
    refine ⟨?_, ?_⟩
    · have bounds := positioned_spans_bounded positioned headToken headStart headStop (by simp)
      exact bounds.1
    · cases restLast : rest.getLast? with
      | none =>
        have bounds := positioned_spans_bounded positioned headToken headStart headStop (by simp)
        simpa only [restLast, Option.elim] using bounds.2.2
      | some last =>
        rcases last with ⟨lastToken, lastStart, lastStop⟩
        have member : (lastToken, lastStart, lastStop) ∈ (headToken, headStart, headStop) :: rest :=
          List.mem_cons_of_mem _ (get_last_mem restLast)
        have bounds := positioned_spans_bounded positioned lastToken lastStart lastStop member
        simpa only [restLast, Option.elim] using bounds.2.2

/-- A code that is the start of a token is at least the source offset and at most the source
end, so every node's consumed tokens stay in the source. -/
theorem span_start_nonnegative (span : Span) : 0 ≤ span.start := Nat.zero_le _


/-- A token slice's span is exactly its first token's start to its last token's stop: the span
is that of its own tokens, not of anything else. -/
theorem node_span_equals_token_span {slice : List (Token × Nat × Nat)} {span : Span}
    (found : TokenSpan slice = some span) :
    ∃ head tail, slice = head :: tail ∧ head.2.1 = span.start ∧
      (tail.getLast?.elim head.2.2 (fun last => last.2.2)) = span.stop :=
  span_is_endpoints _ _ found

end SourceAdmission

end Xsh.Conformance.Vertical
