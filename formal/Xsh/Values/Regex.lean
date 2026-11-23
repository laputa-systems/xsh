namespace Xsh.Values.Regex

/-- A regex in the subset the model reads: the Rust regex surface's literals,
`.`, bracket and ASCII Perl classes, `^`, `$`, `\b`, groups, alternation,
greedy or lazy repetition and the `i`, `m`, `s`, `U`, `R` and `u` flags. -/
inductive Node where
  | literal (char : Char)
  | ranges (negated : Bool) (spans : List (Nat × Nat))
  | textStart
  | textEnd
  /-- `^` under the `m` flag: after a line terminator, `\n` or, under `R`,
  also a `\r` not before a `\n`. -/
  | lineStart (crlf : Bool)
  /-- `$` under the `m` flag. -/
  | lineEnd (crlf : Bool)
  | wordBoundary (negated : Bool)
  /-- `\\<` at a word's start, `\\>` at its end. -/
  | wordEdge (start : Bool)
  | concat (nodes : List Node)
  | alt (branches : List Node)
  | repeat (node : Node) (least : Nat) (most : Option Nat) (greedy : Bool)
  | group (index : Option Nat) (node : Node)

/-- A parsed pattern: its tree, its group count and each named group. -/
structure Pattern where
  node : Node
  groups : Nat
  names : List (String × Nat)

def digitSpans : List (Nat × Nat) := [(48, 57)]
def wordSpans : List (Nat × Nat) := [(48, 57), (65, 90), (95, 95), (97, 122)]
def spaceSpans : List (Nat × Nat) := [(9, 13), (32, 32)]

def inSpans (spans : List (Nat × Nat)) (code : Nat) : Bool := spans.any fun (low, high) => low ≤ code ∧ code ≤ high

def isWordChar (char : Char) : Bool := inSpans wordSpans char.toNat

def isMeta (char : Char) : Bool := "\\.+*?()|[]{}^$-#&~".toList.contains char

/-- A character an escape may stand for itself: a meta character, or ASCII
that is not a letter, a digit, `<` or `>`. -/
def isEscapable (char : Char) : Bool :=
  isMeta char || (char.toNat < 128 && !char.isAlphanum && char != '<' && char != '>')

/-- A Perl class escape: its spans and whether it is negated. -/
def perlClass : Char → Option (Bool × List (Nat × Nat))
  | 'd' => some (false, digitSpans)
  | 'D' => some (true, digitSpans)
  | 'w' => some (false, wordSpans)
  | 'W' => some (true, wordSpans)
  | 's' => some (false, spaceSpans)
  | 'S' => some (true, spaceSpans)
  | _ => none

def hexValue (digits : List Char) : Option Nat :=
  digits.foldl (fun total digit => total.bind fun value =>
    if '0' ≤ digit ∧ digit ≤ '9' then some (value * 16 + (digit.toNat - 48))
    else if 'a' ≤ digit ∧ digit ≤ 'f' then some (value * 16 + (digit.toNat - 87))
    else if 'A' ≤ digit ∧ digit ≤ 'F' then some (value * 16 + (digit.toNat - 55))
    else none) (some 0)

def scalarOf (code : Nat) : Option Char :=
  if code < 55296 ∨ (57343 < code ∧ code < 1114112) then some (Char.ofNat code) else none

/-- The character an escape stands for, given what follows its `\\`:
`\\a`, `\\f`, `\\t`, `\\n`, `\\r` and `\\v`, an escapable character, or a
code point as `\\xNN`, `\\uNNNN`, `\\UNNNNNNNN` or any of them braced. -/
def escapeChar : List Char → Option (Char × List Char)
  | 'a' :: rest => some ('\x07', rest)
  | 'f' :: rest => some ('\x0C', rest)
  | 't' :: rest => some ('\t', rest)
  | 'n' :: rest => some ('\n', rest)
  | 'r' :: rest => some ('\r', rest)
  | 'v' :: rest => some ('\x0B', rest)
  | lead :: '{' :: rest =>
    if lead = 'x' ∨ lead = 'u' ∨ lead = 'U' then
      let digits := rest.takeWhile (· ≠ '}')
      match rest.drop digits.length with
      | '}' :: after =>
        if digits.isEmpty then none else ((hexValue digits).bind scalarOf).map fun char => (char, after)
      | _ => none
    else if isEscapable lead then some (lead, '{' :: rest)
    else none
  | lead :: rest =>
    let width := if lead = 'x' then 2 else if lead = 'u' then 4 else if lead = 'U' then 8 else 0
    if width > 0 then
      let digits := rest.take width
      if digits.length = width then ((hexValue digits).bind scalarOf).map fun char => (char, rest.drop width)
      else none
    else if isEscapable lead then some (lead, rest)
    else none
  | [] => none

/-- One class item: a character, an escape, or a Perl class's spans. -/
def classItem : List Char → Option ((List (Nat × Nat) ⊕ Char) × List Char)
  | '\\' :: escape :: rest =>
    match perlClass escape with
    | some (false, spans) => some (.inl spans, rest)
    | some (true, _) => none
    | none => (escapeChar (escape :: rest)).map fun (char, after) => (.inr char, after)
  | char :: rest => some (.inr char, rest)
  | [] => none

def itemSpans : (List (Nat × Nat) ⊕ Char) → List (Nat × Nat)
  | .inl spans => spans
  | .inr char => [(char.toNat, char.toNat)]

/-- A bracket class's items up to its closing `]`, and the rest: an item, or
a range of two characters when a `-` joins them that is not before `]` or
`-`. -/
def classItems : Nat → List Char → Option (List (Nat × Nat) × List Char)
  | 0, _ => none
  | _ + 1, [] => none
  | _ + 1, ']' :: rest => some ([], rest)
  | _ + 1, '[' :: _ | _ + 1, '&' :: '&' :: _ | _ + 1, '-' :: '-' :: _ | _ + 1, '~' :: '~' :: _ => none
  | fuel + 1, text =>
    (classItem text).bind fun (first, rest) =>
      let single := (classItems fuel rest).map fun (items, after) => (itemSpans first ++ items, after)
      match rest with
      | '-' :: ']' :: _ | '-' :: '-' :: _ => single
      | '-' :: after =>
        (classItem after).bind fun (second, more) =>
          match first, second with
          | .inr low, .inr high =>
            if low.toNat ≤ high.toNat then
              (classItems fuel more).map fun (items, after) => ((low.toNat, high.toNat) :: items, after)
            else none
          | _, _ => none
      | _ => single

/-- A bracket class after its `[`: whether it is negated, its spans and the
rest; leading `-`s are literal, as is a `]` first in the class. -/
def classBody (text : List Char) : Option (Bool × List (Nat × Nat) × List Char) :=
  let (negated, rest) := match text with
    | '^' :: more => (true, more)
    | more => (false, more)
  let dashes := rest.takeWhile (· = '-')
  let (lead, body) := match dashes, rest.drop dashes.length with
    | [], ']' :: more => ([(93, 93)], more)
    | _, more => ([], more)
  (classItems (body.length + 1) body).map fun (spans, after) =>
    (negated, dashes.map (fun _ => (45, 45)) ++ lead ++ spans, after)

/-- The flags in force: `i`, `m`, `s`, `U` and `R`. -/
structure Flags where
  caseless : Bool := false
  multiLine : Bool := false
  dotAll : Bool := false
  swapGreed : Bool := false
  crlf : Bool := false

/-- regex-lite's ASCII case fold of one span: the other case of its part in
`a`–`z`, else of its part in `A`–`Z`. -/
def foldSpan : Nat × Nat → Option (Nat × Nat)
  | (low, high) =>
    if low ≤ 122 ∧ 97 ≤ high then some (max low 97 - 32, min high 122 - 32)
    else if low ≤ 90 ∧ 65 ≤ high then some (max low 65 + 32, min high 90 + 32)
    else none

def foldSpans (spans : List (Nat × Nat)) : List (Nat × Nat) := spans ++ spans.filterMap foldSpan

/-- A class under the flags: folded before it is negated. -/
def classNode (flags : Flags) (negated : Bool) (spans : List (Nat × Nat)) : Node :=
  .ranges negated (if flags.caseless then foldSpans spans else spans)

def literalNode (flags : Flags) (char : Char) : Node :=
  if flags.caseless then
    match foldSpan (char.toNat, char.toNat) with
    | some folded => .ranges false [(char.toNat, char.toNat), folded]
    | none => .literal char
  else .literal char

/-- `.`: every character under `s`, else all but `\n`, and under `R` also
`\r`. -/
def dotNode (flags : Flags) : Node :=
  if flags.dotAll then .ranges false [(0, 1114111)]
  else if flags.crlf then .ranges false [(0, 9), (11, 12), (14, 1114111)]
  else .ranges false [(0, 9), (11, 1114111)]

/-- A flag group's flags up to its `:` or `)`: one optional `-` negating the
flags after it, each flag at most once, none dangling after the `-`. -/
def parseFlags (flags : Flags) : Nat → List Char → Bool → Bool → List Char → Option (Flags × List Char)
  | 0, _, _, _, _ => none
  | _ + 1, [], _, _, _ => none
  | _ + 1, ':' :: rest, _, dangling, _ => if dangling then none else some (flags, ':' :: rest)
  | _ + 1, ')' :: rest, _, dangling, _ => if dangling then none else some (flags, ')' :: rest)
  | fuel + 1, '-' :: rest, negate, _, seen =>
    if negate then none else parseFlags flags fuel rest true true seen
  | fuel + 1, flag :: rest, negate, _, seen =>
    if seen.contains flag then none
    else
      let enabled := !negate
      let next : Option Flags := match flag with
        | 'i' => some { flags with caseless := enabled }
        | 'm' => some { flags with multiLine := enabled }
        | 's' => some { flags with dotAll := enabled }
        | 'U' => some { flags with swapGreed := enabled }
        | 'R' => some { flags with crlf := enabled }
        | 'u' => some flags
        | _ => none
      next.bind fun updated => parseFlags updated fuel rest negate false (flag :: seen)

/-- The decimal number at the front of the text. -/
def number (text : List Char) : Option (Nat × List Char) :=
  let digits := text.takeWhile Char.isDigit
  if digits.isEmpty then none
  else some (digits.foldl (fun value digit => value * 10 + (digit.toNat - 48)) 0, text.drop digits.length)

/-- A repetition operator after an atom, with its laziness. -/
def quantifier (text : List Char) : Option ((Nat × Option Nat) × List Char) :=
  let bounds : Option ((Nat × Option Nat) × List Char) := match text with
    | '*' :: rest => some ((0, none), rest)
    | '+' :: rest => some ((1, none), rest)
    | '?' :: rest => some ((0, some 1), rest)
    | '{' :: rest =>
      (number rest).bind fun (least, after) =>
        match after with
        | '}' :: more => some ((least, some least), more)
        | ',' :: '}' :: more => some ((least, none), more)
        | ',' :: more =>
          (number more).bind fun (most, tail) =>
            match tail with
            | '}' :: final => if least ≤ most then some ((least, some most), final) else none
            | _ => none
        | _ => none
    | _ => none
  bounds

abbrev Parsed := Option (Node × List Char × Nat × List (String × Nat) × Flags)

mutual
  /-- Alternatives separated by `|`, up to a `)` or the end; a flag group's
  flags hold to the end of the group it is in, across `|`. -/
  def parseAlt : Nat → List Char → Nat → List (String × Nat) → Flags → Parsed
    | 0, _, _, _, _ => none
    | fuel + 1, text, groups, names, flags =>
      (parseConcat fuel text groups names flags []).bind fun (first, rest, groups, names, flags) =>
        match rest with
        | '|' :: more =>
          (parseAlt fuel more groups names flags).bind fun (others, after, groups, names, flags) =>
            match others with
            | .alt branches => some (.alt (first :: branches), after, groups, names, flags)
            | other => some (.alt [first, other], after, groups, names, flags)
        | _ => some (first, rest, groups, names, flags)

  /-- A sequence of quantified atoms and flag groups. -/
  def parseConcat : Nat → List Char → Nat → List (String × Nat) → Flags → List Node → Parsed
    | 0, _, _, _, _, _ => none
    | _ + 1, [], groups, names, flags, done => some (.concat done.reverse, [], groups, names, flags)
    | _ + 1, '|' :: rest, groups, names, flags, done => some (.concat done.reverse, '|' :: rest, groups, names, flags)
    | _ + 1, ')' :: rest, groups, names, flags, done => some (.concat done.reverse, ')' :: rest, groups, names, flags)
    | _ + 1, '(' :: '?' :: ')' :: _, _, _, _, _ => none
    | fuel + 1, '(' :: '?' :: rest, groups, names, flags, done =>
      match rest with
      | 'P' :: '<' :: _ | '<' :: _ | ':' :: _ => atomThen fuel ('(' :: '?' :: rest) groups names flags done
      | _ =>
        match parseFlags flags (rest.length + 1) rest false false [] with
        | some (updated, ')' :: after) => parseConcat fuel after groups names updated done
        | _ => atomThen fuel ('(' :: '?' :: rest) groups names flags done
    | fuel + 1, text, groups, names, flags, done => atomThen fuel text groups names flags done

  /-- One atom with its repetition, then the rest of the sequence. -/
  def atomThen : Nat → List Char → Nat → List (String × Nat) → Flags → List Node → Parsed
    | 0, _, _, _, _, _ => none
    | fuel + 1, text, groups, names, flags, done =>
      (parseAtom fuel text groups names flags).bind fun (atom, rest, groups, names, _) =>
        match quantifier rest with
        | some ((least, most), '?' :: after) =>
          parseConcat fuel after groups names flags ((Node.repeat atom least most flags.swapGreed) :: done)
        | some ((least, most), after) =>
          parseConcat fuel after groups names flags ((Node.repeat atom least most !flags.swapGreed) :: done)
        | none => parseConcat fuel rest groups names flags (atom :: done)

  /-- One atom: a group, a class, `.`, an anchor, an escape or a literal; a
  group's flags end with it. -/
  def parseAtom : Nat → List Char → Nat → List (String × Nat) → Flags → Parsed
    | 0, _, _, _, _ => none
    | fuel + 1, text, groups, names, flags =>
      let closed := fun (result : Parsed) (wrap : Node → Node) =>
        result.bind fun (inner, rest, groups, names, _) =>
          match rest with
          | ')' :: after => some (wrap inner, after, groups, names, flags)
          | _ => none
      match text with
      | '(' :: '?' :: ':' :: rest => closed (parseAlt fuel rest groups names flags) (.group none)
      | '(' :: '?' :: 'P' :: '<' :: rest | '(' :: '?' :: '<' :: rest =>
        let label := rest.takeWhile (· ≠ '>')
        if label.isEmpty || !(label.all fun char => isWordChar char) then none
        else
          match rest.drop label.length with
          | '>' :: body =>
            closed (parseAlt fuel body (groups + 1) ((String.ofList label, groups + 1) :: names) flags)
              (.group (some (groups + 1)))
          | _ => none
      | '(' :: '?' :: rest =>
        match parseFlags flags (rest.length + 1) rest false false [] with
        | some (inner, ':' :: body) => closed (parseAlt fuel body groups names inner) (.group none)
        | _ => none
      | '(' :: rest => closed (parseAlt fuel rest (groups + 1) names flags) (.group (some (groups + 1)))
      | '[' :: rest =>
        (classBody rest).map fun (negated, spans, after) => (classNode flags negated spans, after, groups, names, flags)
      | '.' :: rest => some (dotNode flags, rest, groups, names, flags)
      | '^' :: rest =>
        some (if flags.multiLine then .lineStart flags.crlf else .textStart, rest, groups, names, flags)
      | '$' :: rest =>
        some (if flags.multiLine then .lineEnd flags.crlf else .textEnd, rest, groups, names, flags)
      | '\\' :: 'b' :: '{' :: _ => none
      | '\\' :: 'b' :: rest => some (.wordBoundary false, rest, groups, names, flags)
      | '\\' :: 'A' :: rest => some (.textStart, rest, groups, names, flags)
      | '\\' :: 'z' :: rest => some (.textEnd, rest, groups, names, flags)
      | '\\' :: '<' :: rest => some (.wordEdge true, rest, groups, names, flags)
      | '\\' :: '>' :: rest => some (.wordEdge false, rest, groups, names, flags)
      | '\\' :: 'B' :: rest => some (.wordBoundary true, rest, groups, names, flags)
      | '\\' :: escape :: rest =>
        match perlClass escape with
        | some (negated, spans) => some (.ranges negated spans, rest, groups, names, flags)
        | none => (escapeChar (escape :: rest)).map fun (char, after) => (literalNode flags char, after, groups, names, flags)
      | char :: rest =>
        if "*+?{)|]".toList.contains char then none else some (literalNode flags char, rest, groups, names, flags)
      | [] => none
end

/-- The pattern a regex text spells, `none` outside the subset. -/
def parse (source : String) : Option Pattern :=
  (parseAlt (4 * source.length + 4) source.toList 0 [] {}).bind fun (node, rest, groups, names, _) =>
    if rest.isEmpty then some ⟨node, groups, names⟩ else none

abbrev Captures := List (Option (Nat × Nat))

/-- Match `node` at `position` of `text`, then the continuation, trying
alternatives and repetition counts in the leftmost-first priority order. -/
def matchNode (text : List Char) : Nat → Node → Nat → Captures → (Nat → Captures → Option (Nat × Captures)) →
    Option (Nat × Captures)
  | 0, _, _, _, _ => none
  | fuel + 1, node, position, captures, next =>
    match node with
    | .literal char => if text[position]? = some char then next (position + 1) captures else none
    | .ranges negated spans =>
      match text[position]? with
      | some char => if inSpans spans char.toNat != negated then next (position + 1) captures else none
      | none => none
    | .textStart => if position = 0 then next position captures else none
    | .textEnd => if position = text.length then next position captures else none
    | .lineStart crlf =>
      let before := text[position - 1]?
      if position = 0 ∨ before = some '\n' ∨ (crlf ∧ before = some '\r' ∧ text[position]? ≠ some '\n') then
        next position captures
      else none
    | .lineEnd crlf =>
      let after := text[position]?
      if position = text.length ∨ after = some '\n' ∧ ¬(crlf ∧ position > 0 ∧ text[position - 1]? = some '\r') ∨
          (crlf ∧ after = some '\r') then
        next position captures
      else none
    | .wordBoundary negated =>
      let before := (position > 0) && ((text[position - 1]?).map isWordChar).getD false
      let after := ((text[position]?).map isWordChar).getD false
      if (before != after) != negated then next position captures else none
    | .wordEdge start =>
      let before := (position > 0) && ((text[position - 1]?).map isWordChar).getD false
      let after := ((text[position]?).map isWordChar).getD false
      if (if start then !before && after else before && !after) then next position captures else none
    | .concat [] => next position captures
    | .concat (first :: rest) =>
      matchNode text fuel first position captures fun middle held =>
        matchNode text fuel (.concat rest) middle held next
    | .alt [] => none
    | .alt (first :: rest) =>
      (matchNode text fuel first position captures next).or (matchNode text fuel (.alt rest) position captures next)
    | .group none inner => matchNode text fuel inner position captures next
    | .group (some index) inner =>
      matchNode text fuel inner position captures fun finish held => next finish (held.set index (some (position, finish)))
    | .repeat inner least most greedy =>
      let more := match most with
        | some 0 => none
        | _ =>
          matchNode text fuel inner position captures fun finish held =>
            if finish = position ∧ least = 0 then none
            else matchNode text fuel (.repeat inner (least - 1) (most.map (· - 1)) greedy) finish held next
      if least > 0 then more
      else if greedy then more.or (next position captures)
      else (next position captures).or more

/-- The leftmost-first match starting at or after `start`. -/
def searchFrom (pattern : Pattern) (text : List Char) (start : Nat) : Option Captures :=
  let budget := 64 * (text.length + 1) * (text.length + 1) + 4096
  ((List.range (text.length + 1 - start)).map (· + start)).findSome? fun position =>
    (matchNode text budget pattern.node position (List.replicate (pattern.groups + 1) none)
        fun finish held => some (finish, held)).map fun (finish, held) => held.set 0 (some (position, finish))

/-- Every non-overlapping match, as Rust's iterator gives them: after an empty
match where the last one ended, the search moves on one character. -/
def matchesFrom (pattern : Pattern) (text : List Char) : Nat → Nat → Option Nat → List Captures
  | 0, _, _ => []
  | fuel + 1, start, lastEnd =>
    if start > text.length then []
    else
      match searchFrom pattern text start with
      | none => []
      | some held =>
        match held.head? with
        | some (some (first, finish)) =>
          if first = finish ∧ some finish = lastEnd then
            match searchFrom pattern text (start + 1) with
            | none => []
            | some again =>
              match again.head? with
              | some (some (_, finish')) => again :: matchesFrom pattern text fuel finish' (some finish')
              | _ => []
          else held :: matchesFrom pattern text fuel finish (some finish)
        | _ => []

def allMatches (pattern : Pattern) (text : List Char) : List Captures :=
  matchesFrom pattern text (text.length + 2) 0 none

/-- The UTF-8 byte offset of a character index. -/
def byteOffset (text : List Char) (index : Nat) : Nat :=
  ((text.take index).map fun char => (String.utf8EncodeChar char).length).foldl (· + ·) 0

def slice (text : List Char) : Option (Nat × Nat) → List Char
  | some (first, finish) => (text.drop first).take (finish - first)
  | none => []

/-- `replace_all`'s replacement text for one match: `$$` is `$`, `$name` or
`${name}` the group it names (a number naming a group by index), empty when
the group did not take part, and a `$` before anything else stays. -/
def expand (pattern : Pattern) (text : List Char) (held : Captures) : Nat → List Char → List Char
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, '$' :: '$' :: rest => '$' :: expand pattern text held fuel rest
  | fuel + 1, '$' :: '{' :: rest =>
    let label := rest.takeWhile (· ≠ '}')
    match rest.drop label.length with
    | '}' :: after =>
      if label.isEmpty then '$' :: expand pattern text held fuel ('{' :: rest)
      else groupText pattern text held label ++ expand pattern text held fuel after
    | _ => '$' :: expand pattern text held fuel ('{' :: rest)
  | fuel + 1, '$' :: rest =>
    let label := rest.takeWhile isWordChar
    if label.isEmpty then '$' :: expand pattern text held fuel rest
    else groupText pattern text held label ++ expand pattern text held fuel (rest.drop label.length)
  | fuel + 1, char :: rest => char :: expand pattern text held fuel rest
where
  groupText (pattern : Pattern) (text : List Char) (held : Captures) (label : List Char) : List Char :=
    let index := if label.all Char.isDigit then some (label.foldl (fun value digit => value * 10 + (digit.toNat - 48)) 0)
      else pattern.names.lookup (String.ofList label)
    match index with
    | some position => slice text (held[position]?.join)
    | none => []

/-- `replace_all`: each match replaced by its expansion. -/
def replaceAll (pattern : Pattern) (text : List Char) (replacement : List Char) : List Char :=
  let (out, last) := (allMatches pattern text).foldl (fun (out, last) held =>
    match held.head? with
    | some (some (first, finish)) =>
      (out ++ (text.drop last).take (first - last) ++ expand pattern text held (replacement.length + 1) replacement, finish)
    | _ => (out, last)) ([], 0)
  out ++ text.drop last

end Xsh.Values.Regex
