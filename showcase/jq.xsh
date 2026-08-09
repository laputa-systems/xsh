#!/usr/bin/env -S xsh --
# jq — a JSON query mini-language, interpreted on top of XSH.
# Usage: xsh showcase/jq.xsh -- [OPTIONS] FILTER [FILES...]
# Example: echo '{"a":[1,2,3]}' | xsh showcase/jq.xsh -- '.a | map(.+1)'
#
# Why this file looks the way it does
# -----------------------------------
# jq is the "dynamic data / allocation" forcing function: a tree of
# nested dynamic values that gets traversed and rebuilt constantly. It is a real
# interpreter — lexer, precedence parser, and a tree-walking evaluator with jq's
# stream/backtracking semantics — written entirely in XSH `pure` functions.
#
# What this port is — and is not
# ------------------------------
# jq is itself a *language runtime*, so porting it means building an interpreter, and
# interpreters want exactly the three things XSH deliberately omits: first-class
# closures (a jq filter is a stream->stream value you name, store, recurse, and pass
# as an argument), lazy generators (jq backtracks; we model that as eager `List`s, so
# infinite generators outside a `limit` would diverge), and O(1)-append persistent
# collections (immutable `List.push`/`List.get` are O(n) here). We reconstruct all
# three as data — closures become `Closure`/`Env` cons-cells, streams become eager
# lists — which works, but is the tell: this is the *worst-case* workload for XSH's
# design, not a representative one. That is precisely why it is a good forcing
# function (it leans on every weakness at once), and precisely why it is the wrong
# poster child for the language.
#
# XSH does not compete with jq-the-language; it dissolves jq-the-use-case. The ~90% of
# real jq usage — pick fields, filter rows, map a transform, group/aggregate, reformat
# — is *native* in XSH and lowers to fast code: `json.decode(...) |> where { ... } |>
# map { ... } |> sum`. The native XSH side-by-side examples live in this file's
# tests and comments. The lasting value of this interpreter is the
# forcing-function findings it surfaced: the input parser was O(n^2) until
# rewritten to byte-indexed scanning (see the parser section), and the day-to-day
# language friction it hit is filed as proposals in LANG.md (each references the
# pain point here).
#
# Design notes:
# - Self-contained ordered JSON model. XSH's native records/maps are BTreeMap-backed
#   (keys sorted); jq preserves object *insertion order*. So we carry our own `Json`
#   tag union (objects = ordered key/value pairs) with our own parser + serializer,
#   rather than changing the runtime. We still delegate the fiddly *scalar* bits —
#   string-escape decoding and number parsing/encoding — to native `json.decode` /
#   `json.encode`, which only ever sees scalars (never our ordered objects), so it is
#   exact and we never depend on its key ordering.
# - One numeric type. jq numbers are IEEE doubles, so `JNum(Float)` is the only number
#   variant; integral values render without a decimal point. (Decimal literal
#   preservation / bignums are a deliberate gap.)
# - jq filters produce a *stream* of outputs; with no generators in XSH we model a
#   filter as `eval(ast, input, scope) -> Result[List[Json]]` (eager streams).
type Entry = {k: Str, v: Json}

type Json =
    JNull
  | JBool(Bool)
  | JNum(Float)
  | JStr(Str)
  | JArr(List[Json])
  | JObj(List[Entry])

type Parsed = {val: Json, pos: Int}

type RawStr = {raw: Str, pos: Int}

# ---------------------------------------------------------------------------
# JSON parser: hand-written recursive descent over a codepoint list, threading a
# cursor (`pos`). Scalars are handed to native json.decode for exact conversion.
# ---------------------------------------------------------------------------
error JqError = Jq(message: Str)

pure jq_err(msg: Str) -> JqError {
  return JqError.Jq(message: msg)
}

pure is_ws(c: Str) -> Bool {
  return c == " " or c == "\n" or c == "\t" or c == "\r"
}

pure is_digit(c: Str) -> Bool {
  return c != "" and c in "0123456789"
}

# Scan a quoted string starting at `pos` (which points at the opening quote),
# returning the raw quoted substring (escapes intact) and the position after it.
pure scan_string(chars: List[Str], pos: Int) -> Result[RawStr] {
  var p = pos + 1
  var buf = ["\""]

  while p < chars.len() {
    let c = chars.get(p, "")

    if c == "\\" {
      buf = buf.push(c)
      buf = buf.push(chars.get(p + 1, ""))
      p = p + 2
    } else if c == "\"" {
      buf = buf.push("\"")
      return Ok({raw: buf.join(""), pos: p + 1})
    } else {
      buf = buf.push(c)
      p = p + 1
    }
  }

  return Err(jq_err("Unterminated string in JSON"))
}

# Decode a numeric token to a Float. json.decode collapses integral values to Int
# (regardless of literal syntax) and keeps only genuinely-fractional values as Float.
pure decode_num(tok: Str) -> Result[Json] {
  return match json.decode(tok)? {
    i is Int => Ok(JNum(i.float())),
    f is Float => Ok(JNum(f)),
    _ => Err(jq_err("Invalid JSON number")),
  }
}

# Byte-indexed JSON input parser. Walking a `split("")` char list with `List.get`
# was O(n^2) (list indexing isn't O(1)); `Str.byte_at`/`Str.byte_slice` are O(1), so
# this walks bytes directly. The structural scan only compares ASCII bytes; string and
# number tokens are sliced out and handed to json.decode, so multibyte UTF-8 (which
# only appears inside string literals) round-trips untouched. (`scan_string` above is
# retained for the small-program lexer, where O(n^2) is irrelevant.)
pure bws(s: Str, pos: Int) -> Int {
  var p = pos

  while true {
    let b = s.byte_at(p, -1)

    if b == 32 or b == 10 or b == 9 or b == 13 {
      p = p + 1
    } else {
      return p
    }
  }

  return p
}

pure bdigit(b: Int) -> Bool {
  return b >= 48 and b <= 57
}

pure bnumchar(b: Int) -> Bool {
  return bdigit(b) or b == 45 or b == 43 or b == 46 or b == 101 or b == 69
}

# Scan a quoted string starting at byte `pos` (the opening quote); return the raw
# quoted substring (escapes intact) and the byte position after the closing quote.
pure scan_string_b(s: Str, pos: Int) -> Result[RawStr] {
  var p = pos + 1

  while true {
    let b = s.byte_at(p, -1)

    if b == -1 {
      return Err(jq_err("Unterminated string in JSON"))
    }

    if b == 92 {
      p = p + 2
    } else if b == 34 {
      return Ok({raw: s.byte_slice(pos, p + 1 - pos), pos: p + 1})
    } else {
      p = p + 1
    }
  }

  return Err(jq_err("Unterminated string in JSON"))
}

pure parse_number_b(s: Str, pos: Int) -> Result[Parsed] {
  var p = pos

  while bnumchar(s.byte_at(p, -1)) {
    p = p + 1
  }

  if p == pos {
    return Err(jq_err("Invalid JSON value"))
  }

  return Ok({val: decode_num(s.byte_slice(pos, p - pos))?, pos: p})
}

pure parse_value_b(s: Str, pos: Int) -> Result[Parsed] {
  let p = bws(s, pos)
  let b = s.byte_at(p, -1)

  if b == -1 {
    return Err(jq_err("Unexpected end of JSON input"))
  }

  if b == 123 {
    return parse_object_b(s, p)
  }

  if b == 91 {
    return parse_array_b(s, p)
  }

  if b == 34 {
    let r = scan_string_b(s, p)?
    let str: Str = json.decode(r.raw)?
    return Ok({val: JStr(str), pos: r.pos})
  }

  if b == 110 {
    return Ok({val: JNull, pos: p + 4})
  }

  if b == 116 {
    return Ok({val: JBool(true), pos: p + 4})
  }

  if b == 102 {
    return Ok({val: JBool(false), pos: p + 5})
  }

  return parse_number_b(s, p)
}

pure parse_array_b(s: Str, pos: Int) -> Result[Parsed] {
  var p = bws(s, pos + 1)
  var items: List[Json] = []

  if s.byte_at(p, -1) == 93 {
    return Ok({val: JArr(items), pos: p + 1})
  }

  while true {
    let r = parse_value_b(s, p)?
    items = items.push(r.val)
    p = bws(s, r.pos)
    let b = s.byte_at(p, -1)

    if b == 44 {
      p = bws(s, p + 1)
    } else if b == 93 {
      return Ok({val: JArr(items), pos: p + 1})
    } else {
      return Err(jq_err("Expected ',' or ']' in JSON array"))
    }
  }

  return Err(jq_err("unreachable"))
}

pure parse_object_b(s: Str, pos: Int) -> Result[Parsed] {
  var p = bws(s, pos + 1)
  var entries: List[Entry] = []

  if s.byte_at(p, -1) == 125 {
    return Ok({val: JObj(entries), pos: p + 1})
  }

  while true {
    if s.byte_at(p, -1) != 34 {
      return Err(jq_err("Expected string key in JSON object"))
    }

    let kr = scan_string_b(s, p)?
    let key: Str = json.decode(kr.raw)?
    p = bws(s, kr.pos)

    if s.byte_at(p, -1) != 58 {
      return Err(jq_err("Expected ':' in JSON object"))
    }

    let vr = parse_value_b(s, bws(s, p + 1))?
    entries = entries.push({k: key, v: vr.val})
    p = bws(s, vr.pos)
    let b = s.byte_at(p, -1)

    if b == 44 {
      p = bws(s, p + 1)
    } else if b == 125 {
      return Ok({val: JObj(entries), pos: p + 1})
    } else {
      return Err(jq_err("Expected ',' or '}' in JSON object"))
    }
  }

  return Err(jq_err("unreachable"))
}

# Parse a whitespace-separated stream of JSON values (jq reads many).
pure parse_stream(s: Str) -> Result[List[Json]] {
  var values: List[Json] = []
  var p = bws(s, 0)

  while s.byte_at(p, -1) != -1 {
    let r = parse_value_b(s, p)?
    values = values.push(r.val)
    p = bws(s, r.pos)
  }

  values
}

# ---------------------------------------------------------------------------
# Serializer: compact JSON. Objects/arrays are emitted by hand to preserve key
# order; scalars go through native json.encode for exact escaping/number format.
# ---------------------------------------------------------------------------
pure render_num(n: Float) -> Str {
  let i = n.floor() ?? 0

  if i.float() == n {
    return f"${i}"
  }

  return f"${n}"
}

pure encode_str(s: Str) -> Str {
  return json.encode(s) ?? "\"\""
}

pure ser(j: Json) -> Str {
  match j {
    JNull => "null"
    JBool(b) => {
      if b {
        "true"
      } else {
        "false"
      }
    }
    JNum(n) => render_num(n)
    JStr(s) => encode_str(s)
    JArr(xs) => {
      var parts = [ser(x) for x in xs]
      "[" + parts.join(",") + "]"
    }
    JObj(es) => {
      var parts = [encode_str(e.k) + ":" + ser(e.v) for e in es]
      "{" + parts.join(",") + "}"
    }
  }
}

# ---------------------------------------------------------------------------
# Lexer. Produces a flat token list; whitespace/comments are dropped. Field and
# `..` recognition is whitespace-sensitive (`.foo` is one token, `. foo` is not).
# ---------------------------------------------------------------------------
type RawPart = RLit(Str) | RExpr(Str)

type Tok =
    TDot
  | TDotDot
  | TLBracket
  | TRBracket
  | TLBrace
  | TRBrace
  | TLParen
  | TRParen
  | TPipe
  | TComma
  | TColon
  | TSemi
  | TQuestion
  | TField(Str)
  | TIdent(Str)
  | TVar(Str)
  | TFormat(Str)
  | TNum(Float)
  | TStr(Str)
  | TStrInterp(List[RawPart])
  | TOp(Str)
  | TEOF

pure is_alpha(c: Str) -> Bool {
  return c != "" and c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_"
}

pure is_ident_start(c: Str) -> Bool {
  return is_alpha(c)
}

pure is_ident_char(c: Str) -> Bool {
  return is_alpha(c) or is_digit(c)
}

type LexNum = {tok: Tok, pos: Int}

pure lex_ident(chars: List[Str], pos: Int) -> LexNum {
  var p = pos
  var buf: List[Str] = []

  while p < chars.len() and is_ident_char(chars.get(p, "")) {
    buf = buf.push(chars.get(p, ""))
    p = p + 1
  }

  return {tok: TIdent(buf.join("")), pos: p}
}

pure lex_number(chars: List[Str], pos: Int) -> Result[LexNum] {
  var p = pos
  var buf: List[Str] = []

  while p < chars.len() and is_digit(chars.get(p, "")) {
    buf = buf.push(chars.get(p, ""))
    p = p + 1
  }

  if chars.get(p, "") == "." {
    buf = buf.push(".")
    p = p + 1

    while p < chars.len() and is_digit(chars.get(p, "")) {
      buf = buf.push(chars.get(p, ""))
      p = p + 1
    }
  }

  let e = chars.get(p, "")

  if e == "e" or e == "E" {
    buf = buf.push(e)
    p = p + 1
    let sign = chars.get(p, "")

    if sign == "+" or sign == "-" {
      buf = buf.push(sign)
      p = p + 1
    }

    while p < chars.len() and is_digit(chars.get(p, "")) {
      buf = buf.push(chars.get(p, ""))
      p = p + 1
    }
  }

  let num = decode_num(buf.join(""))?

  match num {
    JNum(f) => return Ok({tok: TNum(f), pos: p})
    _ => return Err(jq_err("Invalid number literal"))
  }
}

# Multi-char operator starting at pos; returns the operator string or "".
pure lex_op(chars: List[Str], pos: Int) -> Str {
  let c = chars.get(pos, "")
  let d = chars.get(pos + 1, "")

  if c == "/" and d == "/" {
    if chars.get(pos + 2, "") == "=" {
      return "//="
    }

    return "//"
  }

  if d == "=" and c != "" and c in "=!<>+-*/%|" {
    return c + "="
  }

  if c != "" and c in "=<>+-*/%" {
    return c
  }

  return ""
}

type RawScan = {text: Str, pos: Int}

# Scan a balanced parenthesised expression starting just after `\(`, returning the
# inner text and the position after the matching `)`. Skips nested string literals.
pure scan_balanced(chars: List[Str], pos: Int) -> Result[RawScan] {
  var p = pos
  var depth = 0
  var buf: List[Str] = []

  while p < chars.len() {
    let c = chars.get(p, "")

    if c == "\"" {
      buf = buf.push(c)
      p = p + 1

      while p < chars.len() {
        let d = chars.get(p, "")
        buf = buf.push(d)

        if d == "\\" {
          buf = buf.push(chars.get(p + 1, ""))
          p = p + 2
        } else if d == "\"" {
          p = p + 1
          break
        } else {
          p = p + 1
        }
      }
    } else if c == "(" {
      depth = depth + 1
      buf = buf.push(c)
      p = p + 1
    } else if c == ")" {
      if depth == 0 {
        return Ok({text: buf.join(""), pos: p + 1})
      }

      depth = depth - 1
      buf = buf.push(c)
      p = p + 1
    } else {
      buf = buf.push(c)
      p = p + 1
    }
  }

  return Err(jq_err("Unterminated interpolation"))
}

pure interp_token(parts: List[RawPart]) -> Tok {
  var has_expr = false
  var lit: List[Str] = []

  for pt in parts {
    match pt {
      RExpr(_) => has_expr = true
      RLit(s) => lit = lit.push(s)
    }
  }

  if has_expr {
    return TStrInterp(parts)
  }

  return TStr(lit.join(""))
}

# Lex a string literal, splitting on `\(...)` interpolations. Literal chunks are
# escape-decoded via json.decode (wrapping in quotes); interpolations keep raw text.
pure lex_string(chars: List[Str], pos: Int) -> Result[LexNum] {
  var p = pos + 1
  var parts: List[RawPart] = []
  var litraw: List[Str] = []

  while p < chars.len() {
    let c = chars.get(p, "")

    if c == "\"" {
      let chunk = litraw.join("")

      if chunk != "" {
        let dec: Str = json.decode("\"" + chunk + "\"")?
        parts = parts.push(RLit(dec))
      }

      return Ok({tok: interp_token(parts), pos: p + 1})
    } else if c == "\\" {
      let d = chars.get(p + 1, "")

      if d == "(" {
        let chunk = litraw.join("")

        if chunk != "" {
          let dec: Str = json.decode("\"" + chunk + "\"")?
          parts = parts.push(RLit(dec))
        }

        litraw = []
        let inner = scan_balanced(chars, p + 2)?
        parts = parts.push(RExpr(inner.text))
        p = inner.pos
      } else {
        litraw = litraw.push("\\")
        litraw = litraw.push(d)
        p = p + 2
      }
    } else {
      litraw = litraw.push(c)
      p = p + 1
    }
  }

  return Err(jq_err("Unterminated string"))
}

pure lex(s: Str) -> Result[List[Tok]] {
  let chars = s.split("")
  var toks: List[Tok] = []
  var p = 0

  while p < chars.len() {
    let c = chars.get(p, "")

    if is_ws(c) {
      p = p + 1
    } else if c == "#" {
      while p < chars.len() and chars.get(p, "") != "\n" {
        p = p + 1
      }
    } else if c == "." {
      let d = chars.get(p + 1, "")

      if d == "." {
        toks = toks.push(TDotDot)
        p = p + 2
      } else if is_ident_start(d) {
        let r = lex_ident(chars, p + 1)

        match r.tok {
          TIdent(name) => toks = toks.push(TField(name))
          _ => return Err(jq_err("lex field"))
        }

        p = r.pos
      } else if d == "\"" {
        let rs = scan_string(chars, p + 1)?
        let name: Str = json.decode(rs.raw)?
        toks = toks.push(TField(name))
        p = rs.pos
      } else {
        toks = toks.push(TDot)
        p = p + 1
      }
    } else if c == "$" {
      let r = lex_ident(chars, p + 1)

      match r.tok {
        TIdent(name) => toks = toks.push(TVar(name))
        _ => return Err(jq_err("lex var"))
      }

      p = r.pos
    } else if c == "@" {
      let r = lex_ident(chars, p + 1)

      match r.tok {
        TIdent(name) => toks = toks.push(TFormat(name))
        _ => return Err(jq_err("lex format"))
      }

      p = r.pos
    } else if is_ident_start(c) {
      let r = lex_ident(chars, p)
      toks = toks.push(r.tok)
      p = r.pos
    } else if is_digit(c) {
      let r = lex_number(chars, p)?
      toks = toks.push(r.tok)
      p = r.pos
    } else if c == "\"" {
      let ls = lex_string(chars, p)?
      toks = toks.push(ls.tok)
      p = ls.pos
    } else if c == "|" {
      if chars.get(p + 1, "") == "=" {
        toks = toks.push(TOp("|="))
        p = p + 2
      } else {
        toks = toks.push(TPipe)
        p = p + 1
      }
    } else if c == "(" {
      toks = toks.push(TLParen)
      p = p + 1
    } else if c == ")" {
      toks = toks.push(TRParen)
      p = p + 1
    } else if c == "[" {
      toks = toks.push(TLBracket)
      p = p + 1
    } else if c == "]" {
      toks = toks.push(TRBracket)
      p = p + 1
    } else if c == "{" {
      toks = toks.push(TLBrace)
      p = p + 1
    } else if c == "}" {
      toks = toks.push(TRBrace)
      p = p + 1
    } else if c == "," {
      toks = toks.push(TComma)
      p = p + 1
    } else if c == ":" {
      toks = toks.push(TColon)
      p = p + 1
    } else if c == ";" {
      toks = toks.push(TSemi)
      p = p + 1
    } else if c == "?" {
      toks = toks.push(TQuestion)
      p = p + 1
    } else {
      let op = lex_op(chars, p)

      if op == "" {
        return Err(jq_err("Unexpected character in program: " + c))
      }

      toks = toks.push(TOp(op))
      p = p + op.byte_len()
    }
  }

  toks = toks.push(TEOF)
  toks
}

# ---------------------------------------------------------------------------
# AST + parser. Recursive descent with precedence; the token cursor (`pos`) is
# threaded through every parse function as a value (no shared mutable state).
# ---------------------------------------------------------------------------
type ObjEntry = {key: Jq, val: Jq}

type FnDef = {fname: Str, params: List[Str], fbody: Jq}

type PatField = {key: Str, pat: Pattern}

type Pattern =
    PVar(Str)
  | PArray(List[Pattern])
  | PObjPat(List[PatField])

type Jq =
    Identity
  | RecurseDefault
  | Lit(Json)
  | Field(Jq, Str)
  | Index(Jq, Jq)
  | Slice(Jq, Jq, Jq)
  | Iterate(Jq)
  | Pipe(Jq, Jq)
  | Comma(Jq, Jq)
  | Neg(Jq)
  | BinOp(Str, Jq, Jq)
  | Alt(Jq, Jq)
  | TryCatch(Jq, Jq)
  | Optional(Jq)
  | ArrayC(Jq)
  | ObjectC(List[ObjEntry])
  | IfElse(Jq, Jq, Jq)
  | Call(Str, List[Jq])
  | VarRef(Str)
  | Assign(Jq, Jq)
  | Update(Jq, Jq)
  | ArithUpdate(Str, Jq, Jq)
  | StrInterp(List[Jq], Str)
  | StrLit(Str)
  | StrExpr(Jq)
  | Fmt(Str)
  | BindVar(Jq, Pattern, Jq)
  | Reduce(Jq, Pattern, Jq, Jq)
  | Foreach(Jq, Pattern, Jq, Jq, Bool, Jq)
  | FuncDef(FnDef, Jq)
  | Empty

type Closure = {cbody: Jq, cenv: Env}

# Env is a cons-cell chain: $var values, filter-param closures, and user defs.
type Env =
    EnvEmpty
  | EnvVar(Str, Json, Env)
  | EnvFilter(Str, Closure, Env)
  | EnvFunc(FnDef, Env, Env)

type PJq = {node: Jq, pos: Int}

pure tok_at(toks: List[Tok], pos: Int) -> Tok {
  return toks.get(pos, TEOF)
}

pure is_op(t: Tok, name: Str) -> Bool {
  match t {
    TOp(s) => s == name
    _ => false
  }
}

# parse_expr: full expression (lowest precedence = pipe).
pure parse_expr(toks: List[Tok], pos: Int) -> Result[PJq] {
  return parse_pipe(toks, pos)
}

pure is_ident_tok(t: Tok, name: Str) -> Bool {
  match t {
    TIdent(s) => s == name
    _ => false
  }
}

pure parse_pipe(toks: List[Tok], pos: Int) -> Result[PJq] {
  if is_ident_tok(tok_at(toks, pos), "def") {
    return parse_def(toks, pos)
  }

  let left = parse_comma(toks, pos)?

  if is_ident_tok(tok_at(toks, left.pos), "as") {
    let pat = parse_pattern(toks, left.pos + 1)?

    match tok_at(toks, pat.pos) {
      TPipe => {
        let body = parse_pipe(toks, pat.pos + 1)?
        return Ok({node: BindVar(left.node, pat.pat, body.node), pos: body.pos})
      }
      _ => return Err(jq_err("Expected | after 'as' pattern"))
    }
  }

  match tok_at(toks, left.pos) {
    TPipe => {
      let right = parse_pipe(toks, left.pos + 1)?
      return Ok({node: Pipe(left.node, right.node), pos: right.pos})
    }
    _ => return Ok(left)
  }
}

type PPat = {pat: Pattern, pos: Int}

pure parse_pattern(toks: List[Tok], pos: Int) -> Result[PPat] {
  let t = tok_at(toks, pos)

  match t {
    TVar(name) => return Ok({pat: PVar(name), pos: pos + 1})
    TLBracket => {
      var p = pos + 1
      var pats: List[Pattern] = []

      match tok_at(toks, p) {
        TRBracket => return Ok({pat: PArray(pats), pos: p + 1})
        _ => p = p
      }

      while true {
        let sub = parse_pattern(toks, p)?
        pats = pats.push(sub.pat)

        match tok_at(toks, sub.pos) {
          TComma => p = sub.pos + 1
          TRBracket => return Ok({pat: PArray(pats), pos: sub.pos + 1})
          _ => return Err(jq_err("Expected , or ] in array pattern"))
        }
      }

      return Err(jq_err("unreachable"))
    }
    TLBrace => return parse_obj_pattern(toks, pos)
    _ => return Err(jq_err("Expected pattern"))
  }
}

pure parse_obj_pattern(toks: List[Tok], pos: Int) -> Result[PPat] {
  var p = pos + 1
  var fields: List[PatField] = []

  match tok_at(toks, p) {
    TRBrace => return Ok({pat: PObjPat(fields), pos: p + 1})
    _ => p = p
  }

  while true {
    let t = tok_at(toks, p)

    match t {
      TVar(name) => {
        # {$x}  ->  bind $x = .x
        fields = fields.push({key: name, pat: PVar(name)})
        p = p + 1
      }
      TIdent(name) => {
        # {key: subpat}
        match tok_at(toks, p + 1) {
          TColon => {
            let sub = parse_pattern(toks, p + 2)?
            fields = fields.push({key: name, pat: sub.pat})
            p = sub.pos
          }
          _ => return Err(jq_err("Expected : in object pattern"))
        }
      }
      TStr(name) => {
        match tok_at(toks, p + 1) {
          TColon => {
            let sub = parse_pattern(toks, p + 2)?
            fields = fields.push({key: name, pat: sub.pat})
            p = sub.pos
          }
          _ => return Err(jq_err("Expected : in object pattern"))
        }
      }
      _ => return Err(jq_err("Expected key in object pattern"))
    }

    match tok_at(toks, p) {
      TComma => p = p + 1
      TRBrace => return Ok({pat: PObjPat(fields), pos: p + 1})
      _ => return Err(jq_err("Expected , or } in object pattern"))
    }
  }

  return Err(jq_err("unreachable"))
}

# def NAME (PARAMS)? : BODY ; REST
pure parse_def(toks: List[Tok], pos: Int) -> Result[PJq] {
  let nametok = tok_at(toks, pos + 1)
  var fname = ""

  match nametok {
    TIdent(n) => fname = n
    _ => return Err(jq_err("Expected function name after 'def'"))
  }

  var p = pos + 2
  var params: List[Str] = []

  match tok_at(toks, p) {
    TLParen => {
      p = p + 1

      while true {
        let pt = tok_at(toks, p)

        match pt {
          TVar(vn) => params = params.push("$" + vn)
          TIdent(fn2) => params = params.push(fn2)
          _ => return Err(jq_err("Expected parameter name"))
        }

        p = p + 1

        match tok_at(toks, p) {
          TSemi => p = p + 1
          TRParen => {
            p = p + 1
            break
          }
          _ => return Err(jq_err("Expected ; or ) in parameter list"))
        }
      }
    }
    _ => p = p
  }

  match tok_at(toks, p) {
    TColon => p = p + 1
    _ => return Err(jq_err("Expected : in function definition"))
  }

  let body = parse_pipe(toks, p)?

  match tok_at(toks, body.pos) {
    TSemi => p = body.pos + 1
    _ => return Err(jq_err("Expected ; after function body"))
  }

  let rest = parse_pipe(toks, p)?
  let fdef = {fname: fname, params: params, fbody: body.node}
  return Ok({node: FuncDef(fdef, rest.node), pos: rest.pos})
}

pure parse_comma(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_alt(toks, pos)?

  while true {
    match tok_at(toks, cur.pos) {
      TComma => {
        let right = parse_alt(toks, cur.pos + 1)?
        cur = {node: Comma(cur.node, right.node), pos: right.pos}
      }
      _ => return Ok(cur)
    }
  }

  return Ok(cur)
}

# // alternative operator (right-associative), lower precedence than assignment.
pure parse_alt(toks: List[Tok], pos: Int) -> Result[PJq] {
  let left = parse_assign(toks, pos)?

  if is_op(tok_at(toks, left.pos), "//") {
    let right = parse_alt(toks, left.pos + 1)?
    return Ok({node: Alt(left.node, right.node), pos: right.pos})
  }

  return Ok(left)
}

# Assignment operators (non-associative): =, |=, +=, -=, *=, /=, %=, //=.
pure parse_assign(toks: List[Tok], pos: Int) -> Result[PJq] {
  let left = parse_or(toks, pos)?
  let t = tok_at(toks, left.pos)

  match t {
    TOp(s) => {
      if s == "=" {
        let rhs = parse_or(toks, left.pos + 1)?
        return Ok({node: Assign(left.node, rhs.node), pos: rhs.pos})
      }

      if s == "|=" {
        let rhs = parse_or(toks, left.pos + 1)?
        return Ok({node: Update(left.node, rhs.node), pos: rhs.pos})
      }

      if s == "+=" or s == "-=" or s == "*=" or s == "/=" or s == "%=" {
        let rhs = parse_or(toks, left.pos + 1)?
        let op = s.byte_slice(0, 1)
        return Ok({node: ArithUpdate(op, left.node, rhs.node), pos: rhs.pos})
      }

      if s == "//=" {
        let rhs = parse_or(toks, left.pos + 1)?
        return Ok({node: ArithUpdate("//", left.node, rhs.node), pos: rhs.pos})
      }

      return Ok(left)
    }
    _ => return Ok(left)
  }
}

pure parse_or(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_and(toks, pos)?

  while true {
    match tok_at(toks, cur.pos) {
      TIdent(s) => {
        if s == "or" {
          let right = parse_and(toks, cur.pos + 1)?
          cur = {node: BinOp("or", cur.node, right.node), pos: right.pos}
        } else {
          return Ok(cur)
        }
      }
      _ => return Ok(cur)
    }
  }

  return Ok(cur)
}

pure parse_and(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_cmp(toks, pos)?

  while true {
    match tok_at(toks, cur.pos) {
      TIdent(s) => {
        if s == "and" {
          let right = parse_cmp(toks, cur.pos + 1)?
          cur = {node: BinOp("and", cur.node, right.node), pos: right.pos}
        } else {
          return Ok(cur)
        }
      }
      _ => return Ok(cur)
    }
  }

  return Ok(cur)
}

# Comparison operators are non-associative (parse at most one).
pure parse_cmp(toks: List[Tok], pos: Int) -> Result[PJq] {
  let left = parse_add(toks, pos)?
  let t = tok_at(toks, left.pos)

  match t {
    TOp(s) => {
      if s == "==" or s == "!=" or s == "<" or s == "<=" or s == ">" or s == ">=" {
        let right = parse_add(toks, left.pos + 1)?
        return Ok({node: BinOp(s, left.node, right.node), pos: right.pos})
      }

      return Ok(left)
    }
    _ => return Ok(left)
  }
}

pure parse_add(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_mul(toks, pos)?

  while true {
    let t = tok_at(toks, cur.pos)

    if is_op(t, "+") or is_op(t, "-") {
      let opname = match t { TOp(s) => s, _ => "+" }
      let right = parse_mul(toks, cur.pos + 1)?
      cur = {node: BinOp(opname, cur.node, right.node), pos: right.pos}
    } else {
      return Ok(cur)
    }
  }

  return Ok(cur)
}

pure parse_mul(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_unary(toks, pos)?

  while true {
    let t = tok_at(toks, cur.pos)

    if is_op(t, "*") or is_op(t, "/") or is_op(t, "%") {
      let opname = match t { TOp(s) => s, _ => "*" }
      let right = parse_unary(toks, cur.pos + 1)?
      cur = {node: BinOp(opname, cur.node, right.node), pos: right.pos}
    } else {
      return Ok(cur)
    }
  }

  return Ok(cur)
}

pure parse_unary(toks: List[Tok], pos: Int) -> Result[PJq] {
  if is_op(tok_at(toks, pos), "-") {
    let inner = parse_postfix(toks, pos + 1)?
    return Ok({node: Neg(inner.node), pos: inner.pos})
  }

  return parse_postfix(toks, pos)
}

# Postfix chain: .field, [...], [], [a:b], and trailing ?.
pure parse_postfix(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_primary(toks, pos)?

  while true {
    let t = tok_at(toks, cur.pos)

    match t {
      TField(name) => cur = {node: Field(cur.node, name), pos: cur.pos + 1}
      TLBracket => {
        let r = parse_bracket(toks, cur.pos, cur.node)?
        cur = r
      }
      TQuestion => cur = {node: Optional(cur.node), pos: cur.pos + 1}
      _ => return Ok(cur)
    }
  }

  return Ok(cur)
}

# Parse a [...] suffix applied to `base`. pos points at TLBracket.
pure parse_bracket(toks: List[Tok], pos: Int, base: Jq) -> Result[PJq] {
  let after = pos + 1

  match tok_at(toks, after) {
    TRBracket => return Ok({node: Iterate(base), pos: after + 1})
    TColon => {
      # [:hi]
      let hi = parse_pipe(toks, after + 1)?

      if ! is_rbracket(tok_at(toks, hi.pos)) {
        return Err(jq_err("Expected ] in slice"))
      }

      return Ok({node: Slice(base, Identity, hi.node), pos: hi.pos + 1})
    }
    _ => {
      let lo = parse_pipe(toks, after)?

      match tok_at(toks, lo.pos) {
        TColon => {
          if is_rbracket(tok_at(toks, lo.pos + 1)) {
            # [lo:]
            return Ok({node: Slice(base, lo.node, Identity), pos: lo.pos + 2})
          }

          let hi = parse_pipe(toks, lo.pos + 1)?

          if ! is_rbracket(tok_at(toks, hi.pos)) {
            return Err(jq_err("Expected ] in slice"))
          }

          return Ok({node: Slice(base, lo.node, hi.node), pos: hi.pos + 1})
        }
        TRBracket => return Ok({node: Index(base, lo.node), pos: lo.pos + 1})
        _ => return Err(jq_err("Expected ] or : in index"))
      }
    }
  }
}

pure is_rbracket(t: Tok) -> Bool {
  match t {
    TRBracket => true
    _ => false
  }
}

pure parse_primary(toks: List[Tok], pos: Int) -> Result[PJq] {
  let t = tok_at(toks, pos)

  match t {
    TDot => return Ok({node: Identity, pos: pos + 1})
    TDotDot => return Ok({node: RecurseDefault, pos: pos + 1})
    TField(name) => return Ok({node: Field(Identity, name), pos: pos + 1})
    TNum(f) => return Ok({node: Lit(JNum(f)), pos: pos + 1})
    TStr(s) => return Ok({node: Lit(JStr(s)), pos: pos + 1})
    TVar(name) => return Ok({node: VarRef(name), pos: pos + 1})
    TLParen => {
      let inner = parse_pipe(toks, pos + 1)?

      match tok_at(toks, inner.pos) {
        TRParen => return Ok({node: inner.node, pos: inner.pos + 1})
        _ => return Err(jq_err("Expected )"))
      }
    }
    TLBracket => {
      # array construction; [] is empty
      match tok_at(toks, pos + 1) {
        TRBracket => return Ok({node: ArrayC(Empty), pos: pos + 2})
        _ => {
          let inner = parse_pipe(toks, pos + 1)?

          match tok_at(toks, inner.pos) {
            TRBracket => return Ok({node: ArrayC(inner.node), pos: inner.pos + 1})
            _ => return Err(jq_err("Expected ] in array"))
          }
        }
      }
    }
    TStrInterp(rawparts) => {
      let node = build_interp(rawparts, "")?
      return Ok({node: node, pos: pos + 1})
    }
    TFormat(name) => {
      match tok_at(toks, pos + 1) {
        TStr(s) => return Ok({node: StrInterp([StrLit(s)], name), pos: pos + 2})
        TStrInterp(rp) => {
          let node = build_interp(rp, name)?
          return Ok({node: node, pos: pos + 2})
        }
        _ => return Ok({node: Fmt(name), pos: pos + 1})
      }
    }
    TLBrace => return parse_object_ctor(toks, pos)
    TIdent(name) => return parse_ident_primary(toks, pos, name)
    _ => return Err(jq_err("Unexpected token in expression"))
  }
}

pure build_interp(rawparts: List[RawPart], fmt: Str) -> Result[Jq] {
  var parts: List[Jq] = []

  for rp in rawparts {
    match rp {
      RLit(s) => parts = parts.push(StrLit(s))
      RExpr(text) => parts = parts.push(StrExpr(parse_program(text)?))
    }
  }

  return Ok(StrInterp(parts, fmt))
}

# Identifier primary: keywords (true/false/null/if), else a function call.
# reduce SRC as PAT (INIT; UPDATE)
pure parse_reduce(toks: List[Tok], pos: Int) -> Result[PJq] {
  let src = parse_postfix(toks, pos)?

  if ! is_ident_tok(tok_at(toks, src.pos), "as") {
    return Err(jq_err("Expected 'as' in reduce"))
  }

  let pat = parse_pattern(toks, src.pos + 1)?

  match tok_at(toks, pat.pos) {
    TLParen => {}
    _ => return Err(jq_err("Expected ( in reduce"))
  }

  let init = parse_pipe(toks, pat.pos + 1)?

  match tok_at(toks, init.pos) {
    TSemi => {}
    _ => return Err(jq_err("Expected ; in reduce"))
  }

  let update = parse_pipe(toks, init.pos + 1)?

  match tok_at(toks, update.pos) {
    TRParen => return Ok({node: Reduce(src.node, pat.pat, init.node, update.node), pos: update.pos + 1})
    _ => return Err(jq_err("Expected ) in reduce"))
  }
}

# foreach SRC as PAT (INIT; UPDATE) or (INIT; UPDATE; EXTRACT)
pure parse_foreach(toks: List[Tok], pos: Int) -> Result[PJq] {
  let src = parse_postfix(toks, pos)?

  if ! is_ident_tok(tok_at(toks, src.pos), "as") {
    return Err(jq_err("Expected 'as' in foreach"))
  }

  let pat = parse_pattern(toks, src.pos + 1)?

  match tok_at(toks, pat.pos) {
    TLParen => {}
    _ => return Err(jq_err("Expected ( in foreach"))
  }

  let init = parse_pipe(toks, pat.pos + 1)?

  match tok_at(toks, init.pos) {
    TSemi => {}
    _ => return Err(jq_err("Expected ; in foreach"))
  }

  let update = parse_pipe(toks, init.pos + 1)?

  match tok_at(toks, update.pos) {
    TRParen => return Ok(
      {node: Foreach(src.node, pat.pat, init.node, update.node, false, Identity), pos: update.pos + 1},
    )
    TSemi => {
      let extract = parse_pipe(toks, update.pos + 1)?

      match tok_at(toks, extract.pos) {
        TRParen => return Ok(
          {node: Foreach(src.node, pat.pat, init.node, update.node, true, extract.node), pos: extract.pos + 1},
        )
        _ => return Err(jq_err("Expected ) in foreach"))
      }
    }
    _ => return Err(jq_err("Expected ; or ) in foreach"))
  }
}

pure parse_ident_primary(toks: List[Tok], pos: Int, name: Str) -> Result[PJq] {
  if name == "true" {
    return Ok({node: Lit(JBool(true)), pos: pos + 1})
  }

  if name == "false" {
    return Ok({node: Lit(JBool(false)), pos: pos + 1})
  }

  if name == "null" {
    return Ok({node: Lit(JNull), pos: pos + 1})
  }

  if name == "if" {
    return parse_if(toks, pos + 1)
  }

  if name == "reduce" {
    return parse_reduce(toks, pos + 1)
  }

  if name == "foreach" {
    return parse_foreach(toks, pos + 1)
  }

  if name == "try" {
    let body = parse_postfix(toks, pos + 1)?

    match tok_at(toks, body.pos) {
      TIdent(s2) => {
        if s2 == "catch" {
          let handler = parse_postfix(toks, body.pos + 1)?
          return Ok({node: TryCatch(body.node, handler.node), pos: handler.pos})
        }

        return Ok({node: TryCatch(body.node, Empty), pos: body.pos})
      }
      _ => return Ok({node: TryCatch(body.node, Empty), pos: body.pos})
    }
  }

  # function call: name optionally followed by ( arglist ; ... )
  match tok_at(toks, pos + 1) {
    TLParen => {
      let r = parse_call_args(toks, pos + 2)?
      return Ok({node: Call(name, r.arglist), pos: r.pos})
    }
    _ => return Ok({node: Call(name, []), pos: pos + 1})
  }
}

type PArgs = {arglist: List[Jq], pos: Int}

pure parse_call_args(toks: List[Tok], pos: Int) -> Result[PArgs] {
  var arglist: List[Jq] = []
  var p = pos

  while true {
    let a = parse_pipe(toks, p)?
    arglist = arglist.push(a.node)

    match tok_at(toks, a.pos) {
      TSemi => p = a.pos + 1
      TRParen => return Ok({arglist: arglist, pos: a.pos + 1})
      _ => return Err(jq_err("Expected ; or ) in call arguments"))
    }
  }

  return Ok({arglist: arglist, pos: p})
}

pure parse_if(toks: List[Tok], pos: Int) -> Result[PJq] {
  let cond = parse_pipe(toks, pos)?

  match tok_at(toks, cond.pos) {
    TIdent(s) => {
      if s != "then" {
        return Err(jq_err("Expected 'then'"))
      }
    }
    _ => return Err(jq_err("Expected 'then'"))
  }

  let then_b = parse_pipe(toks, cond.pos + 1)?
  let t = tok_at(toks, then_b.pos)

  match t {
    TIdent(s) => {
      if s == "elif" {
        let rest = parse_if(toks, then_b.pos + 1)?
        return Ok({node: IfElse(cond.node, then_b.node, rest.node), pos: rest.pos})
      }

      if s == "else" {
        let else_b = parse_pipe(toks, then_b.pos + 1)?

        match tok_at(toks, else_b.pos) {
          TIdent(e) => {
            if e != "end" {
              return Err(jq_err("Expected 'end'"))
            }

            return Ok({node: IfElse(cond.node, then_b.node, else_b.node), pos: else_b.pos + 1})
          }
          _ => return Err(jq_err("Expected 'end'"))
        }
      }

      if s == "end" {
        # if c then t end  ==  if c then t else . end
        return Ok({node: IfElse(cond.node, then_b.node, Identity), pos: then_b.pos + 1})
      }

      return Err(jq_err("Expected 'elif', 'else', or 'end'"))
    }
    _ => return Err(jq_err("Expected 'elif', 'else', or 'end'"))
  }
}

pure parse_object_ctor(toks: List[Tok], pos: Int) -> Result[PJq] {
  var p = pos + 1
  var entries: List[ObjEntry] = []

  match tok_at(toks, p) {
    TRBrace => return Ok({node: ObjectC(entries), pos: p + 1})
    _ => p = p
  }

  while true {
    let e = parse_obj_entry(toks, p)?
    entries = entries.push(e.entry)

    match tok_at(toks, e.pos) {
      TComma => p = e.pos + 1
      TRBrace => return Ok({node: ObjectC(entries), pos: e.pos + 1})
      _ => return Err(jq_err("Expected , or } in object"))
    }
  }

  return Ok({node: ObjectC(entries), pos: p})
}

type PObjEntry = {entry: ObjEntry, pos: Int}

pure parse_obj_entry(toks: List[Tok], pos: Int) -> Result[PObjEntry] {
  let t = tok_at(toks, pos)

  # Determine the key and whether a value follows.
  match t {
    TIdent(name) => return obj_entry_after_key(toks, pos + 1, Lit(JStr(name)), Field(Identity, name))
    TStr(s) => return obj_entry_after_key(toks, pos + 1, Lit(JStr(s)), Field(Identity, s))
    TVar(name) => return obj_entry_after_key(toks, pos + 1, Lit(JStr(name)), VarRef(name))
    TField(name) => return obj_entry_after_key(toks, pos + 1, Lit(JStr(name)), Field(Identity, name))
    TLParen => {
      let key = parse_pipe(toks, pos + 1)?

      match tok_at(toks, key.pos) {
        TRParen => return obj_entry_after_key(toks, key.pos + 1, key.node, Identity)
        _ => return Err(jq_err("Expected ) in object key"))
      }
    }
    _ => return Err(jq_err("Expected object key"))
  }
}

# After a key, either `: value` or shorthand (use `default_val`).
pure obj_entry_after_key(toks: List[Tok], pos: Int, key: Jq, default_val: Jq) -> Result[PObjEntry] {
  match tok_at(toks, pos) {
    TColon => {
      let v = parse_objval(toks, pos + 1)?
      return Ok({entry: {key: key, val: v.node}, pos: v.pos})
    }
    _ => return Ok({entry: {key: key, val: default_val}, pos: pos})
  }
}

# Object values bind tighter than comma but allow pipe chains.
pure parse_objval(toks: List[Tok], pos: Int) -> Result[PJq] {
  var cur = parse_alt(toks, pos)?

  while true {
    match tok_at(toks, cur.pos) {
      TPipe => {
        let right = parse_alt(toks, cur.pos + 1)?
        cur = {node: Pipe(cur.node, right.node), pos: right.pos}
      }
      _ => return Ok(cur)
    }
  }

  return Ok(cur)
}

pure parse_program(src: Str) -> Result[Jq] {
  let toks = lex(src)?
  let r = parse_pipe(toks, 0)?

  match tok_at(toks, r.pos) {
    TEOF => return Ok(r.node)
    _ => return Err(jq_err("Unexpected trailing tokens in program"))
  }
}

# ---------------------------------------------------------------------------
# Evaluator. A filter maps one input value to a stream (List) of outputs.
# ---------------------------------------------------------------------------
pure concat_json(a: List[Json], b: List[Json]) -> List[Json] {
  return a.extend(b)
}

pure is_truthy(j: Json) -> Bool {
  match j {
    JNull => false
    JBool(b) => b
    _ => true
  }
}

# Total order over JSON values: null < false < true < numbers < strings < arrays < objects.
pure type_rank(j: Json) -> Int {
  match j {
    JNull => 0
    JBool(_) => 1
    JNum(_) => 2
    JStr(_) => 3
    JArr(_) => 4
    JObj(_) => 5
  }
}

pure str_cmp_raw(a: Str, b: Str) -> Int {
  let la = a.byte_len()
  let lb = b.byte_len()
  var i = 0

  while i < la and i < lb {
    let ca = a.byte_at(i, 0)
    let cb = b.byte_at(i, 0)

    if ca < cb {
      return -1
    }

    if ca > cb {
      return 1
    }

    i = i + 1
  }

  if la < lb {
    return -1
  }

  if la > lb {
    return 1
  }

  return 0
}

pure sort_strs(xs: List[Str]) -> List[Str] {
  var out: List[Str] = []

  for x in xs {
    var inserted = false
    var next: List[Str] = []

    for y in out {
      if ! inserted and str_cmp_raw(x, y) < 0 {
        next = next.push(x)
        inserted = true
      }

      next = next.push(y)
    }

    if ! inserted {
      next = next.push(x)
    }

    out = next
  }

  return out
}

pure json_cmp(a: Json, b: Json) -> Int {
  let ra = type_rank(a)
  let rb = type_rank(b)

  if ra < rb {
    return -1
  }

  if ra > rb {
    return 1
  }

  match a {
    JNull => return 0
    JBool(x) => return bool_cmp(x, b)
    JNum(x) => return num_cmp(x, b)
    JStr(x) => return jstr_cmp(x, b)
    JArr(xs) => return arr_cmp(xs, b)
    JObj(es) => return obj_cmp(es, b)
  }
}

pure bool_cmp(x: Bool, b: Json) -> Int {
  match b {
    JBool(y) => {
      if x == y {
        return 0
      }

      if x {
        return 1
      }

      return -1
    }
    _ => return 0
  }
}

pure num_cmp(x: Float, b: Json) -> Int {
  match b {
    JNum(y) => {
      if x < y {
        return -1
      }

      if x > y {
        return 1
      }

      return 0
    }
    _ => return 0
  }
}

pure jstr_cmp(x: Str, b: Json) -> Int {
  match b {
    JStr(y) => return str_cmp_raw(x, y)
    _ => return 0
  }
}

pure arr_cmp(xs: List[Json], b: Json) -> Int {
  match b {
    JArr(ys) => {
      var i = 0

      while i < xs.len() and i < ys.len() {
        let c = json_cmp(xs.get(i, JNull), ys.get(i, JNull))

        if c != 0 {
          return c
        }

        i = i + 1
      }

      if xs.len() < ys.len() {
        return -1
      }

      if xs.len() > ys.len() {
        return 1
      }

      return 0
    }
    _ => return 0
  }
}

# jq orders objects by their sorted key arrays, then by values in sorted-key order.
pure obj_cmp(es: List[Entry], b: Json) -> Int {
  match b {
    JObj(fents) => {
      let ka = sort_strs(entry_keys(es))
      let kb = sort_strs(entry_keys(fents))
      var i = 0

      while i < ka.len() and i < kb.len() {
        let c = str_cmp_raw(ka.get(i, ""), kb.get(i, ""))

        if c != 0 {
          return c
        }

        i = i + 1
      }

      if ka.len() < kb.len() {
        return -1
      }

      if ka.len() > kb.len() {
        return 1
      }

      var j = 0

      while j < ka.len() {
        let key = ka.get(j, "")
        let c = json_cmp(entry_get(es, key), entry_get(fents, key))

        if c != 0 {
          return c
        }

        j = j + 1
      }

      return 0
    }
    _ => return 0
  }
}

pure json_eq(a: Json, b: Json) -> Bool {
  return json_cmp(a, b) == 0
}

# Object entry helpers (ordered association lists).
pure entry_keys(es: List[Entry]) -> List[Str] {
  [e.k for e in es]
}

pure entry_has(es: List[Entry], k: Str) -> Bool {
  for e in es {
    if e.k == k {
      return true
    }
  }

  return false
}

pure entry_get(es: List[Entry], k: Str) -> Json {
  for e in es {
    if e.k == k {
      return e.v
    }
  }

  return JNull
}

pure obj_set(es: List[Entry], k: Str, v: Json) -> List[Entry] {
  var out: List[Entry] = []
  var found = false

  for e in es {
    if e.k == k {
      out = out.push({k: k, v: v})
      found = true
    } else {
      out = out.push(e)
    }
  }

  if ! found {
    out = out.push({k: k, v: v})
  }

  return out
}

pure obj_merge(a: List[Entry], b: List[Entry]) -> List[Entry] {
  var out = a

  for e in b {
    out = obj_set(out, e.k, e.v)
  }

  return out
}

pure obj_deep_merge(a: List[Entry], b: List[Entry]) -> List[Entry] {
  var out = a

  for e in b {
    if entry_has(a, e.k) {
      let av = entry_get(a, e.k)

      match av {
        JObj(aes) => {
          match e.v {
            JObj(bes) => out = obj_set(out, e.k, JObj(obj_deep_merge(aes, bes)))
            _ => out = obj_set(out, e.k, e.v)
          }
        }
        _ => out = obj_set(out, e.k, e.v)
      }
    } else {
      out = obj_set(out, e.k, e.v)
    }
  }

  return out
}

# jq arithmetic. Numbers are Float throughout; division/modulo guard zero.
pure repeat_str(s: Str, n: Int) -> Str {
  var out: List[Str] = []
  var i = 0

  while i < n {
    out = out.push(s)
    i = i + 1
  }

  return out.join("")
}

pure add_values(a: Json, b: Json) -> Result[Json] {
  match a {
    JNull => return Ok(b)
    JNum(x) => {
      match b {
        JNum(y) => return Ok(JNum(x + y))
        JNull => return Ok(a)
        _ => return Err(jq_err(arith_msg("added", a, b)))
      }
    }
    JStr(x) => {
      match b {
        JStr(y) => return Ok(JStr(x + y))
        JNull => return Ok(a)
        _ => return Err(jq_err(arith_msg("added", a, b)))
      }
    }
    JArr(xs) => {
      match b {
        JArr(ys) => return Ok(JArr(xs.extend(ys)))
        JNull => return Ok(a)
        _ => return Err(jq_err(arith_msg("added", a, b)))
      }
    }
    JObj(es) => {
      match b {
        JObj(fents) => return Ok(JObj(obj_merge(es, fents)))
        JNull => return Ok(a)
        _ => return Err(jq_err(arith_msg("added", a, b)))
      }
    }
    JBool(_) => {
      match b {
        JNull => return Ok(a)
        _ => return Err(jq_err(arith_msg("added", a, b)))
      }
    }
  }
}

pure arith_msg(verb: Str, a: Json, b: Json) -> Str {
  return type_name(a) + " and " + type_name(b) + " cannot be " + verb
}

pure sub_values(a: Json, b: Json) -> Result[Json] {
  match a {
    JNum(x) => {
      match b {
        JNum(y) => return Ok(JNum(x - y))
        _ => return Err(jq_err(arith_msg("subtracted", a, b)))
      }
    }
    JArr(xs) => {
      match b {
        JArr(ys) => {
          var out = [x for x in xs if ! list_contains_json(ys, x)]
          return Ok(JArr(out))
        }
        _ => return Err(jq_err(arith_msg("subtracted", a, b)))
      }
    }
    _ => return Err(jq_err(arith_msg("subtracted", a, b)))
  }
}

pure list_contains_json(xs: List[Json], target: Json) -> Bool {
  for x in xs {
    if json_eq(x, target) {
      return true
    }
  }

  return false
}

pure mul_values(a: Json, b: Json) -> Result[Json] {
  match a {
    JNum(x) => {
      match b {
        JNum(y) => return Ok(JNum(x * y))
        JStr(s) => return mul_str(s, x)
        _ => return Err(jq_err(arith_msg("multiplied", a, b)))
      }
    }
    JStr(s) => {
      match b {
        JNum(y) => return mul_str(s, y)
        _ => return Err(jq_err(arith_msg("multiplied", a, b)))
      }
    }
    JObj(es) => {
      match b {
        JObj(fents) => return Ok(JObj(obj_deep_merge(es, fents)))
        _ => return Err(jq_err(arith_msg("multiplied", a, b)))
      }
    }
    _ => return Err(jq_err(arith_msg("multiplied", a, b)))
  }
}

pure mul_str(s: Str, n: Float) -> Result[Json] {
  let count = n.floor() ?? 0

  if count <= 0 {
    return Ok(JNull)
  }

  return Ok(JStr(repeat_str(s, count)))
}

pure div_values(a: Json, b: Json) -> Result[Json] {
  match a {
    JNum(x) => {
      match b {
        JNum(y) => {
          if y == 0.0 {
            return Err(jq_err("cannot divide " + render_num(x) + " by zero"))
          }

          return Ok(JNum(x / y))
        }
        _ => return Err(jq_err(arith_msg("divided", a, b)))
      }
    }
    JStr(x) => {
      match b {
        JStr(y) => return Ok(split_str(x, y))
        _ => return Err(jq_err(arith_msg("divided", a, b)))
      }
    }
    _ => return Err(jq_err(arith_msg("divided", a, b)))
  }
}

# Split string `s` by separator `sep` into a JArr of JStr (empty sep -> chars).
pure split_str(s: Str, sep: Str) -> Json {
  var parts: List[Str] = []

  if sep == "" {
    for ch in s.split("") {
      parts = parts.push(ch)
    }
  } else {
    parts = s.split(sep)
  }

  var out = [JStr(p) for p in parts]
  return JArr(out)
}

pure mod_values(a: Json, b: Json) -> Result[Json] {
  match a {
    JNum(x) => {
      match b {
        JNum(y) => {
          let yi = y.floor() ?? 0

          if yi == 0 {
            return Err(jq_err("cannot mod by zero"))
          }

          let xi = x.floor() ?? 0
          let m = int_mod(xi, yi)
          return Ok(JNum(m.float()))
        }
        _ => return Err(jq_err(arith_msg("divided", a, b)))
      }
    }
    _ => return Err(jq_err(arith_msg("divided", a, b)))
  }
}

# jq's % follows C: result takes the sign of the dividend, operands truncated.
pure int_mod(x: Int, y: Int) -> Int {
  var ax = x

  if ax < 0 {
    ax = 0 - ax
  }

  var ay = y

  if ay < 0 {
    ay = 0 - ay
  }

  let r = ax % ay

  if x < 0 {
    return 0 - r
  }

  return r
}

pure binop_apply(op: Str, a: Json, b: Json) -> Result[Json] {
  if op == "+" {
    return add_values(a, b)
  }

  if op == "-" {
    return sub_values(a, b)
  }

  if op == "*" {
    return mul_values(a, b)
  }

  if op == "/" {
    return div_values(a, b)
  }

  if op == "%" {
    return mod_values(a, b)
  }

  if op == "==" {
    return Ok(JBool(json_eq(a, b)))
  }

  if op == "!=" {
    return Ok(JBool(! json_eq(a, b)))
  }

  if op == "<" {
    return Ok(JBool(json_cmp(a, b) < 0))
  }

  if op == "<=" {
    return Ok(JBool(json_cmp(a, b) <= 0))
  }

  if op == ">" {
    return Ok(JBool(json_cmp(a, b) > 0))
  }

  if op == ">=" {
    return Ok(JBool(json_cmp(a, b) >= 0))
  }

  return Err(jq_err("unknown operator " + op))
}

pure as_obj_key(j: Json) -> Result[Str] {
  match j {
    JStr(s) => return Ok(s)
    _ => return Err(jq_err("Object keys must be strings"))
  }
}

pure type_name(j: Json) -> Str {
  match j {
    JNull => "null"
    JBool(_) => "boolean"
    JNum(_) => "number"
    JStr(_) => "string"
    JArr(_) => "array"
    JObj(_) => "object"
  }
}

# Index into a value by a key value (string key for objects, int for arrays).
pure index_value(input: Json, key: Json) -> Result[Json] {
  match input {
    JNull => return Ok(JNull)
    JObj(es) => {
      let k = as_obj_key(key)?

      for e in es {
        if e.k == k {
          return Ok(e.v)
        }
      }

      return Ok(JNull)
    }
    JArr(xs) => {
      match key {
        JNum(n) => {
          var i = n.floor() ?? 0

          if i < 0 {
            i = i + xs.len()
          }

          if i < 0 or i >= xs.len() {
            return Ok(JNull)
          }

          return Ok(xs.get(i, JNull))
        }
        _ => return Err(jq_err("Cannot index array with " + type_name(key)))
      }
    }
    _ => {
      match key {
        JStr(_) => return Err(jq_err("Cannot index " + type_name(input) + " with string"))
        _ => return Err(jq_err("Cannot index " + type_name(input)))
      }
    }
  }
}

pure iterate_value(input: Json) -> Result[List[Json]] {
  match input {
    JArr(xs) => return Ok(xs)
    JObj(es) => {
      var out = [e.v for e in es]
      return Ok(out)
    }
    _ => return Err(jq_err("Cannot iterate over " + type_name(input)))
  }
}

pure recurse_all(input: Json) -> List[Json] {
  var out: List[Json] = [input]

  match input {
    JArr(xs) => {
      for x in xs {
        out = out.extend(recurse_all(x))
      }
    }
    JObj(es) => {
      for e in es {
        out = out.extend(recurse_all(e.v))
      }
    }
    _ => out = out
  }

  return out
}

pure slice_value(input: Json, lo: Json, hi: Json) -> Result[Json] {
  match input {
    JNull => return Ok(JNull)
    JArr(xs) => {
      let len = xs.len()
      let bounds = slice_bounds(lo, hi, len)?
      var out: List[Json] = []
      var i = bounds.lo

      while i < bounds.hi {
        out = out.push(xs.get(i, JNull))
        i = i + 1
      }

      return Ok(JArr(out))
    }
    JStr(s) => {
      let cs = s.split("")
      let len = cs.len()
      let bounds = slice_bounds(lo, hi, len)?
      var out: List[Str] = []
      var i = bounds.lo

      while i < bounds.hi {
        out = out.push(cs.get(i, ""))
        i = i + 1
      }

      return Ok(JStr(out.join("")))
    }
    _ => return Err(jq_err("Cannot slice " + type_name(input)))
  }
}

type SliceBounds = {lo: Int, hi: Int}

pure clamp_index(n: Int, len: Int) -> Int {
  var i = n

  if i < 0 {
    i = i + len
  }

  if i < 0 {
    i = 0
  }

  if i > len {
    i = len
  }

  return i
}

pure slice_bounds(lo: Json, hi: Json, len: Int) -> Result[SliceBounds] {
  var lo_i = 0
  var hi_i = len

  match lo {
    JNull => lo_i = 0
    JNum(n) => lo_i = clamp_index(n.floor() ?? 0, len)
    _ => return Err(jq_err("Slice bounds must be numbers"))
  }

  match hi {
    JNull => hi_i = len
    JNum(n) => hi_i = clamp_index(n.floor() ?? 0, len)
    _ => return Err(jq_err("Slice bounds must be numbers"))
  }

  if hi_i < lo_i {
    hi_i = lo_i
  }

  return Ok({lo: lo_i, hi: hi_i})
}

# Evaluate a slice node; lo/hi are sub-filters (Identity used for an omitted
# bound, which yields null via the slice machinery only when wrapped — so we
# special-case omitted bounds in the parser by passing Identity and reading the
# whole-input value; for jq parity an omitted bound is null).
pure eval(ast: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  match ast {
    Identity => return Ok([input])
    Empty => {
      let none: List[Json] = []
      return Ok(none)
    }
    RecurseDefault => return Ok(recurse_all(input))
    Lit(v) => return Ok([v])
    VarRef(name) => {
      let v = lookup_var(scope, name)?
      return Ok([v])
    }
    Field(base, name) => {
      let bases = eval(base, input, scope)?
      var out = [index_value(b, JStr(name))? for b in bases]
      return Ok(out)
    }
    Index(base, idx) => {
      let bases = eval(base, input, scope)?
      var out: List[Json] = []

      for b in bases {
        let keys = eval(idx, input, scope)?

        for k in keys {
          out = out.push(index_value(b, k)?)
        }
      }

      return Ok(out)
    }
    Iterate(base) => {
      let bases = eval(base, input, scope)?
      var out: List[Json] = []

      for b in bases {
        let items = iterate_value(b)?
        out = out.extend(items)
      }

      return Ok(out)
    }
    Slice(base, lo, hi) => {
      let bases = eval(base, input, scope)?
      var out: List[Json] = []

      for b in bases {
        let los = eval_bound(lo, input, scope)?
        let his = eval_bound(hi, input, scope)?

        for lv in los {
          for hv in his {
            out = out.push(slice_value(b, lv, hv)?)
          }
        }
      }

      return Ok(out)
    }
    Pipe(a, b) => {
      let outs = eval(a, input, scope)?
      var acc: List[Json] = []

      for x in outs {
        let ys = eval(b, x, scope)?
        acc = acc.extend(ys)
      }

      return Ok(acc)
    }
    Comma(a, b) => {
      let l = eval(a, input, scope)?
      let r = eval(b, input, scope)?
      return Ok(concat_json(l, r))
    }
    ArrayC(inner) => {
      let outs = eval(inner, input, scope)?
      return Ok([JArr(outs)])
    }
    ObjectC(entries) => return eval_object(entries, input, scope)
    Neg(inner) => {
      let outs = eval(inner, input, scope)?
      var res: List[Json] = []

      for v in outs {
        match v {
          JNum(n) => res = res.push(JNum(0.0 - n))
          _ => return Err(jq_err(type_name(v) + " cannot be negated"))
        }
      }

      return Ok(res)
    }
    BinOp(op, a, b) => return eval_binop(op, a, b, input, scope)
    IfElse(cond, then_b, else_b) => {
      let conds = eval(cond, input, scope)?
      var out: List[Json] = []

      for c in conds {
        if is_truthy(c) {
          out = out.extend(eval(then_b, input, scope)?)
        } else {
          out = out.extend(eval(else_b, input, scope)?)
        }
      }

      return Ok(out)
    }
    TryCatch(a, b) => {
      match eval(a, input, scope) {
        Ok(vs) => return Ok(vs)
        Err(e) => return eval(b, JStr(e.message), scope)
      }
    }
    Optional(inner) => {
      match eval(inner, input, scope) {
        Ok(vs) => return Ok(vs)
        Err(_) => {
          let none: List[Json] = []
          return Ok(none)
        }
      }
    }
    Alt(a, b) => {
      var kept: List[Json] = []

      match eval(a, input, scope) {
        Ok(vs) => {
          for v in vs {
            if is_truthy(v) {
              kept = kept.push(v)
            }
          }
        }
        Err(_) => kept = kept
      }

      if kept.len() > 0 {
        return Ok(kept)
      }

      return eval(b, input, scope)
    }
    Call(name, callargs) => return eval_call_resolved(name, callargs, input, scope)
    FuncDef(fdef, rest) => return eval(rest, input, EnvFunc(fdef, scope, scope))
    BindVar(src, pat, body) => return eval_bind(src, pat, body, input, scope)
    Reduce(src, pat, init, upd) => return eval_reduce(src, pat, init, upd, input, scope)
    Foreach(src, pat, init, upd, has_x, extract) => return eval_foreach(
      src,
      pat,
      init,
      upd,
      has_x,
      extract,
      input,
      scope,
    )
    StrInterp(parts, fmt) => return eval_str_interp(parts, fmt, input, scope)
    Fmt(name) => return Ok([JStr(apply_format(name, input)?)])
    Assign(pathexpr, rhs) => return eval_assign(pathexpr, rhs, input, scope)
    Update(pathexpr, rhs) => return eval_update(pathexpr, rhs, input, scope)
    ArithUpdate(op, pathexpr, rhs) => return eval_arith_update(op, pathexpr, rhs, input, scope)
    _ => return Err(jq_err("unimplemented filter"))
  }
}

# Slice bound sub-filter: Identity here marks an omitted bound -> null.
pure eval_bound(b: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  match b {
    Identity => return Ok([JNull])
    _ => return eval(b, input, scope)
  }
}

pure lookup_var(scope: Env, name: Str) -> Result[Json] {
  match scope {
    EnvVar(n, v, parent) => {
      if n == name {
        return Ok(v)
      }

      return lookup_var(parent, name)
    }
    EnvFilter(_, _, parent) => return lookup_var(parent, name)
    EnvFunc(_, _, parent) => return lookup_var(parent, name)
    EnvEmpty => return Err(jq_err("$" + name + " is not defined"))
  }
}

type FilterLookup = FilterFound(Closure) | FilterNone

pure lookup_filter(scope: Env, name: Str) -> FilterLookup {
  match scope {
    EnvFilter(n, clo, parent) => {
      if n == name {
        return FilterFound(clo)
      }

      return lookup_filter(parent, name)
    }
    EnvVar(_, _, parent) => return lookup_filter(parent, name)
    EnvFunc(_, _, parent) => return lookup_filter(parent, name)
    EnvEmpty => return FilterNone
  }
}

type FuncLookup = FuncFound(FnDef, Env) | FuncNone

pure lookup_func(scope: Env, name: Str, arity: Int) -> FuncLookup {
  match scope {
    EnvFunc(fdef, capture, parent) => {
      if fdef.fname == name and fdef.params.len() == arity {
        return FuncFound(fdef, capture)
      }

      return lookup_func(parent, name, arity)
    }
    EnvVar(_, _, parent) => return lookup_func(parent, name, arity)
    EnvFilter(_, _, parent) => return lookup_func(parent, name, arity)
    EnvEmpty => return FuncNone
  }
}

# Object construction: cartesian product across entries and within each entry's
# key-stream x value-stream.
pure eval_object(entries: List[ObjEntry], input: Json, scope: Env) -> Result[List[Json]] {
  let empty_entries: List[Entry] = []
  var partials: List[List[Entry]] = [empty_entries]

  for ent in entries {
    let keys = eval(ent.key, input, scope)?
    let vals = eval(ent.val, input, scope)?
    var next: List[List[Entry]] = []

    for p in partials {
      for k in keys {
        let ks = as_obj_key(k)?

        for v in vals {
          next = next.push(p.push({k: ks, v: v}))
        }
      }
    }

    partials = next
  }

  var out = [JObj(es) for es in partials]
  out
}

# `=`: RHS evaluated against the ROOT input (jq quirk), one result per RHS value.
pure eval_assign(pathexpr: Jq, rhs: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let rhsvals = eval(rhs, input, scope)?
  var results: List[Json] = []

  for nv in rhsvals {
    let paths = eval_paths(pathexpr, input, scope)?
    var cur = input

    for p in paths {
      cur = setpath(cur, p, nv)?
    }

    results = results.push(cur)
  }

  results
}

# `|=`: per path, replace the value with the FIRST output of RHS on the old value;
# empty output deletes the path.
pure eval_update(pathexpr: Jq, rhs: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let paths = eval_paths(pathexpr, input, scope)?
  var cur = input

  for p in paths {
    let old = getpath(cur, p)?
    let news = eval(rhs, old, scope)?

    if news.len() == 0 {
      cur = delpaths(cur, [p])?
    } else {
      cur = setpath(cur, p, news.get(0, JNull))?
    }
  }

  [cur]
}

# `+=` family: `a OP= b` updates each path with `old OP b`, where b is against root.
pure eval_arith_update(op: Str, pathexpr: Jq, rhs: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let rhsvals = eval(rhs, input, scope)?
  var results: List[Json] = []

  for bval in rhsvals {
    let paths = eval_paths(pathexpr, input, scope)?
    var cur = input

    for p in paths {
      let old = getpath(cur, p)?

      if op == "//" {
        if ! is_truthy(old) {
          cur = setpath(cur, p, bval)?
        }
      } else {
        cur = setpath(cur, p, binop_apply(op, old, bval)?)?
      }
    }

    results = results.push(cur)
  }

  results
}

# String interpolation: cartesian over each \(...) slot, formatting interpolated
# values (literal chunks pass through untouched).
pure eval_str_interp(parts: List[Jq], fmt: Str, input: Json, scope: Env) -> Result[List[Json]] {
  var partials = [""]

  for part in parts {
    match part {
      StrLit(s) => {
        var next = [pre + s for pre in partials]
        partials = next
      }
      StrExpr(e) => {
        let vals = eval(e, input, scope)?
        var next: List[Str] = []

        for pre in partials {
          for v in vals {
            let piece = if fmt == "" { to_string_json(v) } else { apply_format(fmt, v)? }
            next = next.push(pre + piece)
          }
        }

        partials = next
      }
      _ => return Err(jq_err("internal: unexpected Jq variant in string interpolation"))
    }
  }

  var out = [JStr(s) for s in partials]
  out
}

pure str_of(j: Json, ctx: Str) -> Result[Str] {
  match j {
    JStr(s) => return Ok(s)
    _ => return Err(jq_err(ctx + " requires a string, got " + type_name(j)))
  }
}

pure join_values(xs: List[Json], sep: Str) -> Result[Str] {
  var parts: List[Str] = []

  for x in xs {
    match x {
      JNull => parts = parts.push("")
      JStr(s) => parts = parts.push(s)
      JNum(n) => parts = parts.push(render_num(n))
      JBool(b) => parts = parts.push(if b { "true" } else { "false" })
      _ => return Err(jq_err("Cannot join a list containing arrays or objects"))
    }
  }

  parts.join(sep)
}

pure hex2(b: Int) -> Str {
  let digits = "0123456789ABCDEF"
  return digits.byte_slice(b / 16, 1) + digits.byte_slice(b % 16, 1)
}

pure is_uri_unreserved(b: Int) -> Bool {
  if b >= 65 and b <= 90 {
    return true
  }

  if b >= 97 and b <= 122 {
    return true
  }

  if b >= 48 and b <= 57 {
    return true
  }

  return b == 45 or b == 95 or b == 46 or b == 126
}

pure uri_encode(s: Str) -> Str {
  var out: List[Str] = []
  let n = s.byte_len()
  var i = 0

  while i < n {
    let b = s.byte_at(i, 0)

    if is_uri_unreserved(b) {
      out = out.push(s.byte_slice(i, 1))
    } else {
      out = out.push("%" + hex2(b))
    }

    i = i + 1
  }

  return out.join("")
}

pure html_escape(s: Str) -> Str {
  var r = s.replace("&", "&amp;")
  r = r.replace("<", "&lt;")
  r = r.replace(">", "&gt;")
  r = r.replace("'", "&apos;")
  r = r.replace("\"", "&quot;")
  return r
}

pure tsv_escape(s: Str) -> Str {
  var r = s.replace("\\", "\\\\")
  r = r.replace("\t", "\\t")
  r = r.replace("\n", "\\n")
  r = r.replace("\r", "\\r")
  return r
}

pure sh_quote(s: Str) -> Str {
  return "'" + s.replace("'", "'\\''") + "'"
}

pure fmt_cell_sh(x: Json) -> Result[Str] {
  match x {
    JStr(s) => return Ok(sh_quote(s))
    JNum(n) => return Ok(render_num(n))
    JBool(b) => return Ok(if b { "true" } else { "false" })
    JNull => return Ok("null")
    _ => return Err(jq_err("@sh: arrays and objects cannot be escaped"))
  }
}

pure apply_format(name: Str, value: Json) -> Result[Str] {
  if name == "text" {
    return Ok(to_string_json(value))
  }

  if name == "json" {
    return Ok(ser(value))
  }

  if name == "base64" {
    return Ok(bytes.from_text(to_string_json(value)).base64())
  }

  if name == "base64d" {
    let s = to_string_json(value)
    let b = s.base64_decode()?
    return b.utf8()
  }

  if name == "base32" {
    return Ok(bytes.from_text(to_string_json(value)).base32())
  }

  if name == "uri" {
    return Ok(uri_encode(to_string_json(value)))
  }

  if name == "html" {
    return Ok(html_escape(to_string_json(value)))
  }

  if name == "csv" or name == "tsv" {
    let xs = require_array(value, "@csv/@tsv")?
    var cells: List[Str] = []

    for x in xs {
      match x {
        JNum(n) => cells = cells.push(render_num(n))
        JStr(s) => {
          if name == "csv" {
            cells = cells.push("\"" + s.replace("\"", "\"\"") + "\"")
          } else {
            cells = cells.push(tsv_escape(s))
          }
        }
        JNull => cells = cells.push("")
        JBool(b) => cells = cells.push(if b { "true" } else { "false" })
        _ => return Err(jq_err("@csv/@tsv: arrays and objects not valid in a row"))
      }
    }

    if name == "csv" {
      return Ok(cells.join(","))
    }

    return Ok(cells.join("\t"))
  }

  if name == "sh" {
    match value {
      JArr(xs) => {
        var parts = [fmt_cell_sh(x)? for x in xs]
        return Ok(parts.join(" "))
      }
      _ => return fmt_cell_sh(value)
    }
  }

  return Err(jq_err(name + " is not a valid format"))
}

# Binary operators. The cartesian product iterates b outer, a inner (jq order).
pure eval_binop(op: Str, a: Jq, b: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  if op == "and" or op == "or" {
    return eval_logic(op, a, b, input, scope)
  }

  let as_ = eval(a, input, scope)?
  let bs = eval(b, input, scope)?
  var out: List[Json] = []

  for bv in bs {
    for av in as_ {
      out = out.push(binop_apply(op, av, bv)?)
    }
  }

  out
}

# `and`/`or` short-circuit on the left operand's truthiness, per output.
pure eval_logic(op: Str, a: Jq, b: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let as_ = eval(a, input, scope)?
  var out: List[Json] = []

  for av in as_ {
    let short = if op == "and" { ! is_truthy(av) } else { is_truthy(av) }

    if short {
      out = out.push(JBool(op == "or"))
    } else {
      let bs = eval(b, input, scope)?

      for bv in bs {
        out = out.push(JBool(is_truthy(bv)))
      }
    }
  }

  out
}

pure json_error(j: Json) -> Error {
  match j {
    JStr(s) => return jq_err(s)
    _ => return jq_err(type_name(j) + " (" + ser(j) + ") not a string")
  }
}

type KV = {key: Json, val: Json}

# Stable insertion sort over key/value pairs (ties keep input order).
pure sort_kv(items: List[KV]) -> List[KV] {
  var out: List[KV] = []

  for it in items {
    var next: List[KV] = []
    var inserted = false

    for o in out {
      if ! inserted and json_cmp(it.key, o.key) < 0 {
        next = next.push(it)
        inserted = true
      }

      next = next.push(o)
    }

    if ! inserted {
      next = next.push(it)
    }

    out = next
  }

  return out
}

# Build [key, value] pairs where the key is [f] (jq sorts/groups by the array of f outputs).
pure kv_by(xs: List[Json], f: Jq, scope: Env) -> Result[List[KV]] {
  var out: List[KV] = []

  for x in xs {
    let ks = eval(f, x, scope)?
    out = out.push({key: JArr(ks), val: x})
  }

  out
}

pure first_or(vs: List[Json], fallback: Json) -> Json {
  if vs.len() == 0 {
    return fallback
  }

  return vs.get(0, fallback)
}

pure require_array(j: Json, ctx: Str) -> Result[List[Json]] {
  match j {
    JArr(xs) => return Ok(xs)
    _ => return Err(jq_err(ctx + " requires an array, got " + type_name(j)))
  }
}

pure flatten_into(xs: List[Json], depth: Int, out: List[Json]) -> List[Json] {
  var acc = out

  for x in xs {
    match x {
      JArr(inner) => {
        if depth > 0 {
          acc = flatten_into(inner, depth - 1, acc)
        } else {
          acc = acc.push(x)
        }
      }
      _ => acc = acc.push(x)
    }
  }

  return acc
}

pure obj_contains(aes: List[Entry], bes: List[Entry]) -> Bool {
  for be in bes {
    if ! entry_has(aes, be.k) {
      return false
    }

    if ! contains_json(entry_get(aes, be.k), be.v) {
      return false
    }
  }

  return true
}

pure arr_contains(axs: List[Json], bxs: List[Json]) -> Bool {
  for bx in bxs {
    var found = false

    for ax in axs {
      if contains_json(ax, bx) {
        found = true
      }
    }

    if ! found {
      return false
    }
  }

  return true
}

# jq `contains`: recursive structural containment.
pure contains_json(a: Json, b: Json) -> Bool {
  match b {
    JObj(bes) => {
      match a {
        JObj(aes) => return obj_contains(aes, bes)
        _ => return false
      }
    }
    JArr(bxs) => {
      match a {
        JArr(axs) => return arr_contains(axs, bxs)
        _ => return false
      }
    }
    JStr(bs) => {
      match a {
        JStr(as_) => return bs in as_
        _ => return false
      }
    }
    _ => return json_eq(a, b)
  }
}

pure range_gen(from: Float, to: Float, step: Float) -> List[Json] {
  var out: List[Json] = []

  if step == 0.0 {
    return out
  }

  var x = from

  if step > 0.0 {
    while x < to {
      out = out.push(JNum(x))
      x = x + step
    }
  } else {
    while x > to {
      out = out.push(JNum(x))
      x = x + step
    }
  }

  return out
}

pure fsqrt(x: Float) -> Float {
  if x <= 0.0 {
    return 0.0
  }

  var guess = x
  var i = 0

  while i < 60 {
    guess = (guess + x / guess) / 2.0
    i = i + 1
  }

  return guess
}

pure jnum_of(j: Json, ctx: Str) -> Result[Float] {
  match j {
    JNum(n) => return Ok(n)
    _ => return Err(jq_err(ctx + " requires a number, got " + type_name(j)))
  }
}

# tostring: strings pass through; everything else is compact-JSON encoded.
pure to_string_json(j: Json) -> Str {
  match j {
    JStr(s) => s
    _ => ser(j)
  }
}

pure to_entries_of(es: List[Entry]) -> List[Json] {
  [JObj([{k: "key", v: JStr(e.k)}, {k: "value", v: e.v}]) for e in es]
}

# from_entries: accept key/k/name/Name/Key and value/v/Value; keys coerced to string.
pure entry_key_field(es: List[Entry]) -> Str {
  let names = ["key", "k", "name", "Name", "Key", "K"]

  for n in names {
    if entry_has(es, n) {
      return to_string_json(entry_get(es, n))
    }
  }

  return "null"
}

pure entry_val_field(es: List[Entry]) -> Json {
  let names = ["value", "v", "Value"]

  for n in names {
    if entry_has(es, n) {
      return entry_get(es, n)
    }
  }

  return JNull
}

pure from_entries_of(xs: List[Json]) -> Result[Json] {
  var out: List[Entry] = []

  for x in xs {
    match x {
      JObj(es) => out = obj_set(out, entry_key_field(es), entry_val_field(es))
      _ => return Err(jq_err("from_entries requires an array of objects"))
    }
  }

  return Ok(JObj(out))
}

# ---------------------------------------------------------------------------
# Path mode: jq evaluates the LHS of assignment as a set of paths (lists of
# string keys / int indices), then reads/updates/deletes values at those paths.
# ---------------------------------------------------------------------------
pure getpath(v: Json, pth: List[Json]) -> Result[Json] {
  var cur = v

  for seg in pth {
    cur = index_value(cur, seg)?
  }

  return Ok(cur)
}

pure obj_entries_or_empty(v: Json) -> Result[List[Entry]] {
  match v {
    JObj(es) => return Ok(es)
    JNull => {
      let none: List[Entry] = []
      return Ok(none)
    }
    _ => return Err(jq_err("Cannot index " + type_name(v) + " with a string key"))
  }
}

pure arr_or_empty(v: Json) -> Result[List[Json]] {
  match v {
    JArr(xs) => return Ok(xs)
    JNull => {
      let none: List[Json] = []
      return Ok(none)
    }
    _ => return Err(jq_err("Cannot index " + type_name(v) + " with a number"))
  }
}

pure setpath_at(v: Json, pth: List[Json], idx: Int, nv: Json) -> Result[Json] {
  if idx >= pth.len() {
    return Ok(nv)
  }

  let seg = pth.get(idx, JNull)

  match seg {
    JStr(k) => {
      let es = obj_entries_or_empty(v)?
      let child = entry_get(es, k)
      let newchild = setpath_at(child, pth, idx + 1, nv)?
      return Ok(JObj(obj_set(es, k, newchild)))
    }
    JNum(n) => {
      let xs = arr_or_empty(v)?
      var i = n.floor() ?? 0

      if i < 0 {
        i = i + xs.len()

        if i < 0 {
          return Err(jq_err("Out of bounds negative array index"))
        }
      }

      let child = if i < xs.len() { xs.get(i, JNull) } else { JNull }
      let newchild = setpath_at(child, pth, idx + 1, nv)?
      var out: List[Json] = []
      var target_len = xs.len()

      if i + 1 > target_len {
        target_len = i + 1
      }

      var j = 0

      while j < target_len {
        if j == i {
          out = out.push(newchild)
        } else if j < xs.len() {
          out = out.push(xs.get(j, JNull))
        } else {
          out = out.push(JNull)
        }

        j = j + 1
      }

      return Ok(JArr(out))
    }
    _ => return Err(jq_err("Path segments must be strings or numbers"))
  }
}

pure setpath(v: Json, pth: List[Json], nv: Json) -> Result[Json] {
  return setpath_at(v, pth, 0, nv)
}

pure obj_remove(es: List[Entry], k: Str) -> List[Entry] {
  [e for e in es if e.k != k]
}

pure arr_remove(xs: List[Json], idx: Int) -> List[Json] {
  var i = idx

  if i < 0 {
    i = i + xs.len()
  }

  var out: List[Json] = []
  var j = 0

  while j < xs.len() {
    if j != i {
      out = out.push(xs.get(j, JNull))
    }

    j = j + 1
  }

  return out
}

pure remove_key(v: Json, seg: Json) -> Result[Json] {
  match v {
    JNull => return Ok(JNull)
    JObj(es) => {
      let k = as_obj_key(seg)?
      return Ok(JObj(obj_remove(es, k)))
    }
    JArr(xs) => {
      match seg {
        JNum(n) => return Ok(JArr(arr_remove(xs, n.floor() ?? 0)))
        _ => return Err(jq_err("Cannot delete array element with non-number"))
      }
    }
    _ => return Err(jq_err("Cannot delete from " + type_name(v)))
  }
}

pure del_path(v: Json, pth: List[Json], idx: Int) -> Result[Json] {
  if pth.len() == 0 {
    return Ok(JNull)
  }

  let seg = pth.get(idx, JNull)

  if idx == pth.len() - 1 {
    return remove_key(v, seg)
  }

  match v {
    JNull => return Ok(JNull)
    JObj(es) => {
      let k = as_obj_key(seg)?

      if ! entry_has(es, k) {
        return Ok(v)
      }

      let newchild = del_path(entry_get(es, k), pth, idx + 1)?
      return Ok(JObj(obj_set(es, k, newchild)))
    }
    JArr(xs) => {
      match seg {
        JNum(n) => {
          var i = n.floor() ?? 0

          if i < 0 {
            i = i + xs.len()
          }

          if i < 0 or i >= xs.len() {
            return Ok(v)
          }

          let newchild = del_path(xs.get(i, JNull), pth, idx + 1)?
          return setpath(v, [JNum(i.float())], newchild)
        }
        _ => return Err(jq_err("Cannot index array with non-number"))
      }
    }
    _ => return Err(jq_err("Cannot index " + type_name(v)))
  }
}

# Delete several paths; sort descending so earlier deletions don't shift later ones.
pure delpaths(v: Json, paths: List[List[Json]]) -> Result[Json] {
  var items = [{key: JArr(p), val: JArr(p)} for p in paths]
  let sorted = sort_kv(items)
  var cur = v
  var i = sorted.len() - 1

  while i >= 0 {
    let kv = sorted.get(i, {key: JNull, val: JNull})

    match kv.val {
      JArr(p) => cur = del_path(cur, p, 0)?
      _ => cur = cur
    }

    i = i - 1
  }

  return Ok(cur)
}

# All paths into `v` rooted at `prefix` (prefix included).
pure paths_from(v: Json, prefix: List[Json]) -> List[List[Json]] {
  var out: List[List[Json]] = [prefix]

  match v {
    JArr(xs) => {
      var i = 0

      for x in xs {
        out = out.extend(paths_from(x, prefix.push(JNum(i.float()))))
        i = i + 1
      }
    }
    JObj(es) => {
      for e in es {
        out = out.extend(paths_from(e.v, prefix.push(JStr(e.k))))
      }
    }
    _ => out = out
  }

  return out
}

pure path_to_json(p: List[Json]) -> Json {
  return JArr(p)
}

# Evaluate `ast` as a path expression against the current value `input`.
pure eval_paths(ast: Jq, input: Json, scope: Env) -> Result[List[List[Json]]] {
  match ast {
    Identity => {
      let empty: List[Json] = []
      return Ok([empty])
    }
    RecurseDefault => {
      let empty: List[Json] = []
      return Ok(paths_from(input, empty))
    }
    Field(base, name) => {
      let bps = eval_paths(base, input, scope)?
      var out = [bp.push(JStr(name)) for bp in bps]
      return Ok(out)
    }
    Index(base, idx) => {
      let bps = eval_paths(base, input, scope)?
      var out: List[List[Json]] = []

      for bp in bps {
        let keys = eval(idx, input, scope)?

        for k in keys {
          out = out.push(bp.push(k))
        }
      }

      return Ok(out)
    }
    Iterate(base) => {
      let bps = eval_paths(base, input, scope)?
      var out: List[List[Json]] = []

      for bp in bps {
        let v = getpath(input, bp)?

        match v {
          JArr(xs) => {
            var i = 0

            while i < xs.len() {
              out = out.push(bp.push(JNum(i.float())))
              i = i + 1
            }
          }
          JObj(es) => {
            for e in es {
              out = out.push(bp.push(JStr(e.k)))
            }
          }
          JNull => out = out
          _ => return Err(jq_err("Cannot iterate over " + type_name(v)))
        }
      }

      return Ok(out)
    }
    Pipe(a, b) => {
      let aps = eval_paths(a, input, scope)?
      var out: List[List[Json]] = []

      for ap in aps {
        let sub = getpath(input, ap)?
        let bps = eval_paths(b, sub, scope)?

        for bp in bps {
          out = out.push(ap.extend(bp))
        }
      }

      return Ok(out)
    }
    Comma(a, b) => {
      var out = eval_paths(a, input, scope)?
      out = out.extend(eval_paths(b, input, scope)?)
      return Ok(out)
    }
    IfElse(cond, then_b, else_b) => {
      let conds = eval(cond, input, scope)?
      var out: List[List[Json]] = []

      for c in conds {
        if is_truthy(c) {
          out = out.extend(eval_paths(then_b, input, scope)?)
        } else {
          out = out.extend(eval_paths(else_b, input, scope)?)
        }
      }

      return Ok(out)
    }
    Optional(inner) => {
      match eval_paths(inner, input, scope) {
        Ok(ps) => return Ok(ps)
        Err(_) => {
          let none: List[List[Json]] = []
          return Ok(none)
        }
      }
    }
    Call(name, callargs) => {
      if name == "select" and callargs.len() == 1 {
        let conds = eval(callargs.get(0, Identity), input, scope)?
        var keep = false

        for c in conds {
          if is_truthy(c) {
            keep = true
          }
        }

        if keep {
          let empty: List[Json] = []
          return Ok([empty])
        }

        let none: List[List[Json]] = []
        return Ok(none)
      }

      if name == "getpath" and callargs.len() == 1 {
        let pv = eval(callargs.get(0, Identity), input, scope)?
        var out = [json_to_path(p)? for p in pv]
        return Ok(out)
      }

      if name == "empty" {
        let none: List[List[Json]] = []
        return Ok(none)
      }

      if name == "first" and callargs.len() == 1 {
        let ps = eval_paths(callargs.get(0, Identity), input, scope)?
        let empty: List[Json] = []

        if ps.len() == 0 {
          let none: List[List[Json]] = []
          return Ok(none)
        }

        return Ok([ps.get(0, empty)])
      }

      return Err(jq_err("Invalid path expression: " + name))
    }
    _ => return Err(jq_err("Invalid path expression"))
  }
}

pure json_to_path(j: Json) -> Result[List[Json]] {
  match j {
    JArr(xs) => return Ok(xs)
    _ => return Err(jq_err("getpath requires an array pth"))
  }
}

type Dispatch = Handled(List[Json]) | Pass

pure type_filter(input: Json, keep: Bool) -> Dispatch {
  if keep {
    return Handled([input])
  }

  let none: List[Json] = []
  return Handled(none)
}

pure is_obj(j: Json) -> Bool {
  match j {
    JObj(_) => true
    _ => false
  }
}

pure is_arr(j: Json) -> Bool {
  match j {
    JArr(_) => true
    _ => false
  }
}

# Type predicates and scalar conversions (no arguments).
pure bi_typey(name: Str, input: Json) -> Result[Dispatch] {
  if name == "type" {
    return Ok(Handled([JStr(type_name(input))]))
  }

  if name == "not" {
    return Ok(Handled([JBool(! is_truthy(input))]))
  }

  if name == "length" {
    return Ok(Handled([length_of(input)?]))
  }

  if name == "utf8bytelength" {
    match input {
      JStr(s) => return Ok(Handled([JNum(s.byte_len().float())]))
      _ => return Err(jq_err(type_name(input) + " only strings have UTF-8 byte length"))
    }
  }

  if name == "keys" or name == "keys_unsorted" {
    return Ok(Handled([keys_of(input, name == "keys")?]))
  }

  if name == "values" {
    return Ok(type_filter(input, is_truthy_nonnull(input)))
  }

  if name == "nulls" {
    return Ok(type_filter(input, type_name(input) == "null"))
  }

  if name == "booleans" {
    return Ok(type_filter(input, type_name(input) == "boolean"))
  }

  if name == "numbers" {
    return Ok(type_filter(input, type_name(input) == "number"))
  }

  if name == "strings" {
    return Ok(type_filter(input, type_name(input) == "string"))
  }

  if name == "arrays" {
    return Ok(type_filter(input, is_arr(input)))
  }

  if name == "objects" {
    return Ok(type_filter(input, is_obj(input)))
  }

  if name == "iterables" {
    return Ok(type_filter(input, is_arr(input) or is_obj(input)))
  }

  if name == "scalars" {
    return Ok(type_filter(input, ! (is_arr(input) or is_obj(input))))
  }

  if name == "tostring" {
    return Ok(Handled([JStr(to_string_json(input))]))
  }

  if name == "tojson" {
    return Ok(Handled([JStr(ser(input))]))
  }

  if name == "fromjson" {
    match input {
      JStr(s) => {
        let vs = parse_stream(s)?
        return Ok(Handled([first_or(vs, JNull)]))
      }
      _ => return Err(jq_err("fromjson requires a string"))
    }
  }

  if name == "tonumber" {
    match input {
      JNum(_) => return Ok(Handled([input]))
      JStr(s) => return Ok(Handled([decode_num(s)?]))
      _ => return Err(jq_err(type_name(input) + " cannot be parsed as a number"))
    }
  }

  if name == "ascii_downcase" or name == "ascii_upcase" {
    match input {
      JStr(s) => {
        if name == "ascii_downcase" {
          return Ok(Handled([JStr(s.lower())]))
        }

        return Ok(Handled([JStr(s.upper())]))
      }
      _ => return Err(jq_err(type_name(input) + " cannot be case-folded"))
    }
  }

  return Ok(Pass)
}

pure is_truthy_nonnull(j: Json) -> Bool {
  match j {
    JNull => false
    _ => true
  }
}

pure length_of(j: Json) -> Result[Json] {
  match j {
    JNull => return Ok(JNum(0.0))
    JNum(n) => {
      if n < 0.0 {
        return Ok(JNum(0.0 - n))
      }

      return Ok(JNum(n))
    }
    JStr(s) => return Ok(JNum(s.count_chars().float()))
    JArr(xs) => return Ok(JNum(xs.len().float()))
    JObj(es) => return Ok(JNum(es.len().float()))
    JBool(_) => return Err(jq_err("boolean has no length"))
  }
}

pure keys_of(j: Json, sorted: Bool) -> Result[Json] {
  match j {
    JObj(es) => {
      var names = entry_keys(es)

      if sorted {
        names = sort_strs(names)
      }

      var out = [JStr(n) for n in names]
      return Ok(JArr(out))
    }
    JArr(xs) => {
      var out: List[Json] = []
      var i = 0

      while i < xs.len() {
        out = out.push(JNum(i.float()))
        i = i + 1
      }

      return Ok(JArr(out))
    }
    _ => return Err(jq_err(type_name(j) + " has no keys"))
  }
}

# Math + whole-array aggregates (no arguments).
pure bi_agg(name: Str, input: Json) -> Result[Dispatch] {
  if name == "floor" or name == "ceil" or name == "round" or name == "fabs" or name == "abs" or name == "sqrt" {
    let n = jnum_of(input, name)?

    if name == "floor" {
      return Ok(Handled([JNum((n.floor() ?? 0).float())]))
    }

    if name == "ceil" {
      return Ok(Handled([JNum((n.ceil() ?? 0).float())]))
    }

    if name == "round" {
      return Ok(Handled([JNum((n.round() ?? 0).float())]))
    }

    if name == "sqrt" {
      return Ok(Handled([JNum(fsqrt(n))]))
    }

    if n < 0.0 {
      return Ok(Handled([JNum(0.0 - n)]))
    }

    return Ok(Handled([JNum(n)]))
  }

  if name == "add" {
    let xs = require_array(input, "add")?

    if xs.len() == 0 {
      return Ok(Handled([JNull]))
    }

    var acc = xs.get(0, JNull)
    var i = 1

    while i < xs.len() {
      acc = add_values(acc, xs.get(i, JNull))?
      i = i + 1
    }

    return Ok(Handled([acc]))
  }

  if name == "sort" {
    let xs = require_array(input, "sort")?
    var items = [{key: x, val: x} for x in xs]
    return Ok(Handled([kv_vals(sort_kv(items))]))
  }

  if name == "unique" {
    let xs = require_array(input, "unique")?
    var items = [{key: x, val: x} for x in xs]
    return Ok(Handled([JArr(dedupe_sorted(sort_kv(items)))]))
  }

  if name == "reverse" {
    return Ok(Handled([reverse_value(input)?]))
  }

  if name == "min" or name == "max" {
    let xs = require_array(input, name)?
    return Ok(Handled([minmax(xs, name == "max")]))
  }

  if name == "flatten" {
    let xs = require_array(input, "flatten")?
    let none: List[Json] = []
    return Ok(Handled([JArr(flatten_into(xs, 1000000, none))]))
  }

  if name == "to_entries" {
    match input {
      JObj(es) => return Ok(Handled([JArr(to_entries_of(es))]))
      _ => return Err(jq_err("to_entries requires an object"))
    }
  }

  if name == "from_entries" {
    let xs = require_array(input, "from_entries")?
    return Ok(Handled([from_entries_of(xs)?]))
  }

  if name == "any" {
    let xs = require_array(input, "any")?
    var r = false

    for x in xs {
      if is_truthy(x) {
        r = true
      }
    }

    return Ok(Handled([JBool(r)]))
  }

  if name == "all" {
    let xs = require_array(input, "all")?
    var r = true

    for x in xs {
      if ! is_truthy(x) {
        r = false
      }
    }

    return Ok(Handled([JBool(r)]))
  }

  if name == "first" {
    let xs = require_array(input, "first")?
    return Ok(Handled([first_or(xs, JNull)]))
  }

  if name == "last" {
    let xs = require_array(input, "last")?

    if xs.len() == 0 {
      return Ok(Handled([JNull]))
    }

    return Ok(Handled([xs.get(xs.len() - 1, JNull)]))
  }

  if name == "recurse" {
    return Ok(Handled(recurse_all(input)))
  }

  return Ok(Pass)
}

pure kv_vals(items: List[KV]) -> Json {
  var out = [it.val for it in items]
  return JArr(out)
}

pure dedupe_sorted(items: List[KV]) -> List[Json] {
  var out: List[Json] = []
  var have_prev = false
  var prev = JNull

  for it in items {
    if have_prev and json_eq(it.val, prev) {
      have_prev = true
    } else {
      out = out.push(it.val)
      prev = it.val
      have_prev = true
    }
  }

  return out
}

pure minmax(xs: List[Json], want_max: Bool) -> Json {
  if xs.len() == 0 {
    return JNull
  }

  var best = xs.get(0, JNull)
  var i = 1

  while i < xs.len() {
    let x = xs.get(i, JNull)
    let c = json_cmp(x, best)

    if want_max {
      if c >= 0 {
        best = x
      }
    } else {
      if c < 0 {
        best = x
      }
    }

    i = i + 1
  }

  return best
}

pure reverse_value(j: Json) -> Result[Json] {
  match j {
    JArr(xs) => {
      var out: List[Json] = []
      var i = xs.len() - 1

      while i >= 0 {
        out = out.push(xs.get(i, JNull))
        i = i - 1
      }

      return Ok(JArr(out))
    }
    JStr(s) => return Ok(JStr(s.reverse()))
    JNull => return Ok(JArr([]))
    _ => return Err(jq_err(type_name(j) + " cannot be reversed"))
  }
}

# Builtins taking filter/value arguments.
pure bi_args(name: Str, callargs: List[Jq], input: Json, scope: Env) -> Result[Dispatch] {
  let argc = callargs.len()

  if name == "empty" and argc == 0 {
    let none: List[Json] = []
    return Ok(Handled(none))
  }

  if name == "error" and argc == 0 {
    return Err(json_error(input))
  }

  if name == "error" and argc == 1 {
    let vs = eval(callargs.get(0, Identity), input, scope)?

    if vs.len() == 0 {
      let none: List[Json] = []
      return Ok(Handled(none))
    }

    return Err(json_error(vs.get(0, JNull)))
  }

  if name == "select" and argc == 1 {
    let conds = eval(callargs.get(0, Identity), input, scope)?
    var out = [input for c in conds if is_truthy(c)]
    return Ok(Handled(out))
  }

  if name == "map" and argc == 1 {
    let items = iterate_value(input)?
    var out: List[Json] = []

    for it in items {
      out = out.extend(eval(callargs.get(0, Identity), it, scope)?)
    }

    return Ok(Handled([JArr(out)]))
  }

  if name == "map_values" and argc == 1 {
    return Ok(Handled([map_values(input, callargs.get(0, Identity), scope)?]))
  }

  if name == "has" and argc == 1 {
    let ks = eval(callargs.get(0, Identity), input, scope)?
    var out = [JBool(has_key(input, k)?) for k in ks]
    return Ok(Handled(out))
  }

  if name == "in" and argc == 1 {
    let conts = eval(callargs.get(0, Identity), input, scope)?
    var out = [JBool(has_key(cont, input)?) for cont in conts]
    return Ok(Handled(out))
  }

  if name == "contains" and argc == 1 {
    let bs = eval(callargs.get(0, Identity), input, scope)?
    var out = [JBool(contains_json(input, b)) for b in bs]
    return Ok(Handled(out))
  }

  if name == "inside" and argc == 1 {
    let bs = eval(callargs.get(0, Identity), input, scope)?
    var out = [JBool(contains_json(b, input)) for b in bs]
    return Ok(Handled(out))
  }

  if name == "sort_by" and argc == 1 {
    let xs = require_array(input, "sort_by")?
    return Ok(Handled([kv_vals(sort_kv(kv_by(xs, callargs.get(0, Identity), scope)?))]))
  }

  if name == "unique_by" and argc == 1 {
    let xs = require_array(input, "unique_by")?
    let sorted = sort_kv(kv_by(xs, callargs.get(0, Identity), scope)?)
    return Ok(Handled([JArr(dedupe_by_key(sorted))]))
  }

  if name == "group_by" and argc == 1 {
    let xs = require_array(input, "group_by")?
    let sorted = sort_kv(kv_by(xs, callargs.get(0, Identity), scope)?)
    return Ok(Handled([JArr(group_sorted(sorted))]))
  }

  if (name == "min_by" or name == "max_by") and argc == 1 {
    let xs = require_array(input, name)?
    let pairs = kv_by(xs, callargs.get(0, Identity), scope)?
    return Ok(Handled([minmax_by(pairs, name == "max_by")]))
  }

  if name == "range" and (argc == 1 or argc == 2 or argc == 3) {
    return Ok(Handled(eval_range(callargs, input, scope)?))
  }

  if name == "any" and argc == 1 {
    let items = iterate_value(input)?
    var r = false

    for it in items {
      for c in eval(callargs.get(0, Identity), it, scope)? {
        if is_truthy(c) {
          r = true
        }
      }
    }

    return Ok(Handled([JBool(r)]))
  }

  if name == "all" and argc == 1 {
    let items = iterate_value(input)?
    var r = true

    for it in items {
      for c in eval(callargs.get(0, Identity), it, scope)? {
        if ! is_truthy(c) {
          r = false
        }
      }
    }

    return Ok(Handled([JBool(r)]))
  }

  if name == "first" and argc == 1 {
    let vs = eval(callargs.get(0, Identity), input, scope)?

    if vs.len() == 0 {
      let none: List[Json] = []
      return Ok(Handled(none))
    }

    return Ok(Handled([vs.get(0, JNull)]))
  }

  if name == "last" and argc == 1 {
    let vs = eval(callargs.get(0, Identity), input, scope)?

    if vs.len() == 0 {
      let none: List[Json] = []
      return Ok(Handled(none))
    }

    return Ok(Handled([vs.get(vs.len() - 1, JNull)]))
  }

  if name == "flatten" and argc == 1 {
    let xs = require_array(input, "flatten")?
    let dv = eval(callargs.get(0, Identity), input, scope)?
    let d = jnum_of(first_or(dv, JNum(0.0)), "flatten")?.floor() ?? 0
    let none: List[Json] = []
    return Ok(Handled([JArr(flatten_into(xs, d, none))]))
  }

  if name == "recurse" and argc == 1 {
    return Ok(Handled(recurse_f(input, callargs.get(0, Identity), scope)?))
  }

  if name == "limit" and argc == 2 {
    let nv = eval(callargs.get(0, Identity), input, scope)?
    let n = jnum_of(first_or(nv, JNum(0.0)), "limit")?.floor() ?? 0
    let none: List[Json] = []

    if n <= 0 {
      return Ok(Handled(none))
    }

    let all = eval(callargs.get(1, Identity), input, scope)?
    var out: List[Json] = []
    var i = 0

    for v in all {
      if i < n {
        out = out.push(v)
      }

      i = i + 1
    }

    return Ok(Handled(out))
  }

  if name == "nth" and argc == 2 {
    let nv = eval(callargs.get(0, Identity), input, scope)?
    let n = jnum_of(first_or(nv, JNum(0.0)), "nth")?.floor() ?? 0
    let all = eval(callargs.get(1, Identity), input, scope)?
    let none: List[Json] = []

    if n < 0 or n >= all.len() {
      return Ok(Handled(none))
    }

    return Ok(Handled([all.get(n, JNull)]))
  }

  if (name == "startswith" or name == "endswith") and argc == 1 {
    let pv = eval(callargs.get(0, Identity), input, scope)?
    let s = str_of(input, name)?
    var out: List[Json] = []

    for p in pv {
      let pp = str_of(p, name)?

      if name == "startswith" {
        out = out.push(JBool(s.starts_with(pp)))
      } else {
        out = out.push(JBool(s.ends_with(pp)))
      }
    }

    return Ok(Handled(out))
  }

  if (name == "ltrimstr" or name == "rtrimstr") and argc == 1 {
    match input {
      JStr(s) => {
        let pv = eval(callargs.get(0, Identity), input, scope)?
        var out: List[Json] = []

        for p in pv {
          match p {
            JStr(pp) => {
              if name == "ltrimstr" and s.starts_with(pp) {
                out = out.push(JStr(s.byte_slice(pp.byte_len(), s.byte_len() - pp.byte_len())))
              } else if name == "rtrimstr" and s.ends_with(pp) {
                out = out.push(JStr(s.byte_slice(0, s.byte_len() - pp.byte_len())))
              } else {
                out = out.push(input)
              }
            }
            _ => out = out.push(input)
          }
        }

        return Ok(Handled(out))
      }
      _ => return Ok(Handled([input]))
    }
  }

  if name == "split" and argc == 1 {
    let s = str_of(input, "split")?
    let pv = eval(callargs.get(0, Identity), input, scope)?
    var out = [split_str(s, str_of(p, "split")?) for p in pv]
    return Ok(Handled(out))
  }

  if name == "join" and argc == 1 {
    let xs = require_array(input, "join")?
    let sv = eval(callargs.get(0, Identity), input, scope)?
    var out = [JStr(join_values(xs, str_of(sep, "join")?)?) for sep in sv]
    return Ok(Handled(out))
  }

  if name == "del" and argc == 1 {
    let paths = eval_paths(callargs.get(0, Identity), input, scope)?
    return Ok(Handled([delpaths(input, paths)?]))
  }

  if name == "path" and argc == 1 {
    let paths = eval_paths(callargs.get(0, Identity), input, scope)?
    var out = [JArr(p) for p in paths]
    return Ok(Handled(out))
  }

  if name == "paths" and argc == 0 {
    let empty: List[Json] = []
    let all = paths_from(input, empty)
    var out = [JArr(p) for p in all if p.len() > 0]
    return Ok(Handled(out))
  }

  if name == "paths" and argc == 1 {
    let empty: List[Json] = []
    let all = paths_from(input, empty)
    var out: List[Json] = []

    for p in all {
      if p.len() > 0 {
        let v = getpath(input, p)?
        var keep = false

        for c in eval(callargs.get(0, Identity), v, scope)? {
          if is_truthy(c) {
            keep = true
          }
        }

        if keep {
          out = out.push(JArr(p))
        }
      }
    }

    return Ok(Handled(out))
  }

  if name == "leaf_paths" and argc == 0 {
    let empty: List[Json] = []
    let all = paths_from(input, empty)
    var out: List[Json] = []

    for p in all {
      if p.len() > 0 {
        let v = getpath(input, p)?

        if ! (is_arr(v) or is_obj(v)) {
          out = out.push(JArr(p))
        }
      }
    }

    return Ok(Handled(out))
  }

  if name == "getpath" and argc == 1 {
    let pv = eval(callargs.get(0, Identity), input, scope)?
    var out = [getpath(input, json_to_path(p)?)? for p in pv]
    return Ok(Handled(out))
  }

  if name == "setpath" and argc == 2 {
    let pv = eval(callargs.get(0, Identity), input, scope)?
    let vv = eval(callargs.get(1, Identity), input, scope)?
    var out: List[Json] = []

    for p in pv {
      for nv in vv {
        out = out.push(setpath(input, json_to_path(p)?, nv)?)
      }
    }

    return Ok(Handled(out))
  }

  if name == "delpaths" and argc == 1 {
    let pv = eval(callargs.get(0, Identity), input, scope)?
    var out: List[Json] = []

    for p in pv {
      var paths: List[List[Json]] = []

      match p {
        JArr(ps) => {
          for one in ps {
            paths = paths.push(json_to_path(one)?)
          }
        }
        _ => return Err(jq_err("delpaths requires an array of paths"))
      }

      out = out.push(delpaths(input, paths)?)
    }

    return Ok(Handled(out))
  }

  if name == "walk" and argc == 1 {
    return Ok(Handled([walk_f(input, callargs.get(0, Identity), scope)?]))
  }

  if (name == "index" or name == "rindex" or name == "indices") and argc == 1 {
    let needles = eval(callargs.get(0, Identity), input, scope)?
    var out: List[Json] = []

    for nd in needles {
      let idxs = find_indices(input, nd)?

      if name == "indices" {
        out = out.push(JArr(idxs))
      } else if name == "index" {
        out = out.push(first_or(idxs, JNull))
      } else {
        if idxs.len() == 0 {
          out = out.push(JNull)
        } else {
          out = out.push(idxs.get(idxs.len() - 1, JNull))
        }
      }
    }

    return Ok(Handled(out))
  }

  if name == "with_entries" and argc == 1 {
    match input {
      JObj(es) => {
        let entries = to_entries_of(es)
        var mapped: List[Json] = []

        for ent in entries {
          mapped = mapped.extend(eval(callargs.get(0, Identity), ent, scope)?)
        }

        return Ok(Handled([from_entries_of(mapped)?]))
      }
      _ => return Err(jq_err("with_entries requires an object"))
    }
  }

  return Ok(Pass)
}

pure dedupe_by_key(items: List[KV]) -> List[Json] {
  var out: List[Json] = []
  var have_prev = false
  var prev = JNull

  for it in items {
    if have_prev and json_eq(it.key, prev) {
      have_prev = true
    } else {
      out = out.push(it.val)
      prev = it.key
      have_prev = true
    }
  }

  return out
}

pure group_sorted(items: List[KV]) -> List[Json] {
  var out: List[Json] = []
  var cur: List[Json] = []
  var have_prev = false
  var prev = JNull

  for it in items {
    if have_prev and json_eq(it.key, prev) {
      cur = cur.push(it.val)
    } else {
      if have_prev {
        out = out.push(JArr(cur))
      }

      cur = [it.val]
      prev = it.key
      have_prev = true
    }
  }

  if have_prev {
    out = out.push(JArr(cur))
  }

  return out
}

pure minmax_by(pairs: List[KV], want_max: Bool) -> Json {
  if pairs.len() == 0 {
    return JNull
  }

  var best = pairs.get(0, {key: JNull, val: JNull})
  var i = 1

  while i < pairs.len() {
    let p = pairs.get(i, {key: JNull, val: JNull})
    let c = json_cmp(p.key, best.key)

    if want_max {
      if c >= 0 {
        best = p
      }
    } else {
      if c < 0 {
        best = p
      }
    }

    i = i + 1
  }

  return best.val
}

pure eval_range(callargs: List[Jq], input: Json, scope: Env) -> Result[List[Json]] {
  let argc = callargs.len()
  var out: List[Json] = []

  if argc == 1 {
    let tos = eval(callargs.get(0, Identity), input, scope)?

    for t in tos {
      out = out.extend(range_gen(0.0, jnum_of(t, "range")?, 1.0))
    }

    return Ok(out)
  }

  let froms = eval(callargs.get(0, Identity), input, scope)?
  let tos = eval(callargs.get(1, Identity), input, scope)?

  for fr in froms {
    for t in tos {
      if argc == 2 {
        out = out.extend(range_gen(jnum_of(fr, "range")?, jnum_of(t, "range")?, 1.0))
      } else {
        let steps = eval(callargs.get(2, Identity), input, scope)?

        for st in steps {
          out = out.extend(range_gen(jnum_of(fr, "range")?, jnum_of(t, "range")?, jnum_of(st, "range")?))
        }
      }
    }
  }

  out
}

# map_values(f): keep structure; each value becomes f's first output, or is dropped
# if f yields nothing.
pure map_values(input: Json, f: Jq, scope: Env) -> Result[Json] {
  match input {
    JObj(es) => {
      var out: List[Entry] = []

      for e in es {
        let vs = eval(f, e.v, scope)?

        if vs.len() > 0 {
          out = out.push({k: e.k, v: vs.get(0, JNull)})
        }
      }

      return Ok(JObj(out))
    }
    JArr(xs) => {
      var out: List[Json] = []

      for x in xs {
        let vs = eval(f, x, scope)?

        if vs.len() > 0 {
          out = out.push(vs.get(0, JNull))
        }
      }

      return Ok(JArr(out))
    }
    _ => return Err(jq_err(type_name(input) + " cannot be map_values'd"))
  }
}

pure has_key(cont: Json, key: Json) -> Result[Bool] {
  match cont {
    JObj(es) => {
      let k = as_obj_key(key)?
      return Ok(entry_has(es, k))
    }
    JArr(xs) => {
      match key {
        JNum(n) => {
          let i = n.floor() ?? 0
          return Ok(i >= 0 and i < xs.len())
        }
        _ => return Err(jq_err("Cannot check array membership with non-number"))
      }
    }
    _ => return Err(jq_err(type_name(cont) + " has no keys"))
  }
}

# recurse(f): . , (f | recurse(f)) — terminates when f yields nothing (jq uses `?`).
pure recurse_f(input: Json, f: Jq, scope: Env) -> Result[List[Json]] {
  var out: List[Json] = [input]
  let children = eval(f, input, scope)?

  for c in children {
    out = out.extend(recurse_f(c, f, scope)?)
  }

  out
}

pure starts_with_dollar(p: Str) -> Bool {
  return p.byte_at(0, 0) == 36
}

# walk(f): transform children bottom-up, then apply f to the rebuilt value.
pure walk_f(v: Json, f: Jq, scope: Env) -> Result[Json] {
  match v {
    JObj(es) => {
      var out = [{k: e.k, v: walk_f(e.v, f, scope)?} for e in es]
      let r = eval(f, JObj(out), scope)?
      return Ok(first_or(r, JObj(out)))
    }
    JArr(xs) => {
      var out = [walk_f(x, f, scope)? for x in xs]
      let r = eval(f, JArr(out), scope)?
      return Ok(first_or(r, JArr(out)))
    }
    _ => {
      let r = eval(f, v, scope)?
      return Ok(first_or(r, v))
    }
  }
}

pure str_indices(s: Str, sub: Str) -> List[Json] {
  var out: List[Json] = []

  if sub == "" {
    return out
  }

  var start = 0

  while true {
    let i = s.find(sub, start)
    break when i < 0
    out = out.push(JNum(i.float()))
    start = i + 1
  }

  return out
}

pure arr_subseq_indices(xs: List[Json], sub: List[Json]) -> List[Json] {
  var out: List[Json] = []

  if sub.len() == 0 {
    return out
  }

  var i = 0

  while i + sub.len() <= xs.len() {
    var ok = true
    var j = 0

    while j < sub.len() {
      if ! json_eq(xs.get(i + j, JNull), sub.get(j, JNull)) {
        ok = false
      }

      j = j + 1
    }

    if ok {
      out = out.push(JNum(i.float()))
    }

    i = i + 1
  }

  return out
}

pure find_indices(hay: Json, needle: Json) -> Result[List[Json]] {
  let none: List[Json] = []

  match hay {
    JStr(s) => {
      match needle {
        JStr(sub) => return Ok(str_indices(s, sub))
        _ => return Ok(none)
      }
    }
    JArr(xs) => {
      match needle {
        JArr(sub) => return Ok(arr_subseq_indices(xs, sub))
        _ => {
          var out: List[Json] = []
          var i = 0

          for x in xs {
            if json_eq(x, needle) {
              out = out.push(JNum(i.float()))
            }

            i = i + 1
          }

          return Ok(out)
        }
      }
    }
    JNull => return Ok(none)
    _ => return Err(jq_err("Cannot get indices of " + type_name(hay)))
  }
}

# ---------------------------------------------------------------------------
# Regex (T6). XSH's `regex` module exposes match/find (full-match spans)/captures
# (first match's groups)/replace, but no named groups, per-match capture spans, or
# Oniguruma extensions — so test/scan/split/sub/gsub are faithful while match and
# capture are best-effort and Oniguruma-only features are a documented gap.
type Span = {end: Int, start: Int, text: Str}

type ReFlags = {restr: Str, flags: Str}

pure re_and_flags(callargs: List[Jq], input: Json, scope: Env) -> Result[ReFlags] {
  let a0 = first_or(eval(callargs.get(0, Identity), input, scope)?, JNull)

  match a0 {
    JArr(xs) => {
      let re = str_of(xs.get(0, JStr("")), "regex")?
      var fl = ""

      if xs.len() > 1 {
        fl = str_of(xs.get(1, JStr("")), "regex")?
      }

      return Ok({restr: re, flags: fl})
    }
    JStr(s) => {
      var fl = ""

      if callargs.len() >= 2 {
        match first_or(eval(callargs.get(1, Identity), input, scope)?, JNull) {
          JStr(x) => fl = x
          JNull => fl = ""
          _ => return Err(jq_err("regex flags must be a string"))
        }
      }

      return Ok({restr: s, flags: fl})
    }
    _ => return Err(jq_err("regex must be a string"))
  }
}

pure compile_re(restr: Str, flags: Str) -> Result[Regex] {
  var inner = ""

  if "i" in flags {
    inner = inner + "i"
  }

  if "s" in flags {
    inner = inner + "s"
  }

  if "m" in flags {
    inner = inner + "m"
  }

  if "x" in flags {
    inner = inner + "x"
  }

  if inner == "" {
    return regex.compile(restr)
  }

  return regex.compile("(?" + inner + ")" + restr)
}

pure regex_replace_spans(text: Str, spans: List[Span], repl: Str, only_first: Bool) -> Str {
  var out: List[Str] = []
  var last = 0
  var done = false

  for m in spans {
    if ! done {
      out = out.push(text.byte_slice(last, m.start - last))
      out = out.push(repl)
      last = m.end

      if only_first {
        done = true
      }
    }
  }

  out = out.push(text.byte_slice(last, text.byte_len() - last))
  return out.join("")
}

pure regex_split(text: Str, spans: List[Span]) -> List[Json] {
  var out: List[Json] = []
  var last = 0

  for m in spans {
    out = out.push(JStr(text.byte_slice(last, m.start - last)))
    last = m.end
  }

  out = out.push(JStr(text.byte_slice(last, text.byte_len() - last)))
  return out
}

pure match_object(m: Span) -> Json {
  let nocaps: List[Json] = []

  return JObj(
    [
      {
        k: "offset",
        v: JNum(m.start.float()),
      },
      {
        k: "length",
        v: JNum((m.end - m.start).float()),
      },
      {
        k: "string",
        v: JStr(m.text),
      },
      {
        k: "captures",
        v: JArr(nocaps),
      },
    ],
  )
}

pure eval_regex(name: Str, callargs: List[Jq], input: Json, scope: Env) -> Result[Dispatch] {
  let argc = callargs.len()
  let is_re = name == "test" or name == "match" or name == "scan" or name == "splits" or name == "sub" or name == "gsub" or name == "split" and argc == 2

  if ! is_re {
    return Ok(Pass)
  }

  let rf = re_and_flags(callargs, input, scope)?
  let re = compile_re(rf.restr, rf.flags)?
  let s = str_of(input, name)?

  if name == "test" {
    return Ok(Handled([JBool(re.matches(s))]))
  }

  let spans = re.find(s)

  if name == "match" {
    var out: List[Json] = []
    let global = "g" in rf.flags
    var i = 0

    for m in spans {
      if global or i == 0 {
        out = out.push(match_object(m))
      }

      i = i + 1
    }

    return Ok(Handled(out))
  }

  if name == "scan" {
    var out = [JStr(m.text) for m in spans]
    return Ok(Handled(out))
  }

  if name == "splits" {
    return Ok(Handled(regex_split(s, spans)))
  }

  if name == "split" {
    return Ok(Handled([JArr(regex_split(s, spans))]))
  }

  if name == "sub" or name == "gsub" {
    let replv = eval(callargs.get(1, Identity), input, scope)?
    let repl = str_of(first_or(replv, JStr("")), "sub")?
    return Ok(Handled([JStr(regex_replace_spans(s, spans, repl, name == "sub"))]))
  }

  return Ok(Pass)
}

# Resolve a call: filter parameter (0-arg), then user def, then builtin.
pure eval_call_resolved(name: Str, callargs: List[Jq], input: Json, scope: Env) -> Result[List[Json]] {
  if callargs.len() == 0 {
    match lookup_filter(scope, name) {
      FilterFound(clo) => return eval(clo.cbody, input, clo.cenv)
      FilterNone => {}
    }
  }

  match lookup_func(scope, name, callargs.len()) {
    FuncFound(fdef, capture) => return eval_user_func(fdef, capture, callargs, scope, input)
    FuncNone => {}
  }

  return eval_call(name, callargs, input, scope)
}

# Invoke a user def: re-inject self (for recursion), bind filter params as closures
# over the caller env, then cartesian-bind value ($-prefixed) params.
pure eval_user_func(fdef: FnDef, capture: Env, callargs: List[Jq], caller: Env, input: Json) -> Result[List[Json]] {
  var base = EnvFunc(fdef, capture, capture)
  var vnames: List[Str] = []
  var vargs: List[Jq] = []
  var i = 0

  for p in fdef.params {
    if starts_with_dollar(p) {
      vnames = vnames.push(p.byte_slice(1, p.byte_len() - 1))
      vargs = vargs.push(callargs.get(i, Identity))
    } else {
      base = EnvFilter(p, {cbody: callargs.get(i, Identity), cenv: caller}, base)
    }

    i = i + 1
  }

  return bind_value_params(fdef.fbody, vnames, vargs, 0, input, caller, base)
}

pure bind_value_params(
  body: Jq,
  vnames: List[Str],
  vargs: List[Jq],
  idx: Int,
  input: Json,
  caller: Env,
  base: Env,
) -> Result[List[Json]] {
  if idx >= vnames.len() {
    return eval(body, input, base)
  }

  let vals = eval(vargs.get(idx, Identity), input, caller)?
  var out: List[Json] = []

  for v in vals {
    let base2 = EnvVar(vnames.get(idx, ""), v, base)
    out = out.extend(bind_value_params(body, vnames, vargs, idx + 1, input, caller, base2)?)
  }

  out
}

# Bind a destructuring pattern against `value`, extending `scope`.
pure bind_pattern(pat: Pattern, value: Json, scope: Env) -> Result[Env] {
  match pat {
    PVar(name) => return Ok(EnvVar(name, value, scope))
    PArray(pats) => {
      var sc = scope
      var i = 0

      for sub in pats {
        let elem = index_value(value, JNum(i.float()))?
        sc = bind_pattern(sub, elem, sc)?
        i = i + 1
      }

      return Ok(sc)
    }
    PObjPat(fields) => {
      var sc = scope

      for f in fields {
        let elem = index_value(value, JStr(f.key))?
        sc = bind_pattern(f.pat, elem, sc)?
      }

      return Ok(sc)
    }
  }
}

pure eval_bind(src: Jq, pat: Pattern, body: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let vals = eval(src, input, scope)?
  var out: List[Json] = []

  for v in vals {
    let scope2 = bind_pattern(pat, v, scope)?
    out = out.extend(eval(body, input, scope2)?)
  }

  out
}

pure eval_reduce(src: Jq, pat: Pattern, init: Jq, upd: Jq, input: Json, scope: Env) -> Result[List[Json]] {
  let items = eval(src, input, scope)?
  let inits = eval(init, input, scope)?
  var results: List[Json] = []

  for acc0 in inits {
    var acc = acc0

    for it in items {
      let scope2 = bind_pattern(pat, it, scope)?
      let ups = eval(upd, acc, scope2)?

      if ups.len() == 0 {
        acc = JNull
      } else {
        acc = ups.get(ups.len() - 1, JNull)
      }
    }

    results = results.push(acc)
  }

  results
}

pure eval_foreach(
  src: Jq,
  pat: Pattern,
  init: Jq,
  upd: Jq,
  has_x: Bool,
  extract: Jq,
  input: Json,
  scope: Env,
) -> Result[List[Json]] {
  let items = eval(src, input, scope)?
  let inits = eval(init, input, scope)?
  var out: List[Json] = []

  for acc0 in inits {
    var acc = acc0

    for it in items {
      let scope2 = bind_pattern(pat, it, scope)?
      let ups = eval(upd, acc, scope2)?

      for s in ups {
        if has_x {
          out = out.extend(eval(extract, s, scope2)?)
        } else {
          out = out.push(s)
        }
      }

      if ups.len() > 0 {
        acc = ups.get(ups.len() - 1, JNull)
      }
    }
  }

  out
}

# Builtin / user function dispatch.
pure eval_call(name: Str, callargs: List[Jq], input: Json, scope: Env) -> Result[List[Json]] {
  if callargs.len() == 0 {
    let dt = bi_typey(name, input)?

    match dt {
      Handled(r) => return Ok(r)
      Pass => {}
    }

    let da = bi_agg(name, input)?

    match da {
      Handled(r) => return Ok(r)
      Pass => {}
    }
  }

  let dr = bi_args(name, callargs, input, scope)?

  match dr {
    Handled(r) => return Ok(r)
    Pass => {}
  }

  let dx = eval_regex(name, callargs, input, scope)?

  match dx {
    Handled(r) => return Ok(r)
    Pass => {}
  }

  return Err(jq_err(name + "/" + f"${callargs.len()}" + " is not defined"))
}

# ---------------------------------------------------------------------------
# CLI + main.
# ---------------------------------------------------------------------------
type Args = {
  filter: Str,
  files: List[Str],
  compact: Bool,
  raw_output: Bool,
  null_input: Bool,
  slurp: Bool,
  sort_keys: Bool,
}

pure parse_args(argv: List[Str]) -> Result[Args] {
  let opts = cli.applet(
    argv,
    {
      compact: {
        form: "-c --compact-output",
        default: false,
      },
      raw_output: {
        form: "-r -j --raw-output",
        default: false,
      },
      null_input: {
        form: "-n --null-input",
        default: false,
      },
      slurp: {
        form: "-s --slurp",
        default: false,
      },
      sort_keys: {
        form: "-S --sort-keys",
        default: false,
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let filter = opts.operands.get(0, "")
  let files = opts.operands |> drop(1)

  return Ok({
    filter: filter,
    files: files,
    compact: opts.compact,
    raw_output: opts.raw_output,
    null_input: opts.null_input,
    slurp: opts.slurp,
    sort_keys: opts.sort_keys,
  })
}

# Emit one output value per jq rules: -r unwraps a top-level string.
pure format_output(j: Json, raw: Bool) -> Str {
  if raw {
    match j {
      JStr(s) => return s
      _ => return ser(j)
    }
  }

  return ser(j)
}

proc main(...argv: List[Str]) [error, io] {
  let opts = parse_args(argv)?
  let ast = parse_program(opts.filter)?
  let input_text = io.stdin_text() ?? ""
  let inputs = parse_stream(input_text)?

  for v in inputs {
    let outs = eval(ast, v, EnvEmpty)?

    for o in outs {
      print format_output(o, opts.raw_output)
    }
  }
}
