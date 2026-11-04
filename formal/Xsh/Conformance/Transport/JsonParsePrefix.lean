import Xsh.Conformance.Transport.Json
namespace Xsh.Conformance.Transport

def parsePrefix : List Token → List Frame → Expect → Option (List Frame × Expect)
  | [], stack, expect => some (stack, expect)
  | _ :: _, _, .finished _ => none
  | .string text :: rest, stack, .value _ =>
      match complete stack (.string text) with
      | some (next, expect) => parsePrefix rest next expect
      | none => none
  | .leftBracket :: rest, stack, .value _ =>
      parsePrefix rest (.array [] :: stack) (.value true)
  | .leftBrace :: rest, stack, .value _ =>
      parsePrefix rest (.object [] none :: stack) (.key true)
  | .rightBracket :: rest, .array [] :: outer, .value true =>
      match complete outer (.array []) with
      | some (next, expect) => parsePrefix rest next expect
      | none => none
  | .string text :: rest, .object fields none :: outer, .key _ =>
      parsePrefix rest (.object fields (some text) :: outer) .colon
  | .rightBrace :: rest, .object [] none :: outer, .key true =>
      match complete outer (.object []) with
      | some (next, expect) => parsePrefix rest next expect
      | none => none
  | .colon :: rest, stack, .colon => parsePrefix rest stack (.value false)
  | .comma :: rest, stack@(.array _ :: _), .afterValue =>
      parsePrefix rest stack (.value false)
  | .comma :: rest, stack@(.object _ none :: _), .afterValue =>
      parsePrefix rest stack (.key false)
  | .rightBracket :: rest, .array items :: outer, .afterValue =>
      match complete outer (.array items.reverse) with
      | some (next, expect) => parsePrefix rest next expect
      | none => none
  | .rightBrace :: rest, .object fields none :: outer, .afterValue =>
      if keysAreDistinct fields.reverse then
        match complete outer (.object fields.reverse) with
        | some (next, expect) => parsePrefix rest next expect
        | none => none
      else none
  | _, _, _ => none

theorem parse_append_prefix (front suffix : List Token) (stack : List Frame) (expect : Expect) :
    parseTokens (front ++ suffix) stack expect =
      (parsePrefix front stack expect).bind (fun (next, needed) => parseTokens suffix next needed) := by
  induction front generalizing stack expect with
  | nil => simp [parsePrefix]
  | cons token rest ih =>
    cases token <;> cases expect <;> cases stack with
    | nil => simp [parseTokens, parsePrefix, complete, ih]
    | cons top outer =>
      cases top with
      | array items =>
        cases items <;> simp only [List.cons_append, parseTokens, parsePrefix]
        all_goals repeat' first | split | simp_all [parseTokens, parsePrefix, ih, complete]
        all_goals try (cases ‹Bool› <;> simp [parseTokens, parsePrefix, complete, ih] <;> repeat' first | split | simp_all [ih, complete])
      | object fields key =>
        cases fields <;> cases key <;> simp only [List.cons_append, parseTokens, parsePrefix]
        all_goals repeat' first | split | simp_all [parseTokens, parsePrefix, ih, complete]
        all_goals try (cases ‹Bool› <;> simp [parseTokens, parsePrefix, complete, ih] <;> repeat' first | split | simp_all [ih, complete])

def parseFrom (state : Option (List Frame × Expect)) (tokens : List Token) : Option WireValue :=
  state.bind fun (stack, expect) => parseTokens tokens stack expect

theorem parseFrom_append (state : Option (List Frame × Expect)) (front suffix : List Token) :
    parseFrom state (front ++ suffix) =
      parseFrom (state.bind (fun (stack, expect) => parsePrefix front stack expect)) suffix := by
  cases state with
  | none => rfl
  | some pair =>
    obtain ⟨stack, expect⟩ := pair
    simp [parseFrom, parse_append_prefix, Option.bind_assoc]

theorem parseCodes_from_parts (text : ExchangeCodes) (tokens : List Token) (wire : WireValue)
    (within : ¬ maxDocumentBytes < text.length)
    (lexed : lex text .outside [] = some tokens)
    (parsed : parseTokens tokens [] (.value false) = some wire) :
    parseCodes text = some wire := by
  simp only [parseCodes, within, ↓reduceIte, lexed, Option.bind_some, parsed]

end Xsh.Conformance.Transport
