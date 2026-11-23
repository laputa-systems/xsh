namespace Xsh.Values.Regex

/-- A regex in the subset the model reads: the Rust regex surface's literals,
`.`, bracket and ASCII Perl classes, `^`, `$`, `\b`, groups, alternation and
greedy or lazy repetition. -/
inductive Node where
  | literal (char : Char)
  | any
  | ranges (negated : Bool) (spans : List (Nat × Nat))
  | textStart
  | textEnd
  | wordBoundary (negated : Bool)
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

def isMeta (char : Char) : Bool := "\\.+*?()|[]{}^$-#&~ ".toList.contains char

/-- A Perl class escape: its spans and whether it is negated. -/
def perlClass : Char → Option (Bool × List (Nat × Nat))
  | 'd' => some (false, digitSpans)
  | 'D' => some (true, digitSpans)
  | 'w' => some (false, wordSpans)
  | 'W' => some (true, wordSpans)
  | 's' => some (false, spaceSpans)
  | 'S' => some (true, spaceSpans)
  | _ => none

/-- The character a simple escape stands for. -/
def escapedChar : Char → Option Char
  | 'n' => some '\n'
  | 't' => some '\t'
  | 'r' => some '\r'
  | char => if isMeta char then some char else none

/-- A bracket class's items up to its closing `]`, and the rest. -/
def classItems : Nat → List Char → Bool → Option (List (Nat × Nat) × List Char)
  | 0, _, _ => none
  | _ + 1, [], _ => none
  | _ + 1, ']' :: rest, false => some ([], rest)
  | fuel + 1, '\\' :: escape :: rest, _ =>
    match perlClass escape with
    | some (false, spans) => (classItems fuel rest false).map fun (items, after) => (spans ++ items, after)
    | some (true, _) => none
    | none =>
      (escapedChar escape).bind fun char =>
        (classItems fuel rest false).map fun (items, after) => ((char.toNat, char.toNat) :: items, after)
  | fuel + 1, low :: '-' :: high :: rest, _ =>
    if high = ']' then
      (classItems fuel (']' :: rest) false).map fun (items, after) =>
        ((low.toNat, low.toNat) :: (45, 45) :: items, after)
    else if low.toNat ≤ high.toNat then
      (classItems fuel rest false).map fun (items, after) => ((low.toNat, high.toNat) :: items, after)
    else none
  | fuel + 1, char :: rest, _ =>
    (classItems fuel rest false).map fun (items, after) => ((char.toNat, char.toNat) :: items, after)

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

mutual
  /-- Alternatives separated by `|`, up to a `)` or the end. -/
  def parseAlt : Nat → List Char → Nat → List (String × Nat) →
      Option (Node × List Char × Nat × List (String × Nat))
    | 0, _, _, _ => none
    | fuel + 1, text, groups, names =>
      (parseConcat fuel text groups names []).bind fun (first, rest, groups, names) =>
        match rest with
        | '|' :: more =>
          (parseAlt fuel more groups names).bind fun (others, after, groups, names) =>
            match others with
            | .alt branches => some (.alt (first :: branches), after, groups, names)
            | other => some (.alt [first, other], after, groups, names)
        | _ => some (first, rest, groups, names)

  /-- A sequence of quantified atoms. -/
  def parseConcat : Nat → List Char → Nat → List (String × Nat) → List Node →
      Option (Node × List Char × Nat × List (String × Nat))
    | 0, _, _, _, _ => none
    | _ + 1, [], groups, names, done => some (.concat done.reverse, [], groups, names)
    | _ + 1, '|' :: rest, groups, names, done => some (.concat done.reverse, '|' :: rest, groups, names)
    | _ + 1, ')' :: rest, groups, names, done => some (.concat done.reverse, ')' :: rest, groups, names)
    | fuel + 1, text, groups, names, done =>
      (parseAtom fuel text groups names).bind fun (atom, rest, groups, names) =>
        match quantifier rest with
        | some ((least, most), '?' :: after) =>
          parseConcat fuel after groups names (.repeat atom least most false :: done)
        | some ((least, most), after) => parseConcat fuel after groups names (.repeat atom least most true :: done)
        | none => parseConcat fuel rest groups names (atom :: done)

  /-- One atom: a group, a class, `.`, an anchor, an escape or a literal. -/
  def parseAtom : Nat → List Char → Nat → List (String × Nat) →
      Option (Node × List Char × Nat × List (String × Nat))
    | 0, _, _, _ => none
    | fuel + 1, text, groups, names =>
      let closed := fun (result : Option (Node × List Char × Nat × List (String × Nat))) (wrap : Node → Node) =>
        result.bind fun (inner, rest, groups, names) =>
          match rest with
          | ')' :: after => some (wrap inner, after, groups, names)
          | _ => none
      match text with
      | '(' :: '?' :: ':' :: rest => closed (parseAlt fuel rest groups names) (.group none)
      | '(' :: '?' :: 'P' :: '<' :: rest | '(' :: '?' :: '<' :: rest =>
        let label := rest.takeWhile (· ≠ '>')
        if label.isEmpty || !(label.all fun char => isWordChar char) then none
        else
          match rest.drop label.length with
          | '>' :: body =>
            closed (parseAlt fuel body (groups + 1) ((String.ofList label, groups + 1) :: names)) (.group (some (groups + 1)))
          | _ => none
      | '(' :: '?' :: _ => none
      | '(' :: rest => closed (parseAlt fuel rest (groups + 1) names) (.group (some (groups + 1)))
      | '[' :: '^' :: rest => (classItems (rest.length + 1) rest true).map fun (spans, after) => (.ranges true spans, after, groups, names)
      | '[' :: rest => (classItems (rest.length + 1) rest true).map fun (spans, after) => (.ranges false spans, after, groups, names)
      | '.' :: rest => some (.any, rest, groups, names)
      | '^' :: rest => some (.textStart, rest, groups, names)
      | '$' :: rest => some (.textEnd, rest, groups, names)
      | '\\' :: 'b' :: rest => some (.wordBoundary false, rest, groups, names)
      | '\\' :: 'B' :: rest => some (.wordBoundary true, rest, groups, names)
      | '\\' :: escape :: rest =>
        match perlClass escape with
        | some (negated, spans) => some (.ranges negated spans, rest, groups, names)
        | none => (escapedChar escape).map fun char => (.literal char, rest, groups, names)
      | char :: rest =>
        if "*+?{)|]".toList.contains char then none else some (.literal char, rest, groups, names)
      | [] => none
end

/-- The pattern a regex text spells, `none` outside the subset. -/
def parse (source : String) : Option Pattern :=
  (parseAlt (4 * source.length + 4) source.toList 0 []).bind fun (node, rest, groups, names) =>
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
    | .any =>
      match text[position]? with
      | some char => if char = '\n' then none else next (position + 1) captures
      | none => none
    | .ranges negated spans =>
      match text[position]? with
      | some char => if inSpans spans char.toNat != negated then next (position + 1) captures else none
      | none => none
    | .textStart => if position = 0 then next position captures else none
    | .textEnd => if position = text.length then next position captures else none
    | .wordBoundary negated =>
      let before := (position > 0) && ((text[position - 1]?).map isWordChar).getD false
      let after := ((text[position]?).map isWordChar).getD false
      if (before != after) != negated then next position captures else none
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
