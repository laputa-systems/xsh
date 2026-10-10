##! Indexed AWK syntax and evaluation implemented in XSH.

# Messages are complete diagnostics without the leading "awk: ". Syntax errors
# exit 1 after parsing (as gawk does); runtime failures and refused features exit 2.
## The failure kinds of the awk interpreter: syntax errors exit 1, the rest 2.
export error AwkError = Syntax : Usage | Fatal(message: Str, pending: Str) : InvalidArgument | Plain(message: Str, pending: Str) : InvalidArgument

# TokCall is a name written immediately before "(". Only that spelling makes a
# user function call, so "v (a)" stays a concatenation as POSIX requires.
enum Token { TokWord(Str), TokCall(Str), TokNumber(Float), TokText(Str), TokRegex(Str), TokOp(Str), TokEnd }
enum Expression {
  Number(Float), Text(Str), RegexLiteral(Str), Variable(Str), Field(Int), Index(Str, List[Int]), Call(Str, List[Int]),
  Unary(Str, Int), Binary(Str, Int, Int), Assign(Str, Int, Int), Increment(Int, Float, Bool), Conditional(Int, Int, Int),
  Member(List[Int], Str), CommandInput(Int, Int?), GetlineMain(Int?), GetlineFile(Int?, Int),
}
# Print(arguments, is_printf, destination, mode): mode is "", ">", ">>" or "|".
enum Statement {
  Block(List[Int]), ExpressionStatement(Int), Print(List[Int], Bool, Int?, Str), If(Int, Int, Int?), While(Int, Int), Do(Int, Int),
  For(Int?, Int?, Int?, Int), Each(Str, Str, Int), Delete(Str, List[Int]?), Next, NextFile, Break, Continue, Exit(Int?), Return(Int?),
}
type Rule = {pattern: Int?, end: Int?, action: Int, site: Str}
type Function = {parameters: List[Str], body: Int, site: Str}
# Child ids address immutable expression and statement arenas, so recursive syntax
# never requires recursive XSH record types. `site` labels each statement with the
# source position runtime diagnostics report.
type Program = {expressions: List[Expression], statements: List[Statement], site: List[Str], begin: List[Int], end: List[Int], rules: List[Rule], functions: Map[Function], notes: List[Str], uses: List[Use], environ: Bool, grouped: List[Int]}
# A name seen as a variable, array or function call, kept for the checks that need every definition.
type Use = {name: Str, site: Str, call: Bool, argc: Int}
# A piece is one program source: the command-line text or one -f file.
type Piece = {label: Str, text: Str}
type Lexed = {tokens: List[Token], starts: List[Int], lines: List[Int], notes: List[Str]}
type Parser = {tokens: List[Token], starts: List[Int], lines: List[Int], at: Int, program: Program, printing: Bool, depth: Int, piece: Piece, loops: Int, context: Str, errors: List[Str], sandbox: Bool}
type Parsed[T] = {parser: Parser, value: T}
type Escaped = {content: Str, at: Int, notes: List[Str]}

pure syntax_error(message: Str) -> Error { AwkError.Syntax(message) }
pure runtime(message: Str) -> Error { AwkError.Fatal(message: f"fatal: {message}", pending: "") }
pure unsupported(message: Str) -> Error { AwkError.Fatal(message: f"fatal: unsupported: {message}", pending: "") }
pure digit(byte: Int) -> Bool { byte >= 48 and byte <= 57 }
pure letter(byte: Int) -> Bool { byte == 95 or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) }
pure hex_digit(byte: Int) -> Bool { digit(byte) or (byte >= 65 and byte <= 70) or (byte >= 97 and byte <= 102) }
pure digit_value(byte: Int) -> Int { if digit(byte) { byte - 48 } else if byte >= 97 { byte - 87 } else { byte - 55 } }
# A numeric literal with a leading zero and only octal digits is octal in program
# text, as in GNU awk; input fields never reach this and keep decimal spelling.
pure octal_literal(text: Str) -> Bool {
  if text.byte_len() < 2 or text.byte_at(0) != 48 { return false }
  for at in range(text.byte_len()) { if (text.byte_at(at) ?? 0) not in [48, 49, 50, 51, 52, 53, 54, 55] { return false } }
  true
}


# A located diagnostic as gawk prints it: the source line, then a caret under
# the offending column with the message.
pure caret_error(piece: Piece, offset: Int, message: Str, reported_line_shift: Int = 0) -> Str {
  let source = bytes.from_text(piece.text)
  let at = if offset > source.len() { source.len() } else { offset }
  var line = 1
  var start = 0
  for index in range(at) { if source.byte_at(index) == 10 { line += 1; start = index + 1 } }
  var end = start
  while end < source.len() and source.byte_at(end) != 10 { end += 1 }
  let text = source[start..end].utf8() ?? ""
  let column = (source[start..at].utf8() ?? "").count_chars()
  let pad = [" " for _ in range(column)].join("")
  let reported = line + reported_line_shift
  f"{piece.label}:{reported}: {text}\nawk: {piece.label}:{reported}: {pad}^ {message}"
}

# Escape processing shared by string literals, option values and operand
# assignments. Unknown escapes keep the character and report a note.
pure escaped(source: Bytes, start: Int, quote: Int) -> Result[Escaped] {
  var at = start
  var out: List[Int] = []
  var notes: List[Str] = []
  while at < source.len() {
    let byte = source.byte_at(at) ?? 0
    at += 1
    if byte == quote and quote >= 0 { return Ok({content: bytes.from_ints(out)?.utf8()?, at: at, notes: notes}) }
    if byte == 10 and quote >= 0 { return Err(syntax_error("unterminated string")) }
    if byte != 92 { out += [byte]; continue }
    if at >= source.len() { out += [92]; continue }
    let next = source.byte_at(at) ?? 0
    at += 1
    if next >= 48 and next <= 55 {
      var number = next - 48
      var count = 0
      while count < 2 and (source.byte_at(at) ?? -1) >= 48 and (source.byte_at(at) ?? -1) <= 55 {
        number = number * 8 + (source.byte_at(at) ?? 48) - 48
        at += 1
        count += 1
      }
      out += [number % 256]
    } else if next == 120 {
      var number = 0
      var count = 0
      while count < 2 and hex_digit(source.byte_at(at) ?? 0) {
        number = number * 16 + digit_value(source.byte_at(at) ?? 0)
        at += 1
        count += 1
      }
      if count == 0 { notes += ["no hex digits in `\\x' escape sequence"]; out += [120] } else { out += [number] }
    } else if next == 10 { continue } else if next == 110 { out += [10] } else if next == 116 { out += [9] } else if next == 114 { out += [13] } else if next == 98 { out += [8] } else if next == 102 { out += [12] } else if next == 118 { out += [11] } else if next == 97 { out += [7] } else if next == 92 { out += [92] } else if next == 34 { out += [34] } else {
      let shown = bytes.from_ints([next])?.utf8() ?? "?"
      notes += [f"escape sequence `\\{shown}' treated as plain `{shown}'"]
      out += [next]
    }
  }
  if quote < 0 { Ok({content: bytes.from_ints(out)?.utf8()?, at: at, notes: notes}) } else { Err(syntax_error("unterminated string")) }
}

# Reads a regular expression literal body: the text up to the closing slash,
# where a slash inside a bracket expression does not close it and "\/" is a
# slash. Returns -1 as the end when the literal is unterminated.
type Body = {content: Str, at: Int}
pure regex_body(source: Bytes, start: Int) -> Result[Body] {
  var at = start
  var out: List[Int] = []
  var bracket = false
  while at < source.len() {
    let byte = source.byte_at(at) ?? 0
    if byte == 10 { return Ok({content: "", at: -1}) }
    if byte == 92 and at + 1 < source.len() {
      let next = source.byte_at(at + 1) ?? 0
      if next == 47 { out += [47] } else if next == 10 { } else { out += [92, next] }
      at += 2
      continue
    }
    if bracket {
      if byte == 93 { bracket = false }
      out += [byte]
      at += 1
      continue
    }
    if byte == 91 {
      bracket = true
      out += [91]
      at += 1
      if source.byte_at(at) == 94 { out += [94]; at += 1 }
      if source.byte_at(at) == 93 { out += [93]; at += 1 }
      continue
    }
    if byte == 47 { return Ok({content: bytes.from_ints(out)?.utf8()?, at: at + 1}) }
    out += [byte]
    at += 1
  }
  Ok({content: "", at: -1})
}

pure lex(piece: Piece) -> Result[Lexed] {
  let source = bytes.from_text(piece.text)
  var tokens: List[Token] = []
  var starts: List[Int] = []
  var lines: List[Int] = []
  var notes: List[Str] = []
  var at = 0
  var line = 1
  var counted = 0
  var operand = true
  while at < source.len() {
    let byte = source.byte_at(at) ?? 0
    if byte == 92 and source.byte_at(at + 1) == 10 { at += 2; continue }
    if byte == 92 and source.byte_at(at + 1) == 13 and source.byte_at(at + 2) == 10 { at += 3; continue }
    if byte == 35 { while at < source.len() and source.byte_at(at) != 10 { at += 1 }; continue }
    if byte == 32 or byte == 9 or byte == 13 { at += 1; continue }
    while counted < at { if source.byte_at(counted) == 10 { line += 1 }; counted += 1 }
    let start = at
    if byte == 34 {
      let found = escaped(source, at + 1, byte)
      match found {
        Ok(text) => {
          for note in text.notes { notes += [f"{piece.label}:{line}: warning: {note}"] }
          tokens += [TokText(text.content)]
          at = text.at
        }
        Err(_) => {
          # A backslash ending the text continues the string over the newline
          # GNU awk appends, which advances its line count.
          var stop = start + 1
          while stop < source.len() and source.byte_at(stop) != 10 { stop += if source.byte_at(stop) == 92 { 2 } else { 1 } }
          let continued = stop >= source.len() and source.byte_at(source.len() - 1) == 92
          return Err(syntax_error(caret_error(piece, start, "unterminated string", if continued { 1 } else { 0 })))
        }
      }
      starts += [start]
      lines += [line]
      operand = false
      continue
    }
    if byte == 47 and operand {
      let body = regex_body(source, at + 1)?
      if body.at < 0 { return Err(syntax_error(caret_error(piece, start + 1, "unterminated regexp"))) }
      tokens += [TokRegex(body.content)]
      starts += [start]
      lines += [line]
      at = body.at
      operand = false
      continue
    }
    if byte == 48 and (source.byte_at(at + 1) == 120 or source.byte_at(at + 1) == 88) and hex_digit(source.byte_at(at + 2) ?? 0) {
      let digits = at + 2
      at = digits
      while hex_digit(source.byte_at(at) ?? 0) { at += 1 }
      var value = 0.0
      for position in range(digits, at) { value = value * 16.0 + digit_value(source.byte_at(position) ?? 0).float() }
      tokens += [TokNumber(value)]
      starts += [start]
      lines += [line]
      operand = false
      continue
    }
    if digit(byte) or (byte == 46 and digit(source.byte_at(at + 1) ?? 0)) {
      at += 1
      while digit(source.byte_at(at) ?? 0) { at += 1 }
      if source.byte_at(at) == 46 {
        at += 1
        while digit(source.byte_at(at) ?? 0) { at += 1 }
      }
      if source.byte_at(at) == 101 or source.byte_at(at) == 69 {
        var look = at + 1
        if source.byte_at(look) == 43 or source.byte_at(look) == 45 { look += 1 }
        if digit(source.byte_at(look) ?? 0) {
          while digit(source.byte_at(look) ?? 0) { look += 1 }
          at = look
        }
      }
      let text = source[start..at].utf8()?
      var value = 0.0
      if octal_literal(text) {
        for position in range(start, at) { value = value * 8.0 + digit_value(source.byte_at(position) ?? 48).float() }
      } else { value = (if text.ends_with(".") { text + "0" } else { text }).parse_float()? }
      tokens += [TokNumber(value)]
      starts += [start]
      lines += [line]
      operand = false
      continue
    }
    if letter(byte) {
      at += 1
      while letter(source.byte_at(at) ?? 0) or digit(source.byte_at(at) ?? 0) { at += 1 }
      let word = source[start..at].utf8()?
      tokens += [if source.byte_at(at) == 40 { TokCall(word) } else { TokWord(word) }]
      starts += [start]
      lines += [line]
      operand = word in ["print", "printf", "return", "exit", "in", "delete", "else", "do", "if", "while", "for"]
      continue
    }
    let pair = source.slice(at, length: if at + 1 < source.len() { 2 } else { 1 }).utf8() ?? "\u{1}"
    var op = pair
    if pair in ["++", "--", "==", "!=", "<=", ">=", "&&", "||", "!~", "+=", "-=", "*=", "/=", "%=", "^=", ">>", "**", "|&"] {
      at += 2
      if pair == "**" {
        if source.byte_at(at) == 61 { op = "^="; at += 1 } else { op = "^" }
      }
    } else {
      op = source[at..at + 1].utf8() ?? "\u{1}"
      if op not in ["{", "}", "(", ")", "[", "]", ",", "$", ";", "\n", "?", ":", "+", "-", "*", "/", "%", "^", "!", "<", ">", "=", "~", "|", "@"] {
        return Err(syntax_error(caret_error(piece, start, "syntax error")))
      }
      at += 1
    }
    operand = op not in [")", "]", "++", "--"]
    tokens += [TokOp(op)]
    starts += [start]
    lines += [line]
  }
  let size = source.len()
  Ok({tokens: tokens + [TokEnd], starts: starts + [size], lines: lines + [line], notes: notes})
}

pure spelling(token: Token) -> Str { match token { TokWord(word) => word; TokCall(word) => word; TokOp(op) => op; _ => "" } }
pure peek(p: Parser) -> Token { p.tokens[p.at] }
pure peek_at(p: Parser, offset: Int) -> Token { p.tokens.get(p.at + offset) ?? TokEnd }
pure is_at(p: Parser, content: Str) -> Bool { spelling(peek(p)) == content }
pure advance(parser: Parser) -> Parser { var p = parser; p.at += 1; p }
pure token_start(p: Parser) -> Int { p.starts.get(p.at) ?? 0 }
pure label_here(p: Parser) -> Str { f"{p.piece.label}:{p.lines.get(p.at) ?? 1}" }
# The line GNU awk has reached when it reports an error found with the token
# at the parser position as lookahead: that token's line, or the last line of
# the source when none remains.
pure lookahead_line(p: Parser) -> Int {
  if peek(p) != TokEnd { return p.lines.get(p.at) ?? 1 }
  let lines = p.piece.text.split("\n").len()
  let counted = if p.piece.text.ends_with("\n") { lines - 1 } else { lines }
  if counted < 1 { 1 } else { counted }
}
pure label_of(p: Parser, index: Int) -> Str { f"{p.piece.label}:{p.lines.get(index) ?? 1}" }

# The diagnostic for a token the grammar cannot accept. End of a program file
# inside a rule has its own wording, as in gawk.
pure unexpected(p: Parser) -> Error {
  if peek(p) == TokEnd and p.piece.label != "cmd. line" {
    let text = p.piece.text
    let last = text.split("\n")
    let lines = last.len()
    var shown = last[lines - 1]
    var line = lines
    var column = 0
    if text.ends_with("\n") { column = 0 } else { column = if shown.count_chars() > 0 { shown.count_chars() - 1 } else { 0 } }
    let pad = [" " for _ in range(column)].join("")
    return syntax_error(f"{p.piece.label}:{line}: (END OF FILE)\nawk: {p.piece.label}:{line}: {pad}^ source files / command-line arguments must contain complete functions or rules")
  }
  # GNU awk has already consumed the newline when it complains about it, so
  # it names the following line while showing the line that holds the newline.
  if is_at(p, "\n") { return syntax_error(caret_error(p.piece, token_start(p), "unexpected newline or end of string", 1)) }
  if peek(p) == TokEnd { return syntax_error(end_of_command_error(p)) }
  syntax_error(caret_error(p.piece, token_start(p), "syntax error"))
}
# An error at the end of command-line program text. GNU awk appends a newline
# and parses it as a token of its own: where the grammar accepts that newline
# the error is found one token later, on the last line; where it does not, the
# newline itself is the culprit and the line after it is named. After
# && || ? and : the lexer skips newlines itself and reports the end of the
# source in its own words.
pure end_of_command_error(p: Parser) -> Str {
  let label = p.piece.label
  let rows = p.piece.text.split("\n")
  let trailing_newline = p.piece.text.ends_with("\n")
  let counted = if trailing_newline { rows.len() - 1 } else { rows.len() }
  var last = p.at - 1
  while last >= 0 and spelling(p.tokens[last]) == "\n" { last -= 1 }
  if last < 0 { return caret_error(p.piece, token_start(p), "unexpected newline or end of string") }
  let previous = p.tokens[last]
  let symbol = spelling(previous)
  if symbol in ["&&", "||", "?", ":"] {
    let line = rows.len() + 1 + (if trailing_newline { 0 } else { 1 })
    let before = bytes.from_text(p.piece.text)[0..p.starts[last]].utf8() ?? ""
    let pieces_before = before.split("\n")
    let row_start = pieces_before[pieces_before.len() - 1]
    let pad = [" " for _ in range(row_start.count_chars())].join("")
    return f"{label}:{line}: (END OF FILE)\nawk: {label}:{line}: {pad}^ source files / command-line arguments must contain complete functions or rules"
  }
  var depth = 0
  for index in range(last + 1) {
    let text = spelling(p.tokens[index])
    if text in ["(", "["] { depth += 1 }
    if text in [")", "]"] { depth -= 1 }
  }
  let ends_operand = match previous { TokOp(op) => op in [")", "]", "++", "--"]; TokWord(word) => word != "in"; _ => true }
  var pending_conditionals = 0
  for index in range(last + 1) {
    let text = spelling(p.tokens[index])
    if text == "?" { pending_conditionals += 1 }
    if text == ":" { pending_conditionals -= 1 }
  }
  let absorbed = symbol in [",", ";", "{", "}"] or (depth <= 0 and pending_conditionals <= 0 and ends_operand)
  let line = if absorbed { counted } else { counted + 1 }
  let shown = rows[counted - 1]
  let pad = [" " for _ in range(shown.count_chars())].join("")
  f"{label}:{line}: {shown}\nawk: {label}:{line}: {pad}^ unexpected newline or end of string"
}
pure require(parser: Parser, content: Str) -> Result[Parser] {
  if is_at(parser, content) { Ok(advance(parser)) } else { Err(unexpected(parser)) }
}
pure skip_newlines(parser: Parser) -> Parser {
  var p = parser
  while is_at(p, "\n") { p.at += 1 }
  p
}
pure separators(parser: Parser) -> Parser {
  var p = parser
  while is_at(p, ";") or is_at(p, "\n") { p.at += 1 }
  p
}
pure name(parser: Parser) -> Result[Parsed[Str]] {
  match peek(parser) { TokWord(word) => Ok({parser: advance(parser), value: word}); TokCall(word) => Ok({parser: advance(parser), value: word}); _ => Err(unexpected(parser)) }
}
pure expression_node(parser: Parser, node: Expression) -> Parsed[Int] {
  var p = parser
  let id = p.program.expressions.len()
  p.program.expressions += [node]
  {parser: p, value: id}
}
pure statement_node(parser: Parser, node: Statement) -> Parsed[Int] {
  var p = parser
  let id = p.program.statements.len()
  p.program.statements += [node]
  p.program.site += [label_here(p)]
  {parser: p, value: id}
}

const BUILTINS = ["length", "substr", "index", "split", "sub", "gsub", "match", "sprintf", "sin", "cos", "atan2", "exp", "log", "sqrt", "int", "rand", "srand", "tolower", "toupper", "system", "close", "fflush", "and", "or", "xor", "lshift", "rshift", "compl", "strtonum", "systime", "strftime", "mktime", "gensub", "asort", "asorti", "patsplit", "typeof", "isarray"]
# Names no function parameter may take: the POSIX built-in functions. GNU
# extension functions are accepted as parameter names.
const POSIX_BUILTINS = ["length", "substr", "index", "split", "sub", "gsub", "match", "sprintf", "sin", "cos", "atan2", "exp", "log", "sqrt", "int", "rand", "srand", "tolower", "toupper", "system", "close", "fflush"]
const SPECIAL_VARIABLES = ["ARGC", "ARGIND", "ARGV", "BINMODE", "CONVFMT", "ENVIRON", "ERRNO", "FIELDWIDTHS", "FILENAME", "FNR", "FPAT", "FS", "IGNORECASE", "LINT", "NF", "NR", "OFMT", "OFS", "ORS", "PREC", "PROCINFO", "RLENGTH", "ROUNDMODE", "RS", "RSTART", "RT", "SUBSEP", "TEXTDOMAIN"]
const KEYWORDS = ["BEGIN", "END", "function", "func", "if", "else", "while", "for", "do", "break", "continue", "next", "nextfile", "exit", "return", "delete", "in", "getline", "print", "printf"]

# The tokens that can start an operand of an implicit concatenation.
pure starts_operand(p: Parser) -> Bool {
  match peek(p) {
    TokNumber(_) => true
    TokText(_) => true
    TokRegex(_) => true
    TokWord(word) => word not in ["else", "in", "while", "BEGIN", "END", "function", "func", "if", "for", "do", "break", "continue", "next", "nextfile", "exit", "return", "delete", "print", "printf"] and word != "getline"
    TokCall(word) => true
    TokOp(symbol) => symbol in ["$", "(", "++", "--", "!", "-", "+"]
    _ => false
  }
}

pure expression_list(parser: Parser, end: Str) -> Result[Parsed[List[Int]]] {
  var p = parser
  var ids: List[Int] = []
  if is_at(p, end) { return Ok({parser: advance(p), value: ids}) }
  loop {
    let parsed = expression(p, 0)?
    p = parsed.parser
    ids += [parsed.value]
    if is_at(p, end) { return Ok({parser: advance(p), value: ids}) }
    if ! is_at(p, ",") { return Err(unexpected(p)) }
    p = skip_newlines(advance(p))
  }
  Err(unexpected(p))
}

const PRECEDENCE: Map[Int] = {"=": 1, "+=": 1, "-=": 1, "*=": 1, "/=": 1, "%=": 1, "^=": 1, "||": 3, "&&": 4, "~": 6, "!~": 6, "==": 7, "!=": 7, "<": 7, ">": 7, "<=": 7, ">=": 7, "+": 9, "-": 9, "*": 10, "/": 10, "%": 10, "^": 12}

# Builtins may be written with a space before "(" (GNU awk); user functions may not.
pure word_term(parser: Parser, word: Str, tight: Bool) -> Result[Parsed[Int]] {
  if is_at(parser, "(") and (tight or word in BUILTINS) {
    let args = expression_list(advance(parser), ")")?
    var after = args.parser
    if word not in BUILTINS {
      after.program.uses += [Use(name: word, site: label_here(parser), call: true, argc: args.value.len())]
      for index in range(args.value.len()) {
        if is_regex_literal(after.program, args.value[index]) { after.program.notes += [f"{label_of(after, after.at - 1)}: warning: regexp constant for parameter #{index + 1} yields boolean value"] }
      }
    }
    if word in ["sub", "gsub"] and args.value.len() == 3 {
      let changeable = match after.program.expressions[args.value[2]] { Variable(_) => true; Index(_, _) => true; Field(_) => true; Text(_) => true; Number(_) => true; _ => false }
      if ! changeable { return Err(syntax_error(caret_error(after.piece, after.starts[after.at - 1], f"{word} third parameter is not a changeable object"))) }
    }
    if word == "index" and args.value.len() == 2 and is_regex_literal(after.program, args.value[1]) {
      return Err(AwkError.Plain(message: f"{label_here(parser)}: fatal: index: regexp constant as second argument is not allowed", pending: ""))
    }
    if after.sandbox and word == "system" { return Err(AwkError.Plain(message: f"{label_here(parser)}: fatal: 'system' function not allowed in sandbox mode", pending: "")) }
    return Ok(expression_node(after, Call(word, args.value)))
  }
  if word in BUILTINS and word != "length" { return Err(syntax_error(caret_error(parser.piece, token_start(parser), "syntax error"))) }
  var noted = parser
  noted.program.uses += [Use(name: word, site: label_of(parser, parser.at - 1), call: false, argc: 0)]
  if is_at(noted, "[") {
    let args = expression_list(advance(noted), "]")?
    return Ok(expression_node(args.parser, Index(word, args.value)))
  }
  # A bare length is the length of the current record, not a variable.
  if word == "length" { return Ok(expression_node(noted, Call(word, []))) }
  Ok(expression_node(noted, Variable(word)))
}
# A regexp constant standing on its own; one wrapped in parentheses is not
# diagnosed by GNU awk and is recorded in grouped.
pure is_regex_literal(program: Program, id: Int) -> Bool {
  if id in program.grouped { return false }
  match program.expressions[id] { RegexLiteral(_) => true; _ => false }
}
pure lvalue(program: Program, id: Int) -> Bool {
  match program.expressions[id] { Variable(_) => true; Field(_) => true; Index(_, _) => true; _ => false }
}
# The optional variable after "getline"; without one the record becomes $0.
pure getline_target(parser: Parser) -> Result[Parsed[Int?]] {
  match peek(parser) {
    TokWord(word) => {
      if word in KEYWORDS or word in BUILTINS { return Ok({parser: parser, value: null}) }
      let advanced = advance(parser)
      if is_at(advanced, "[") {
        let keys = expression_list(advance(advanced), "]")?
        let node = expression_node(keys.parser, Index(word, keys.value))
        return Ok({parser: node.parser, value: node.value})
      }
      let node = expression_node(advanced, Variable(word))
      Ok({parser: node.parser, value: node.value})
    }
    TokOp(op) => {
      if op != "$" { return Ok({parser: parser, value: null}) }
      let child = expression(advance(parser), 14)?
      let node = expression_node(child.parser, Field(child.value))
      Ok({parser: node.parser, value: node.value})
    }
    _ => Ok({parser: parser, value: null})
  }
}

pure expression(parser: Parser, minimum: Int) -> Result[Parsed[Int]] {
  var p = parser
  if p.depth >= 128 { return Err(syntax_error(caret_error(p.piece, token_start(p), "expression nesting limit exceeded"))) }
  p.depth += 1
  let token = peek(p)
  let begin = p
  p = advance(p)
  var first: Parsed[Int] = {parser: p, value: 0}
  match token {
    TokNumber(number) => { first = expression_node(p, Number(number)) }
    TokText(content) => { first = expression_node(p, Text(content)) }
    TokRegex(pattern) => {
      if let Err(failure) = check_regex(pattern) { return Err(syntax_error(f"{label_of(begin, begin.at)}: error: {failure.message}")) }
      first = expression_node(p, RegexLiteral(pattern))
    }
    TokWord(word) => {
      if word == "getline" {
        let target = getline_target(p)?
        if is_at(target.parser, "<") {
          if target.parser.sandbox { return Err(AwkError.Plain(message: f"{label_here(target.parser)}: fatal: redirection not allowed in sandbox mode", pending: "")) }
          let file = expression(advance(target.parser), 9)?
          first = expression_node(file.parser, GetlineFile(target.value, file.value))
        } else {
          first = expression_node(target.parser, GetlineMain(target.value))
        }
      } else if word in KEYWORDS and word != "in" {
        return Err(unexpected(begin))
      } else { first = word_term(p, word, false)? }
    }
    TokCall(word) => {
      if word in KEYWORDS { return Err(unexpected(begin)) }
      first = word_term(p, word, true)?
    }
    TokOp(op) => {
      if op == "@" { return Err(unexpected(p)) }
      if op == "(" {
        let printing = p.printing
        p.printing = false
        let inner = expression(p, 0)?
        var after = inner.parser
        if is_at(after, ",") {
          var keys = [inner.value]
          while is_at(after, ",") {
            let next = expression(skip_newlines(advance(after)), 0)?
            keys += [next.value]
            after = next.parser
          }
          after = require(after, ")")?
          if ! is_at(after, "in") { return Err(unexpected(after)) }
          let array = name(advance(after))?
          first = expression_node(array.parser, Member(keys, array.value))
        } else {
          first = {parser: require(after, ")")?, value: inner.value}
          if is_regex_literal(first.parser.program, inner.value) { first.parser.program.grouped += [inner.value] }
        }
        first.parser.printing = printing
      } else if op == "$" {
        let child = expression(p, 14)?
        first = expression_node(child.parser, Field(child.value))
      } else if op in ["!", "+", "-"] {
        let child = expression(p, 11)?
        first = expression_node(child.parser, Unary(op, child.value))
      } else if op in ["++", "--"] {
        let child = expression(p, 13)?
        if ! lvalue(child.parser.program, child.value) { return Err(unexpected(begin)) }
        first = expression_node(child.parser, Increment(child.value, if op == "++" { 1.0 } else { -1.0 }, false))
      } else { return Err(unexpected(begin)) }
    }
    _ => { return Err(unexpected(begin)) }
  }
  p = first.parser
  var left = first.value
  var relational = false
  loop {
    let op = spelling(peek(p))
    # "str"++ is not a postfix increment: the operator then starts a concatenated prefix increment.
    if minimum <= 13 and op in ["++", "--"] and lvalue(p.program, left) {
      let next = expression_node(advance(p), Increment(left, if op == "++" { 1.0 } else { -1.0 }, true))
      p = next.parser
      left = next.value
      continue
    }
    if minimum <= 2 and op == "?" {
      let yes = expression(skip_newlines(advance(p)), 0)?
      let no = expression(skip_newlines(require(yes.parser, ":")?), 2)?
      let next = expression_node(no.parser, Conditional(left, yes.value, no.value))
      p = next.parser
      left = next.value
      continue
    }
    if p.printing and (op == ">" or op == ">>" or op == "|") and ! (op == "|" and spelling(peek_at(p, 1)) == "getline") { break }
    if minimum <= 8 and op == "|" and spelling(peek_at(p, 1)) == "getline" {
      if p.sandbox { return Err(AwkError.Plain(message: f"{label_here(p)}: fatal: redirection not allowed in sandbox mode", pending: "")) }
      let target = getline_target(advance(advance(p)))?
      let next = expression_node(target.parser, CommandInput(left, target.value))
      p = next.parser
      left = next.value
      relational = false
      continue
    }
    if op == "|&" { return Err(unsupported("coprocess operator `|&'")) }
    if op == "|" and spelling(peek_at(p, 1)) != "getline" { return Err(unexpected(advance(p))) }
    if minimum <= 5 and op == "in" {
      let array = name(advance(p))?
      let next = expression_node(array.parser, Member([left], array.value))
      p = next.parser
      left = next.value
      relational = false
      continue
    }
    let known = PRECEDENCE.get(op) ?? 0
    let concat = known == 0 and starts_operand(p)
    let power = if concat { 8 } else { known }
    # An assignment whose target is already parsed binds to that target even as the right operand, as in $1==$1="x".
    let assigns = power == 1 and lvalue(p.program, left)
    if power == 0 or (power < minimum and ! assigns) { break }
    if power == 1 and ! lvalue(p.program, left) { return Err(unexpected(p)) }
    if power == 7 {
      if relational { return Err(unexpected(p)) }
      relational = true
    } else { relational = false }
    let operator = if concat { "concat" } else { op }
    let right = if concat { expression(p, 9)? } else { expression(skip_newlines_after(p, op), if power == 1 or power == 12 { power } else { power + 1 })? }
    var after_right = right.parser
    if (op == "~" or op == "!~") and is_regex_literal(after_right.program, left) {
      after_right.program.notes += [f"{label_of(after_right, after_right.at - 1)}: warning: regular expression on left of `~' or `!~' operator"]
    }
    let next = expression_node(after_right, if power == 1 { Assign(operator, left, right.value) } else { Binary(operator, left, right.value) })
    p = next.parser
    left = next.value
  }
  p.depth -= 1
  Ok({parser: p, value: left})
}
# A newline may follow && and || as well as a comma or opening brace.
pure skip_newlines_after(parser: Parser, op: Str) -> Parser {
  var p = advance(parser)
  if op in ["&&", "||"] { p = skip_newlines(p) }
  p
}

pure terminated(p: Parser) -> Bool { peek(p) == TokEnd or spelling(peek(p)) in [";", "\n", "}"] }
pure add_error(parser: Parser, message: Str) -> Parser {
  var p = parser
  p.errors += [f"{label_here(p)}: error: {message}"]
  p
}
pure block(parser: Parser) -> Result[Parsed[Int]] {
  var p = separators(require(parser, "{")?)
  var body: List[Int] = []
  while ! is_at(p, "}") {
    if peek(p) == TokEnd { return Err(unexpected(p)) }
    let next = statement(p)?
    p = separators(next.parser)
    body += [next.value]
  }
  Ok(statement_node(advance(p), Block(body)))
}
pure optional_expression(parser: Parser, end: Str) -> Result[Parsed[Int?]] {
  if is_at(parser, end) { return Ok({parser: advance(parser), value: null}) }
  let parsed = expression(parser, 0)?
  Ok({parser: require(parsed.parser, end)?, value: parsed.value})
}
# A simple statement ends at a separator, a closing brace or the end of the text.
pure finish_simple(parsed: Parsed[Int]) -> Result[Parsed[Int]] {
  if terminated(parsed.parser) { Ok(parsed) } else { Err(unexpected(parsed.parser)) }
}
pure statement(parser: Parser) -> Result[Parsed[Int]] {
  var p = parser
  if p.depth >= 128 { return Err(syntax_error(caret_error(p.piece, token_start(p), "statement nesting limit exceeded"))) }
  p.depth += 1
  let here = label_here(p)
  let next = statement_inner(p)?
  p = next.parser
  p.depth -= 1
  p.program.site[next.value] = here
  Ok({parser: p, value: next.value})
}
# True when the parenthesized group opening at the current token is followed by
# a print terminator or redirection, making it the argument list of print.
pure group_is_argument_list(p: Parser) -> Bool {
  var depth = 0
  var at = p.at
  while at < p.tokens.len() {
    let token = p.tokens[at]
    let symbol = spelling(token)
    if token == TokEnd { return false }
    if symbol == "(" { depth += 1 }
    if symbol == ")" {
      depth -= 1
      if depth == 0 {
        let after = spelling(p.tokens.get(at + 1) ?? TokEnd)
        return (p.tokens.get(at + 1) ?? TokEnd) == TokEnd or after in [";", "\n", "}", ">", ">>", "|"]
      }
    }
    at += 1
  }
  false
}
pure statement_inner(parser: Parser) -> Result[Parsed[Int]] {
  var p = skip_newlines(parser)
  if is_at(p, ";") { return Ok(statement_node(advance(p), Block([]))) }
  if is_at(p, "{") { return block(p) }
  let word = spelling(peek(p))
  if word == "if" {
    let condition = expression(require(advance(p), "(")?, 0)?
    let yes = statement(skip_newlines(require(condition.parser, ")")?))?
    p = separators(yes.parser)
    var no: Int? = null
    if is_at(p, "else") { let branch = statement(skip_newlines(advance(p)))?; p = branch.parser; no = branch.value } else { p = yes.parser }
    return Ok(statement_node(p, If(condition.value, yes.value, no)))
  }
  if word == "while" {
    let condition = expression(require(advance(p), "(")?, 0)?
    var inner = require(condition.parser, ")")?
    if is_at(inner, ";") {
      let empty = statement_node(advance(inner), Block([]))
      return Ok(statement_node(empty.parser, While(condition.value, empty.value)))
    }
    inner.loops += 1
    let body = statement(skip_newlines(inner))?
    var after = body.parser
    after.loops -= 1
    return Ok(statement_node(after, While(condition.value, body.value)))
  }
  if word == "do" {
    var inner = skip_newlines(advance(p))
    inner.loops += 1
    let body = statement(inner)?
    var after = body.parser
    after.loops -= 1
    p = require(separators(after), "while")?
    let condition = expression(require(p, "(")?, 0)?
    return finish_simple(statement_node(require(condition.parser, ")")?, Do(body.value, condition.value)))
  }
  if word == "for" {
    p = require(advance(p), "(")?
    if spelling(peek_at(p, 1)) == "in" and spelling(peek_at(p, 3)) == ")" {
      let key = name(p)?
      let array = name(require(key.parser, "in")?)?
      var inner = skip_newlines(require(array.parser, ")")?)
      inner.loops += 1
      let body = statement(inner)?
      var after = body.parser
      after.loops -= 1
      return Ok(statement_node(after, Each(key.value, array.value, body.value)))
    }
    let initial = optional_expression(p, ";")?
    let condition = optional_expression(skip_newlines(initial.parser), ";")?
    let step = optional_expression(skip_newlines(condition.parser), ")")?
    var inner = skip_newlines(step.parser)
    if is_at(inner, ";") {
      let empty = statement_node(advance(inner), Block([]))
      return Ok(statement_node(empty.parser, For(initial.value, condition.value, step.value, empty.value)))
    }
    inner.loops += 1
    let body = statement(inner)?
    var after = body.parser
    after.loops -= 1
    return Ok(statement_node(after, For(initial.value, condition.value, step.value, body.value)))
  }
  if word == "next" or word == "nextfile" {
    var after = advance(p)
    if p.context in ["BEGIN", "END"] { after = advance(add_error(p, f"`{word}' used in {p.context} action")) }
    return finish_simple(statement_node(after, if word == "next" { Next } else { NextFile }))
  }
  if word == "break" or word == "continue" {
    var after = advance(p)
    if p.loops == 0 {
      let message = if word == "break" { "`break' is not allowed outside a loop or switch" } else { "`continue' is not allowed outside a loop" }
      after = advance(add_error(add_error(p, message), message))
    }
    return finish_simple(statement_node(after, if word == "break" { Break } else { Continue }))
  }
  if word == "delete" {
    let target = name(advance(p))?
    p = target.parser
    var keys: List[Int]? = null
    if is_at(p, "[") { let next = expression_list(advance(p), "]")?; p = next.parser; keys = next.value }
    return finish_simple(statement_node(p, Delete(target.value, keys)))
  }
  if word == "exit" or word == "return" {
    if word == "return" and p.context != "function" {
      return Err(syntax_error(caret_error(p.piece, token_start(p), "`return' used outside function context")))
    }
    p = advance(p)
    var value: Int? = null
    if ! terminated(p) { let next = expression(p, 0)?; p = next.parser; value = next.value }
    return finish_simple(statement_node(p, if word == "exit" { Exit(value) } else { Return(value) }))
  }
  if word in ["print", "printf"] {
    p = advance(p)
    var args: List[Int] = []
    if is_at(p, "(") and group_is_argument_list(p) {
      if is_at(advance(p), ")") { return Err(unexpected(advance(p))) }
      let next = expression_list(advance(p), ")")?
      p = next.parser
      args = next.value
    } else if ! terminated(p) and ! is_at(p, ">") and ! is_at(p, ">>") and ! is_at(p, "|") {
      loop {
        p.printing = true
        let next = expression(p, 0)?
        p = next.parser
        p.printing = false
        args += [next.value]
        if ! is_at(p, ",") { break }
        p = skip_newlines(advance(p))
      }
    }
    var target: Int? = null
    var mode = ""
    if is_at(p, ">") or is_at(p, ">>") or is_at(p, "|") {
      if p.sandbox { return Err(AwkError.Plain(message: f"{label_here(p)}: fatal: redirection not allowed in sandbox mode", pending: "")) }
      mode = spelling(peek(p))
      let destination = expression(advance(p), 8)?
      p = destination.parser
      target = destination.value
    }
    return finish_simple(statement_node(p, Print(args, word == "printf", target, mode)))
  }
  let next = expression(p, 0)?
  finish_simple(statement_node(next.parser, ExpressionStatement(next.value)))
}

# Parses one program piece into the shared program: functions, BEGIN and END
# actions, and pattern-action rules.
pure parse_piece(program: Program, piece: Piece, sandbox: Bool) -> Result[Parsed[Int]] {
  let lexed = lex(piece)?
  var p = Parser(tokens: lexed.tokens, starts: lexed.starts, lines: lexed.lines, at: 0, program: program, printing: false, depth: 0, piece: piece, loops: 0, context: "", errors: [], sandbox: sandbox)
  for note in lexed.notes { p.program.notes += [note] }
  if TokWord("ENVIRON") in lexed.tokens { p.program.environ = true }
  p = skip_newlines(p)
  while peek(p) != TokEnd {
    let word = spelling(peek(p))
    if word == ";" {
      p = skip_newlines(advance(p))
      p.errors += [f"{p.piece.label}:{lookahead_line(p)}: each rule must have a pattern or an action part"]
      continue
    }
    if word in ["function", "func"] {
      let function_name = name(advance(p))?
      let location = label_here(p)
      if function_name.value in KEYWORDS or function_name.value in ["switch", "case", "default", "BEGINFILE", "ENDFILE"] {
        return Err(syntax_error(caret_error(piece, token_start(advance(p)), "syntax error")))
      }
      if function_name.value in BUILTINS {
        return Err(syntax_error(caret_error(piece, token_start(advance(p)), f"`{function_name.value}' is a built-in function, it cannot be redefined")))
      }
      p = require(function_name.parser, "(")?
      if function_name.value in SPECIAL_VARIABLES or function_name.value in ["FUNCTAB", "SYMTAB"] {
        p.errors += [f"{location}: error: function name `{function_name.value}' previously defined"]
      }
      var parameters: List[Str] = []
      if ! is_at(p, ")") {
        loop {
          let parameter_start = token_start(p)
          let parameter = name(p)?
          if parameter.value in KEYWORDS or parameter.value in POSIX_BUILTINS { return Err(syntax_error(caret_error(piece, parameter_start, "syntax error"))) }
          p = parameter.parser
          if parameter.value in SPECIAL_VARIABLES {
            p = add_error(p, f"function `{function_name.value}': parameter `{parameter.value}': POSIX disallows using a special variable as a function parameter")
          }
          if parameter.value == function_name.value { p = add_error(p, f"function `{function_name.value}': cannot use function name as parameter name") }
          if parameter.value in parameters {
            let first = 1 + [at for at in range(parameters.len()) if parameters[at] == parameter.value][0]
            p = add_error(p, f"function `{function_name.value}': parameter #{parameters.len() + 1}, `{parameter.value}', duplicates parameter #{first}")
          }
          parameters += [parameter.value]
          if is_at(p, ")") { break }
          p = skip_newlines(require(p, ",")?)
        }
      }
      p.context = "function"
      let body = block(skip_newlines(require(p, ")")?))?
      p = body.parser
      p.context = ""
      if function_name.value in p.program.functions { p.errors += [f"{location}: error: function name `{function_name.value}' previously defined"] }
      p.program.functions = p.program.functions.set(function_name.value, Function(parameters: parameters, body: body.value, site: location))
    } else if word in ["BEGIN", "END"] {
      let after = advance(p)
      if is_at(after, "\n") or is_at(after, ";") or peek(after) == TokEnd {
        # GNU awk reports the line of the token that follows the rule's
        # terminator, or the last line of the source when none follows, and
        # keeps parsing to report further errors.
        var ahead = after
        if is_at(ahead, ";") { ahead = advance(ahead) }
        ahead = skip_newlines(ahead)
        ahead.errors += [f"{p.piece.label}:{lookahead_line(ahead)}: {word} blocks must have an action part"]
        p = ahead
        continue
      }
      var inner = after
      inner.context = word
      let body = block(inner)?
      p = body.parser
      p.context = ""
      if word == "BEGIN" { p.program.begin += [body.value] } else { p.program.end += [body.value] }
    } else {
      var pattern: Int? = null
      var end: Int? = null
      let site = label_here(p)
      if ! is_at(p, "{") { let next = expression(p, 0)?; p = next.parser; pattern = next.value }
      if is_at(p, ",") { let next = expression(skip_newlines(advance(p)), 0)?; p = next.parser; end = next.value }
      var action: Parsed[Int] = {parser: p, value: 0}
      if is_at(p, "{") {
        p.context = "main"
        action = block(p)?
        p = action.parser
        p.context = ""
      } else {
        if ! terminated(p) { return Err(unexpected(p)) }
        action = statement_node(p, Print([], false, null, ""))
        p = action.parser
      }
      p.program.rules += [Rule(pattern: pattern, end: end, action: action.value, site: site)]
    }
    # One semicolon may follow an item; newlines are free.
    p = skip_newlines(p)
    if is_at(p, ";") { p = skip_newlines(advance(p)) }
  }
  if ! p.errors.is_empty() { return Err(syntax_error(p.errors.join("\nawk: "))) }
  Ok({parser: p, value: 0})
}

pure parse(pieces: List[Piece], sandbox: Bool) -> Result[Program] {
  var program = Program(expressions: [], statements: [], site: [], begin: [], end: [], rules: [], functions: {}, notes: [], uses: [], environ: false, grouped: [])
  for piece in pieces { program = parse_piece(program, piece, sandbox)?.parser.program }
  var errors: List[Str] = []
  for seen in program.uses {
    if seen.name not in program.functions { continue }
    # A call with more arguments than declared draws a warning from GNU awk;
    # the BusyBox suite requires a quiet run, so it is only evaluated.
    if ! seen.call {
      errors += [f"{seen.site}: error: function `{seen.name}' called with space between name and `(',\nor used as a variable or an array"]
    }
  }
  if ! errors.is_empty() { return Err(syntax_error(errors.join("\nawk: "))) }
  Ok(program)
}


# Input preserves spelling and numeric classification; string literals keep string
# comparison and truth semantics even when their spelling is a decimal number.
# Positive and negative NaN print differently in GNU awk, so NaN carries its sign.
enum Scalar { Empty, Numeric(Float), String(Str), Input(Str, Float?), Nan(Bool) }

pure decimal_end(content: Str) -> Int? {
  var at = 0
  if content.byte_at(at) == 43 or content.byte_at(at) == 45 { at += 1 }
  var digits = 0
  while digit(content.byte_at(at) ?? 0) { at += 1; digits += 1 }
  if content.byte_at(at) == 46 { at += 1; while digit(content.byte_at(at) ?? 0) { at += 1; digits += 1 } }
  if digits == 0 { return null }
  var end = at
  if content.byte_at(at) == 101 or content.byte_at(at) == 69 {
    at += 1
    if content.byte_at(at) == 43 or content.byte_at(at) == 45 { at += 1 }
    let start = at
    while digit(content.byte_at(at) ?? 0) { at += 1 }
    if at > start { end = at }
  }
  end
}
pure trimmed(content: Str) -> Str {
  var start = 0
  var end = content.byte_len()
  while start < end and (content.byte_at(start) ?? 0) in [9, 10, 11, 12, 13, 32] { start += 1 }
  while end > start and (content.byte_at(end - 1) ?? 0) in [9, 10, 11, 12, 13, 32] { end -= 1 }
  content.byte_slice(start, length: end - start)
}
# GNU awk reads "+inf", "-inf", "+nan" and "-nan" (sign required, four
# characters) as the special values; other spellings are plain strings.
# Returns 0 when plain is not special, 1 and 2 for +inf and -inf, 3 and 4 for
# +nan and -nan.
pure ieee_kind(plain: Str) -> Int {
  if plain.byte_len() != 4 { return 0 }
  let sign = plain.byte_at(0) ?? 0
  if sign != 43 and sign != 45 { return 0 }
  let word = plain.byte_slice(1, length: 3).lower()
  if word == "inf" { return if sign == 43 { 1 } else { 2 } }
  if word == "nan" { return if sign == 43 { 3 } else { 4 } }
  0
}
pure infinity() -> Float { let zero = 0.0; 1.0 / zero }
pure not_a_number() -> Float { let zero = 0.0; zero / zero }
pure input(content: Str) -> Result[Scalar] {
  let plain = trimmed(content)
  let end = decimal_end(plain)
  if end != null and end == plain.byte_len() { return Ok(Input(content, plain.parse_float()?)) }
  let kind = ieee_kind(plain)
  if kind == 1 { return Ok(Input(content, infinity())) }
  if kind == 2 { return Ok(Input(content, -infinity())) }
  if kind >= 3 { return Ok(Input(content, not_a_number())) }
  Ok(Input(content, null))
}
pure numeric_prefix(content: Str) -> Result[Float] {
  let plain = trimmed(content)
  let end = decimal_end(plain)
  if end == null {
    let kind = ieee_kind(plain)
    if kind == 1 { return Ok(infinity()) }
    if kind == 2 { return Ok(-infinity()) }
    if kind >= 3 { return Ok(not_a_number()) }
    return Ok(0.0)
  }
  Ok(plain.byte_slice(0, length: end).parse_float()?)
}
pure number(value: Scalar) -> Result[Float] {
  match value { Empty => Ok(0.0); Numeric(n) => Ok(n); Nan(_) => Ok(not_a_number()); String(content) => numeric_prefix(content); Input(content, n) => if n != null { Ok(n) } else { numeric_prefix(content) } }
}
pure numeric(value: Scalar) -> Bool { match value { Empty => true; Numeric(_) => true; Nan(_) => true; Input(_, n) => n != null; _ => false } }
pure truth(value: Scalar) -> Result[Bool] {
  if numeric(value) { return Ok(number(value)? != 0.0) }
  Ok(match value { String(content) => ! content.is_empty(); Input(content, _) => ! content.is_empty(); _ => false })
}
# NaN equals itself in XSH, so it is recognized as the non-infinite special.
pure is_nan(n: Float) -> Bool { n - n != 0.0 and ! (n > 0.0) and ! (n < 0.0) }
pure integer(value: Float) -> Result[Int] {
  if is_nan(value) or value >= 9223372036854775808.0 or value < -9223372036854775808.0 { return Ok(if is_nan(value) or value < 0.0 { -9223372036854775807 - 1 } else { 9223372036854775807 }) }
  if value < 0.0 { value.ceil() } else { value.floor() }
}
# The integer part of a float; floats this large have no fraction already.
pure truncate(n: Float) -> Float {
  if n - n != 0.0 { return n }
  if n >= 4503599627370496.0 or n <= -4503599627370496.0 { return n }
  let whole = (if n < 0.0 { n.ceil() } else { n.floor() }) ?? 0
  whole.float()
}
pure special_text(n: Float, negative_nan: Bool) -> Str {
  if is_nan(n) { return if negative_nan { "-nan" } else { "+nan" } }
  if n > 0.0 { "+inf" } else { "-inf" }
}
# Numbers print as integers when integral, and through the conversion format
# (CONVFMT or OFMT) otherwise, as GNU awk does.
pure float_text(n: Float, conversion: Str) -> Result[Str] {
  if n - n != 0.0 { return Ok(special_text(n, true)) }
  if n == 0.0 or n == -0.0 { return Ok("0") }
  if truncate(n) == n { return Ok(n.format_number("f", 0)?) }
  let parsed = parse_format(conversion, 1, [], 0)?
  let spec = parsed.format ?? DEFAULT_FORMAT
  if ! conversion.starts_with("%") or parsed.at != conversion.byte_len() or parsed.format == null or spec.conversion in ["s", "c", "%"] { return Err(runtime("invalid numeric conversion format")) }
  formatted(Numeric(n), spec, "%.6g")
}
pure plain_text(value: Scalar) -> Result[Str] {
  match value {
    Empty => Ok("")
    String(content) => Ok(content)
    Input(content, _) => Ok(content)
    Nan(positive) => Ok(if positive { "+nan" } else { "-nan" })
    Numeric(n) => float_text(n, "%.6g")
  }
}
pure scalar_text(value: Scalar, conversion: Str) -> Result[Str] {
  match value {
    Numeric(n) => float_text(n, conversion)
    _ => plain_text(value)
  }
}

type Format = {conversion: Str, width: Int, precision: Int?, left: Bool, plus: Bool, space: Bool, zero: Bool, alternate: Bool, argument: Int?}
type ParsedFormat = {format: Format?, at: Int, argument: Int, literal: Str}
const DEFAULT_FORMAT: Format = {conversion: "s", width: 0, precision: null, left: false, plus: false, space: false, zero: false, alternate: false, argument: null}

# One conversion specification after "%". A specification GNU awk does not
# recognize is returned as literal text and printed unchanged. Stars consume
# arguments before the value; numeric rendering receives only the conversion
# and precision after flags, width and padding were interpreted here.
pure parse_format(format: Str, start: Int, values: List[Scalar], argument: Int) -> Result[ParsedFormat] {
  var at = start
  var arg = argument
  var spec = Format(conversion: "", width: 0, precision: null, left: false, plus: false, space: false, zero: false, alternate: false, argument: null)
  var look = at
  while digit(format.byte_at(look) ?? 0) { look += 1 }
  if look > at and format.byte_at(look) == 36 {
    spec.argument = format.byte_slice(at, length: look - at).parse_int_decimal()?
    at = look + 1
  }
  while at < format.byte_len() {
    let flag = format.byte_slice(at, length: 1)
    if flag == "-" { spec.left = true } else if flag == "+" { spec.plus = true } else if flag == " " { spec.space = true } else if flag == "0" { spec.zero = true } else if flag == "#" { spec.alternate = true } else if flag == "'" { } else { break }
    at += 1
  }
  if format.byte_at(at) == 42 {
    if arg >= values.len() { return Err(runtime("not enough arguments to satisfy format string")) }
    spec.width = integer(number(values[arg])?)?
    arg += 1
    at += 1
    if spec.width < 0 { spec.left = true; spec.width = -spec.width }
  } else {
    let begin = at
    while digit(format.byte_at(at) ?? 0) { at += 1 }
    if at > begin { spec.width = format.byte_slice(begin, length: at - begin).parse_int_decimal()? }
  }
  if format.byte_at(at) == 46 {
    at += 1
    if format.byte_at(at) == 42 {
      if arg >= values.len() { return Err(runtime("not enough arguments to satisfy format string")) }
      let precision = integer(number(values[arg])?)?
      spec.precision = if precision < 0 { null } else { precision }
      arg += 1
      at += 1
    } else {
      let begin = at
      while digit(format.byte_at(at) ?? 0) { at += 1 }
      spec.precision = if at == begin { 0 } else { format.byte_slice(begin, length: at - begin).parse_int_decimal()? }
    }
  }
  if at < format.byte_len() and format.byte_slice(at, length: 1) in ["h", "l", "L", "q", "z", "j", "t"] { at += 1 }
  if at >= format.byte_len() { return Ok({format: null, at: at, argument: arg, literal: "%" + format.byte_slice(start)}) }
  let code = format.byte_slice(at, length: 1)
  if code not in ["s", "c", "d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G", "a", "A", "%"] {
    # A character that is not a conversion (UTF-8 aware) is taken whole.
    let tail = format.byte_slice(at)
    let shown = tail.split("")[0]
    return Ok({format: null, at: at + shown.byte_len(), argument: arg, literal: "%" + format.byte_slice(start, length: at - start) + shown})
  }
  if spec.width > 1000000 or (spec.precision ?? 0) > 1000000 { return Err(runtime("printf width or precision exceeds limit")) }
  spec.conversion = if code == "i" { "d" } else { code }
  Ok({format: spec, at: at + 1, argument: arg, literal: ""})
}
pure padding(char: Str, count: Int) -> Str { [char for _ in range(count)].join("") }
pure utf8_char(code: Int) -> Str {
  if code < 0 or code > 1114111 or (code >= 55296 and code <= 57343) { return "\u{fffd}" }
  var encoded: List[Int] = []
  if code < 128 { encoded = [code] } else if code < 2048 { encoded = [192 + code / 64, 128 + code % 64] } else if code < 65536 { encoded = [224 + code / 4096, 128 + code / 64 % 64, 128 + code % 64] } else { encoded = [240 + code / 262144, 128 + code / 4096 % 64, 128 + code / 64 % 64, 128 + code % 64] }
  let encoded_bytes = bytes.from_ints(encoded) ?? b""
  encoded_bytes.utf8() ?? "\u{fffd}"
}
pure formatted(value: Scalar, spec: Format, conversion: Str) -> Result[Str] {
  let code = spec.conversion
  var result = ""
  var prefix = ""
  if code == "s" {
    result = scalar_text(value, conversion)?
    if spec.precision != null and result.count_chars() > spec.precision { result = result.split("")[0..spec.precision].join("") }
  } else if code == "c" {
    if numeric(value) {
      result = utf8_char(integer(number(value)?)?)
    } else {
      let content = scalar_text(value, conversion)?
      result = if content.is_empty() { "\0" } else { content.split("")[0] }
    }
  } else {
    let n = number(value)?
    if n - n != 0.0 {
      result = special_text(n, match value { Nan(positive) => ! positive; _ => true })
      let count = spec.width - result.byte_len()
      if count > 0 { result = if spec.left { result + padding(" ", count) } else { padding(" ", count) + result } }
      return Ok(result)
    }
    let signed = code in ["d", "f", "F", "e", "E", "g", "G", "a", "A"]
    let precision = spec.precision ?? (if code in ["d", "o", "u", "x", "X"] { 1 } else { 6 })
    let integral_code = code in ["d", "o", "u", "x", "X"]
    if integral_code and (n >= 9223372036854775808.0 or n <= -9223372036854775808.0) {
      # Beyond the 64-bit range GNU awk prints the exact decimal digits.
      result = n.format_number("f", 0)?
    } else {
      result = n.format_number(code, precision)?
    }
    if signed {
      if result.starts_with("-") { prefix = "-"; result = result[1..] } else if spec.plus { prefix = "+" } else if spec.space { prefix = " " }
    }
    if spec.alternate {
      if code == "o" and ! result.starts_with("0") { prefix += "0" }
      if code == "x" and n != 0.0 { prefix += "0x" }
      if code == "X" and n != 0.0 { prefix += "0X" }
      if code in ["f", "F", "e", "E"] and precision == 0 {
        let exp = result.find("e") ?? result.find("E") ?? result.byte_len()
        result = result.byte_slice(0, length: exp) + "." + result.byte_slice(exp)
      }
      if code in ["g", "G"] {
        let exp = result.find("e") ?? result.find("E") ?? result.byte_len()
        var mantissa = result.byte_slice(0, length: exp)
        let digits = mantissa.delete(".")
        var leading = 0
        while leading < digits.byte_len() - 1 and digits.byte_at(leading) == 48 { leading += 1 }
        let significant = digits.byte_len() - leading
        if "." not in mantissa { mantissa += "." }
        let wanted = if precision == 0 { 1 } else { precision }
        if significant < wanted { mantissa += padding("0", wanted - significant) }
        result = mantissa + result.byte_slice(exp)
      }
    }
  }
  let count = spec.width - prefix.count_chars() - result.count_chars()
  if count > 0 {
    let zero = spec.zero and ! spec.left and code not in ["s", "c"] and (spec.precision == null or code in ["f", "F", "e", "E", "g", "G"])
    if spec.left { result += padding(" ", count) } else if zero { result = padding("0", count) + result } else { prefix = padding(" ", count) + prefix }
  }
  Ok(prefix + result)
}
pure printf(format: Str, values: List[Scalar], conversion: Str) -> Result[Str] {
  var at = 0
  var argument = 0
  var out = ""
  while at < format.byte_len() {
    if format.byte_at(at) != 37 {
      let next = format.find("%", at) ?? format.byte_len()
      out += format.byte_slice(at, length: next - at)
      at = next
      continue
    }
    let begin = at
    at += 1
    if format.byte_at(at) == 37 { out += "%"; at += 1; continue }
    let parsed = parse_format(format, at, values, argument)?
    let spec = parsed.format
    if spec == null { out += parsed.literal; at = parsed.at; argument = parsed.argument; continue }
    let used = spec ?? DEFAULT_FORMAT
    if used.conversion == "%" { out += "%"; at = parsed.at; continue }
    argument = parsed.argument
    at = parsed.at
    var index = argument
    if used.argument != null { index = (used.argument ?? 1) - 1 }
    if index >= values.len() or index < 0 {
      let pad = padding(" ", begin + 1)
      return Err(runtime(f"not enough arguments to satisfy format string\n\t`{format}'\n\t{pad}^ ran out for this one"))
    }
    out += formatted(values[index], used, conversion)?
    if used.argument == null { argument += 1 }
  }
  Ok(out)
}

type Reader = {fd: Int, text: Str, at: Int, done: Bool, tail: Bytes, pollable: Bool, lossy: Bool, job: ProcessHandle?, name: Str, command: Bool}
type Writer = {fd: Int, pending: Str, job: ProcessHandle?, name: Str, command: Bool}
type Cell = {value: Scalar, seq: Int}
# kind is 1 when the first subscript was a non-negative integer, 2 for a
# negative one and 3 otherwise; size and growths track the hash table GNU awk
# would hold, which fixes the order a for-in loop visits the subscripts in.
type Table = {cells: Map[Cell], next: Int, kind: Int, size: Int, growths: List[Int]}

#
# The host POSIX engine works on bytes and does not know the GNU awk
# conventions (a leading star is literal, backslash escapes inside brackets,
# UTF-8 characters count as one). Patterns are rewritten for it, and subjects
# with non-ASCII text are matched in a one-byte-per-character alphabet whose
# offsets map back to the original bytes.

type Match = {start: Int, end: Int}
type Alphabet = {chars: Map[Str], free: List[Int], invalid: Str}

pure is_ascii_text(content: Str) -> Bool { content.byte_len() == content.count_chars() and "\0" not in content }
pure shown_char(code: Int) -> Str {
  let one = bytes.from_ints([code]) ?? b"?"
  one.utf8() ?? "?"
}
const REGEX_SPECIAL = [".", "[", "]", "(", ")", "*", "+", "?", "{", "}", "|", "^", "$", "\\"]
const SIMPLE_ESCAPES: Map[Int] = {"a": 7, "b": 8, "f": 12, "n": 10, "r": 13, "t": 9, "v": 11}

# The replacement for a pattern character: ASCII text passes through, other
# characters take their alphabet byte. Escaped when the byte is a metacharacter.
pure literal_char(character: Str, alphabet: Alphabet) -> Str {
  if character.byte_len() == 1 and character != "\0" { return if character in REGEX_SPECIAL { "\\" + character } else { character } }
  alphabet.chars.get(character) ?? character
}

pure ere_error(message: Str) -> Error { AwkError.Syntax(message) }

# Translates one awk regular expression into the host's ERE dialect.
pure rx_translate(pattern: Str, alphabet: Alphabet) -> Result[Str] {
  let chars = pattern.split("")
  var out: List[Str] = []
  var at = 0
  var depth = 0
  var operand_start = true
  while at < chars.len() {
    let c = chars[at]
    if c == "\\" {
      if at + 1 >= chars.len() { return Err(ere_error("Trailing backslash")) }
      let next = chars[at + 1]
      at += 2
      operand_start = false
      if next in SIMPLE_ESCAPES {
        let code = SIMPLE_ESCAPES.get(next) ?? 0
        out += [literal_char(shown_char(code), alphabet)]
      } else if next in ["0", "1", "2", "3", "4", "5", "6", "7"] {
        var number = digit_value(next.byte_at(0) ?? 48)
        var count = 1
        while count < 3 and at < chars.len() and chars[at] in ["0", "1", "2", "3", "4", "5", "6", "7"] {
          number = number * 8 + digit_value(chars[at].byte_at(0) ?? 48)
          at += 1
          count += 1
        }
        out += [literal_char(shown_char(number % 256), alphabet)]
      } else if next == "x" {
        var number = 0
        var count = 0
        while count < 2 and at < chars.len() and chars[at].byte_len() == 1 and hex_digit(chars[at].byte_at(0) ?? 0) {
          number = number * 16 + digit_value(chars[at].byte_at(0) ?? 48)
          at += 1
          count += 1
        }
        out += [if count == 0 { "x" } else { literal_char(shown_char(number), alphabet) }]
      } else if next in ["<", ">", "y", "B", "`", "'"] {
        return Err(unsupported(f"regular expression operator \\{next} (word boundaries) is not available"))
      } else if next in ["w", "W", "s", "S"] {
        out += ["\\" + next]
      } else if next in ["1", "2", "3", "4", "5", "6", "7", "8", "9"] {
        out += ["\\" + next]
      } else if next in REGEX_SPECIAL {
        out += ["\\" + next]
      } else if next == "/" or next == "\"" or next == "-" or next == "&" or next == "=" or next == "," or next == ";" or next == ":" or next == "'" or next == " " or next == "!" or next == "@" or next == "#" or next == "%" or next == "~" or next == "_" {
        out += [next]
      } else {
        out += [literal_char(next, alphabet)]
      }
      continue
    }
    if c == "[" {
      let translated = rx_bracket(chars, at, alphabet)?
      out += [translated.text]
      at = translated.at
      operand_start = false
      continue
    }
    if c == "(" {
      depth += 1
      out += ["("]
      at += 1
      operand_start = true
      continue
    }
    if c == ")" {
      if depth == 0 { return Err(ere_error("Unmatched ) or \\)")) }
      depth -= 1
      out += [")"]
      at += 1
      operand_start = false
      continue
    }
    if c == "|" {
      out += ["|"]
      at += 1
      operand_start = true
      continue
    }
    if c == "^" {
      out += ["^"]
      at += 1
      # A repetition operator right after an anchor is literal in GNU syntax.
      operand_start = true
      continue
    }
    if c in ["*", "+", "?"] and operand_start {
      out += ["\\" + c]
      at += 1
      operand_start = false
      continue
    }
    if c == "{" {
      var end = at + 1
      var comma = false
      var low = ""
      var high = ""
      while end < chars.len() and chars[end].byte_len() == 1 and digit(chars[end].byte_at(0) ?? 0) { low += chars[end]; end += 1 }
      if end < chars.len() and chars[end] == "," {
        comma = true
        end += 1
        while end < chars.len() and chars[end].byte_len() == 1 and digit(chars[end].byte_at(0) ?? 0) { high += chars[end]; end += 1 }
      }
      if end < chars.len() and chars[end] == "}" and (! low.is_empty() or ! high.is_empty()) and ! operand_start {
        let min = if low.is_empty() { 0 } else { low.parse_int_decimal()? }
        if comma and ! high.is_empty() {
          let max = high.parse_int_decimal()?
          if max < min { return Err(ere_error("Invalid content of \\{\\}")) }
        }
        if min > 32767 or (high.is_empty() == false and (high.parse_int_decimal() ?? 0) > 32767) { return Err(ere_error("Regular expression too big")) }
        out += [f"{{{min}" + (if comma { "," + high } else { "" }) + "}"]
        at = end + 1
        continue
      }
      out += ["\\{"]
      at += 1
      operand_start = false
      continue
    }
    if c == "." and ! alphabet.invalid.is_empty() { out += ["[^" + alphabet.invalid + "]"] } else if c in [".", "$", "*", "+", "?", "]", "}"] { out += [c] } else { out += [literal_char(c, alphabet)] }
    at += 1
    operand_start = false
  }
  if depth != 0 { return Err(ere_error("Unmatched ( or \\(")) }
  Ok(out.join(""))
}
type Bracket = {text: Str, at: Int}
# A bracket expression starting at chars[at] == "[". GNU awk lets a backslash
# escape inside brackets; equivalence and collating elements stand for their
# single character.
pure rx_bracket(chars: List[Str], start: Int, alphabet: Alphabet) -> Result[Bracket] {
  var at = start + 1
  var negate = false
  if at < chars.len() and chars[at] == "^" { negate = true; at += 1 }
  var items: List[Str] = []
  var has_close = false
  var has_dash = false
  var has_caret = false
  var first = true
  var closed = false
  while at < chars.len() {
    let c = chars[at]
    if c == "]" and ! first { closed = true; at += 1; break }
    first = false
    if c == "[" and at + 1 < chars.len() and chars[at + 1] in [":", "=", "."] {
      let kind = chars[at + 1]
      var end = at + 2
      var name = ""
      while end + 1 < chars.len() and ! (chars[end] == kind and chars[end + 1] == "]") { name += chars[end]; end += 1 }
      if end + 1 >= chars.len() { return Err(ere_error("Unmatched [, [^, [:, [., or [=")) }
      if kind == ":" { items += ["[:" + name + ":]"] } else {
        if name.count_chars() != 1 { return Err(ere_error("Invalid collation character")) }
        if name == "]" { has_close = true } else if name == "-" { has_dash = true } else if name == "^" { has_caret = true } else { items += [bracket_char(name, alphabet)] }
      }
      at = end + 2
      continue
    }
    var current = c
    at += 1
    if c == "\\" and at < chars.len() {
      let next = chars[at]
      at += 1
      if next in SIMPLE_ESCAPES { current = shown_char(SIMPLE_ESCAPES.get(next) ?? 0) } else { current = next }
    }
    # A range: current-endpoint.
    if at + 1 < chars.len() and chars[at] == "-" and chars[at + 1] != "]" {
      var high = chars[at + 1]
      var after = at + 2
      if high == "\\" and after < chars.len() { high = chars[after]; after += 1 }
      if current.byte_len() > 1 or high.byte_len() > 1 {
        # A range over non-ASCII characters lists the alphabet members inside it.
        let low_code = code_point(current)
        let high_code = code_point(high)
        for pair in alphabet.chars.keys() {
          let code = code_point(pair)
          if code >= low_code and code <= high_code { items += [alphabet.chars.get(pair) ?? ""] }
        }
        if current.byte_len() == 1 and current not in ["]", "-", "^"] { items += [bracket_char(current, alphabet)] }
      } else {
        items += [bracket_char(current, alphabet) + "-" + bracket_char(high, alphabet)]
      }
      at = after
      continue
    }
    if current == "]" { has_close = true } else if current == "-" { has_dash = true } else if current == "^" { has_caret = true } else { items += [bracket_char(current, alphabet)] }
  }
  if ! closed { return Err(ere_error("Unmatched [, [^, [:, [., or [=")) }
  let body = (if has_close { "]" } else { "" }) + items.join("") + (if negate { alphabet.invalid } else { "" }) + (if has_caret { "^" } else { "" }) + (if has_dash { "-" } else { "" })
  Ok({text: "[" + (if negate { "^" } else { "" }) + body + "]", at: at})
}
pure bracket_char(character: Str, alphabet: Alphabet) -> Str {
  if character.byte_len() == 1 and character != "\0" { return character }
  alphabet.chars.get(character) ?? character
}
pure code_point(character: Str) -> Int {
  let encoded = bytes.from_text(character)
  let lead = encoded.byte_at(0) ?? 0
  if lead < 128 { return lead }
  var count = if lead >= 240 { 3 } else if lead >= 224 { 2 } else { 1 }
  var code = if lead >= 240 { lead - 240 } else if lead >= 224 { lead - 224 } else { lead - 192 }
  for index in range(1, count + 1) { code = code * 64 + ((encoded.byte_at(index) ?? 128) - 128) }
  code
}

# An alphabet mapping every distinct non-ASCII character of the pattern and
# subject to an unused control byte (all remaining ones to one shared byte).
pure make_alphabet(pattern: Str, subject: Str) -> Alphabet {
  var present = [false for _ in range(128)]
  let source = bytes.from_text(pattern + subject)
  for index in range(source.len()) {
    let byte = source.byte_at(index) ?? 0
    if byte < 128 { present[byte] = true }
  }
  var free: List[Int] = []
  for code in [1, 2, 3, 4, 5, 6, 7, 8, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 127] { if ! present[code] { free += [code] } }
  var chars: Map[Str] = {}
  var next = 0
  for character in (pattern + subject).split("") {
    if (character.byte_len() > 1 or character == "\0") and ! (character in chars) {
      if next < free.len() { chars = chars.set(character, shown_char(free[next])); next += 1 } else { chars = chars.set(character, shown_char(free[0])) }
    }
  }
  # Characters standing for invalid input bytes (U+10FF80 and up) never match
  # "." or a negated bracket expression.
  var invalid = ""
  for character in chars.keys() {
    if code_point(character) >= 1113984 { invalid += chars.get(character) ?? "" }
  }
  {chars: chars, free: free, invalid: invalid}
}

const NO_ALPHABET: Alphabet = {chars: {}, free: [], invalid: ""}

# GNU word-boundary operators (\\<, \\>, \\y, \\B) cannot be expressed to the
# host engine; one at the very start or end of a pattern is supported by
# filtering its matches instead.
type Bounds = {lead: Str, body: Str, trail: Str}
pure split_boundaries(pattern: Str) -> Bounds {
  var body = pattern
  var lead = ""
  var trail = ""
  for operator in ["<", "y", "B"] {
    if lead.is_empty() and body.starts_with("\\" + operator) { lead = operator; body = body.byte_slice(2) }
  }
  for operator in [">", "y", "B"] {
    if trail.is_empty() and body.ends_with("\\" + operator) {
      var slashes = 0
      var at = body.byte_len() - 3
      while at >= 0 and body.byte_at(at) == 92 { slashes += 1; at -= 1 }
      if slashes % 2 == 0 { trail = operator; body = body.byte_slice(0, length: body.byte_len() - 2) }
    }
  }
  {lead: lead, body: body, trail: trail}
}
pure word_byte(byte: Int, marked: List[Bool]) -> Bool {
  if byte >= 128 { return true }
  if byte >= 48 and byte <= 57 { return true }
  if byte >= 65 and byte <= 90 { return true }
  if byte >= 97 and byte <= 122 { return true }
  if byte == 95 { return true }
  marked[byte]
}
# Whether the lead and trail boundary conditions hold for a match.
pure boundaries_hold(subject: Bytes, start: Int, end: Int, lead: Str, trail: Str, marked: List[Bool]) -> Bool {
  if ! lead.is_empty() {
    let before = start > 0 and word_byte(subject.byte_at(start - 1) ?? 0, marked)
    let after = start < subject.len() and word_byte(subject.byte_at(start) ?? 0, marked)
    let boundary = before != after
    if lead == "<" and ! (! before and after) { return false }
    if lead == "y" and ! boundary { return false }
    if lead == "B" and boundary { return false }
  }
  if ! trail.is_empty() {
    let before = end > 0 and word_byte(subject.byte_at(end - 1) ?? 0, marked)
    let after = end < subject.len() and word_byte(subject.byte_at(end) ?? 0, marked)
    let boundary = before != after
    if trail == ">" and ! (before and ! after) { return false }
    if trail == "y" and ! boundary { return false }
    if trail == "B" and boundary { return false }
  }
  true
}
# Matches of a translated pattern over a byte subject, keeping only those
# whose boundary conditions hold.
pure engine_matches(translated: Str, subject: Bytes, bounds: Bounds, marked: List[Bool], ignore_case: Bool) -> Result[List[Match]] {
  if bounds.lead.is_empty() and bounds.trail.is_empty() {
    let hits = regex.find_bytes(translated, subject, extended: true, ignore_case: ignore_case)?
    return Ok([{start: hit.start, end: hit.end} for hit in hits])
  }
  var found: List[Match] = []
  var offset = 0
  while offset <= subject.len() {
    let caps = regex.captures_bytes(translated, subject, offset, extended: true, ignore_case: ignore_case)?
    if caps.is_empty() { break }
    let span = caps[0] ?? {start: 0, end: 0}
    if boundaries_hold(subject, span.start, span.end, bounds.lead, bounds.trail, marked) {
      found += [{start: span.start, end: span.end}]
      offset = if span.end > span.start { span.end } else { span.end + 1 }
    } else { offset = span.start + 1 }
  }
  Ok(found)
}

# All non-overlapping leftmost-longest matches of an awk regular expression,
# as byte offsets into content.
pure rx_matches(pattern: Str, content: Str, ignore_case: Bool) -> Result[List[Match]] {
  let bounds = split_boundaries(pattern)
  var no_marks = [false for _ in range(128)]
  if is_ascii_text(content) and is_ascii_text(pattern) {
    let translated = rx_translate(bounds.body, NO_ALPHABET)?
    return engine_matches(translated, bytes.from_text(content), bounds, no_marks, ignore_case)
  }
  let alphabet = make_alphabet(pattern, content)
  let translated = rx_translate(bounds.body, alphabet)?
  let characters = content.split("")
  var mapped: List[Int] = []
  var offsets: List[Int] = []
  var position = 0
  for character in characters {
    offsets += [position]
    position += character.byte_len()
    if character.byte_len() == 1 and character != "\0" { mapped += [character.byte_at(0) ?? 0] } else {
      let shown = alphabet.chars.get(character) ?? "\u{80}"
      mapped += [if shown.byte_len() == 1 { shown.byte_at(0) ?? 1 } else { 1 }]
    }
  }
  offsets += [position]
  # Characters outside ASCII count as word characters for the boundary tests.
  for shown in alphabet.chars.values() {
    if shown.byte_len() == 1 { no_marks[shown.byte_at(0) ?? 0] = true }
  }
  let hits = engine_matches(translated, bytes.from_ints(mapped)?, bounds, no_marks, ignore_case)?
  Ok([{start: offsets[hit.start], end: offsets[hit.end]} for hit in hits])
}
# Parse-time validation of a regular expression literal.
pure check_regex(pattern: Str) -> Result[Unit] {
  match rx_matches(pattern, "", false) {
    Ok(_) => Ok()
    Err(failure) => Err(syntax_error(f"{failure.message}: /{pattern}/"))
  }
}

# The leftmost-longest match with its parenthesized groups (index 0 is the
# whole match; null for groups that did not take part).
pure rx_captures(pattern: Str, content: Str, ignore_case: Bool) -> Result[List[Match?]] {
  if is_ascii_text(content) and is_ascii_text(pattern) {
    let translated = rx_translate(pattern, NO_ALPHABET)?
    let caps = regex.captures_bytes(translated, bytes.from_text(content), 0, extended: true, ignore_case: ignore_case)?
    return Ok([if item == null { null } else { {start: (item ?? {start: 0, end: 0}).start, end: (item ?? {start: 0, end: 0}).end} } for item in caps])
  }
  let alphabet = make_alphabet(pattern, content)
  let translated = rx_translate(pattern, alphabet)?
  let characters = content.split("")
  var mapped: List[Int] = []
  var offsets: List[Int] = []
  var position = 0
  for character in characters {
    offsets += [position]
    position += character.byte_len()
    if character.byte_len() == 1 and character != "\0" { mapped += [character.byte_at(0) ?? 0] } else {
      let shown = alphabet.chars.get(character) ?? "\u{80}"
      mapped += [if shown.byte_len() == 1 { shown.byte_at(0) ?? 1 } else { 1 }]
    }
  }
  offsets += [position]
  let caps = regex.captures_bytes(translated, bytes.from_ints(mapped)?, 0, extended: true, ignore_case: ignore_case)?
  Ok([if item == null { null } else { {start: offsets[(item ?? {start: 0, end: 0}).start], end: offsets[(item ?? {start: 0, end: 0}).end]} } for item in caps])
}


# How many bytes at the end of data start a UTF-8 sequence that continues in
# the next chunk (0 when the data ends on a character boundary).
pure incomplete_tail(data: Bytes) -> Int {
  let size = data.len()
  for back in range(1, 4) {
    if back > size { break }
    let byte = data.byte_at(size - back) ?? 0
    if byte >= 192 {
      let wanted = if byte >= 240 { 4 } else if byte >= 224 { 3 } else { 2 }
      return if wanted > back { back } else { 0 }
    }
    if byte < 128 { return 0 }
  }
  0
}
type Chunk = {text: Str, tail: Bytes, lossy: Bool}
pure continuation(byte: Int) -> Bool { byte >= 128 and byte <= 191 }
# Text holds only valid UTF-8, so each byte of input that is not part of a valid
# sequence is kept as the private-use character U+10FF00 plus the byte (and
# turned back into that byte on output), as GNU awk keeps such bytes in place.
pure decode_lossy(data: Bytes) -> Result[Str] {
  var out: List[Int] = []
  let size = data.len()
  var at = 0
  while at < size {
    let byte = data.byte_at(at) ?? 0
    var width = 0
    if byte < 128 { width = 1 } else if byte >= 194 and byte <= 223 {
      if at + 1 < size and continuation(data.byte_at(at + 1) ?? 0) { width = 2 }
    } else if byte >= 224 and byte <= 239 {
      if at + 2 < size {
        let second = data.byte_at(at + 1) ?? 0
        let lowest = if byte == 224 { 160 } else { 128 }
        let highest = if byte == 237 { 159 } else { 191 }
        if second >= lowest and second <= highest and continuation(data.byte_at(at + 2) ?? 0) { width = 3 }
      }
    } else if byte >= 240 and byte <= 244 {
      if at + 3 < size {
        let second = data.byte_at(at + 1) ?? 0
        let lowest = if byte == 240 { 144 } else { 128 }
        let highest = if byte == 244 { 143 } else { 191 }
        if second >= lowest and second <= highest and continuation(data.byte_at(at + 2) ?? 0) and continuation(data.byte_at(at + 3) ?? 0) { width = 4 }
      }
    }
    if width > 0 {
      for index in range(width) { out += [data.byte_at(at + index) ?? 0] }
      at += width
    } else {
      out += [244, 143, 190 + (byte - 128) / 64, 128 + byte % 64]
      at += 1
    }
  }
  Ok(bytes.from_ints(out)?.utf8()?)
}
pure decode_chunk(carry: Bytes, fresh: Bytes) -> Result[Chunk] {
  let data = if carry.is_empty() { fresh } else { bytes.concat([carry, fresh]) }
  match data.utf8() {
    Ok(text) => Ok({text: text, tail: b"", lossy: false})
    Err(_) => {
      let keep = incomplete_tail(data)
      let head = if keep > 0 { data.slice(0, length: data.len() - keep) } else { data }
      let rest = if keep > 0 { data.slice(data.len() - keep, length: keep) } else { b"" }
      match head.utf8() {
        Ok(text) => Ok({text: text, tail: rest, lossy: false})
        Err(_) => Ok({text: decode_lossy(head)?, tail: rest, lossy: true})
      }
    }
  }
}
# The bytes to write for text that may hold mapped input bytes.
pure output_bytes(text: Str, lossy: Bool) -> Bytes {
  let encoded = bytes.from_text(text)
  if ! lossy { return encoded }
  var out: List[Int] = []
  let size = encoded.len()
  var at = 0
  while at < size {
    let byte = encoded.byte_at(at) ?? 0
    if byte == 244 and at + 3 < size and encoded.byte_at(at + 1) == 143 and (encoded.byte_at(at + 2) ?? 0) >= 190 and (encoded.byte_at(at + 2) ?? 0) <= 191 {
      out += [128 + ((encoded.byte_at(at + 2) ?? 190) - 190) * 64 + (encoded.byte_at(at + 3) ?? 128) - 128]
      at += 4
    } else {
      out += [byte]
      at += 1
    }
  }
  bytes.from_ints(out) ?? encoded
}
# Reads another chunk into the reader's buffer; sets done at end of input.
proc fill(source: Reader) -> Result[Reader] {
  var r = source
  if r.done { return Ok(r) }
  if r.at > 0 and r.at >= 1048576 {
    r.text = r.text.byte_slice(r.at)
    r.at = 0
  }
  loop {
    if r.pollable {
      let ready = unix.poll_fd(r.fd, ["readable"], 3600000)?
      if ready.is_empty() { continue }
    }
    let chunk = unix.read_fd(r.fd, 65536)?
    if chunk.is_empty() {
      r.done = true
      if ! r.tail.is_empty() {
        r.text += decode_lossy(r.tail)?
        r.tail = b""
        r.lossy = true
      }
      return Ok(r)
    }
    let decoded = decode_chunk(r.tail, chunk)?
    r.tail = decoded.tail
    if decoded.lossy { r.lossy = true }
    if decoded.text.is_empty() { continue }
    r.text += decoded.text
    return Ok(r)
  }
  Ok(r)
}

type Scan = {kind: Int, content: Str, next: Int, terminator: Str}
# kind: 0 = need more input, 1 = a record, 2 = end of input.
pure scan_record(text: Str, position: Int, separator: Str, done: Bool, ignore_case: Bool) -> Result[Scan] {
  let size = text.byte_len()
  if position >= size {
    return Ok({kind: if done { 2 } else { 0 }, content: "", next: position, terminator: ""})
  }
  if separator.is_empty() {
    var at = position
    while at < size and text.byte_at(at) == 10 { at += 1 }
    if at >= size { return Ok({kind: if done { 2 } else { 0 }, content: "", next: at, terminator: ""}) }
    let blank = text.find("\n\n", at)
    if blank == null {
      if ! done { return Ok({kind: 0, content: "", next: at, terminator: ""}) }
      var end = size
      while end > at and text.byte_at(end - 1) == 10 { end -= 1 }
      return Ok({kind: 1, content: text.byte_slice(at, length: end - at), next: size, terminator: ""})
    }
    let first = blank ?? 0
    var after = first + 2
    while after < size and text.byte_at(after) == 10 { after += 1 }
    if after >= size and ! done { return Ok({kind: 0, content: "", next: at, terminator: ""}) }
    return Ok({kind: 1, content: text.byte_slice(at, length: first - at), next: after, terminator: text.byte_slice(first, length: after - first)})
  }
  if separator.count_chars() == 1 and ! ignore_case {
    let found = text.find(separator, position)
    if found == null {
      if ! done { return Ok({kind: 0, content: "", next: position, terminator: ""}) }
      return Ok({kind: 1, content: text.byte_slice(position), next: size, terminator: ""})
    }
    let at = found ?? 0
    return Ok({kind: 1, content: text.byte_slice(position, length: at - position), next: at + separator.byte_len(), terminator: separator})
  }
  let rest = text.byte_slice(position)
  let hits = rx_matches(separator, rest, ignore_case)?
  for hit in hits {
    if hit.end == hit.start { continue }
    # A match that reaches the end of the buffered text might extend with more input.
    if hit.end >= rest.byte_len() and ! done { return Ok({kind: 0, content: "", next: position, terminator: ""}) }
    return Ok({kind: 1, content: rest.byte_slice(0, length: hit.start), next: position + hit.end, terminator: rest.byte_slice(hit.start, length: hit.end - hit.start)})
  }
  if ! done { return Ok({kind: 0, content: "", next: position, terminator: ""}) }
  Ok({kind: 1, content: rest, next: size, terminator: ""})
}
type Read = {reader: Reader, record: Str?, terminator: Str}
proc read_record(source: Reader, separator: Str, ignore_case: Bool) -> Result[Read] {
  var r = source
  loop {
    let scanned = scan_record(r.text, r.at, separator, r.done, ignore_case)?
    if scanned.kind == 1 {
      r.at = scanned.next
      return Ok({reader: r, record: scanned.content, terminator: scanned.terminator})
    }
    if scanned.kind == 2 { return Ok({reader: r, record: null, terminator: ""}) }
    r = fill(r)?
  }
  Ok({reader: r, record: null, terminator: ""})
}

#
# GNU awk visits an array's subscripts in the order of its internal hash tables.
# The order is reproduced here from each element's insertion sequence number:
# a string-keyed array is a chained hash table that grows by fixed steps, a
# negative-integer-keyed array hashes integers into small buckets, and
# non-negative integer subscripts list in ascending order ahead of anything
# that does not fit them.

const TABLE_SIZES = [13, 127, 1021, 8191, 131071, 1048573, 8388593, 16777213, 33554393, 67108859, 134217689, 268435399, 536870909, 1073741789, 2147483647]
const MODULUS = 4294967296

pure next_table_size(size: Int) -> Int {
  for candidate in TABLE_SIZES { if candidate > size { return candidate } }
  size
}
# 1 for a non-negative integer subscript, 2 for a negative one, 3 otherwise.
pure subscript_kind(key: Str) -> Int {
  let size = key.byte_len()
  if size == 0 or size > 18 { return 3 }
  var at = 0
  var negative = false
  if key.byte_at(0) == 45 { negative = true; at = 1 }
  if at >= size { return 3 }
  if (key.byte_at(at) ?? 0) == 48 and (size - at > 1 or negative) { return 3 }
  for position in range(at, size) { if ! digit(key.byte_at(position) ?? 0) { return 3 } }
  if negative { return 2 }
  if size - at > 10 { return 2 }
  if size - at == 10 and (key.parse_int() ?? 0) > 2147483647 { return 2 }
  1
}
pure xor32(a: Int, b: Int) -> Int { a + b - 2 * a.bit_and(b) }
# The hash of a string subscript (sdbm, 32 bits).
pure string_code(key: Str) -> Int {
  var h = 0
  for byte in bytes.from_text(key) { h = (byte + h * 65599) % MODULUS }
  h
}
# The hash of an integer subscript (the final mixing step of SuperFastHash).
pure integer_code(value: Int) -> Int {
  var k = (value % MODULUS + MODULUS) % MODULUS
  k = xor32(k, k * 8 % MODULUS)
  k = (k + k / 32) % MODULUS
  k = xor32(k, k * 16 % MODULUS)
  k = (k + k / 131072) % MODULUS
  k = xor32(k, k * 33554432 % MODULUS)
  k = (k + k / 64) % MODULUS
  k
}
# Hash chains of string subscripts, newest first. `events` are the insertion
# sequence numbers at which the table grew; null simulates growth from counts.
pure string_order(keys: List[Str], seqs: List[Int], events: List[Int]?) -> List[Str] {
  if keys.is_empty() { return [] }
  var codes: List[Int] = []
  for key in keys { codes += [string_code(key)] }
  var size = 13
  var chains: List[List[Int]] = [[] for _ in range(size)]
  var pointer = 0
  for at in range(keys.len()) {
    var grow = false
    if events == null { grow = (at + 1) / size > 10 } else {
      let recorded = events ?? []
      if pointer < recorded.len() and recorded[pointer] <= seqs[at] { grow = true; pointer += 1 }
    }
    if grow {
      let bigger = next_table_size(size)
      var rehashed: List[List[Int]] = [[] for _ in range(bigger)]
      for index in range(size) {
        for item in chains[index] { rehashed[codes[item] % bigger] = [item] + rehashed[codes[item] % bigger] }
      }
      chains = rehashed
      size = bigger
    }
    chains[codes[at] % size] = [at] + chains[codes[at] % size]
  }
  var listing: List[Str] = []
  for chain in chains { for item in chain { listing += [keys[item]] } }
  listing
}
# Integer subscripts hash into buckets of two, newest bucket first.
pure integer_order(keys: List[Str], seqs: List[Int], events: List[Int]?) -> List[Str] {
  if keys.is_empty() { return [] }
  var values: List[Int] = []
  for key in keys { values += [key.parse_int() ?? 0] }
  var size = 13
  var chains: List[List[List[Int]]] = [[] for _ in range(size)]
  var pointer = 0
  for at in range(keys.len()) {
    var grow = false
    if events == null { grow = (at + 1) / size > 10 } else {
      let recorded = events ?? []
      if pointer < recorded.len() and recorded[pointer] <= seqs[at] { grow = true; pointer += 1 }
    }
    if grow {
      let bigger = next_table_size(size)
      var rehashed: List[List[List[Int]]] = [[] for _ in range(bigger)]
      for index in range(size) {
        for bucket in chains[index] {
          for item in bucket {
            let slot = integer_code(values[item]) % bigger
            if rehashed[slot].is_empty() or rehashed[slot][0].len() >= 2 { rehashed[slot] = [[item]] + rehashed[slot] } else { rehashed[slot][0] = rehashed[slot][0] + [item] }
          }
        }
      }
      chains = rehashed
      size = bigger
    }
    let slot = integer_code(values[at]) % size
    if chains[slot].is_empty() or chains[slot][0].len() >= 2 { chains[slot] = [[at]] + chains[slot] } else { chains[slot][0] = chains[slot][0] + [at] }
  }
  var listing: List[Str] = []
  for chain in chains { for bucket in chain { for item in bucket { listing += [keys[item]] } } }
  listing
}
# Non-negative integer subscripts in ascending order.
pure ascending_integers(keys: List[Str]) -> List[Str] {
  if keys.len() <= 1 { return keys }
  var numbers: List[Int] = []
  for key in keys { numbers += [key.parse_int() ?? 0] }
  let ordered = merge_integers(numbers)
  [f"{value}" for value in ordered]
}
pure merge_integers(values: List[Int]) -> List[Int] {
  if values.len() <= 1 { return values }
  let middle = values.len() / 2
  let left = merge_integers(values[..middle])
  let right = merge_integers(values[middle..])
  var merged: List[Int] = []
  var i = 0
  var j = 0
  while i < left.len() and j < right.len() {
    if right[j] < left[i] { merged += [right[j]]; j += 1 } else { merged += [left[i]]; i += 1 }
  }
  while i < left.len() { merged += [left[i]]; i += 1 }
  while j < right.len() { merged += [right[j]]; j += 1 }
  merged
}

# Non-negative integer subscripts live in power-of-two blocks of leaf arrays;
# one that would waste too much allocated capacity overflows to the hash table.
# Returns, for each key in insertion order, whether it stayed in the blocks.
pure leaf_blocks(keys: List[Str]) -> List[Bool] {
  var stays: List[Bool] = []
  var capacity = 0
  var count = 0
  var leaves: Map[Bool] = {}
  for key in keys {
    let k = key.parse_int() ?? 0
    var width = 0
    var remaining = k
    while remaining > 0 { width += 1; remaining = remaining / 2 }
    let h1 = if width - 1 < 10 { 10 } else { width }
    let m = h1 - 1
    var estimate = if m > 10 { m } else { 10 }
    while estimate >= 10 { estimate = (estimate + 1) / 2 }
    var allocated = 1
    for _ in range(estimate) { allocated *= 2 }
    if capacity + allocated - count > 2048 {
      stays += [false]
      continue
    }
    count += 1
    stays += [true]
    var level = if m < 10 { 10 } else { m }
    var base = 0
    if m >= 10 { base = 1; for _ in range(m) { base *= 2 } }
    var trail = f"{h1}"
    loop {
      let n = (level + 1) / 2
      var size = 1
      for _ in range(n) { size *= 2 }
      let index = (k - base) / size
      if n > 10 {
        trail += f"/{index}"
        base = base + size * index
        level = n
      } else {
        let leaf = f"{trail}:{index}"
        if leaf not in leaves {
          leaves = leaves.set(leaf, true)
          capacity += size
        }
        break
      }
    }
  }
  stays
}
# The subscripts of a table in GNU awk's for-in order.
pure array_order(table: Table) -> List[Str] {
  if table.cells.is_empty() { return [] }
  var slots: List[Str] = ["" for _ in range(table.next)]
  var present: List[Bool] = [false for _ in range(table.next)]
  for key in table.cells.keys() {
    let seq = (table.cells.get(key) ?? Cell(value: Empty, seq: 0)).seq
    slots[seq] = key
    present[seq] = true
  }
  var keys: List[Str] = []
  var seqs: List[Int] = []
  for at in range(table.next) { if present[at] { keys += [slots[at]]; seqs += [at] } }
  let events: List[Int]? = if table.size > 0 { table.growths } else { null }
  if table.kind == 3 { return string_order(keys, seqs, events) }
  var strings: List[Str] = []
  var string_seqs: List[Int] = []
  var integers: List[Str] = []
  var integer_seqs: List[Int] = []
  var naturals: List[Str] = []
  for at in range(keys.len()) {
    let kind = subscript_kind(keys[at])
    if kind == 1 { naturals += [keys[at]] }
    if kind == 2 { integers += [keys[at]]; integer_seqs += [seqs[at]] }
    if kind == 3 { strings += [keys[at]]; string_seqs += [seqs[at]] }
  }
  if table.kind == 2 {
    # Every integer subscript is in the integer table, strings in its overflow.
    var all_seqs: List[Int] = []
    for at in range(keys.len()) { if subscript_kind(keys[at]) != 3 { all_seqs += [seqs[at]] } }
    var in_order: List[Str] = []
    for at in range(keys.len()) { if subscript_kind(keys[at]) != 3 { in_order += [keys[at]] } }
    return string_order(strings, string_seqs, null) + integer_order(in_order, all_seqs, events)
  }
  # Non-negative integers still held by the blocks list ascending after the
  # overflow table, which is an integer table when its first subscript was an
  # integer, else a string table.
  var natural_keys: List[Str] = []
  for at in range(keys.len()) { if subscript_kind(keys[at]) == 1 { natural_keys += [keys[at]] } }
  let stays = leaf_blocks(natural_keys)
  var held: List[Str] = []
  var overflow: List[Str] = []
  var overflow_seqs: List[Int] = []
  var first_overflow_kind = 0
  var natural_at = 0
  for at in range(keys.len()) {
    let kind = subscript_kind(keys[at])
    var kept = false
    if kind == 1 {
      kept = stays[natural_at]
      natural_at += 1
    }
    if kept { held += [keys[at]] } else {
      if overflow.is_empty() { first_overflow_kind = if kind == 3 { 3 } else { 2 } }
      overflow += [keys[at]]
      overflow_seqs += [seqs[at]]
    }
  }
  var head: List[Str] = []
  if first_overflow_kind == 2 {
    var overflow_strings: List[Str] = []
    var overflow_string_seqs: List[Int] = []
    var overflow_integers: List[Str] = []
    var overflow_integer_seqs: List[Int] = []
    for at in range(overflow.len()) {
      if subscript_kind(overflow[at]) == 3 { overflow_strings += [overflow[at]]; overflow_string_seqs += [overflow_seqs[at]] } else { overflow_integers += [overflow[at]]; overflow_integer_seqs += [overflow_seqs[at]] }
    }
    head = string_order(overflow_strings, overflow_string_seqs, null) + integer_order(overflow_integers, overflow_integer_seqs, null)
  } else { head = string_order(overflow, overflow_seqs, null) }
  head + ascending_integers(held)
}

# A table built in one step from cells numbered in insertion order.
pure grown_table(cells: Map[Cell], first_key: Str) -> Table {
  let count = cells.len()
  let kind = if count == 0 { 0 } else { subscript_kind(first_key) }
  var size = 0
  var growths: List[Int] = []
  if kind != 1 and count > 0 {
    size = 13
    for seq in range(count) { if (seq + 1) / size > 10 { size = next_table_size(size); growths += [seq] } }
  }
  Table(cells: cells, next: count, kind: kind, size: size, growths: growths)
}
pure empty_table() -> Table { Table(cells: {}, next: 0, kind: 0, size: 0, growths: []) }

#
# The program is compiled to a flat list of stack-machine operations and run by
# one loop that owns every piece of interpreter state as a local variable, so
# state is updated in place instead of copied through each evaluation step.

# A storage place named by an operation. Field indexes and array subscripts
# are on the operand stack, below any value being stored.
enum Spot {
  SpotGlobal(Str), SpotLocal(Int, Str), SpotField, SpotIndexGlobal(Str, Int), SpotIndexLocal(Int, Str, Int),
  SpotRecord, SpotNF, SpotNone,
}
# How a user-function argument is passed: a computed value (on the stack), or
# the name of a variable that may turn out to be an array.
enum Arg { ArgValue, ArgGlobal(Str), ArgLocal(Int) }
# Jump offsets are relative to the following operation, so code fragments can
# be concatenated without patching.
enum Op {
  OpPushNumber(Float), OpPushText(Str),
  OpLoadGlobal(Str), OpLoadLocal(Int, Str), OpLoadField, OpLoadIndexGlobal(Str, Int), OpLoadIndexLocal(Int, Str, Int),
  OpMatchRecord(Str),
  OpStore(Spot), OpArith(Spot, Str), OpIncrement(Spot, Float, Bool),
  OpAdd, OpSubtract, OpMultiply, OpDivide, OpModulo, OpPower, OpConcat,
  OpLess, OpLessEqual, OpEqual, OpNotEqual, OpGreater, OpGreaterEqual,
  OpMatch, OpNotMatch, OpMatchLiteral(Str), OpNotMatchLiteral(Str),
  OpNegate, OpPlus, OpNot, OpTruth,
  OpJump(Int), OpJumpFalse(Int), OpJumpTrue(Int), OpShortAnd(Int), OpShortOr(Int), OpPop,
  OpJumpUnless(Int, Int), OpStoreGlobal(Str), OpIncGlobal(Str, Float),
  OpMember(Spot), OpDelete(Spot), OpDeleteAll(Spot),
  OpForInStart(Spot), OpForInNext(Int, Spot), OpForInEnd,
  OpCall(Str, List[Arg]), OpReturn, OpReturnEmpty,
  OpBuiltin(Str, Int),
  OpArrayBuiltin(Str, List[Spot], Int),
  OpPrint(Int, Int, Bool, Bool),
  OpGetlineMain(Spot), OpGetlineFile(Spot), OpGetlineCommand(Spot),
  OpReadMain(Int), OpNext, OpNextFile, OpExit(Bool),
  OpSetSite(Int), OpRangeActive(Int, Int), OpSetRange(Int, Bool),
  OpHalt, OpBreakMarker, OpContinueMarker,
}
# One user function activation: parameters by value, and for array use an
# alias naming the array the parameter stands for.
type Frame = {slots: List[Scalar], aliases: List[Str], return_to: Int, base: Int, iterators: Int, site: Int}
type Iteration = {keys: List[Str], at: Int}
type Linked = {ops: List[Op], entries: Map[Int], labels: List[Str], loop_at: Int, end_at: Int, rules: Int}

pure slot_of(names: List[Str], name: Str) -> Int {
  for at in range(names.len()) { if names[at] == name { return at } }
  -1
}
pure variable_spot(names: List[Str], name: Str) -> Spot {
  if name == "NF" { return SpotNF }
  let slot = slot_of(names, name)
  if slot >= 0 { SpotLocal(slot, name) } else { SpotGlobal(name) }
}
pure array_spot(names: List[Str], name: Str) -> Spot {
  let slot = slot_of(names, name)
  if slot >= 0 { SpotLocal(slot, name) } else { SpotGlobal(name) }
}
type Place = {ops: List[Op], spot: Spot}

# The code evaluating subscripts or a field index, and the Spot to use.
pure compile_place(x: List[Expression], names: List[Str], id: Int) -> Result[Place] {
  match x[id] {
    Variable(name) => Ok({ops: [], spot: variable_spot(names, name)})
    Field(child) => {
      if is_dollar_zero(x, child) { return Ok({ops: [], spot: SpotRecord}) }
      Ok({ops: compile_expression(x, names, child)?, spot: SpotField})
    }
    Index(name, keys) => {
      var ops: List[Op] = []
      for key in keys { ops += compile_expression(x, names, key)? }
      let slot = slot_of(names, name)
      Ok({ops: ops, spot: if slot >= 0 { SpotIndexLocal(slot, name, keys.len()) } else { SpotIndexGlobal(name, keys.len()) }})
    }
    _ => Err(syntax_error("assignment requires variable, field, or array element"))
  }
}
pure is_dollar_zero(x: List[Expression], id: Int) -> Bool {
  match x[id] { Number(n) => n == 0.0; _ => false }
}

# Regular expression operands that are literals push their pattern text
# rather than the result of matching the record.
pure compile_regex_source(x: List[Expression], names: List[Str], id: Int) -> Result[List[Op]] {
  match x[id] {
    RegexLiteral(pattern) => Ok([OpPushText(pattern)])
    _ => compile_expression(x, names, id)
  }
}

pure compile_expression(x: List[Expression], names: List[Str], id: Int) -> Result[List[Op]] {
  match x[id] {
    Number(n) => Ok([OpPushNumber(n)])
    Text(content) => Ok([OpPushText(content)])
    RegexLiteral(pattern) => Ok([OpMatchRecord(pattern)])
    Variable(name) => {
      let slot = slot_of(names, name)
      Ok([if slot >= 0 { OpLoadLocal(slot, name) } else { OpLoadGlobal(name) }])
    }
    Field(child) => Ok(compile_expression(x, names, child)? + [OpLoadField])
    Index(name, keys) => {
      var ops: List[Op] = []
      for key in keys { ops += compile_expression(x, names, key)? }
      let slot = slot_of(names, name)
      Ok(ops + [if slot >= 0 { OpLoadIndexLocal(slot, name, keys.len()) } else { OpLoadIndexGlobal(name, keys.len()) }])
    }
    Unary(op, child) => Ok(compile_expression(x, names, child)? + [if op == "!" { OpNot } else if op == "-" { OpNegate } else { OpPlus }])
    Increment(target, delta, post) => {
      let place = compile_place(x, names, target)?
      Ok(place.ops + [OpIncrement(place.spot, delta, post)])
    }
    Assign(op, target, right) => {
      let place = compile_place(x, names, target)?
      let value = compile_expression(x, names, right)?
      Ok(place.ops + value + [if op == "=" { OpStore(place.spot) } else { OpArith(place.spot, op.byte_slice(0, length: 1)) }])
    }
    Conditional(condition, yes, no) => {
      let cond_ops = compile_expression(x, names, condition)?
      let then_part = compile_expression(x, names, yes)?
      let else_part = compile_expression(x, names, no)?
      Ok(cond_ops + [OpJumpFalse(then_part.len() + 1)] + then_part + [OpJump(else_part.len())] + else_part)
    }
    Binary(op, left, right) => {
      if op == "&&" or op == "||" {
        let first = compile_expression(x, names, left)?
        let second = compile_expression(x, names, right)?
        return Ok(first + [if op == "&&" { OpShortAnd(second.len() + 1) } else { OpShortOr(second.len() + 1) }] + second + [OpTruth])
      }
      if op == "~" or op == "!~" {
        let subject = compile_expression(x, names, left)?
        match x[right] {
          RegexLiteral(pattern) => { return Ok(subject + [if op == "~" { OpMatchLiteral(pattern) } else { OpNotMatchLiteral(pattern) }]) }
          _ => { return Ok(subject + compile_expression(x, names, right)? + [if op == "~" { OpMatch } else { OpNotMatch }]) }
        }
      }
      let first = compile_expression(x, names, left)?
      let second = compile_expression(x, names, right)?
      let operation = if op == "+" { OpAdd } else if op == "-" { OpSubtract } else if op == "*" { OpMultiply } else if op == "/" { OpDivide } else if op == "%" { OpModulo } else if op == "^" { OpPower } else if op == "concat" { OpConcat } else if op == "<" { OpLess } else if op == "<=" { OpLessEqual } else if op == "==" { OpEqual } else if op == "!=" { OpNotEqual } else if op == ">" { OpGreater } else { OpGreaterEqual }
      Ok(first + second + [operation])
    }
    Member(keys, array) => {
      var ops: List[Op] = []
      for key in keys { ops += compile_expression(x, names, key)? }
      let slot = slot_of(names, array)
      Ok(ops + [OpMember(if slot >= 0 { SpotIndexLocal(slot, array, keys.len()) } else { SpotIndexGlobal(array, keys.len()) })])
    }
    CommandInput(command, target) => {
      let source = compile_expression(x, names, command)?
      if target == null { return Ok(source + [OpGetlineCommand(SpotRecord)]) }
      let place = compile_place(x, names, target ?? 0)?
      Ok(source + place.ops + [OpGetlineCommand(place.spot)])
    }
    GetlineMain(target) => {
      if target == null { return Ok([OpGetlineMain(SpotRecord)]) }
      let place = compile_place(x, names, target ?? 0)?
      Ok(place.ops + [OpGetlineMain(place.spot)])
    }
    GetlineFile(target, file) => {
      let source = compile_expression(x, names, file)?
      if target == null { return Ok(source + [OpGetlineFile(SpotRecord)]) }
      let place = compile_place(x, names, target ?? 0)?
      Ok(source + place.ops + [OpGetlineFile(place.spot)])
    }
    Call(name, args) => compile_call(x, names, name, args)
  }
}

# A user function, a builtin that takes values, or one that names arrays or
# assignable targets.
pure compile_call(x: List[Expression], names: List[Str], name: Str, args: List[Int]) -> Result[List[Op]] {
  var ops: List[Op] = []
  if name in BUILTINS {
    if name in ["sub", "gsub"] {
      if args.len() < 2 or args.len() > 3 { return Err(fatal_error(f"{name}: wrong number of arguments")) }
      ops += compile_regex_source(x, names, args[0])? + compile_expression(x, names, args[1])?
      if args.len() == 2 { return Ok(ops + [OpArrayBuiltin(name, [SpotRecord], 2)]) }
      match x[args[2]] {
        Variable(_) => { let place = compile_place(x, names, args[2])?; return Ok(ops + place.ops + [OpArrayBuiltin(name, [place.spot], 2)]) }
        Field(_) => { let place = compile_place(x, names, args[2])?; return Ok(ops + place.ops + [OpArrayBuiltin(name, [place.spot], 2)]) }
        Index(_, _) => { let place = compile_place(x, names, args[2])?; return Ok(ops + place.ops + [OpArrayBuiltin(name, [place.spot], 2)]) }
        _ => { return Ok(ops + compile_expression(x, names, args[2])? + [OpArrayBuiltin(name, [SpotNone], 2)]) }
      }
    }
    if name == "split" {
      if args.len() < 2 or args.len() > 4 { return Err(fatal_error("split: wrong number of arguments")) }
      ops += compile_expression(x, names, args[0])?
      var literal = false
      if args.len() >= 3 {
        ops += compile_regex_source(x, names, args[2])?
        match x[args[2]] { RegexLiteral(_) => { literal = true }; _ => {} }
      }
      let array = match x[args[1]] { Variable(variable) => array_spot(names, variable); _ => SpotNone }
      var spots = [array]
      if args.len() == 4 { spots += [match x[args[3]] { Variable(variable) => array_spot(names, variable); _ => SpotNone }] }
      return Ok(ops + [OpArrayBuiltin(if literal { "split_regex" } else { "split" }, spots, args.len())])
    }
    if name == "match" {
      if args.len() < 2 or args.len() > 3 { return Err(fatal_error("match: wrong number of arguments")) }
      ops += compile_expression(x, names, args[0])? + compile_regex_source(x, names, args[1])?
      var spots: List[Spot] = []
      if args.len() == 3 { spots += [match x[args[2]] { Variable(variable) => array_spot(names, variable); _ => SpotNone }] }
      return Ok(ops + [OpArrayBuiltin("match", spots, args.len())])
    }
    if name == "gensub" {
      if args.len() < 3 or args.len() > 4 { return Err(fatal_error("gensub: wrong number of arguments")) }
      ops += compile_regex_source(x, names, args[0])?
      for child in args[1..] { ops += compile_expression(x, names, child)? }
      return Ok(ops + [OpBuiltin("gensub", args.len())])
    }
    if name in ["patsplit", "asort", "asorti", "typeof", "isarray", "length"] and args.len() >= 1 {
      var spots: List[Spot] = []
      var plain: List[Int] = []
      for at in range(args.len()) {
        let is_array_position = (name == "patsplit" and (at == 1 or at == 3)) or (name in ["asort", "asorti"] and at <= 1) or (name in ["typeof", "isarray", "length"] and at == 0)
        match x[args[at]] {
          Variable(variable) => {
            if is_array_position { spots += [array_spot(names, variable)] } else { plain += [at] }
          }
          Index(variable, keys) => {
            if name in ["typeof", "isarray"] and at == 0 {
              let place = compile_place(x, names, args[at])?
              ops += place.ops
              spots += [place.spot]
            } else { plain += [at] }
          }
          _ => { plain += [at] }
        }
      }
      if name == "length" and args.len() == 1 and ! spots.is_empty() { return Ok([OpArrayBuiltin("length", spots, 1)]) }
      if name in ["typeof", "isarray"] and ! spots.is_empty() { return Ok(ops + [OpArrayBuiltin(name, spots, 1)]) }
      if name in ["asort", "asorti"] {
        var how_ops: List[Op] = []
        if args.len() == 3 { how_ops = compile_expression(x, names, args[2])? }
        if spots.len() < 1 { return Err(fatal_error(f"{name}: first argument is not an array")) }
        return Ok(how_ops + [OpArrayBuiltin(name, spots, args.len())])
      }
      if name == "patsplit" {
        var value_ops = compile_expression(x, names, args[0])?
        if args.len() >= 3 { value_ops += compile_regex_source(x, names, args[2])? }
        return Ok(value_ops + [OpArrayBuiltin("patsplit", spots, args.len())])
      }
    }
    for child in args { ops += compile_expression(x, names, child)? }
    return Ok(ops + [OpBuiltin(name, args.len())])
  }
  # Anything else is a user function, resolved when the call runs.
  var kinds: List[Arg] = []
  for child in args {
    match x[child] {
      Variable(variable) => {
        let slot = slot_of(names, variable)
        kinds += [if slot >= 0 { ArgLocal(slot) } else { ArgGlobal(variable) }]
      }
      _ => {
        ops += compile_expression(x, names, child)?
        kinds += [ArgValue]
      }
    }
  }
  Ok(ops + [OpCall(name, kinds)])
}

# Statements. Loop bodies carry BreakMarker and ContinueMarker placeholders
# that the enclosing loop turns into jumps once its layout is known.
pure patch_markers(body: List[Op], break_to: Int, continue_to: Int) -> List[Op] {
  var out: List[Op] = []
  for at in range(body.len()) {
    match body[at] {
      OpBreakMarker => { out += [OpJump(break_to - (at + 1))] }
      OpContinueMarker => { out += [OpJump(continue_to - (at + 1))] }
      _ => { out += [body[at]] }
    }
  }
  out
}
# A condition's code followed by the jump taken when it is false; a relational
# comparison fuses into one operation.
pure compile_condition(x: List[Expression], names: List[Str], id: Int, offset: Int) -> Result[List[Op]] {
  match x[id] {
    Binary(op, left, right) => {
      let code = if op == "<" { 0 } else if op == "<=" { 1 } else if op == "==" { 2 } else if op == "!=" { 3 } else if op == ">" { 4 } else if op == ">=" { 5 } else { -1 }
      if code >= 0 {
        return Ok(compile_expression(x, names, left)? + compile_expression(x, names, right)? + [OpJumpUnless(code, offset)])
      }
    }
    _ => {}
  }
  Ok(compile_expression(x, names, id)? + [OpJumpFalse(offset)])
}
pure compile_statement(x: List[Expression], s: List[Statement], names: List[Str], id: Int) -> Result[List[Op]] {
  let site = [OpSetSite(id)]
  match s[id] {
    Block(body) => {
      var ops: List[Op] = []
      for child in body { ops += compile_statement(x, s, names, child)? }
      Ok(ops)
    }
    ExpressionStatement(child) => {
      # An increment or plain assignment to a global whose value is not used.
      match x[child] {
        Increment(target, delta, post) => {
          match x[target] {
            Variable(name) => {
              if slot_of(names, name) < 0 and name != "NF" { return Ok(site + [OpIncGlobal(name, delta)]) }
            }
            _ => {}
          }
        }
        Assign(op, target, right) => {
          match x[target] {
            Variable(name) => {
              if op == "=" and slot_of(names, name) < 0 and name != "NF" { return Ok(site + compile_expression(x, names, right)? + [OpStoreGlobal(name)]) }
            }
            _ => {}
          }
        }
        _ => {}
      }
      Ok(site + compile_expression(x, names, child)? + [OpPop])
    }
    Print(args, formatted_output, target, mode) => {
      var ops = site
      for child in args { ops += compile_expression(x, names, child)? }
      var redirect = 0
      if target != null {
        ops += compile_expression(x, names, target ?? 0)?
        redirect = if mode == ">" { 1 } else if mode == ">>" { 2 } else { 3 }
      }
      Ok(ops + [OpPrint(args.len(), redirect, target != null, formatted_output)])
    }
    If(condition, yes, no) => {
      let then_part = compile_statement(x, s, names, yes)?
      if no == null { return Ok(site + compile_condition(x, names, condition, then_part.len())? + then_part) }
      let else_part = compile_statement(x, s, names, no ?? 0)?
      Ok(site + compile_condition(x, names, condition, then_part.len() + 1)? + then_part + [OpJump(else_part.len())] + else_part)
    }
    While(condition, body) => {
      let inner = compile_statement(x, s, names, body)?
      let gate = compile_condition(x, names, condition, inner.len() + 1)?
      let total = gate.len() + inner.len() + 1
      let patched = patch_markers(inner, total - gate.len(), 0 - gate.len())
      Ok(site + gate + patched + [OpJump(0 - total)])
    }
    Do(body, condition) => {
      let inner = compile_statement(x, s, names, body)?
      let cond_ops = compile_expression(x, names, condition)?
      let total = inner.len() + cond_ops.len() + 1
      let patched = patch_markers(inner, total, inner.len())
      Ok(site + patched + cond_ops + [OpJumpTrue(0 - total)])
    }
    For(initial, condition, step, body) => {
      var init_ops: List[Op] = []
      if initial != null { init_ops = compile_expression(x, names, initial ?? 0)? + [OpPop] }
      var step_ops: List[Op] = []
      if step != null {
        match x[step ?? 0] {
          Increment(target, delta, post) => {
            match x[target] {
              Variable(name) => {
                if slot_of(names, name) < 0 and name != "NF" { step_ops = [OpIncGlobal(name, delta)] }
              }
              _ => {}
            }
          }
          _ => {}
        }
        if step_ops.is_empty() { step_ops = compile_expression(x, names, step ?? 0)? + [OpPop] }
      }
      let inner = compile_statement(x, s, names, body)?
      # [condition + jump] [body] [step] [Jump back]
      var gate: List[Op] = []
      if condition != null { gate = compile_condition(x, names, condition ?? 0, inner.len() + step_ops.len() + 1)? }
      let total = gate.len() + inner.len() + step_ops.len() + 1
      let patched = patch_markers(inner, total - gate.len(), inner.len())
      Ok(site + init_ops + gate + patched + step_ops + [OpJump(0 - total)])
    }
    Each(variable, array, body) => {
      let inner = compile_statement(x, s, names, body)?
      let loop_variable = variable_spot(names, variable)
      let source = array_spot(names, array)
      # [ForInStart] [ForInNext exit] [body] [Jump back] [ForInEnd]; break
      # lands on ForInEnd so the key snapshot is dropped.
      let patched = patch_markers(inner, inner.len() + 1, 0 - 1)
      Ok(site + [OpForInStart(source), OpForInNext(inner.len() + 1, loop_variable)] + patched + [OpJump(0 - (inner.len() + 2)), OpForInEnd])
    }
    Delete(array, keys) => {
      let slot = slot_of(names, array)
      if keys == null { return Ok(site + [OpDeleteAll(if slot >= 0 { SpotLocal(slot, array) } else { SpotGlobal(array) })]) }
      var ops = site
      for key in keys ?? [] { ops += compile_expression(x, names, key)? }
      let count = (keys ?? []).len()
      Ok(ops + [OpDelete(if slot >= 0 { SpotIndexLocal(slot, array, count) } else { SpotIndexGlobal(array, count) })])
    }
    Next => Ok(site + [OpNext])
    NextFile => Ok(site + [OpNextFile])
    Break => Ok([OpBreakMarker])
    Continue => Ok([OpContinueMarker])
    Exit(child) => {
      if child == null { return Ok(site + [OpExit(false)]) }
      Ok(site + compile_expression(x, names, child ?? 0)? + [OpExit(true)])
    }
    Return(child) => {
      if child == null { return Ok(site + [OpReturnEmpty]) }
      Ok(site + compile_expression(x, names, child ?? 0)? + [OpReturn])
    }
  }
}


# Lays out BEGIN code, the record loop with its rules, END code, and the
# functions after a final Halt.
pure link(program: Program) -> Result[Linked] {
  let x = program.expressions
  let s = program.statements
  var labels = program.site
  var begin_ops: List[Op] = []
  for id in program.begin { begin_ops += compile_statement(x, s, [], id)? }
  var rule_ops: List[Op] = []
  var range_index = 0
  for rule in program.rules {
    let label = labels.len()
    labels += [rule.site]
    let action = compile_statement(x, s, [], rule.action)?
    if rule.pattern == null { rule_ops += [OpSetSite(label)] + action; continue }
    let cond_ops = compile_expression(x, [], rule.pattern ?? 0)?
    if rule.end == null {
      rule_ops += [OpSetSite(label)] + compile_condition(x, [], rule.pattern ?? 0, action.len())? + action
      continue
    }
    let stop = compile_expression(x, [], rule.end ?? 0)?
    let index = range_index
    range_index += 1
    # [RangeActive -> check] [start] [JumpFalse skip] [SetRange true]
    # check: [stop] [JumpTrue close] [Jump act] close: [SetRange false] act: [action]
    let tail = stop + [OpJumpTrue(1), OpJump(1), OpSetRange(index, false)] + action
    rule_ops += [OpSetSite(label), OpRangeActive(index, cond_ops.len() + 2)] + cond_ops + [OpJumpFalse(1 + tail.len()), OpSetRange(index, true)] + tail
  }
  var end_ops: List[Op] = []
  for id in program.end { end_ops += compile_statement(x, s, [], id)? }
  let main_section = ! program.rules.is_empty() or ! program.end.is_empty()
  var ops = begin_ops
  var loop_at = begin_ops.len()
  var end_at = begin_ops.len()
  if main_section {
    ops += [OpReadMain(rule_ops.len() + 1)] + rule_ops + [OpJump(0 - (rule_ops.len() + 2))]
    end_at = ops.len()
  }
  ops += end_ops + [OpHalt]
  var entries: Map[Int] = {}
  for name in program.functions.keys() {
    let function = program.functions.get(name) ?? Function(parameters: [], body: 0, site: "")
    entries = entries.set(name, ops.len())
    ops += compile_statement(x, s, function.parameters, function.body)? + [OpReturnEmpty]
  }
  Ok({ops: ops, entries: entries, labels: labels, loop_at: loop_at, end_at: end_at, rules: program.rules.len()})
}



pure no_reader() -> Reader { Reader(fd: -1, text: "", at: 0, done: true, tail: b"", pollable: false, lossy: false, job: null, name: "", command: false) }
pure no_writer() -> Writer { Writer(fd: -1, pending: "", job: null, name: "", command: false) }

# The shell-style reason of a host error without its path and errno suffix.
pure reason_of(failure: Error) -> Str {
  var text = failure.message
  let cut = text.find(" (os error")
  if cut != null { text = text.byte_slice(0, length: cut ?? 0) }
  var tail = text
  loop {
    let at = tail.find(": ")
    if at == null { break }
    tail = tail.byte_slice((at ?? 0) + 2)
  }
  tail
}
pure fatal_error(text: Str) -> Error { AwkError.Fatal(message: f"fatal: {text}", pending: "") }
# A failure reported without a source location.
pure bare_fatal(text: Str) -> Error { AwkError.Plain(message: f"fatal: {text}", pending: "") }

proc write_all(fd: Int, data: Bytes) -> Result[Unit] {
  var rest = data
  while ! rest.is_empty() {
    let count = unix.write_fd(fd, rest)?
    if count <= 0 { return error.fail("write failed") }
    rest = rest.slice(count, length: rest.len() - count)
  }
}
proc warn_plain(text: Str) -> Result[Unit] {
  write_all(2, bytes.from_text(f"awk: warning: {text}\n"))?
}

# A child's exit status as awk numbers it: the exit code, or 256 plus the
# signal number.
pure status_code(status: Status) -> Int {
  if status.signaled() { return 256 + (status.signal_number() ?? 0) }
  status.exit_code() ?? 0
}

# What the program has written but not yet handed to standard output, and the
# open streams, are flushed together before a child process starts.
type Flushed = {writers: Map[Writer], failure: Str}
proc flush_writers(streams: Map[Writer], lossy: Bool) -> Result[Flushed] {
  var writers = streams
  for key in writers.keys() {
    let writer = writers.get(key) ?? no_writer()
    if writer.fd >= 0 and ! writer.pending.is_empty() {
      let data = writer.pending
      writers[key].pending = ""
      match write_all(writer.fd, output_bytes(data, lossy)) {
        Ok(_) => {}
        Err(failure) => { return Ok({writers: writers, failure: f"print to \"{writer.name}\" failed ({reason_of(failure)})"}) }
      }
    }
  }
  Ok({writers: writers, failure: ""})
}

# A shell prefix that exports the changes the program made to ENVIRON, so
# children see them as they do under GNU awk.
proc environment_prefix(arrays: Map[Table]) -> Str {
  if "ENVIRON" not in arrays { return "" }
  var prefix = ""
  let table = arrays.get("ENVIRON") ?? empty_table()
  var current: Map[Str] = {}
  for entry in env.list() ?? [] { current = current.set(entry.name, entry.value) }
  for name in table.cells.keys() {
    if ! binding_name(name) { continue }
    let wanted = plain_text((table.cells.get(name) ?? Cell(value: Empty, seq: 0)).value) ?? ""
    if name in current and (current.get(name) ?? "") == wanted { continue }
    prefix += f"export {name}='{wanted.replace("'", with: "'\\''")}'; "
  }
  for name in current.keys() {
    if name not in table.cells and binding_name(name) { prefix += f"unset {name}; " }
  }
  prefix
}

type Temporary = {root: FsRoot?, base: Path?, count: Int}
type Started = {temporary: Temporary, fifo: Path}
proc next_fifo(temporary: Temporary) -> Result[Started] {
  var state = temporary
  if state.root == null {
    let root = fs.tempdir()?
    state.base = root.host_path()?
    state.root = root
  }
  state.count += 1
  let base = state.base ?? fp"/tmp"
  let made = fp"{base}/c{state.count}"
  fs.mkfifo(made, 0o600)?
  Ok({temporary: state, fifo: made})
}
type OpenedReader = {reader: Reader, temporary: Temporary}
type OpenedWriter = {writer: Writer, temporary: Temporary}
# Starts `sh -c command` with standard output into a FIFO this process reads.
proc open_command_reader(temporary: Temporary, command: Str, prefix: Str) -> Result[OpenedReader] {
  let started = next_fifo(temporary)?
  let pipe_path = started.fifo
  let fd = unix.open_fd(pipe_path, nonblock: true)?
  let job = spawn run.status sh -c ${prefix + command} > $pipe_path ?
  Ok({reader: Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: true, lossy: false, job: job, name: command, command: true), temporary: started.temporary})
}
# Starts `sh -c command` with standard input from a FIFO this process writes.
proc open_command_writer(temporary: Temporary, command: Str, prefix: Str) -> Result[OpenedWriter] {
  let started = next_fifo(temporary)?
  let pipe_path = started.fifo
  # Holding the read end open lets the write end open at once; the child then
  # inherits the read side and the held descriptor is released.
  let hold = unix.open_fd(pipe_path, nonblock: true)?
  let fd = unix.open_fd(pipe_path, write: true)?
  let job = spawn run.status sh -c ${prefix + command} < $pipe_path ?
  unix.close_fd(hold)?
  Ok({writer: Writer(fd: fd, pending: "", job: job, name: command, command: true), temporary: started.temporary})
}
# Opens a file for output: truncated, or appended to (and created when missing).
proc open_output(target: Path, append: Bool) -> Result[Int] {
  if append {
    if ! target.exists() { target.write("")? }
    return unix.open_fd(target, write: true, flags: ["append"])
  }
  target.write("")?
  unix.open_fd(target, write: true)
}
proc open_file_reader(name: Str, stdin_reader: Reader) -> Result[Reader] {
  if name == "-" or name == "/dev/stdin" { return Ok(stdin_reader) }
  let metadata = fs.stat(fp"{name}", follow_symlinks: true)?
  if metadata.kind == "dir" { return Err(error.failure("is a directory")) }
  let fd = unix.open_fd(fp"{name}")?
  Ok(Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: false, lossy: false, job: null, name: name, command: false))
}

## True when the text is a valid awk variable name.
export pure binding_name(content: Str) -> Bool {
  if content.is_empty() or ! letter(content.byte_at(0) ?? 0) { return false }
  for at in range(1, content.byte_len()) { if ! letter(content.byte_at(at) ?? 0) and ! digit(content.byte_at(at) ?? 0) { return false } }
  true
}
# Whether an ARGV entry is a var=value assignment operand.
pure assignment_operand(argument: Str) -> Bool {
  let equals = argument.find("=")
  if equals == null { return false }
  binding_name(argument.byte_slice(0, length: equals ?? 0))
}


type Generator = {words: List[Int], front: Int, rear: Int}
# GNU awk's random(): an additive feedback generator seeded by a linear
# congruential recurrence, with the first 630 outputs discarded.
pure seed_generator(seed: Int) -> Generator {
  var words = [0 for _ in range(63)]
  var word = seed % 4294967296
  if word < 0 { word += 4294967296 }
  words[0] = word
  for index in range(1, 63) {
    let previous = if words[index - 1] >= 2147483648 { words[index - 1] - 4294967296 } else { words[index - 1] }
    let high = previous / 127773
    let low = previous % 127773
    var value = 16807 * low - 2836 * high
    if value < 0 { value += 2147483647 }
    words[index] = value % 4294967296
  }
  var front = 1
  var rear = 0
  for _ in range(630) {
    words[front] = (words[front] + words[rear]) % 4294967296
    front += 1
    if front >= 63 { front = 0; rear += 1 } else { rear += 1; if rear >= 63 { rear = 0 } }
  }
  {words: words, front: front, rear: rear}
}


# The field separator in force: with RS = "" newline separates fields too.
pure field_separator(globals: Map[Scalar]) -> Result[Str] {
  var separator = plain_text(globals.get("FS") ?? Empty)?
  if (plain_text(globals.get("RS") ?? Empty)?).is_empty() and separator != " " and ! separator.is_empty() {
    if separator.count_chars() == 1 and separator in REGEX_SPECIAL { separator = "\\" + separator }
    if separator == "\\" { separator = "\\\\" }
    separator = f"({separator})|\n"
  }
  Ok(separator)
}
pure fold_case(globals: Map[Scalar]) -> Bool {
  if "IGNORECASE" not in globals { return false }
  truth(globals.get("IGNORECASE") ?? Empty) ?? false
}
# Splits a record into fields by the current FS rules: " " is runs of blanks
# and newlines, "" is single characters, one other character is literal, and
# anything longer is a regular expression (regex_mode forces that for regex
# literals).
pure split_fields(content: Str, separator: Str, regex_mode: Bool, ignore_case: Bool) -> Result[List[Scalar]] {
  if content.is_empty() { return Ok([]) }
  if separator == " " and ! regex_mode { return Ok([input(piece)? for piece in content.replace("\n", with: " ").replace("\t", with: " ").split(" ") if ! piece.is_empty()]) }
  if separator.is_empty() and ! regex_mode { return Ok([input(piece)? for piece in content.split("")]) }
  if separator.count_chars() == 1 and ! regex_mode { return Ok([input(piece)? for piece in content.split(separator)]) }
  var fields: List[Scalar] = []
  var start = 0
  for hit in rx_matches(separator, content, ignore_case)? {
    if hit.start == hit.end { continue }
    fields += [input(content.byte_slice(start, length: hit.start - start))?]
    start = hit.end
  }
  Ok(fields + [input(content.byte_slice(start))?])
}
# The fields and the separator texts between them, for split's fourth argument.
type Split = {fields: List[Str], separators: List[Str]}
pure split_with_separators(content: Str, separator: Str, regex_mode: Bool, ignore_case: Bool) -> Result[Split] {
  if content.is_empty() { return Ok({fields: [], separators: []}) }
  var pattern = separator
  if separator == " " and ! regex_mode { pattern = "[ \t\n]+" } else if separator.is_empty() and ! regex_mode {
    let characters = content.split("")
    return Ok({fields: characters, separators: ["" for _ in range(characters.len() - 1)]})
  } else if separator.count_chars() == 1 and ! regex_mode {
    pattern = if separator in REGEX_SPECIAL { "\\" + separator } else { separator }
  }
  var fields: List[Str] = []
  var seps: List[Str] = []
  var start = 0
  var leading = ""
  for hit in rx_matches(pattern, content, ignore_case)? {
    if hit.start == hit.end { continue }
    if separator == " " and ! regex_mode and hit.start == 0 {
      leading = content.byte_slice(0, length: hit.end)
      start = hit.end
      continue
    }
    fields += [content.byte_slice(start, length: hit.start - start)]
    seps += [content.byte_slice(hit.start, length: hit.end - hit.start)]
    start = hit.end
  }
  let rest = content.byte_slice(start)
  if ! (separator == " " and ! regex_mode and rest.is_empty()) { fields += [rest] }
  Ok({fields: fields, separators: if leading.is_empty() { seps } else { [leading] + seps }})
}
# Rebuilds $0 from the fields.
pure joined_fields(fields: List[Scalar], separator: Str, conversion: Str) -> Result[Str] {
  Ok([scalar_text(value, conversion)? for value in fields].join(separator))
}


type MainState = {reader: Reader, active: Bool, index: Int, opened: Bool, stdin_used: Bool, finished: Bool}
type Advanced = {state: MainState, stdin: Reader, record: Str?, terminator: Str, filename: Str?, bindings: List[Str], failed: Bool}

# The next record of the main input, opening files named by ARGV on demand.
# ARGV and ARGC are consulted live, so the program can edit the operand list.
proc advance_main(start: MainState, standard_input: Reader, operands: Table, count: Int, separator: Str, ignore_case: Bool) -> Result[Advanced] {
  var state = start
  var stdin = standard_input
  var bindings: List[Str] = []
  var filename: Str? = null
  loop {
    if state.finished { return Ok({state: state, stdin: stdin, record: null, terminator: "", filename: filename, bindings: bindings, failed: false}) }
    if state.active {
      let uses_stdin = state.reader.fd == 0
      let current = if uses_stdin { stdin } else { state.reader }
      let got = read_record(current, separator, ignore_case)?
      if uses_stdin { stdin = got.reader }
      state.reader = got.reader
      if got.record != null { return Ok({state: state, stdin: stdin, record: got.record, terminator: got.terminator, filename: filename, bindings: bindings, failed: false}) }
      if got.reader.fd > 0 { unix.close_fd(got.reader.fd)? }
      state.active = false
      continue
    }
    var opened = false
    while state.index < count {
      let index = state.index
      state.index += 1
      let key = f"{index}"
      if key not in operands.cells { continue }
      let argument = plain_text((operands.cells.get(key) ?? Cell(value: Empty, seq: 0)).value)?
      if argument.is_empty() { continue }
      if assignment_operand(argument) { bindings += [argument]; continue }
      state.opened = true
      if argument == "-" or argument == "/dev/stdin" {
        filename = argument
        state.reader = stdin
        state.active = true
        opened = true
        break
      }
      match fs.stat(fp"{argument}", follow_symlinks: true) {
        Ok(info) => {
          if info.kind == "dir" {
            warn_plain(f"command line argument `{argument}' is a directory: skipped")?
            continue
          }
        }
        Err(failure) => { return Err(bare_fatal(f"cannot open file `{argument}' for reading: {reason_of(failure)}")) }
      }
      match unix.open_fd(fp"{argument}") {
        Ok(fd) => {
          filename = argument
          state.reader = Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: false, lossy: false, job: null, name: argument, command: false)
          state.active = true
          opened = true
        }
        Err(failure) => { return Err(bare_fatal(f"cannot open file `{argument}' for reading: {reason_of(failure)}")) }
      }
      break
    }
    if opened { continue }
    if ! state.opened and ! state.stdin_used {
      state.stdin_used = true
      state.opened = true
      filename = "-"
      state.reader = stdin
      state.active = true
      continue
    }
    state.finished = true
  }
  Ok({state: state, stdin: stdin, record: null, terminator: "", filename: filename, bindings: bindings, failed: false})
}


type Closing = {readers: Map[Reader], writers: Map[Writer], code: Int}
# close(name): finishes a reader or writer with that name and returns its
# status: 0 for files, the command's exit status for commands, -1 when
# nothing by that name is open.
proc close_stream(open_readers: Map[Reader], open_writers: Map[Writer], target: Str, lossy: Bool) -> Result[Closing] {
  var readers = open_readers
  var writers = open_writers
  var code = -1
  var found = false
  for key in [f">{target}", f">>{target}", f"|{target}"] {
    if key not in writers { continue }
    let writer = writers.get(key) ?? no_writer()
    writers = writers.remove(key)
    found = true
    if writer.fd >= 0 and ! writer.pending.is_empty() { write_all(writer.fd, output_bytes(writer.pending, lossy))? }
    if writer.fd >= 0 { unix.close_fd(writer.fd)? }
    code = 0
    if writer.command {
      if let handle = writer.job {
        let status = wait handle ?
        code = status_code(status)
      }
    }
  }
  for key in [f"<{target}", f"|{target}"] {
    if key not in readers { continue }
    let reader = readers.get(key) ?? no_reader()
    readers = readers.remove(key)
    found = true
    if reader.fd > 0 { unix.close_fd(reader.fd)? }
    code = 0
    if reader.command {
      if let handle = reader.job {
        let status = wait handle ?
        code = status_code(status)
      }
    }
  }
  Ok({readers: readers, writers: writers, code: if found { code } else { -1 }})
}
# Everything still open is closed at exit: files and commands first (waiting
# for the commands); standard output is flushed by the caller afterwards.
proc close_all(open_readers: Map[Reader], open_writers: Map[Writer], lossy: Bool) -> Result[Unit] {
  for key in open_writers.keys() {
    let writer = open_writers.get(key) ?? no_writer()
    if writer.fd >= 0 and ! writer.pending.is_empty() { write_all(writer.fd, output_bytes(writer.pending, lossy))? }
    if writer.fd >= 0 { unix.close_fd(writer.fd)? }
    if writer.command {
      if let handle = writer.job { let _ = wait handle ? }
    }
  }
  for key in open_readers.keys() {
    let reader = open_readers.get(key) ?? no_reader()
    if reader.fd > 0 { unix.close_fd(reader.fd)? }
    if reader.command {
      if let handle = reader.job { let _ = wait handle ? }
    }
  }
}

# Dynamic regular expressions fail at run time with the engine's message.
pure dynamic_matches(pattern: Str, content: Str, ignore_case: Bool) -> Result[List[Match]] {
  match rx_matches(pattern, content, ignore_case) {
    Ok(hits) => Ok(hits)
    Err(failure) => Err(error.failure(f"fatal: invalid regexp: {failure.message}: /{pattern}/"))
  }
}

type SortItem = {rank: Int, number: Float, text: Str, value: Scalar}

pure upper_text(content: Str) -> Str {
  if is_ascii_text(content) { return content.upper() }
  [convert_case(character, true) for character in content.split("")].join("")
}
pure lower_text(content: Str) -> Str {
  if is_ascii_text(content) { return content.lower() }
  [convert_case(character, false) for character in content.split("")].join("")
}
# One character at a time, as towupper does: a conversion that would change
# the length (other than sharp s) leaves the character alone.
pure convert_case(character: Str, upper: Bool) -> Str {
  if character == "\u{df}" { return if upper { "\u{1e9e}" } else { character } }
  if code_point(character) >= 1113984 { return "\u{fffd}" }
  let changed = if upper { character.upper() } else { character.lower() }
  if changed.count_chars() == 1 { changed } else { character }
}
# Replacement text of sub and gsub with GNU awk's backslash rules: "\\\&" is a
# literal backslash and ampersand, "\\&" a backslash followed by the matched
# text, "\&" a literal ampersand; any other backslash stays.
pure substitute(replacement: Str, matched: Str) -> Str {
  var out = ""
  var at = 0
  while at < replacement.byte_len() {
    let byte = replacement.byte_at(at) ?? 0
    if byte == 38 { out += matched; at += 1 } else if byte == 92 {
      if replacement.byte_slice(at, length: 4) == "\\\\\\&" {
        out += "\\&"
        at += 4
      } else if replacement.byte_slice(at, length: 3) == "\\\\&" {
        out += "\\" + matched
        at += 3
      } else if replacement.byte_slice(at, length: 2) == "\\&" {
        out += "&"
        at += 2
      } else {
        out += "\\"
        at += 1
      }
    } else {
      let ampersand = replacement.find("&", at) ?? replacement.byte_len()
      let slash = replacement.find("\\", at) ?? replacement.byte_len()
      let end = if ampersand < slash { ampersand } else { slash }
      out += replacement.byte_slice(at, length: end - at)
      at = end
    }
  }
  out
}
pure strtonum(text: Str) -> Result[Float] {
  let plain = trimmed(text)
  var negative = false
  var body = plain
  if body.starts_with("-") { negative = true; body = body.byte_slice(1) } else if body.starts_with("+") { body = body.byte_slice(1) }
  var value = 0.0
  if (body.starts_with("0x") or body.starts_with("0X")) and body.byte_len() > 2 {
    var at = 2
    while at < body.byte_len() and hex_digit(body.byte_at(at) ?? 0) { value = value * 16.0 + digit_value(body.byte_at(at) ?? 48).float(); at += 1 }
  } else if body.starts_with("0") and body.byte_len() > 1 and octal_literal(body) {
    for at in range(body.byte_len()) { value = value * 8.0 + digit_value(body.byte_at(at) ?? 48).float() }
  } else {
    value = numeric_prefix(body)?
  }
  Ok(if negative { -value } else { value })
}
pure type_name(value: Scalar) -> Str {
  match value {
    Empty => "unassigned"
    Numeric(_) => "number"
    Nan(_) => "number"
    String(_) => "string"
    Input(_, n) => if n != null { "strnum" } else { "string" }
  }
}
# Sort keys follow the requested order: numbers sort before strings for the
# value-type orders, and a numeric order compares numeric value only.
pure sort_item(value: Scalar, subscript: Str, how: Str) -> Result[SortItem] {
  let text = plain_text(value)?
  let numeric_order = how.starts_with("@ind_num") or how.starts_with("@val_num")
  if how.starts_with("@val_str") or how.starts_with("@ind_str") { return Ok({rank: 0, number: 0.0, text: text, value: value}) }
  if how.starts_with("@ind_num") { return Ok({rank: 0, number: number(input(subscript)?)?, text: subscript, value: value}) }
  if numeric_order or numeric(value) { return Ok({rank: 0, number: number(value)?, text: text, value: value}) }
  Ok({rank: 1, number: 0.0, text: text, value: value})
}
pure item_before(a: SortItem, b: SortItem, descending: Bool) -> Bool {
  var less = false
  if a.rank != b.rank { less = a.rank < b.rank } else if a.rank == 0 and (a.number < b.number or a.number > b.number) { less = a.number < b.number } else { less = a.text < b.text }
  if descending { return ! less and (a.rank != b.rank or a.number < b.number or a.number > b.number or a.text != b.text) }
  less
}
pure merge_sort(items: List[SortItem], descending: Bool) -> List[SortItem] {
  if items.len() <= 1 { return items }
  let middle = items.len() / 2
  let left = merge_sort(items[..middle], descending)
  let right = merge_sort(items[middle..], descending)
  var merged: List[SortItem] = []
  var i = 0
  var j = 0
  while i < left.len() and j < right.len() {
    if item_before(right[j], left[i], descending) { merged += [right[j]]; j += 1 } else { merged += [left[i]]; i += 1 }
  }
  while i < left.len() { merged += [left[i]]; i += 1 }
  while j < right.len() { merged += [right[j]]; j += 1 }
  merged
}
# Days since 1970-01-01 of a proleptic Gregorian date.
pure days_from_civil(year: Int, month: Int, day: Int) -> Int {
  let shifted = if month <= 2 { year - 1 } else { year }
  let era = if shifted >= 0 { shifted / 400 } else { (shifted - 399) / 400 }
  let year_of_era = shifted - era * 400
  let month_index = if month > 2 { month - 3 } else { month + 9 }
  let day_of_year = (153 * month_index + 2) / 5 + day - 1
  let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
  era * 146097 + day_of_era - 719468
}
# mktime("YYYY MM DD HH MM SS [DST]"): out-of-range fields roll over; the
# result is local time as seconds since the epoch, or -1 for malformed text.
proc make_time(spec: Str) -> Result[Int] {
  var parts: List[Int] = []
  for word in spec.replace("\t", with: " ").replace("\n", with: " ").split(" ") {
    if word.is_empty() { continue }
    match word.parse_int_decimal() {
      Ok(number) => { parts += [number] }
      Err(_) => { return Ok(-1) }
    }
  }
  if parts.len() < 6 { return Ok(-1) }
  var year = parts[0]
  var month = parts[1] - 1
  year += month / 12
  month = month % 12
  if month < 0 { month += 12; year -= 1 }
  let days = days_from_civil(year, month + 1, 1) + parts[2] - 1
  let seconds = days * 86400 + parts[3] * 3600 + parts[4] * 60 + parts[5]
  let calendar = time.to_calendar(seconds * 1000000000, true)?
  match time.from_calendar(calendar.year, calendar.month, calendar.day, calendar.hour, calendar.minute, calendar.second) {
    Ok(stamp) => Ok(stamp / 1000000000)
    Err(_) => Ok(-1)
  }
}
# gensub: how is "g"/"G" for all matches or the number of the match to replace;
# \\0 and & stand for the match, \\1 to \\9 for groups, \\\\ and \\& for literals.
proc gensub(globals: Map[Scalar], pattern: Str, replacement: Str, how: Scalar, target: Str) -> Result[Str] {
  let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
  let mode = scalar_text(how, conversion)?
  var global = false
  var which = 1
  if mode.starts_with("g") or mode.starts_with("G") { global = true } else {
    which = integer(number(how)?)?
    if which < 1 { which = 1 }
  }
  var out = ""
  var copied = 0
  var counted = 0
  var previous_end = -1
  for hit in rx_matches(pattern, target, fold_case(globals))? {
    if hit.start == hit.end and previous_end == hit.start { continue }
    counted += 1
    previous_end = hit.end
    if ! global and counted != which { continue }
    let matched = target.byte_slice(hit.start, length: hit.end - hit.start)
    var groups: List[Str] = [matched]
    if "\\" in replacement {
      let caps = rx_captures(pattern, matched, fold_case(globals))?
      for item in caps[1..] {
        if item == null { groups += [""] } else {
          let span = item ?? Match(start: 0, end: 0)
          groups += [matched.byte_slice(span.start, length: span.end - span.start)]
        }
      }
    }
    var piece = ""
    let chars = replacement.split("")
    var at = 0
    while at < chars.len() {
      let c = chars[at]
      if c == "\\" and at + 1 < chars.len() {
        let next = chars[at + 1]
        if next.byte_len() == 1 and digit(next.byte_at(0) ?? 0) {
          let index = (next.byte_at(0) ?? 48) - 48
          piece += if index < groups.len() { groups[index] } else { "" }
          at += 2
        } else if next == "&" { piece += "&"; at += 2 } else if next == "\\" { piece += "\\"; at += 2 } else { piece += "\\"; at += 1 }
      } else if c == "&" { piece += matched; at += 1 } else { piece += c; at += 1 }
    }
    out += target.byte_slice(copied, length: hit.start - copied) + piece
    copied = hit.end
    if ! global { break }
  }
  Ok(out + target.byte_slice(copied))
}
pure to_bit_operand(name: Str, position: Int, value: Float) -> Result[Float] {
  if value < 0.0 { return Err(fatal_error(f"{name}: argument {position} negative value {float_text(value, "%.6g") ?? ""} is not allowed")) }
  Ok(truncate(value))
}
pure bit_step(name: Str, a: Float, b: Float) -> Float {
  var x = a
  var y = b
  var result = 0.0
  var weight = 1.0
  for _ in range(64) {
    let bx = x - 2.0 * truncate(x / 2.0)
    let by = y - 2.0 * truncate(y / 2.0)
    let bit = if name == "and" { bx * by } else if name == "or" { if bx + by > 0.0 { 1.0 } else { 0.0 } } else { if bx != by { 1.0 } else { 0.0 } }
    result += bit * weight
    weight *= 2.0
    x = truncate(x / 2.0)
    y = truncate(y / 2.0)
  }
  result
}
pure arith(op: Str, a: Float, b: Float) -> Result[Float] {
  if op == "+" { return Ok(a + b) }
  if op == "-" { return Ok(a - b) }
  if op == "*" { return Ok(a * b) }
  if op == "/" { return Ok(a / b) }
  if op == "^" { return Ok(a.pow(b)) }
  if op == "%" {
    let q = truncate(a / b)
    return Ok(a - q * b)
  }
  Err(unsupported(f"operator {op}"))
}
pure compare(left: Scalar, right: Scalar, conversion: Str) -> Result[Int] {
  if numeric(left) and numeric(right) {
    # Negative zero equals zero in awk although it orders before it here.
    var a = number(left)?
    var b = number(right)?
    if a == -0.0 { a = 0.0 }
    if b == -0.0 { b = 0.0 }
    return Ok(if a < b { -1 } else if a > b { 1 } else { 0 })
  }
  let a = scalar_text(left, conversion)?
  let b = scalar_text(right, conversion)?
  Ok(if a < b { -1 } else if a > b { 1 } else { 0 })
}


pure context_text(globals: Map[Scalar]) -> Str {
  let fnr = number(globals.get("FNR") ?? Empty) ?? 0.0
  if fnr > 0.0 {
    let name = plain_text(globals.get("FILENAME") ?? Empty) ?? ""
    let line = float_text(fnr, "%.6g") ?? "0"
    return f"(FILENAME={name} FNR={line}) "
  }
  ""
}
# Relational operators on numbers where NaN is unordered; op is 0 for <, 1 for
# <=, 2 for ==, 3 for !=, 4 for >, 5 for >=.
pure relation(op: Int, left: Scalar, right: Scalar, conversion: Str) -> Result[Bool] {
  if numeric(left) and numeric(right) {
    let x = number(left)?
    let y = number(right)?
    if is_nan(x) or is_nan(y) { return Ok(op == 3) }
  }
  let order = compare(left, right, conversion)?
  Ok(if op == 0 { order < 0 } else if op == 1 { order <= 0 } else if op == 2 { order == 0 } else if op == 3 { order != 0 } else if op == 4 { order > 0 } else { order >= 0 })
}
# Subscripts on the stack joined with SUBSEP.
pure subscript_key(stack: List[Scalar], top: Int, count: Int, separator: Str, conversion: Str) -> Result[Str] {
  if count == 1 { return scalar_text(stack[top - 1], conversion) }
  var pieces: List[Str] = []
  for at in range(top - count, top) { pieces += [scalar_text(stack[at], conversion)?] }
  Ok(pieces.join(separator))
}
pure no_cell() -> Cell { Cell(value: Empty, seq: -1) }
# A failure with its source position and context, and the output the program
# had produced but not yet written.
pure placed(labels: List[Str], site: Int, globals: Map[Scalar], out: Str, failure: Error) -> Error {
  AwkError.Fatal(message: f"{labels[site]}: {context_text(globals)}{failure.message}", pending: out)
}

# A resolved assignment target. kind: 1 global, 2 NF, 3 local slot, 4 field
# (slot is the field index), 5 array element (name is the array), 6 $0, 7
# nowhere (a constant). `used` is how many operands the target took off the
# stack, and `current` its present value.
type Target = {kind: Int, name: Str, slot: Int, key: Str, used: Int, current: Scalar}
pure array_target(array: Str, name: Str, key: Str, used: Int, arrays: Map[Table], globals: Map[Scalar]) -> Result[Target] {
  if array in globals { return Err(fatal_error(f"attempt to use scalar `{name}' as an array")) }
  var current: Scalar = Empty
  if array in arrays { current = (arrays[array].cells.get(key) ?? no_cell()).value }
  Ok({kind: 5, name: array, slot: 0, key: key, used: used, current: current})
}
# top is the stack height with any value to store already removed.
pure resolve_target(spot: Spot, stack: List[Scalar], top: Int, globals: Map[Scalar], arrays: Map[Table], slots: List[Scalar], aliases: List[Str], fields: List[Scalar], record: Str) -> Result[Target] {
  let separator = plain_text(globals.get("SUBSEP") ?? Empty)?
  let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
  match spot {
    SpotGlobal(name) => Ok({kind: 1, name: name, slot: 0, key: "", used: 0, current: globals.get(name) ?? Empty})
    SpotNF => Ok({kind: 2, name: "NF", slot: 0, key: "", used: 0, current: globals.get("NF") ?? Empty})
    SpotLocal(slot, name) => Ok({kind: 3, name: name, slot: slot, key: "", used: 0, current: slots[slot]})
    SpotField => {
      let index = integer(number(stack[top - 1])?)?
      if index < 0 or index > 10000000 { return Err(fatal_error(f"attempt to access field {index}")) }
      let current = if index == 0 { Input(record, null) } else { fields.get(index - 1) ?? Input("", null) }
      Ok({kind: 4, name: "", slot: index, key: "", used: 1, current: current})
    }
    SpotRecord => Ok({kind: 6, name: "", slot: 0, key: "", used: 0, current: Input(record, null)})
    SpotIndexGlobal(name, count) => array_target(name, name, subscript_key(stack, top, count, separator, conversion)?, count, arrays, globals)
    SpotIndexLocal(slot, name, count) => {
      let array = aliases[slot]
      if array == "" or slots[slot] != Empty { return Err(fatal_error(f"attempt to use scalar `{name}' as an array")) }
      array_target(array, name, subscript_key(stack, top, count, separator, conversion)?, count, arrays, globals)
    }
    SpotNone => Ok({kind: 7, name: "", slot: 0, key: "", used: 0, current: Empty})
  }
}
# The array a spot names, for operations on whole arrays.
pure array_of(spot: Spot, slots: List[Scalar], aliases: List[Str], globals: Map[Scalar]) -> Result[Str] {
  match spot {
    SpotGlobal(name) => {
      if name in globals { return Err(fatal_error(f"attempt to use scalar `{name}' as an array")) }
      Ok(name)
    }
    SpotLocal(slot, name) => {
      if aliases[slot] == "" or slots[slot] != Empty { return Err(fatal_error(f"attempt to use scalar `{name}' as an array")) }
      Ok(aliases[slot])
    }
    _ => Err(fatal_error("an array name is required"))
  }
}

# Writes the program's pending standard output to the host.
proc emit_stdout(text: Str, lossy: Bool) -> Result[Unit] {
  if text.is_empty() { return Ok() }
  match io.write_stdout_bytes(output_bytes(text, lossy)) {
    Ok(_) => {}
    Err(failure) => { return Err(fatal_error(f"print to \"standard output\" failed ({reason_of(failure)})")) }
  }
  match io.flush_stdout() {
    Ok(_) => Ok()
    Err(failure) => Err(fatal_error(f"print to \"standard output\" failed ({reason_of(failure)})"))
  }
}

# Runs a linked program over ARGV-selected input and returns the exit status.
proc run_machine(program: Program, linked: Linked, arguments: List[Str], assignments: List[Str], interactive: Bool) -> Result[Int] {
  let ops = linked.ops
  let labels = linked.labels
  let entries = linked.entries
  let functions = program.functions
  var globals: Map[Scalar] = {"FS": String(" "), "OFS": String(" "), "RS": String("\n"), "RT": String(""), "ORS": String("\n"), "SUBSEP": String("\u{1c}"), "OFMT": String("%.6g"), "CONVFMT": String("%.6g"), "NR": Numeric(0.0), "FNR": Numeric(0.0), "NF": Numeric(0.0), "RSTART": Numeric(0.0), "RLENGTH": Numeric(0.0), "FILENAME": String("")}
  var arrays: Map[Table] = {}
  var operand_cells: Map[Cell] = {"0": Cell(value: String("awk"), seq: 0)}
  for at in range(arguments.len()) { operand_cells = operand_cells.set(f"{at + 1}", Cell(value: input(arguments[at])?, seq: at + 1)) }
  arrays = arrays.set("ARGV", grown_table(operand_cells, "0"))
  globals["ARGC"] = Numeric((arguments.len() + 1).float())
  if program.environ {
    var environment: Map[Cell] = {}
    var count = 0
    var first_name = ""
    for entry in env.list()? {
      if count == 0 { first_name = entry.name }
      environment = environment.set(entry.name, Cell(value: input(entry.value)?, seq: count))
      count += 1
    }
    arrays = arrays.set("ENVIRON", grown_table(environment, first_name))
  }
  for item in assignments {
    let equals = item.find("=") ?? 0
    let decoded = escaped(bytes.from_text(item.byte_slice(equals + 1)), 0, -1)?
    for note in decoded.notes { warn_plain(note)? }
    globals[item.byte_slice(0, length: equals)] = input(decoded.content)?
  }
  var fields: List[Scalar] = []
  var record = ""
  var stack: List[Scalar] = [Empty for _ in range(512)]
  var sp = 0
  var slots: List[Scalar] = []
  var aliases: List[Str] = []
  var frames: List[Frame] = []
  var iterators: List[Iteration] = []
  var readers: Map[Reader] = {}
  var writers: Map[Writer] = {}
  var stdin_reader = Reader(fd: 0, text: "", at: 0, done: false, tail: b"", pollable: false, lossy: false, job: null, name: "-", command: false)
  var out = ""
  var lossy = false
  var status = 0
  var site = 0
  var pc = 0
  var phase = 0
  var main = MainState(reader: no_reader(), active: false, index: 1, opened: false, stdin_used: false, finished: false)
  var ranges = [false for _ in range(linked.rules)]
  var rng = seed_generator(1)
  var seed_value = 1.0
  var temporary = Temporary(root: null, base: null, count: 0)
  var calls = 0
  var result: Scalar = Empty
  var produced = false
  var pending = 0
  var pending_name = ""
  var pending_slot = 0
  var pending_key = ""
  var pending_value: Scalar = Empty
  let outcome = try {
    loop {
      let op = ops[pc]
      pc += 1
      produced = false
      pending = 0
      match op {
        OpPushNumber(n) => { result = Numeric(n); produced = true }
        OpPushText(content) => { result = String(content); produced = true }
        OpLoadGlobal(name) => {
          if name in globals { result = globals.get(name) ?? Empty } else {
            if name in arrays { Err(fatal_error(f"attempt to use array `{name}' in a scalar context"))? }
            result = Empty
          }
          produced = true
        }
        OpLoadLocal(slot, name) => {
          result = slots[slot]
          if aliases[slot] != "" and aliases[slot] in arrays { Err(fatal_error(f"attempt to use array `{name}' in a scalar context"))? }
          produced = true
        }
        OpLoadField => {
          let index = integer(number(stack[sp - 1])?)?
          sp -= 1
          if index < 0 or index > 10000000 { Err(fatal_error(f"attempt to access field {index}"))? }
          result = if index == 0 { input(record)? } else { fields.get(index - 1) ?? Input("", null) }
          produced = true
        }
        OpLoadIndexGlobal(name, count) => {
          let key = subscript_key(stack, sp, count, plain_text(globals.get("SUBSEP") ?? Empty)?, plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= count
          if name in globals { Err(fatal_error(f"attempt to use scalar `{name}' as an array"))? }
          if name not in arrays { arrays[name] = empty_table() }
          let cell = arrays[name].cells.get(key) ?? no_cell()
          if cell.seq >= 0 { result = cell.value } else {
            pending = 5
            pending_name = name
            pending_key = key
            pending_value = Empty
            result = Empty
          }
          produced = true
        }
        OpLoadIndexLocal(slot, name, count) => {
          let key = subscript_key(stack, sp, count, plain_text(globals.get("SUBSEP") ?? Empty)?, plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= count
          let array = aliases[slot]
          if array == "" or slots[slot] != Empty { Err(fatal_error(f"attempt to use scalar `{name}' as an array"))? }
          if array not in arrays { arrays[array] = empty_table() }
          let cell = arrays[array].cells.get(key) ?? no_cell()
          if cell.seq >= 0 { result = cell.value } else {
            pending = 5
            pending_name = array
            pending_key = key
            pending_value = Empty
            result = Empty
          }
          produced = true
        }
        OpMatchRecord(pattern) => {
          result = Numeric(if dynamic_matches(pattern, record, fold_case(globals))?.is_empty() { 0.0 } else { 1.0 })
          produced = true
        }
        OpStore(spot) => {
          let target = resolve_target(spot, stack, sp - 1, globals, arrays, slots, aliases, fields, record)?
          pending_value = stack[sp - 1]
          pending = target.kind
          pending_name = target.name
          pending_slot = target.slot
          pending_key = target.key
          # The value replaces the operands of its target and stays as the result.
          stack[sp - 1 - target.used] = pending_value
          sp -= target.used
        }
        OpArith(spot, operator) => {
          let operand = number(stack[sp - 1])?
          if (operator == "/" or operator == "%") and operand == 0.0 { Err(fatal_error(f"division by zero attempted in `{operator}='"))? }
          let target = resolve_target(spot, stack, sp - 1, globals, arrays, slots, aliases, fields, record)?
          pending_value = Numeric(arith(operator, number(target.current)?, operand)?)
          pending = target.kind
          pending_name = target.name
          pending_slot = target.slot
          pending_key = target.key
          stack[sp - 1 - target.used] = pending_value
          sp -= target.used
        }
        OpIncrement(spot, delta, post) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          let old = number(target.current)?
          pending_value = Numeric(old + delta)
          pending = target.kind
          pending_name = target.name
          pending_slot = target.slot
          pending_key = target.key
          sp -= target.used
          result = if post { Numeric(old) } else { pending_value }
          produced = true
        }
        OpAdd => { let b = number(stack[sp - 1])?; let a = number(stack[sp - 2])?; sp -= 2; result = Numeric(a + b); produced = true }
        OpSubtract => { let b = number(stack[sp - 1])?; let a = number(stack[sp - 2])?; sp -= 2; result = Numeric(a - b); produced = true }
        OpMultiply => { let b = number(stack[sp - 1])?; let a = number(stack[sp - 2])?; sp -= 2; result = Numeric(a * b); produced = true }
        OpDivide => {
          let b = number(stack[sp - 1])?
          let a = number(stack[sp - 2])?
          sp -= 2
          if b == 0.0 { Err(fatal_error("division by zero attempted"))? }
          result = Numeric(a / b)
          produced = true
        }
        OpModulo => {
          let b = number(stack[sp - 1])?
          let a = number(stack[sp - 2])?
          sp -= 2
          if b == 0.0 { Err(fatal_error("division by zero attempted in `%'"))? }
          result = Numeric(arith("%", a, b)?)
          produced = true
        }
        OpPower => { let b = number(stack[sp - 1])?; let a = number(stack[sp - 2])?; sp -= 2; result = Numeric(a.pow(b)); produced = true }
        OpConcat => {
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          let b = scalar_text(stack[sp - 1], conversion)?
          let a = scalar_text(stack[sp - 2], conversion)?
          sp -= 2
          result = String(a + b)
          produced = true
        }
        OpLess => { result = Numeric(if relation(0, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpLessEqual => { result = Numeric(if relation(1, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpEqual => { result = Numeric(if relation(2, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpNotEqual => { result = Numeric(if relation(3, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpGreater => { result = Numeric(if relation(4, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpGreaterEqual => { result = Numeric(if relation(5, stack[sp - 2], stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)? { 1.0 } else { 0.0 }); sp -= 2; produced = true }
        OpMatch => {
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          let pattern = scalar_text(stack[sp - 1], conversion)?
          let subject = scalar_text(stack[sp - 2], conversion)?
          sp -= 2
          result = Numeric(if dynamic_matches(pattern, subject, fold_case(globals))?.is_empty() { 0.0 } else { 1.0 })
          produced = true
        }
        OpNotMatch => {
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          let pattern = scalar_text(stack[sp - 1], conversion)?
          let subject = scalar_text(stack[sp - 2], conversion)?
          sp -= 2
          result = Numeric(if dynamic_matches(pattern, subject, fold_case(globals))?.is_empty() { 1.0 } else { 0.0 })
          produced = true
        }
        OpMatchLiteral(pattern) => {
          let subject = scalar_text(stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= 1
          result = Numeric(if dynamic_matches(pattern, subject, fold_case(globals))?.is_empty() { 0.0 } else { 1.0 })
          produced = true
        }
        OpNotMatchLiteral(pattern) => {
          let subject = scalar_text(stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= 1
          result = Numeric(if dynamic_matches(pattern, subject, fold_case(globals))?.is_empty() { 1.0 } else { 0.0 })
          produced = true
        }
        OpNegate => {
          let value = stack[sp - 1]
          sp -= 1
          match value {
            Nan(positive) => { result = Nan(! positive) }
            _ => {
              let n = number(value)?
              result = if is_nan(n) { Nan(true) } else { Numeric(-n) }
            }
          }
          produced = true
        }
        OpPlus => {
          let value = stack[sp - 1]
          sp -= 1
          match value {
            Nan(_) => { result = value }
            _ => {
              let n = number(value)?
              result = if is_nan(n) { Nan(false) } else { Numeric(n) }
            }
          }
          produced = true
        }
        OpNot => {
          let yes = truth(stack[sp - 1])?
          sp -= 1
          result = Numeric(if yes { 0.0 } else { 1.0 })
          produced = true
        }
        OpTruth => {
          let yes = truth(stack[sp - 1])?
          sp -= 1
          result = Numeric(if yes { 1.0 } else { 0.0 })
          produced = true
        }
        OpJumpUnless(code, offset) => {
          sp -= 2
          let left = stack[sp]
          let right = stack[sp + 1]
          var holds = false
          match left {
            Numeric(a) => {
              match right {
                Numeric(b) => {
                  holds = if code == 0 { a < b } else if code == 1 { a <= b } else if code == 2 { a == b } else if code == 3 { a != b } else if code == 4 { a > b } else { a >= b }
                  if a == -0.0 or b == -0.0 { holds = relation(code, left, right, "%.6g")? }
                }
                _ => { holds = relation(code, left, right, plain_text(globals.get("CONVFMT") ?? Empty)?)? }
              }
            }
            _ => { holds = relation(code, left, right, plain_text(globals.get("CONVFMT") ?? Empty)?)? }
          }
          if ! holds { pc += offset }
        }
        OpStoreGlobal(name) => {
          if name in arrays { Err(fatal_error(f"attempt to use array `{name}' in a scalar context"))? }
          globals[name] = stack[sp - 1]
          sp -= 1
        }
        OpIncGlobal(name, delta) => {
          if name in arrays { Err(fatal_error(f"attempt to use array `{name}' in a scalar context"))? }
          let current = globals.get(name) ?? Empty
          match current {
            Numeric(n) => { globals[name] = Numeric(n + delta) }
            _ => { globals[name] = Numeric(number(current)? + delta) }
          }
        }
        OpJump(offset) => { pc += offset }
        OpJumpFalse(offset) => {
          sp -= 1
          if ! truth(stack[sp])? { pc += offset }
        }
        OpJumpTrue(offset) => {
          sp -= 1
          if truth(stack[sp])? { pc += offset }
        }
        OpShortAnd(offset) => {
          sp -= 1
          if ! truth(stack[sp])? { result = Numeric(0.0); produced = true; pc += offset }
        }
        OpShortOr(offset) => {
          sp -= 1
          if truth(stack[sp])? { result = Numeric(1.0); produced = true; pc += offset }
        }
        OpPop => { sp -= 1 }
        OpSetSite(index) => { site = index }
        OpHalt => { break }
        OpRangeActive(index, offset) => { if ranges[index] { pc += offset } }
        OpSetRange(index, active) => { ranges[index] = active }
        OpMember(spot) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          sp -= target.used
          result = Numeric(if target.kind == 5 and target.name in arrays and target.key in arrays[target.name].cells { 1.0 } else { 0.0 })
          produced = true
        }
        OpDelete(spot) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          sp -= target.used
          if target.name in arrays and target.key in arrays[target.name].cells {
            arrays[target.name].cells = arrays[target.name].cells.remove(target.key)
            if arrays[target.name].cells.is_empty() { arrays[target.name] = empty_table() }
          }
        }
        OpDeleteAll(spot) => {
          let array = array_of(spot, slots, aliases, globals)?
          arrays[array] = empty_table()
        }
        OpForInStart(spot) => {
          let array = array_of(spot, slots, aliases, globals)?
          iterators += [Iteration(keys: if array in arrays { array_order(arrays[array]) } else { [] }, at: 0)]
        }
        OpForInNext(offset, spot) => {
          let top = iterators.len() - 1
          if iterators[top].at >= iterators[top].keys.len() { pc += offset } else {
            pending_value = String(iterators[top].keys[iterators[top].at])
            iterators[top].at += 1
            match spot {
              SpotGlobal(name) => { pending = 1; pending_name = name }
              SpotLocal(slot, name) => { pending = 3; pending_slot = slot }
              SpotNF => { pending = 2 }
              _ => {}
            }
          }
        }
        OpForInEnd => { iterators = iterators[..iterators.len() - 1] }
        OpCall(name, kinds) => {
          if name not in entries { Err(fatal_error(f"function `{name}' not defined"))? }
          let function = functions.get(name) ?? Function(parameters: [], body: 0, site: "")
          calls += 1
          # Arguments computed as values sit on the stack in order; names are
          # passed as references so the callee may use them as arrays.
          var computed = 0
          for kind in kinds {
            match kind {
              ArgValue => { computed += 1 }
              _ => {}
            }
          }
          var taken = sp - computed
          var new_slots: List[Scalar] = []
          var new_aliases: List[Str] = []
          for at in range(function.parameters.len()) {
            if at >= kinds.len() {
              new_slots += [Empty]
              new_aliases += [f"\0{calls}:{at}"]
              continue
            }
            match kinds[at] {
              ArgValue => {
                new_slots += [stack[taken]]
                new_aliases += [""]
                taken += 1
              }
              ArgGlobal(variable) => {
                if variable in arrays {
                  new_slots += [Empty]
                  new_aliases += [variable]
                } else if variable in globals {
                  new_slots += [globals.get(variable) ?? Empty]
                  new_aliases += [""]
                } else {
                  new_slots += [Empty]
                  new_aliases += [variable]
                }
              }
              ArgLocal(slot) => {
                if aliases[slot] != "" and slots[slot] == Empty {
                  new_slots += [Empty]
                  new_aliases += [aliases[slot]]
                } else {
                  new_slots += [slots[slot]]
                  new_aliases += [""]
                }
              }
            }
          }
          sp = sp - computed
          if frames.len() > 2000 { Err(fatal_error("function call nesting too deep"))? }
          frames += [Frame(slots: slots, aliases: aliases, return_to: pc, base: sp, iterators: iterators.len(), site: site)]
          slots = new_slots
          aliases = new_aliases
          pc = entries.get(name) ?? 0
        }
        OpReturn => {
          let value = stack[sp - 1]
          let frame = frames[frames.len() - 1]
          frames = frames[..frames.len() - 1]
          sp = frame.base
          slots = frame.slots
          aliases = frame.aliases
          iterators = iterators[..frame.iterators]
          pc = frame.return_to
          site = frame.site
          result = value
          produced = true
        }
        OpReturnEmpty => {
          let frame = frames[frames.len() - 1]
          frames = frames[..frames.len() - 1]
          sp = frame.base
          slots = frame.slots
          aliases = frame.aliases
          iterators = iterators[..frame.iterators]
          pc = frame.return_to
          site = frame.site
          result = Empty
          produced = true
        }
        OpReadMain(offset) => {
          phase = 1
          let operands = arrays.get("ARGV") ?? empty_table()
          let got = advance_main(main, stdin_reader, operands, integer(number(globals.get("ARGC") ?? Empty)?)?, plain_text(globals.get("RS") ?? Empty)?, fold_case(globals))?
          main = got.state
          stdin_reader = got.stdin
          if got.state.reader.lossy or got.stdin.lossy { lossy = true }
          if got.filename != null {
            globals["FILENAME"] = String(got.filename ?? "")
            globals["FNR"] = Numeric(0.0)
          }
          for item in got.bindings {
            let equals = item.find("=") ?? 0
            let decoded = escaped(bytes.from_text(item.byte_slice(equals + 1)), 0, -1)?
            for note in decoded.notes { warn_plain(note)? }
            globals[item.byte_slice(0, length: equals)] = input(decoded.content)?
          }
          if got.record == null { pc += offset } else {
            globals["NR"] = Numeric(number(globals.get("NR") ?? Empty)? + 1.0)
            globals["FNR"] = Numeric(number(globals.get("FNR") ?? Empty)? + 1.0)
            globals["RT"] = String(got.terminator)
            pending = 6
            pending_value = String(got.record ?? "")
          }
        }
        OpNext => {
          if phase == 0 { Err(fatal_error("`next' cannot be called from a `BEGIN' rule"))? }
          if phase == 2 { Err(fatal_error("`next' cannot be called from an `END' rule"))? }
          frames = []
          sp = 0
          iterators = []
          slots = []
          aliases = []
          pc = linked.loop_at
        }
        OpNextFile => {
          if phase == 0 { Err(fatal_error("`nextfile' cannot be called from a `BEGIN' rule"))? }
          if phase == 2 { Err(fatal_error("`nextfile' cannot be called from an `END' rule"))? }
          if main.active {
            if main.reader.fd > 0 { unix.close_fd(main.reader.fd)? }
            main.active = false
          }
          frames = []
          sp = 0
          iterators = []
          slots = []
          aliases = []
          pc = linked.loop_at
        }
        OpExit(has_value) => {
          if has_value {
            status = integer(number(stack[sp - 1])?)?
            sp -= 1
          }
          frames = []
          sp = 0
          iterators = []
          slots = []
          aliases = []
          if phase == 2 { break }
          phase = 2
          pc = linked.end_at
        }
        OpPrint(count, redirect, has_destination, is_printf) => {
          var destination = ""
          if has_destination {
            destination = scalar_text(stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)?
            sp -= 1
          }
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          var text = ""
          if is_printf {
            if count == 0 { Err(fatal_error("printf: no arguments"))? }
            text = printf(scalar_text(stack[sp - count], conversion)?, stack[sp - count + 1..sp], conversion)?
          } else {
            if count == 0 { text = record } else {
              let output_format = plain_text(globals.get("OFMT") ?? Empty)?
              var pieces: List[Str] = []
              for at in range(sp - count, sp) { pieces += [scalar_text(stack[at], output_format)?] }
              text = pieces.join(plain_text(globals.get("OFS") ?? Empty)?)
            }
            text += plain_text(globals.get("ORS") ?? Empty)?
          }
          sp -= count
          if ! has_destination or (redirect != 3 and (destination == "/dev/stdout" or destination == "/dev/fd/1")) {
            out += text
            if interactive or out.byte_len() > 8192 {
              emit_stdout(out, lossy)?
              out = ""
            }
          } else if redirect != 3 and (destination == "/dev/stderr" or destination == "/dev/fd/2") {
            write_all(2, output_bytes(text, lossy))?
          } else {
            if destination.is_empty() { Err(fatal_error(f"expression for `{if redirect == 1 { ">" } else if redirect == 2 { ">>" } else { "|" }}' redirection has null string value"))? }
            let key = f"{if redirect == 1 { ">" } else if redirect == 2 { ">>" } else { "|" }}{destination}"
            if key not in writers {
              if redirect == 3 {
                # Output commands synchronize everything written so far first.
                let flushed = flush_writers(writers, lossy)?
                writers = flushed.writers
                if ! flushed.failure.is_empty() { Err(fatal_error(flushed.failure))? }
                emit_stdout(out, lossy)?
                out = ""
                let started = open_command_writer(temporary, destination, environment_prefix(arrays))?
                writers[key] = started.writer
                temporary = started.temporary
              } else {
                match open_output(fp"{destination}", redirect == 2) {
                  Ok(fd) => { writers[key] = Writer(fd: fd, pending: "", job: null, name: destination, command: false) }
                  Err(failure) => { Err(fatal_error(f"cannot redirect to `{destination}': {reason_of(failure)}"))? }
                }
              }
            }
            writers[key].pending += text
          }
        }
        OpGetlineMain(spot) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          sp -= target.used
          phase = if phase == 0 { 0 } else { phase }
          let operands = arrays.get("ARGV") ?? empty_table()
          let got = advance_main(main, stdin_reader, operands, integer(number(globals.get("ARGC") ?? Empty)?)?, plain_text(globals.get("RS") ?? Empty)?, fold_case(globals))?
          main = got.state
          stdin_reader = got.stdin
          if got.state.reader.lossy or got.stdin.lossy { lossy = true }
          if got.filename != null {
            globals["FILENAME"] = String(got.filename ?? "")
            globals["FNR"] = Numeric(0.0)
          }
          for item in got.bindings {
            let equals = item.find("=") ?? 0
            let decoded = escaped(bytes.from_text(item.byte_slice(equals + 1)), 0, -1)?
            for note in decoded.notes { warn_plain(note)? }
            globals[item.byte_slice(0, length: equals)] = input(decoded.content)?
          }
          if got.record == null { result = Numeric(0.0) } else {
            globals["NR"] = Numeric(number(globals.get("NR") ?? Empty)? + 1.0)
            globals["FNR"] = Numeric(number(globals.get("FNR") ?? Empty)? + 1.0)
            globals["RT"] = String(got.terminator)
            pending = if target.kind == 7 { 0 } else { target.kind }
            pending_name = target.name
            pending_slot = target.slot
            pending_key = target.key
            pending_value = if target.kind == 6 { String(got.record ?? "") } else { input(got.record ?? "")? }
            result = Numeric(1.0)
          }
          produced = true
        }
        OpGetlineFile(spot) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          sp -= target.used
          let name = scalar_text(stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= 1
          if name.is_empty() { Err(fatal_error("expression for `<' redirection has null string value"))? }
          result = Numeric(-1.0)
          var reader = no_reader()
          var usable = true
          var uses_stdin = name == "-" or name == "/dev/stdin"
          if uses_stdin { reader = stdin_reader } else {
            let key = f"<{name}"
            if key not in readers {
              match open_file_reader(name, stdin_reader) {
                Ok(opened) => { readers[key] = opened }
                Err(failure) => {
                  usable = false
                  globals["ERRNO"] = String(reason_of(failure))
                }
              }
            }
            if usable { reader = readers.get(key) ?? no_reader() }
          }
          if usable {
            match read_record(reader, plain_text(globals.get("RS") ?? Empty)?, fold_case(globals)) {
              Ok(got) => {
                if got.reader.lossy { lossy = true }
                if uses_stdin { stdin_reader = got.reader } else { readers[f"<{name}"] = got.reader }
                if got.record == null { result = Numeric(0.0) } else {
                  globals["RT"] = String(got.terminator)
                  pending = if target.kind == 7 { 0 } else { target.kind }
                  pending_name = target.name
                  pending_slot = target.slot
                  pending_key = target.key
                  pending_value = if target.kind == 6 { String(got.record ?? "") } else { input(got.record ?? "")? }
                  result = Numeric(1.0)
                }
              }
              Err(_) => { result = Numeric(-1.0) }
            }
          }
          produced = true
        }
        OpGetlineCommand(spot) => {
          let target = resolve_target(spot, stack, sp, globals, arrays, slots, aliases, fields, record)?
          sp -= target.used
          let command = scalar_text(stack[sp - 1], plain_text(globals.get("CONVFMT") ?? Empty)?)?
          sp -= 1
          if command.is_empty() { Err(fatal_error("expression for `|' redirection has null string value"))? }
          let key = f"|{command}"
          if key not in readers {
            let started = open_command_reader(temporary, command, environment_prefix(arrays))?
            readers[key] = started.reader
            temporary = started.temporary
          }
          result = Numeric(-1.0)
          match read_record(readers.get(key) ?? no_reader(), plain_text(globals.get("RS") ?? Empty)?, fold_case(globals)) {
            Ok(got) => {
              if got.reader.lossy { lossy = true }
              readers[key] = got.reader
              if got.record == null { result = Numeric(0.0) } else {
                globals["RT"] = String(got.terminator)
                pending = if target.kind == 7 { 0 } else { target.kind }
                pending_name = target.name
                pending_slot = target.slot
                pending_key = target.key
                pending_value = if target.kind == 6 { String(got.record ?? "") } else { input(got.record ?? "")? }
                result = Numeric(1.0)
              }
            }
            Err(_) => { result = Numeric(-1.0) }
          }
          produced = true
        }
        OpBuiltin(name, count) => {
          let base = sp - count
          let values = stack[base..sp]
          sp = base
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          result = Empty
          if name == "length" {
            result = Numeric((if count == 0 { record } else { scalar_text(values[0], conversion)? }).count_chars().float())
          } else if name == "substr" {
            if count < 2 or count > 3 { Err(fatal_error("substr: wrong number of arguments"))? }
            let content = scalar_text(values[0], conversion)?
            let pieces = content.split("")
            let given = truncate(number(values[1])?)
            let size = pieces.len()
            let first = if is_nan(given) or given < 1.0 { 0 } else if given > size.float() { size } else { integer(given - 1.0)? }
            var length = size
            if count == 3 {
              let wanted = truncate(number(values[2])?)
              length = if is_nan(wanted) or wanted < 0.0 { 0 } else if wanted > size.float() { size } else { integer(wanted)? }
            }
            let end = if first + length > size { size } else { first + length }
            result = String(pieces[first..end].join(""))
          } else if name == "index" {
            if count != 2 { Err(fatal_error("index: wrong number of arguments"))? }
            let content = scalar_text(values[0], conversion)?
            let found = content.find(scalar_text(values[1], conversion)?)
            result = Numeric(if found == null { 0.0 } else { (content.byte_slice(0, length: found ?? 0).count_chars() + 1).float() })
          } else if name == "tolower" or name == "toupper" {
            if count != 1 { Err(fatal_error(f"{name}: wrong number of arguments"))? }
            let text = scalar_text(values[0], conversion)?
            result = String(if name == "tolower" { lower_text(text) } else { upper_text(text) })
          } else if name == "sprintf" {
            if count == 0 { Err(fatal_error("sprintf: no arguments"))? }
            result = String(printf(scalar_text(values[0], conversion)?, values[1..], conversion)?)
          } else if name == "int" or name == "sqrt" or name == "exp" or name == "log" or name == "sin" or name == "cos" {
            if count != 1 { Err(fatal_error(f"{name}: wrong number of arguments"))? }
            let n = number(values[0])?
            var computed = 0.0
            if name == "int" { computed = truncate(n) } else if name == "sqrt" {
              if n < 0.0 { write_all(2, bytes.from_text(f"awk: {labels[site]}: {context_text(globals)}warning: sqrt: received negative argument {float_text(n, conversion)?}\n"))? }
              computed = n.sqrt()
            } else if name == "log" {
              if n < 0.0 { write_all(2, bytes.from_text(f"awk: {labels[site]}: {context_text(globals)}warning: log: received negative argument {float_text(n, conversion)?}\n"))? }
              computed = n.ln()
            } else if name == "exp" { computed = n.exp() } else if name == "sin" { computed = n.sin() } else { computed = n.cos() }
            result = Numeric(computed)
          } else if name == "atan2" {
            if count != 2 { Err(fatal_error("atan2: wrong number of arguments"))? }
            result = Numeric(number(values[0])?.atan2(number(values[1])?))
          } else if name == "rand" {
            var high = 0
            var low = 0
            rng.words[rng.front] = (rng.words[rng.front] + rng.words[rng.rear]) % 4294967296
            high = rng.words[rng.front] / 2 % 2147483648
            rng.front += 1
            if rng.front >= 63 { rng.front = 0; rng.rear += 1 } else { rng.rear += 1; if rng.rear >= 63 { rng.rear = 0 } }
            rng.words[rng.front] = (rng.words[rng.front] + rng.words[rng.rear]) % 4294967296
            low = rng.words[rng.front] / 2 % 2147483648
            rng.front += 1
            if rng.front >= 63 { rng.front = 0; rng.rear += 1 } else { rng.rear += 1; if rng.rear >= 63 { rng.rear = 0 } }
            var draw = 0.5 + (high.float() / 2147483648.0 + low.float()) / 2147483648.0
            draw = draw - 0.5
            if draw == 1.0 { draw = 0.0 }
            result = Numeric(draw)
          } else if name == "srand" {
            let previous = seed_value
            seed_value = if count == 0 { (time.now() / 1000).float() } else { number(values[0])? }
            rng = seed_generator(integer(truncate(seed_value))?)
            result = Numeric(previous)
          } else if name == "system" {
            if count != 1 { Err(fatal_error("system: wrong number of arguments"))? }
            let command = scalar_text(values[0], conversion)?
            let flushed = flush_writers(writers, lossy)?
            writers = flushed.writers
            if ! flushed.failure.is_empty() { Err(fatal_error(flushed.failure))? }
            emit_stdout(out, lossy)?
            out = ""
            let finished = run.status sh -c ${environment_prefix(arrays) + command}
            result = Numeric(status_code(finished).float())
          } else if name == "close" {
            if count < 1 or count > 2 { Err(fatal_error("close: wrong number of arguments"))? }
            let closed = close_stream(readers, writers, scalar_text(values[0], conversion)?, lossy)?
            readers = closed.readers
            writers = closed.writers
            result = Numeric(closed.code.float())
          } else if name == "fflush" {
            if count > 1 { Err(fatal_error("fflush: wrong number of arguments"))? }
            let target = if count == 0 { "" } else { scalar_text(values[0], conversion)? }
            if target.is_empty() {
              let flushed = flush_writers(writers, lossy)?
              writers = flushed.writers
              if ! flushed.failure.is_empty() { Err(fatal_error(flushed.failure))? }
              emit_stdout(out, lossy)?
              out = ""
              result = Numeric(0.0)
            } else {
              var found = false
              for key in [f">{target}", f">>{target}", f"|{target}"] {
                if key in writers {
                  found = true
                  let held = writers[key]
                  if held.fd >= 0 and ! held.pending.is_empty() {
                    write_all(held.fd, output_bytes(held.pending, lossy))?
                    writers[key].pending = ""
                  }
                }
              }
              if target in ["/dev/stdout", "-"] {
                found = true
                emit_stdout(out, lossy)?
                out = ""
              }
              if found { result = Numeric(0.0) } else {
                write_all(2, bytes.from_text(f"awk: {labels[site]}: {context_text(globals)}warning: fflush: `{target}' is not an open file, pipe or co-process\n"))?
                result = Numeric(-1.0)
              }
            }
          } else if name == "and" or name == "or" or name == "xor" {
            if count < 2 { Err(fatal_error(f"{name}: called with less than two arguments"))? }
            var accumulator = to_bit_operand(name, 1, number(values[0])?)?
            for at in range(1, count) { accumulator = bit_step(name, accumulator, to_bit_operand(name, at + 1, number(values[at])?)?) }
            result = Numeric(accumulator)
          } else if name == "lshift" or name == "rshift" {
            if count != 2 { Err(fatal_error(f"{name}: wrong number of arguments"))? }
            let a = to_bit_operand(name, 1, number(values[0])?)?
            let b = to_bit_operand(name, 2, number(values[1])?)?
            result = Numeric(if name == "lshift" { a * 2.0.pow(b) } else { truncate(a / 2.0.pow(b)) })
          } else if name == "compl" {
            if count != 1 { Err(fatal_error("compl: wrong number of arguments"))? }
            result = Numeric(9007199254740991.0 - to_bit_operand(name, 1, number(values[0])?)?)
          } else if name == "strtonum" {
            if count != 1 { Err(fatal_error("strtonum: wrong number of arguments"))? }
            result = Numeric(strtonum(scalar_text(values[0], conversion)?)?)
          } else if name == "typeof" {
            result = String(if count == 1 { type_name(values[0]) } else { "unknown" })
          } else if name == "isarray" {
            result = Numeric(0.0)
          } else if name == "systime" {
            result = Numeric((time.now() / 1000).float())
          } else if name == "strftime" {
            var format = "%a %b %e %H:%M:%S %Z %Y"
            if count >= 1 { format = scalar_text(values[0], conversion)? }
            var stamp = time.now() / 1000
            if count >= 2 { stamp = integer(truncate(number(values[1])?))? }
            var utc = false
            if count >= 3 { utc = truth(values[2])? }
            result = String(time.format(stamp * 1000000000, format, utc)?)
          } else if name == "mktime" {
            if count != 1 { Err(fatal_error("mktime: wrong number of arguments"))? }
            result = Numeric(make_time(scalar_text(values[0], conversion)?)?.float())
          } else if name == "gensub" {
            var target = record
            if count == 4 { target = scalar_text(values[3], conversion)? }
            result = String(gensub(globals, scalar_text(values[0], conversion)?, scalar_text(values[1], conversion)?, values[2], target)?)
          } else {
            Err(fatal_error(f"function `{name}' not defined"))?
          }
          produced = true
        }
        OpArrayBuiltin(name, spots, count) => {
          let conversion = plain_text(globals.get("CONVFMT") ?? Empty)?
          let ignore_case = fold_case(globals)
          result = Empty
          if name == "length" {
            var size = -1
            match spots[0] {
              SpotGlobal(variable) => {
                if variable in arrays { size = arrays[variable].cells.len() } else { result = Numeric((scalar_text(globals.get(variable) ?? Empty, conversion)?).count_chars().float()) }
              }
              SpotLocal(slot, variable) => {
                if aliases[slot] != "" and slots[slot] == Empty {
                  size = if aliases[slot] in arrays { arrays[aliases[slot]].cells.len() } else { 0 }
                } else { result = Numeric((scalar_text(slots[slot], conversion)?).count_chars().float()) }
              }
              _ => {}
            }
            if size >= 0 { result = Numeric(size.float()) }
          } else if name == "typeof" or name == "isarray" {
            var kind = "untyped"
            var is_array = false
            match spots[0] {
              SpotGlobal(variable) => {
                if variable in arrays { kind = "array"; is_array = true } else if variable in globals { kind = type_name(globals.get(variable) ?? Empty) }
              }
              SpotLocal(slot, variable) => {
                if aliases[slot] != "" and slots[slot] == Empty {
                  if aliases[slot] in arrays { kind = "array"; is_array = true }
                } else { kind = type_name(slots[slot]) }
              }
              _ => {
                let target = resolve_target(spots[0], stack, sp, globals, arrays, slots, aliases, fields, record)?
                sp -= target.used
                let present = target.name in arrays and target.key in arrays[target.name].cells
                kind = if present { type_name(target.current) } else { "unassigned" }
              }
            }
            result = if name == "isarray" { Numeric(if is_array { 1.0 } else { 0.0 }) } else { String(kind) }
          } else if name == "sub" or name == "gsub" {
            var target = resolve_target(spots[0], stack, sp, globals, arrays, slots, aliases, fields, record)?
            sp -= target.used
            var current = target.current
            match spots[0] {
              SpotNone => { current = stack[sp - 1]; sp -= 1 }
              _ => {}
            }
            let replacement = scalar_text(stack[sp - 1], conversion)?
            let pattern = scalar_text(stack[sp - 2], conversion)?
            sp -= 2
            let content = scalar_text(current, conversion)?
            var rebuilt = ""
            var copied = 0
            var changes = 0
            var previous_end = -1
            for hit in dynamic_matches(pattern, content, ignore_case)? {
              if hit.start == hit.end and previous_end == hit.start { continue }
              rebuilt += content.byte_slice(copied, length: hit.start - copied) + substitute(replacement, content.byte_slice(hit.start, length: hit.end - hit.start))
              copied = hit.end
              previous_end = hit.end
              changes += 1
              if name == "sub" { break }
            }
            rebuilt += content.byte_slice(copied)
            if changes > 0 and target.kind != 7 {
              pending = target.kind
              pending_name = target.name
              pending_slot = target.slot
              pending_key = target.key
              pending_value = String(rebuilt)
            }
            result = Numeric(changes.float())
          } else if name == "split" or name == "split_regex" {
            let base = sp - (if count >= 3 { 2 } else { 1 })
            let content = scalar_text(stack[base], conversion)?
            var separator = plain_text(globals.get("FS") ?? Empty)?
            if count >= 3 { separator = scalar_text(stack[base + 1], conversion)? }
            sp = base
            let regex_mode = name == "split_regex"
            var array = ""
            match array_of(spots[0], slots, aliases, globals) {
              Ok(named) => { array = named }
              Err(_) => { Err(fatal_error("split: second argument is not an array"))? }
            }
            if count == 4 {
              let seps_array = array_of(spots[1], slots, aliases, globals)?
              let parts = split_with_separators(content, separator, regex_mode, ignore_case)?
              var cells: Map[Cell] = {}
              for at in range(parts.fields.len()) { cells = cells.set(f"{at + 1}", Cell(value: input(parts.fields[at])?, seq: at)) }
              arrays[array] = grown_table(cells, "1")
              var separator_cells: Map[Cell] = {}
              for at in range(parts.separators.len()) { separator_cells = separator_cells.set(f"{at + 1}", Cell(value: String(parts.separators[at]), seq: at)) }
              arrays[seps_array] = grown_table(separator_cells, "1")
              result = Numeric(parts.fields.len().float())
            } else {
              let pieces = split_fields(content, separator, regex_mode, ignore_case)?
              var cells: Map[Cell] = {}
              for at in range(pieces.len()) { cells = cells.set(f"{at + 1}", Cell(value: pieces[at], seq: at)) }
              arrays[array] = grown_table(cells, "1")
              result = Numeric(pieces.len().float())
            }
          } else if name == "match" {
            let pattern = scalar_text(stack[sp - 1], conversion)?
            let content = scalar_text(stack[sp - 2], conversion)?
            sp -= 2
            let hits = dynamic_matches(pattern, content, ignore_case)?
            let first = if hits.is_empty() { Match(start: 0, end: 0) } else { hits[0] }
            let start = if hits.is_empty() { 0 } else { content.byte_slice(0, length: first.start).count_chars() + 1 }
            let length = if hits.is_empty() { -1 } else { content.byte_slice(first.start, length: first.end - first.start).count_chars() }
            globals["RSTART"] = Numeric(start.float())
            globals["RLENGTH"] = Numeric(length.float())
            if count == 3 {
              let array = array_of(spots[0], slots, aliases, globals)?
              var cells: Map[Cell] = {}
              if ! hits.is_empty() {
                let caps = rx_captures(pattern, content, ignore_case)?
                let separator = plain_text(globals.get("SUBSEP") ?? Empty)?
                var number_of = 0
                for at in range(caps.len()) {
                  if caps[at] == null { continue }
                  let span = caps[at] ?? Match(start: 0, end: 0)
                  let text = content.byte_slice(span.start, length: span.end - span.start)
                  let from = content.byte_slice(0, length: span.start).count_chars() + 1
                  cells = cells.set(f"{at}", Cell(value: input(text)?, seq: number_of))
                  cells = cells.set(f"{at}{separator}start", Cell(value: Numeric(from.float()), seq: number_of + 1))
                  cells = cells.set(f"{at}{separator}length", Cell(value: Numeric(text.count_chars().float()), seq: number_of + 2))
                  number_of += 3
                }
              }
              arrays[array] = grown_table(cells, "0")
            }
            result = Numeric(start.float())
          } else if name == "asort" or name == "asorti" {
            var how = if name == "asorti" { "@ind_str_asc" } else { "@val_type_asc" }
            if count == 3 {
              how = scalar_text(stack[sp - 1], conversion)?
              sp -= 1
            }
            let source = array_of(spots[0], slots, aliases, globals)?
            var destination = source
            if spots.len() >= 2 { destination = array_of(spots[1], slots, aliases, globals)? }
            var items: List[SortItem] = []
            if source in arrays {
              for subscript in arrays[source].cells.keys() {
                let held = (arrays[source].cells.get(subscript) ?? no_cell()).value
                let shown = if name == "asorti" { input(subscript)? } else { held }
                items += [sort_item(shown, subscript, how)?]
              }
            }
            let ordered = merge_sort(items, how.ends_with("_desc"))
            var cells: Map[Cell] = {}
            for at in range(ordered.len()) { cells = cells.set(f"{at + 1}", Cell(value: if name == "asorti" { String(ordered[at].text) } else { ordered[at].value }, seq: at)) }
            arrays[destination] = grown_table(cells, "1")
            result = Numeric(ordered.len().float())
          } else if name == "patsplit" {
            let base = sp - (if count >= 3 { 2 } else { 1 })
            let content = scalar_text(stack[base], conversion)?
            var pattern = plain_text(globals.get("FPAT") ?? Empty)?
            if count >= 3 { pattern = scalar_text(stack[base + 1], conversion)? }
            sp = base
            if pattern.is_empty() { pattern = "[^[:space:]]+" }
            let array = array_of(spots[0], slots, aliases, globals)?
            var cells: Map[Cell] = {}
            var separator_cells: Map[Cell] = {}
            var matched = 0
            var cursor = 0
            for hit in dynamic_matches(pattern, content, ignore_case)? {
              if hit.end == hit.start { continue }
              matched += 1
              separator_cells = separator_cells.set(f"{matched - 1}", Cell(value: String(content.byte_slice(cursor, length: hit.start - cursor)), seq: matched))
              cells = cells.set(f"{matched}", Cell(value: input(content.byte_slice(hit.start, length: hit.end - hit.start))?, seq: matched))
              cursor = hit.end
            }
            separator_cells = separator_cells.set(f"{matched}", Cell(value: String(content.byte_slice(cursor)), seq: matched + 1))
            arrays[array] = grown_table(cells, "1")
            if count == 4 {
              let seps_array = array_of(spots[1], slots, aliases, globals)?
              arrays[seps_array] = grown_table(separator_cells, "0")
            }
            result = Numeric(matched.float())
          } else {
            Err(fatal_error(f"function `{name}' not defined"))?
          }
          produced = true
        }
        _ => { Err(fatal_error("unimplemented operation"))? }
      }
      if produced {
        stack[sp] = result
        sp += 1
      }
      if pending != 0 {
        if pending == 1 {
          if pending_name in arrays { Err(fatal_error(f"attempt to use array `{pending_name}' in a scalar context"))? }
          globals[pending_name] = pending_value
        } else if pending == 3 {
          slots[pending_slot] = pending_value
        } else if pending == 5 {
          if pending_name not in arrays { arrays[pending_name] = empty_table() }
          let cell = arrays[pending_name].cells.get(pending_key) ?? no_cell()
          if cell.seq >= 0 { arrays[pending_name].cells[pending_key].value = pending_value } else {
            if arrays[pending_name].cells.is_empty() {
              arrays[pending_name].kind = subscript_kind(pending_key)
              arrays[pending_name].size = 0
              arrays[pending_name].growths = []
            }
            if arrays[pending_name].kind != 1 {
              if arrays[pending_name].size == 0 { arrays[pending_name].size = 13 }
              if (arrays[pending_name].cells.len() + 1) / arrays[pending_name].size > 10 {
                arrays[pending_name].size = next_table_size(arrays[pending_name].size)
                arrays[pending_name].growths += [arrays[pending_name].next]
              }
            }
            arrays[pending_name].cells[pending_key] = Cell(value: pending_value, seq: arrays[pending_name].next)
            arrays[pending_name].next += 1
          }
        } else if pending == 2 {
          let count = integer(number(pending_value)?)?
          if count < 0 { Err(fatal_error("NF set to negative value"))? }
          if count > 10000000 { Err(fatal_error(f"NF set to too large a value {count}"))? }
          if count < fields.len() { fields = fields[..count] } else { while fields.len() < count { fields += [Input("", null)] } }
          record = joined_fields(fields, plain_text(globals.get("OFS") ?? Empty)?, plain_text(globals.get("CONVFMT") ?? Empty)?)?
          globals["NF"] = Numeric(count.float())
        } else if pending == 4 {
          if pending_slot < 0 or pending_slot > 10000000 { Err(fatal_error(f"attempt to access field {pending_slot}"))? }
          if pending_slot == 0 {
            record = scalar_text(pending_value, plain_text(globals.get("CONVFMT") ?? Empty)?)?
            fields = split_fields(record, field_separator(globals)?, false, fold_case(globals))?
            globals["NF"] = Numeric(fields.len().float())
          } else {
            while fields.len() < pending_slot { fields += [Input("", null)] }
            fields[pending_slot - 1] = pending_value
            record = joined_fields(fields, plain_text(globals.get("OFS") ?? Empty)?, plain_text(globals.get("CONVFMT") ?? Empty)?)?
            globals["NF"] = Numeric(fields.len().float())
          }
        } else if pending == 6 {
          record = scalar_text(pending_value, plain_text(globals.get("CONVFMT") ?? Empty)?)?
          fields = split_fields(record, field_separator(globals)?, false, fold_case(globals))?
          globals["NF"] = Numeric(fields.len().float())
        }
      }
    }
    close_all(readers, writers, lossy)?
    emit_stdout(out, lossy)?
    out = ""
    (status % 256 + 256) % 256
  }
  match outcome {
    Ok(value) => Ok(value)
    Err(AwkError.Plain {message}) => Err(AwkError.Plain(message: message, pending: out))
    Err(failure) => Err(placed(labels, site, globals, out, failure))
  }
}


## Parse and execute a program; the pieces are the command-line text or -f files
## in order, and variable items (name=value, escapes undecoded) are applied
## before BEGIN. Returns the exit status.
export proc execute(pieces: List[Piece], arguments: List[Str], variables: List[Str] = [], sandbox: Bool = false) -> Result[Int, Error] {
  let program = parse(pieces, sandbox)?
  for item in variables {
    let equals = item.find("=") ?? 0
    let name = item.byte_slice(0, length: equals)
    if name in program.functions {
      let function = program.functions.get(name) ?? Function(parameters: [], body: 0, site: "cmd. line:1")
      return Err(AwkError.Syntax(f"{function.site}: error: function name `{name}' previously defined"))
    }
  }
  for note in program.notes { write_all(2, bytes.from_text(f"awk: {note}\n"))? }
  let linked = link(program)?
  run_machine(program, linked, arguments, variables, unix.isatty(1))
}
