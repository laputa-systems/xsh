#!/bin/xsh
use lib.gnu

const USAGE = """Usage: expr EXPRESSION
  or:  expr OPTION

      --help        display this help and exit
      --version     output version information and exit

Print the value of EXPRESSION to standard output.  A blank line below
separates increasing precedence groups.  EXPRESSION may be:

  ARG1 | ARG2       ARG1 if it is neither null nor 0, otherwise ARG2

  ARG1 & ARG2       ARG1 if neither argument is null or 0, otherwise 0

  ARG1 < ARG2       ARG1 is less than ARG2
  ARG1 <= ARG2      ARG1 is less than or equal to ARG2
  ARG1 = ARG2       ARG1 is equal to ARG2
  ARG1 != ARG2      ARG1 is unequal to ARG2
  ARG1 >= ARG2      ARG1 is greater than or equal to ARG2
  ARG1 > ARG2       ARG1 is greater than ARG2

  ARG1 + ARG2       arithmetic sum of ARG1 and ARG2
  ARG1 - ARG2       arithmetic difference of ARG1 and ARG2

  ARG1 * ARG2       arithmetic product of ARG1 and ARG2
  ARG1 / ARG2       arithmetic quotient of ARG1 divided by ARG2
  ARG1 % ARG2       arithmetic remainder of ARG1 divided by ARG2

  STRING : REGEXP   anchored pattern match of REGEXP in STRING

  match STRING REGEXP        same as STRING : REGEXP
  substr STRING POS LENGTH   substring of STRING, POS counted from 1
  index STRING CHARS         index in STRING where any CHARS is found, or 0
  length STRING              length of STRING
  + TOKEN                    interpret TOKEN as a string, even if it is a
                               keyword like 'match' or an operator like '/'

  ( EXPRESSION )             value of EXPRESSION

Beware that many operators need to be escaped or quoted for shells.
Comparisons are arithmetic if both ARGs are numbers, else lexicographical.
Pattern matches return the string matched between \\( and \\) or null; if
\\( and \\) are not used, they return the number of characters matched or 0.

Exit status is 0 if EXPRESSION is neither null nor 0, 1 if EXPRESSION is null
or 0, 2 if EXPRESSION is syntactically invalid, and 3 if an error occurred.
"""

# Magnitudes are little-endian base 10^9 limbs without high zero limbs; zero
# is the empty list.
const BASE = 1000000000

# Largest repeat count of a regular expression interval.
const DUP_MAX = 32767

const WORD_RANGES = [[48, 57], [65, 90], [95, 95], [97, 122], [128, 1114111]]
const SPACE_RANGES = [[9, 13], [32, 32]]

const PRECEDENCE: Map[Int] = {
  "|": 1,
  "&": 2,
  "<": 3,
  "<=": 3,
  "=": 3,
  "==": 3,
  "!=": 3,
  ">=": 3,
  ">": 3,
  "+": 4,
  "-": 4,
  "*": 5,
  "/": 5,
  "%": 5,
  ":": 6,
}

const ARITY: Map[Int] = {length: 1, match: 2, index: 2, substr: 3}

# One regular expression instruction. Jumps are relative to the instruction.
# CHAR a; ANY; SET a (index into the sets); SPLIT a b (try a first); JMP a;
# SAVE a; BOL; EOL; BACKREF a; MATCH.
type Instr = {op: Int, a: Int, b: Int}

# A bracket expression: `lows[i]..=highs[i]` ranges, optionally negated.
type CharSet = {negated: Bool, lows: List[Int], highs: List[Int]}

# The pattern decoded to units (bytes in a single-byte locale, characters in
# UTF-8) with each unit's byte offset (one extra entry for the end).
type Units = {units: List[Int], offsets: List[Int]}

type Parsed = {code: List[Instr], at: Int, groups: Int, done: List[Int], sets: List[CharSet], refs: Bool, failure: Str}

type Matched = {found: Bool, end: Int, caps: List[Int]}

pure trim_limbs(parts: List[Int]) -> List[Int] {
  var end = parts.len()

  while end > 0 and parts[end - 1] == 0 {
    end -= 1
  }

  if end == parts.len() { parts } else { parts[..end] }
}

pure big_from(text: Str) -> List[Int] {
  var end = text.byte_len()

  let out: List[Int] = collect {
    while end > 0 {
      let start = if end > 9 { end - 9 } else { 0 }
      yield text.byte_slice(start, length: end - start).parse_int() ?? 0
      end = start
    }
  }

  trim_limbs(out)
}

pure big_text(parts: List[Int]) -> Str {
  return "0" when parts.is_empty()

  var out = f"{parts[-1]}"
  var at = parts.len() - 2

  while at >= 0 {
    let piece = f"{parts[at]}"
    out = out + "000000000".byte_slice(0, length: 9 - piece.byte_len()) + piece
    at -= 1
  }

  out
}

pure big_cmp(a: List[Int], b: List[Int]) -> Int {
  return -1 when a.len() < b.len()
  return 1 when a.len() > b.len()

  var at = a.len() - 1

  while at >= 0 {
    return -1 when a[at] < b[at]
    return 1 when a[at] > b[at]

    at -= 1
  }

  0
}

pure big_add(a: List[Int], b: List[Int]) -> List[Int] {
  var carry = 0
  var at = 0
  let count = if a.len() > b.len() { a.len() } else { b.len() }

  let out: List[Int] = collect {
    while at < count or carry > 0 {
      let sum = (if at < a.len() { a[at] } else { 0 }) + (if at < b.len() { b[at] } else { 0 }) + carry
      yield sum % BASE
      carry = sum / BASE
      at += 1
    }
  }

  out
}

# A - B for A >= B.
pure big_sub(a: List[Int], b: List[Int]) -> List[Int] {
  var borrow = 0
  var at = 0

  let out: List[Int] = collect {
    while at < a.len() {
      var gap = a[at] - (if at < b.len() { b[at] } else { 0 }) - borrow

      if gap < 0 {
        gap += BASE
        borrow = 1
      } else {
        borrow = 0
      }

      yield gap
      at += 1
    }
  }

  trim_limbs(out)
}

pure big_mul_small(a: List[Int], factor: Int) -> List[Int] {
  return [] when factor == 0 or a.is_empty()

  var carry = 0

  let out: List[Int] = collect {
    for part in a {
      let product = part * factor + carry
      yield product % BASE
      carry = product / BASE
    }

    while carry > 0 {
      yield carry % BASE
      carry = carry / BASE
    }
  }

  out
}

pure big_mul(a: List[Int], b: List[Int]) -> List[Int] {
  return [] when a.is_empty() or b.is_empty()

  var out: List[Int] = [0 for slot in range(a.len() + b.len())]

  for i in range(a.len()) {
    var carry = 0
    let factor = a[i]

    if factor != 0 {
      for j in range(b.len()) {
        let current = out[i + j] + factor * b[j] + carry
        out[i + j] = current % BASE
        carry = current / BASE
      }

      out[i + b.len()] += carry
    }
  }

  trim_limbs(out)
}

type Division = {quotient: List[Int], remainder: List[Int]}

# A / B and A % B for nonzero B.
pure big_divmod(a: List[Int], b: List[Int]) -> Division {
  return {quotient: [], remainder: a} when big_cmp(a, b) < 0

  if b.len() == 1 {
    var quotient: List[Int] = [0 for slot in range(a.len())]
    var rest = 0
    var at = a.len() - 1

    while at >= 0 {
      let current = rest * BASE + a[at]
      quotient[at] = current / b[0]
      rest = current % b[0]
      at -= 1
    }

    return {quotient: trim_limbs(quotient), remainder: if rest == 0 { [] } else { [rest] }}
  }

  let width = b.len()
  let divisor_top = b[width - 1].float() * 1000000000.0 + b[width - 2].float()
  var quotient: List[Int] = [0 for slot in range(a.len())]
  var rest: List[Int] = []
  var at = a.len() - 1

  while at >= 0 {
    rest = trim_limbs([a[at], @rest])

    if big_cmp(rest, b) >= 0 {
      let top = if rest.len() == width {
        rest[width - 1].float() * 1000000000.0 + rest[width - 2].float()
      } else {
        (rest[width].float() * 1000000000.0 + rest[width - 1].float()) * 1000000000.0 + rest[width - 2].float()
      }
      var guess = (top / divisor_top).floor() ?? 0

      guess = if guess >= BASE { BASE - 1 } else { guess }

      var product = big_mul_small(b, guess)

      while big_cmp(product, rest) > 0 {
        guess -= 1
        product = big_sub(product, b)
      }

      var left = big_sub(rest, product)

      while big_cmp(left, b) >= 0 {
        guess += 1
        left = big_sub(left, b)
      }

      quotient[at] = guess
      rest = left
    }

    at -= 1
  }

  {quotient: trim_limbs(quotient), remainder: rest}
}

# A signed integer: the sign is kept apart so zero is never negative.
type Whole = {neg: Bool, mag: List[Int]}

pure whole_from(text: Str) -> Whole {
  let neg = text.starts_with("-")
  let mag = big_from(if neg { text.byte_slice(1) } else { text })

  {neg: neg and ! mag.is_empty(), mag: mag}
}

pure whole_text(value: Whole) -> Str {
  if value.neg { f"-{big_text(value.mag)}" } else { big_text(value.mag) }
}

pure whole_cmp(a: Whole, b: Whole) -> Int {
  return -1 when a.neg and ! b.neg
  return 1 when b.neg and ! a.neg

  let order = big_cmp(a.mag, b.mag)

  if a.neg { -order } else { order }
}

pure whole_add(a: Whole, b: Whole) -> Whole {
  return {neg: a.neg, mag: big_add(a.mag, b.mag)} when a.neg == b.neg

  let order = big_cmp(a.mag, b.mag)

  return {neg: false, mag: []} when order == 0
  return {neg: a.neg, mag: big_sub(a.mag, b.mag)} when order > 0

  {neg: b.neg, mag: big_sub(b.mag, a.mag)}
}

pure whole_negate(a: Whole) -> Whole {
  {neg: ! a.neg and ! a.mag.is_empty(), mag: a.mag}
}

pure looks_like_integer(text: Str) -> Bool {
  rx"^-?[0-9]+$".matches(text)
}

# Whether the value is null: empty, or an integer spelled as zeros.
pure is_null(text: Str) -> Bool {
  text == "" or rx"^-?0+$".matches(text)
}

pure continuation(data: Bytes, at: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= 128 and byte < 192
}

pure within(data: Bytes, at: Int, low: Int, high: Int) -> Bool {
  let byte = data.byte_at(at) ?? 0
  byte >= low and byte <= high
}

# Length of the valid UTF-8 sequence starting at `at`, or 0.
pure sequence_width(data: Bytes, at: Int) -> Int {
  let lead = data.byte_at(at) ?? 0

  return 1 when lead < 128
  return 2 when lead >= 194 and lead <= 223 and continuation(data, at + 1)
  return 3 when lead == 224 and within(data, at + 1, 160, 191) and continuation(data, at + 2)
  return 3 when ((lead >= 225 and lead <= 236) or lead == 238 or lead == 239) and continuation(data, at + 1) and continuation(
    data,
    at + 2,
  )
  return 3 when lead == 237 and within(data, at + 1, 128, 159) and continuation(data, at + 2)
  return 4 when lead == 240 and within(data, at + 1, 144, 191) and continuation(data, at + 2) and continuation(
    data,
    at + 3,
  )
  return 4 when lead >= 241 and lead <= 243 and continuation(data, at + 1) and continuation(data, at + 2) and continuation(
    data,
    at + 3,
  )
  return 4 when lead == 244 and within(data, at + 1, 128, 143) and continuation(data, at + 2) and continuation(
    data,
    at + 3,
  )

  0
}

# Units of `data`: bytes in a single-byte locale; in UTF-8 each character's
# code point, and 0x110000 plus the byte for a byte that is not part of one.
pure decode_units(data: Bytes, utf8: Bool) -> Units {
  var units: List[Int] = []
  var offsets: List[Int] = []
  var at = 0
  let total = data.len()

  while at < total {
    let lead = data.byte_at(at) ?? 0
    let width = if utf8 { sequence_width(data, at) } else { 1 }

    offsets += [at]

    if width == 0 {
      units += [1114112 + lead]
      at += 1
    } else if width == 1 {
      units += [lead]
      at += 1
    } else if width == 2 {
      units += [(lead - 192) * 64 + (data.byte_at(at + 1) ?? 128) - 128]
      at += 2
    } else if width == 3 {
      units += [(lead - 224) * 4096 + ((data.byte_at(at + 1) ?? 128) - 128) * 64 + (data.byte_at(at + 2) ?? 128) - 128]
      at += 3
    } else {
      units += [
        (lead - 240) * 262144 + ((data.byte_at(at + 1) ?? 128) - 128) * 4096 + ((data.byte_at(at + 2) ?? 128) - 128) * 64 + (data.byte_at(
          at + 3,
        ) ?? 128) - 128,
      ]
      at += 4
    }
  }

  offsets += [total]

  {units: units, offsets: offsets}
}

pure range_set(negated: Bool, lows: List[Int], highs: List[Int]) -> CharSet {
  {negated: negated, lows: lows, highs: highs}
}

pure fail_with(state: Parsed, message: Str) -> Parsed {
  {...state, failure: message}
}

# The ranges of a named character class, or null for an unknown name.
pure class_ranges(name: Str) -> List[List[Int]]? {
  if name == "alpha" {
    return [[65, 90], [97, 122], [128, 1114111]]
  } else if name == "digit" {
    return [[48, 57]]
  } else if name == "alnum" {
    return [[48, 57], [65, 90], [97, 122], [128, 1114111]]
  } else if name == "upper" {
    return [[65, 90]]
  } else if name == "lower" {
    return [[97, 122]]
  } else if name == "space" {
    return [[9, 13], [32, 32]]
  } else if name == "blank" {
    return [[9, 9], [32, 32]]
  } else if name == "punct" {
    return [[33, 47], [58, 64], [91, 96], [123, 126]]
  } else if name == "print" {
    return [[32, 126], [128, 1114111]]
  } else if name == "graph" {
    return [[33, 126], [128, 1114111]]
  } else if name == "cntrl" {
    return [[0, 31], [127, 127]]
  } else if name == "xdigit" {
    return [[48, 57], [65, 70], [97, 102]]
  }

  null
}

# A bracket expression; `state.at` is just past the `[`. The set is left in
# `state.sets` (last element).
pure parse_bracket(pat: List[Int], state: Parsed) -> Parsed {
  let unmatched = "Unmatched [, [^, [:, [., or [="
  var at = state.at
  var negated = false
  var lows: List[Int] = []
  var highs: List[Int] = []

  if at < pat.len() and pat[at] == 94 {
    negated = true
    at += 1
  }

  var first = true

  loop {
    return fail_with(state, unmatched) when at >= pat.len()

    let c = pat[at]

    break when c == 93 and ! first

    first = false

    if c == 91 and at + 1 < pat.len() and (pat[at + 1] == 58 or pat[at + 1] == 61 or pat[at + 1] == 46) {
      let kind = pat[at + 1]
      var close = at + 2

      while close + 1 < pat.len() and ! (pat[close] == kind and pat[close + 1] == 93) {
        close += 1
      }

      return fail_with(state, unmatched) when close + 1 >= pat.len()

      let body = pat[at + 2..close]

      if kind == 58 {
        let name = bytes.from_ints([unit for unit in body if unit < 128]) ?? b""
        let known = class_ranges(name.utf8() ?? "")

        guard let ranges = known else {
          return fail_with(state, "Invalid character class name")
        }

        for pair in ranges {
          lows += [pair[0]]
          highs += [pair[1]]
        }
      } else {
        return fail_with(state, "Invalid collation character") when body.len() != 1

        lows += [body[0]]
        highs += [body[0]]
      }

      at = close + 2
    } else {
      var low = c
      var high = c
      at += 1

      if at + 1 < pat.len() and pat[at] == 45 and pat[at + 1] != 93 {
        high = pat[at + 1]
        at += 2

        return fail_with(state, "Invalid range end") when high < low
      }

      lows += [low]
      highs += [high]
    }
  }

  {...state, at: at + 1, sets: state.sets + [range_set(negated, lows, highs)]}
}

pure ranges_set(negated: Bool, ranges: List[List[Int]]) -> CharSet {
  {negated: negated, lows: [pair[0] for pair in ranges], highs: [pair[1] for pair in ranges]}
}

pure parse_branch(pat: List[Int], state: Parsed, depth: Int) -> Parsed {
  var st = {...state, code: []}
  var code: List[Instr] = []
  var first = true

  loop {
    break when st.at >= pat.len()

    let c = pat[st.at]
    var atom: List[Instr] = []
    var anchor = false

    if c == 92 {
      return fail_with(st, "Trailing backslash") when st.at + 1 >= pat.len()

      let d = pat[st.at + 1]

      break when d == 124

      if d == 41 {
        return fail_with(st, "Unmatched ) or \\)") when depth == 0

        break
      }
    }

    if c == 92 and pat[st.at + 1] == 40 {
      let index = st.groups + 1
      let inner = parse_alt(pat, {...st, at: st.at + 2, groups: index}, depth + 1)

      return inner when inner.failure != ""
      return fail_with(inner, "Unmatched ( or \\(") when inner.at + 1 >= pat.len() or pat[inner.at] != 92 or pat[inner.at + 1] != 41

      atom = [{op: 5, a: 2 * index, b: 0}] + inner.code + [{op: 5, a: 2 * index + 1, b: 0}]
      st = {...inner, at: inner.at + 2, done: [@inner.done, index]}
    } else if c == 92 and pat[st.at + 1] >= 49 and pat[st.at + 1] <= 57 {
      let index = pat[st.at + 1] - 48

      return fail_with(st, "Invalid back reference") when ! (index in st.done)

      atom = [{op: 8, a: index, b: 0}]
      st = {...st, at: st.at + 2, refs: true}
    } else if c == 92 and (pat[st.at + 1] == 119 or pat[st.at + 1] == 87 or pat[st.at + 1] == 115 or pat[st.at + 1] == 83) {
      let kind = pat[st.at + 1]
      let word = kind == 119 or kind == 87
      let class = ranges_set(kind == 87 or kind == 83, if word { WORD_RANGES } else { SPACE_RANGES })

      atom = [{op: 2, a: st.sets.len(), b: 0}]
      st = {...st, at: st.at + 2, sets: st.sets + [class]}
    } else if c == 92 and (pat[st.at + 1] == 60 or pat[st.at + 1] == 62 or pat[st.at + 1] == 98 or pat[st.at + 1] == 66 or pat[st.at + 1] == 96 or pat[st.at + 1] == 39) {
      return fail_with(st, "unsupported regular expression operator")
    } else if c == 92 {
      atom = [{op: 0, a: pat[st.at + 1], b: 0}]
      st = {...st, at: st.at + 2}
    } else if c == 91 {
      let inner = parse_bracket(pat, {...st, at: st.at + 1})

      return inner when inner.failure != ""

      atom = [{op: 2, a: inner.sets.len() - 1, b: 0}]
      st = inner
    } else if c == 46 {
      atom = [{op: 1, a: 0, b: 0}]
      st = {...st, at: st.at + 1}
    } else if c == 94 and first {
      atom = [{op: 6, a: 0, b: 0}]
      anchor = true
      st = {...st, at: st.at + 1}
    } else if c == 36 and (st.at + 1 == pat.len() or (st.at + 2 < pat.len() and pat[st.at + 1] == 92 and (pat[st.at + 2] == 41 or pat[st.at + 2] == 124))) {
      atom = [{op: 7, a: 0, b: 0}]
      anchor = true
      st = {...st, at: st.at + 1}
    } else {
      atom = [{op: 0, a: c, b: 0}]
      st = {...st, at: st.at + 1}
    }

    first = false

    # Repetition operators; an anchor takes none, so a following `*` is literal.
    while ! anchor and st.at < pat.len() {
      let r = pat[st.at]
      let next = if st.at + 1 < pat.len() { pat[st.at + 1] } else { -1 }
      let size = atom.len()
      var lo = 0
      var hi = -1
      var used = 0

      if r == 42 {
        used = 1
      } else if r == 92 and next == 43 {
        lo = 1
        used = 2
      } else if r == 92 and next == 63 {
        hi = 1
        used = 2
      } else if r == 92 and next == 123 {
        var close = st.at + 2

        while close + 1 < pat.len() and ! (pat[close] == 92 and pat[close + 1] == 125) {
          close += 1
        }

        return fail_with(st, "Unmatched \\{") when close + 1 >= pat.len()

        let body = bytes.from_ints([unit for unit in pat[st.at + 2..close] if unit < 128]) ?? b""
        let text = body.utf8() ?? ""
        let parts = rx"^([0-9]*)(,?)([0-9]*)$".captures(text)

        return fail_with(st, "Invalid content of \\{\\}") when parts.is_empty() or text == ""
        return fail_with(st, "Regular expression too big") when parts[1].byte_len() > 5 or parts[3].byte_len() > 5

        let low = if parts[1] == "" { 0 } else { parts[1].parse_int() ?? 0 }
        let high = if parts[2] == "" { low } else if parts[3] == "" { -1 } else { parts[3].parse_int() ?? 0 }

        return fail_with(st, "Regular expression too big") when low > DUP_MAX or high > DUP_MAX
        return fail_with(st, "Invalid content of \\{\\}") when high >= 0 and low > high

        lo = low
        hi = high
        used = close + 2 - st.at
      } else {
        break
      }

      return fail_with(st, "Regular expression too big") when size * (if hi > lo { hi } else if lo > 0 { lo } else { 1 }) > 200000

      var expanded: List[Instr] = []

      repeat lo times {
        expanded += atom
      }

      if hi < 0 {
        if lo > 0 {
          expanded = expanded[..expanded.len() - size] + atom + [{op: 3, a: -size, b: 1}]
        } else {
          expanded = [{op: 3, a: 1, b: size + 2}] + atom + [{op: 4, a: -(size + 1), b: 0}]
        }
      } else {
        var tail: List[Instr] = []

        repeat hi - lo times {
          tail = [{op: 3, a: 1, b: size + tail.len() + 1}] + atom + tail
        }

        expanded += tail
      }

      atom = expanded
      st = {...st, at: st.at + used}
    }

    code += atom
  }

  {...st, code: code}
}

pure parse_alt(pat: List[Int], state: Parsed, depth: Int) -> Parsed {
  var st = state
  let branches: List[List[Instr]] = collect {
    loop {
      st = parse_branch(pat, st, depth)

      return st when st.failure != ""

      yield st.code

      if st.at + 1 < pat.len() and pat[st.at] == 92 and pat[st.at + 1] == 124 {
        st = {...st, at: st.at + 2}
      } else {
        break
      }
    }
  }

  var joined = branches[-1]
  var index = branches.len() - 2

  while index >= 0 {
    let branch = branches[index]
    joined = [{op: 3, a: 1, b: branch.len() + 2}] + branch + [{op: 4, a: joined.len() + 1, b: 0}] + joined
    index -= 1
  }

  {...st, code: joined}
}

pure in_set(class: CharSet, unit: Int) -> Bool {
  var hit = false
  var at = 0

  while at < class.lows.len() {
    if unit >= class.lows[at] and unit <= class.highs[at] {
      hit = true
      break
    }

    at += 1
  }

  hit != class.negated
}

# The longest match of the program at the start of `units`, trying paths in
# priority order so the first path to reach the longest end wins.
pure run_program(
  code: List[Instr],
  sets: List[CharSet],
  units: List[Int],
  refs: Bool,
  caps_size: Int,
  utf8: Bool,
) -> Matched {
  let total = units.len()
  let ops = [item.op for item in code]
  let first = [item.a for item in code]
  let second = [item.b for item in code]
  var best = -1
  var best_caps: List[Int] = []
  var seen: Set[Str] = set.empty()
  var stack_pc: List[Int] = [0]
  var stack_pos: List[Int] = [0]
  var stack_caps: List[List[Int]] = [[-1 for slot in range(caps_size)]]
  var top = 1

  while top > 0 {
    top -= 1

    var pc = stack_pc[top]
    var pos = stack_pos[top]
    var caps = stack_caps[top]

    loop {
      let key = if refs { f"{pc}:{pos}:{[f"{value}" for value in caps].join(",")}" } else { f"{pc}:{pos}" }

      break when key in seen

      seen = seen.add(key)

      let op = ops[pc]

      if op == 0 {
        break when pos >= total or units[pos] != first[pc]

        pc += 1
        pos += 1
      } else if op == 1 {
        break when pos >= total or (utf8 and units[pos] >= 1114112)

        pc += 1
        pos += 1
      } else if op == 2 {
        break when pos >= total or ! in_set(sets[first[pc]], units[pos])

        pc += 1
        pos += 1
      } else if op == 3 {
        if top < stack_pc.len() {
          stack_pc[top] = pc + second[pc]
          stack_pos[top] = pos
          stack_caps[top] = caps
        } else {
          stack_pc += [pc + second[pc]]
          stack_pos += [pos]
          stack_caps += [caps]
        }

        top += 1
        pc += first[pc]
      } else if op == 4 {
        pc += first[pc]
      } else if op == 5 {
        caps[first[pc]] = pos
        pc += 1
      } else if op == 6 {
        break when pos != 0

        pc += 1
      } else if op == 7 {
        break when pos != total

        pc += 1
      } else if op == 8 {
        let begin = caps[2 * first[pc]]
        let finish = caps[2 * first[pc] + 1]

        break when begin < 0 or finish < 0

        let size = finish - begin

        break when pos + size > total or units[begin..finish] != units[pos..pos + size]

        pos += size
        pc += 1
      } else {
        if pos > best {
          best = pos
          best_caps = caps
        }

        break
      }
    }

    if best == total {
      top = 0
    }
  }

  {found: best >= 0, end: best, caps: best_caps}
}

# How strings are measured and ordered, from the locale variables.
type Locale = {utf8: Bool, collate_c: Bool}

# A computed value, or the message of the error that computing it raised.
# Errors travel with the values so that a branch `|` or `&` never evaluates
# cannot fail, and so a syntax error anywhere outranks every evaluation error.
type Outcome = {value: Bytes, error: Str}

proc locale_variable(category: Str) [env] -> Str {
  for name in ["LC_ALL", category, "LANG"] {
    let found = env.get_or(name, "") ?? ""

    return found when found != ""
  }

  ""
}

proc locale() [env] -> Locale {
  let ctype = locale_variable("LC_CTYPE").lower()
  let collate = locale_variable("LC_COLLATE")

  {
    utf8: ctype.find("utf-8") != null or ctype.find("utf8") != null,
    collate_c: collate == "" or collate == "C" or collate == "POSIX" or collate.starts_with("C."),
  }
}

pure text_of(value: Bytes) -> Str {
  value.utf8() ?? "�"
}

pure value_of(text: Str) -> Bytes {
  bytes.from_text(text)
}

pure ok(text: Str) -> Outcome {
  {value: bytes.from_text(text), error: ""}
}

pure failed(message: Str) -> Outcome {
  {value: b"", error: message}
}

pure integer_of(value: Bytes) -> Whole? {
  let text = text_of(value)

  return null when ! looks_like_integer(text)

  whole_from(text)
}

pure arithmetic(op: Str, left: Bytes, right: Bytes) -> Outcome {
  guard let a = integer_of(left) else {
    return failed("non-integer argument")
  }

  guard let b = integer_of(right) else {
    return failed("non-integer argument")
  }

  if op == "+" {
    return ok(whole_text(whole_add(a, b)))
  } else if op == "-" {
    return ok(whole_text(whole_add(a, whole_negate(b))))
  } else if op == "*" {
    return ok(
      whole_text({neg: a.neg != b.neg and ! a.mag.is_empty() and ! b.mag.is_empty(), mag: big_mul(a.mag, b.mag)}),
    )
  }

  return failed("division by zero") when b.mag.is_empty()

  let division = big_divmod(a.mag, b.mag)

  if op == "/" {
    return ok(whole_text({neg: a.neg != b.neg and ! division.quotient.is_empty(), mag: division.quotient}))
  }

  ok(whole_text({neg: a.neg and ! division.remainder.is_empty(), mag: division.remainder}))
}

pure alphanumeric_key(text: Str) -> Str {
  rx"[^0-9A-Za-z]".replace(text, with: "").lower()
}

# -1, 0, or 1 for the order of two strings; outside the C locale punctuation
# is ignored first, as the usual collations do.
pure string_order(left: Bytes, right: Bytes, plain: Bool) -> Int {
  if ! plain {
    if let [Ok(a), Ok(b)] = [left.utf8(), right.utf8()] {
      let ka = alphanumeric_key(a)
      let kb = alphanumeric_key(b)

      return -1 when ka < kb
      return 1 when ka > kb
    }
  }

  let order = left.compare(right)

  return 0 when order.equal
  return -1 when order.left < order.right

  1
}

pure comparison(op: Str, left: Bytes, right: Bytes, plain: Bool) -> Bytes {
  let l = text_of(left)
  let r = text_of(right)
  let order = if looks_like_integer(l) and looks_like_integer(r) {
    whole_cmp(whole_from(l), whole_from(r))
  } else {
    string_order(left, right, plain)
  }
  let holds = if op == "<" {
    order < 0
  } else if op == "<=" {
    order <= 0
  } else if op == "=" or op == "==" {
    order == 0
  } else if op == "!=" {
    order != 0
  } else if op == ">=" {
    order >= 0
  } else {
    order > 0
  }

  value_of(if holds { "1" } else { "0" })
}

pure length_of(value: Bytes, utf8: Bool) -> Bytes {
  return value_of(f"{value.len()}") when ! utf8

  if let Ok(text) = value.utf8() {
    value_of(f"{text.count_chars()}")
  } else {
    value_of(f"{decode_units(value, true).units.len()}")
  }
}

pure index_of(value: Bytes, chars: Bytes, utf8: Bool) -> Bytes {
  let haystack = decode_units(value, utf8).units
  let wanted = decode_units(chars, utf8).units

  for position in range(haystack.len()) {
    return value_of(f"{position + 1}") when haystack[position] in wanted
  }

  value_of("0")
}

# A whole number clamped to what fits an Int.
pure clamped(value: Whole) -> Int {
  return if value.neg { -9223372036854775807 } else { 9223372036854775807 } when value.mag.len() > 2

  let magnitude = (if ! value.mag.is_empty() { value.mag[0] } else { 0 }) + (if value.mag.len() > 1 { value.mag[1] * BASE } else { 0 })

  if value.neg { -magnitude } else { magnitude }
}

pure substring(value: Bytes, from: Bytes, length: Bytes, utf8: Bool) -> Outcome {
  guard let first = integer_of(from) else {
    return failed("non-integer argument")
  }

  guard let size = integer_of(length) else {
    return failed("non-integer argument")
  }

  let start = clamped(first)
  let count = clamped(size)

  return ok("") when start <= 0 or count <= 0

  let pieces = decode_units(value, utf8)
  let total = pieces.units.len()

  return ok("") when start > total

  let stop = if count > total - start + 1 { total } else { start - 1 + count }

  {value: value[pieces.offsets[start - 1]..pieces.offsets[stop]], error: ""}
}

pure match_value(subject: Bytes, pattern: Bytes, utf8: Bool) -> Outcome {
  let pat = decode_units(pattern, utf8).units
  let compiled = parse_alt(pat, {code: [], at: 0, groups: 0, done: [], sets: [], refs: false, failure: ""}, 0)

  return failed(compiled.failure) when compiled.failure != ""
  return failed("Unmatched ) or \\)") when compiled.at < pat.len()

  let pieces = decode_units(subject, utf8)
  let program = compiled.code + [{op: 9, a: 0, b: 0}]
  let result = run_program(program, compiled.sets, pieces.units, compiled.refs, 2 * compiled.groups + 2, utf8)

  return ok(if result.found { f"{result.end}" } else { "0" }) when compiled.groups == 0

  return ok("") when ! result.found or result.caps[2] < 0 or result.caps[3] < 0

  {value: subject[pieces.offsets[result.caps[2]]..pieces.offsets[result.caps[3]]], error: ""}
}

# The value of `op` applied to two computed operands.
pure apply_operator(op: Str, left: Outcome, right: Outcome, here: Locale) -> Outcome {
  let left_null = is_null(text_of(left.value))

  if op == "|" {
    return left when left.error != "" or ! left_null
    return right when right.error != ""

    return ok("0") when is_null(text_of(right.value))

    return right
  }

  if op == "&" {
    return left when left.error != ""
    return ok("0") when left_null
    return right when right.error != ""
    return ok("0") when is_null(text_of(right.value))

    return left
  }

  return left when left.error != ""
  return right when right.error != ""
  return match_value(left.value, right.value, here.utf8) when op == ":"

  if (PRECEDENCE.get(op) ?? 0) == 3 {
    return {value: comparison(op, left.value, right.value, here.collate_c), error: ""}
  }

  arithmetic(op, left.value, right.value)
}

# Evaluate a prefix function over its collected operands.
pure apply_function(name: Str, operands: List[Outcome], here: Locale) -> Outcome {
  for operand in operands {
    return operand when operand.error != ""
  }

  if name == "length" {
    return {value: length_of(operands[0].value, here.utf8), error: ""}
  } else if name == "match" {
    return match_value(operands[0].value, operands[1].value, here.utf8)
  } else if name == "index" {
    return {value: index_of(operands[0].value, operands[1].value, here.utf8), error: ""}
  }

  substring(operands[0].value, operands[1].value, operands[2].value, here.utf8)
}

proc syntax_error(message: Str) [process, env] -> Unit {
  gnu.error(f"syntax error: {message}")
  exit 2
}

# Parse and evaluate in one pass with explicit stacks (no recursion, so deeply
# nested input is fine). Operands are computed as they complete; `ops` holds
# parentheses, pending binary operators (`weights` is the precedence), and
# prefix functions (`weights` counts the operands collected). The stacks only
# grow: `otop` and `vtop` mark the live part, and entries above are reused.
# Every user-function call copies the script arguments, so the hot cases (small
# integer arithmetic, `length`, parentheses) are handled inline.
proc evaluate(args: List[Bytes], here: Locale) [process, env] -> Outcome {
  let total = args.len()
  var vals: List[Outcome] = []
  var ops: List[Str] = []
  var weights: List[Int] = []
  var vtop = 0
  var otop = 0
  var at = 0
  var want = true
  var primary_done = false
  var phase = 0
  var pending = ""
  var pending_weight = 0
  let empty = {value: b"", error: ""}

  loop {
    if primary_done {
      primary_done = false

      if otop > 0 and ops[otop - 1] != "(" and (PRECEDENCE.get(ops[otop - 1]) ?? 0) == 0 {
        let name = ops[otop - 1]
        let have = weights[otop - 1] + 1

        if have == (ARITY.get(name) ?? 1) {
          var result = empty

          if name == "length" and vals[vtop - 1].error == "" and ! here.utf8 {
            result = {value: bytes.from_text(f"{vals[vtop - 1].value.len()}"), error: ""}
          } else if name == "length" and vals[vtop - 1].error == "" and (vals[vtop - 1].value.utf8() ?? "�") != "�" {
            result = {value: bytes.from_text(f"{(vals[vtop - 1].value.utf8() ?? "").count_chars()}"), error: ""}
          } else {
            result = apply_function(name, vals[vtop - have..vtop], here)
          }

          vtop -= have
          otop -= 1

          if vtop < vals.len() {
            vals[vtop] = result
          } else {
            vals += [result]
          }

          vtop += 1
          primary_done = true
        } else {
          weights[otop - 1] = have
          want = true
        }
      } else {
        want = false
      }

      continue
    }

    if phase == 0 {
      if at >= total {
        if want {
          syntax_error(f"missing argument after {gnu.quote_bytes(args[at - 1])}")
        }

        phase = 3
        continue
      }

      let token_bytes = args[at]
      let token = token_bytes.utf8() ?? ""

      if want and (token == "(" or token in ["length", "match", "index", "substr"]) {
        if otop < ops.len() {
          ops[otop] = token
          weights[otop] = 0
        } else {
          ops += [token]
          weights += [0]
        }

        otop += 1
        at += 1
      } else if want {
        var literal = token_bytes

        if token == "+" {
          if at + 1 >= total {
            syntax_error(f"missing argument after {gnu.quote(token)}")
          }

          literal = args[at + 1]
          at += 1
        }

        let item = {value: literal, error: ""}

        if vtop < vals.len() {
          vals[vtop] = item
        } else {
          vals += [item]
        }

        vtop += 1
        at += 1
        primary_done = true
      } else if token == ")" {
        phase = 2
      } else {
        let level = PRECEDENCE.get(token) ?? 0

        if level == 0 {
          for index in range(otop) {
            if ops[index] == "(" {
              syntax_error(f"expecting ')' instead of {gnu.quote_bytes(token_bytes)}")
            }
          }

          syntax_error(f"unexpected argument {gnu.quote_bytes(token_bytes)}")
        }

        if at + 1 >= total {
          syntax_error(f"missing argument after {gnu.quote_bytes(token_bytes)}")
        }

        pending = token
        pending_weight = level
        phase = 1
      }

      continue
    }

    # Reduce the pending binary operators that bind at least as tightly as the
    # incoming operator (phase 1), up to the nearest `(` (phase 2), or all of
    # them (phase 3).
    let binary = otop > 0 and (PRECEDENCE.get(ops[otop - 1]) ?? 0) > 0

    if binary and (phase != 1 or weights[otop - 1] >= pending_weight) {
      let right = vals[vtop - 1]
      let left = vals[vtop - 2]
      let op = ops[otop - 1]
      var result = empty
      let left_text = left.value.utf8() ?? "?"
      let right_text = right.value.utf8() ?? "?"

      if left.error == "" and right.error == "" and (op == "+" or op == "-" or op == "*" or op == "/" or op == "%") and left_text.byte_len() <= 17 and right_text.byte_len() <= 17 and rx"^-?[0-9]+$".matches(
        left_text,
      ) and rx"^-?[0-9]+$".matches(right_text) and (op != "*" or (left_text.byte_len() <= 9 and right_text.byte_len() <= 9)) {
        let a = left_text.parse_int() ?? 0
        let b = right_text.parse_int() ?? 0

        if op == "+" {
          result = {value: bytes.from_text(f"{a + b}"), error: ""}
        } else if op == "-" {
          result = {value: bytes.from_text(f"{a - b}"), error: ""}
        } else if op == "*" {
          result = {value: bytes.from_text(f"{a * b}"), error: ""}
        } else if b == 0 {
          result = {value: b"", error: "division by zero"}
        } else {
          result = {value: bytes.from_text(if op == "/" { f"{a / b}" } else { f"{a - a / b * b}" }), error: ""}
        }
      } else {
        result = apply_operator(op, left, right, here)
      }

      vtop -= 2
      vals[vtop] = result
      vtop += 1
      otop -= 1
      continue
    }

    if phase == 1 {
      if otop < ops.len() {
        ops[otop] = pending
        weights[otop] = pending_weight
      } else {
        ops += [pending]
        weights += [pending_weight]
      }

      otop += 1
      at += 1
      want = true
      phase = 0
    } else if phase == 2 {
      if otop > 0 and ops[otop - 1] == "(" {
        otop -= 1
        at += 1
        phase = 0
        primary_done = true
      } else {
        # A `)` with no `(` to close ends the expression.
        break
      }
    } else {
      if otop > 0 {
        syntax_error(f"expecting ')' after {gnu.quote_bytes(args[total - 1])}")
      }

      break
    }
  }

  if at < total {
    syntax_error(f"unexpected argument {gnu.quote_bytes(args[at])}")
  }

  vals[0]
}

proc write_expr_value(value: Bytes) [process, env, error, io] -> Unit {
  if let Err(_) = io.write_stdout_bytes(bytes.concat([value, b"\n"])) {
    gnu.error("write error")
    exit 3
  }

  if let Err(_) = io.flush_stdout() {
    gnu.error("write error")
    exit 3
  }
}

proc main(...argv: List[Bytes]) [process, env, error, io] {
  if argv.len() == 1 and argv[0] == b"--help" {
    gnu.help(USAGE)
    return
  }

  if argv.len() == 1 and argv[0] == b"--version" {
    gnu.version("expr")
    return
  }

  let args = if ! argv.is_empty() and argv[0] == b"--" { argv[1..] } else { argv }

  if args.is_empty() {
    gnu.missing_operand(2)
  }

  let result = evaluate(args, locale())

  if result.error != "" {
    gnu.error(result.error)
    exit 2
  }

  write_expr_value(result.value)

  if is_null(text_of(result.value)) {
    exit 1
  }
}
