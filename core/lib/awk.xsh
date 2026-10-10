##! Indexed AWK syntax and evaluation implemented in XSH.

# Messages are complete diagnostics without the leading "awk: ". Syntax errors
# exit 1 after parsing (as gawk does); runtime failures and refused features exit 2.
## The failure kinds of the awk interpreter: syntax errors exit 1, the rest 2.
export error AwkError = Syntax : Usage | Fatal : InvalidArgument | Unsupported : InvalidArgument

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
type Program = {expressions: List[Expression], statements: List[Statement], site: List[Str], begin: List[Int], end: List[Int], rules: List[Rule], functions: Map[Function], notes: List[Str], uses: List[Use], environ: Bool}
# A name seen as a variable, array or function call, kept for the checks that need every definition.
type Use = {name: Str, site: Str, call: Bool, argc: Int}
# A piece is one program source: the command-line text or one -f file.
type Piece = {label: Str, text: Str}
type Lexed = {tokens: List[Token], starts: List[Int], lines: List[Int], notes: List[Str]}
type Parser = {tokens: List[Token], starts: List[Int], lines: List[Int], at: Int, program: Program, printing: Bool, depth: Int, piece: Piece, loops: Int, context: Str, errors: List[Str], sandbox: Bool}
type Parsed[T] = {parser: Parser, value: T}
type Escaped = {content: Str, at: Int, notes: List[Str]}

pure syntax_error(message: Str) -> Error { AwkError.Syntax(message) }
pure runtime(message: Str) -> Error { AwkError.Fatal(f"fatal: {message}") }
pure unsupported(message: Str) -> Error { AwkError.Unsupported(f"fatal: unsupported: {message}") }
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

# Line number (1-based) of a byte offset within program text.
pure line_at(text: Str, offset: Int) -> Int {
  let source = bytes.from_text(text)
  var line = 1
  for at in range(if offset > source.len() { source.len() } else { offset }) { if source.byte_at(at) == 10 { line += 1 } }
  line
}
pure label_at(piece: Piece, offset: Int) -> Str { f"{piece.label}:{line_at(piece.text, offset)}" }

# A located diagnostic as gawk prints it: the source line, then a caret under
# the offending column with the message.
pure caret_error(piece: Piece, offset: Int, message: Str) -> Str {
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
  f"{piece.label}:{line}: {text}\nawk: {piece.label}:{line}: {pad}^ {message}"
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
        Err(_) => { return Err(syntax_error(caret_error(piece, start, "unterminated string"))) }
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
pure at_end(p: Parser) -> Bool { peek(p) == TokEnd }
pure newline_or_end(p: Parser) -> Bool { peek(p) == TokEnd or is_at(p, "\n") }
pure token_start(p: Parser) -> Int { p.starts.get(p.at) ?? 0 }
pure label_here(p: Parser) -> Str { f"{p.piece.label}:{p.lines.get(p.at) ?? 1}" }
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
  syntax_error(caret_error(p.piece, token_start(p), if newline_or_end(p) { "unexpected newline or end of string" } else { "syntax error" }))
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
  var p = skip_newlines(parser)
  var ids: List[Int] = []
  if is_at(p, end) { return Ok({parser: advance(p), value: ids}) }
  loop {
    let parsed = expression(p, 0)?
    p = skip_newlines(parsed.parser)
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
    if word not in BUILTINS { after.program.uses += [Use(name: word, site: label_here(parser), call: true, argc: args.value.len())] }
    if word in ["sub", "gsub"] and args.value.len() == 3 {
      let changeable = match after.program.expressions[args.value[2]] { Variable(_) => true; Index(_, _) => true; Field(_) => true; Text(_) => true; Number(_) => true; _ => false }
      if ! changeable { return Err(syntax_error(caret_error(after.piece, after.starts[after.at - 1], f"{word} third parameter is not a changeable object"))) }
    }
    if after.sandbox and word == "system" { return Err(AwkError.Fatal(f"{label_here(parser)}: fatal: 'system' function not allowed in sandbox mode")) }
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
          if target.parser.sandbox { return Err(AwkError.Fatal(f"{label_here(target.parser)}: fatal: redirection not allowed in sandbox mode")) }
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
      let no = expression(skip_newlines(require(skip_newlines(yes.parser), ":")?), 2)?
      let next = expression_node(no.parser, Conditional(left, yes.value, no.value))
      p = next.parser
      left = next.value
      continue
    }
    if p.printing and (op == ">" or op == ">>" or op == "|") and ! (op == "|" and spelling(peek_at(p, 1)) == "getline") { break }
    if minimum <= 8 and op == "|" and spelling(peek_at(p, 1)) == "getline" {
      if p.sandbox { return Err(AwkError.Fatal(f"{label_here(p)}: fatal: redirection not allowed in sandbox mode")) }
      let target = getline_target(advance(advance(p)))?
      let next = expression_node(target.parser, CommandInput(left, target.value))
      p = next.parser
      left = next.value
      relational = false
      continue
    }
    if op == "|&" { return Err(unsupported("coprocess operator `|&'")) }
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
    let next = expression_node(right.parser, if power == 1 { Assign(operator, left, right.value) } else { Binary(operator, left, right.value) })
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
pure add_bare_error(parser: Parser, message: Str) -> Parser {
  var p = parser
  p.errors += [f"{label_here(p)}: {message}"]
  p
}
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
      if p.sandbox { return Err(AwkError.Fatal(f"{label_here(p)}: fatal: redirection not allowed in sandbox mode")) }
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
      p = advance(add_bare_error(p, "each rule must have a pattern or an action part"))
      p = skip_newlines(p)
      continue
    }
    if word in ["function", "func"] {
      let function_name = name(advance(p))?
      let location = label_here(p)
      if function_name.value in BUILTINS {
        return Err(syntax_error(caret_error(piece, token_start(advance(p)), f"`{function_name.value}' is a built-in function, it cannot be redefined")))
      }
      p = require(function_name.parser, "(")?
      var parameters: List[Str] = []
      p = skip_newlines(p)
      if ! is_at(p, ")") {
        loop {
          let parameter = name(p)?
          p = skip_newlines(parameter.parser)
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
      if is_at(after, "\n") or peek(after) == TokEnd {
        return Err(syntax_error(caret_error(piece, token_start(after), f"{word} blocks must have an action part")))
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
  var program = Program(expressions: [], statements: [], site: [], begin: [], end: [], rules: [], functions: {}, notes: [], uses: [], environ: false)
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
pure is_integral(n: Float) -> Bool { n - n == 0.0 and truncate(n) == n }
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

# ---- Regular expressions -------------------------------------------------
#
# The host POSIX engine works on bytes and does not know the GNU awk
# conventions (a leading star is literal, backslash escapes inside brackets,
# UTF-8 characters count as one). Patterns are rewritten for it, and subjects
# with non-ASCII text are matched in a one-byte-per-character alphabet whose
# offsets map back to the original bytes.

type Match = {start: Int, end: Int}
type Alphabet = {chars: Map[Str], free: List[Int]}

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
    if c in [".", "$", "*", "+", "?", "]", "}"] { out += [c] } else { out += [literal_char(c, alphabet)] }
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
  let body = (if has_close { "]" } else { "" }) + items.join("") + (if has_caret { "^" } else { "" }) + (if has_dash { "-" } else { "" })
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
  {chars: chars, free: free}
}

const NO_ALPHABET: Alphabet = {chars: {}, free: []}

# All non-overlapping leftmost-longest matches of an awk regular expression,
# as byte offsets into content.
pure rx_matches(pattern: Str, content: Str, ignore_case: Bool) -> Result[List[Match]] {
  if is_ascii_text(content) and is_ascii_text(pattern) {
    let translated = rx_translate(pattern, NO_ALPHABET)?
    let hits = regex.find_bytes(translated, bytes.from_text(content), extended: true, ignore_case: ignore_case)?
    return Ok([{start: hit.start, end: hit.end} for hit in hits])
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
  let hits = regex.find_bytes(translated, bytes.from_ints(mapped)?, extended: true, ignore_case: ignore_case)?
  Ok([{start: offsets[hit.start], end: offsets[hit.end]} for hit in hits])
}
pure first_match(pattern: Str, content: Str, offset: Int, ignore_case: Bool) -> Result[Match?] {
  let hits = rx_matches(pattern, content, ignore_case)?
  for hit in hits { if hit.start >= offset { return Ok(hit) } }
  Ok(null)
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

# ---- Machine state -------------------------------------------------------

enum Flow { Normal, NextRecord, SkipFile, BreakLoop, ContinueLoop, ExitProgram, ReturnFunction }
enum Place { Named(Str), FieldPlace(Int), Element(Str, Str) }
# A function activation: parameter scalars by value, and for array use an
# alias naming the caller's array (an untyped parameter passed a bare name).
type Scope = {values: Map[Scalar], arrays: Map[Str], aliases: Map[Str], site: Str}
type Cell = {value: Scalar, seq: Int}
type Table = {cells: Map[Cell], next: Int}

# Streams. A reader holds decoded text and a position; a writer holds text
# not yet flushed. FIFO-backed commands run concurrently with the program.
type Reader = {fd: Int, text: Str, at: Int, done: Bool, tail: Bytes, pollable: Bool, job: ProcessHandle?, name: Str, command: Bool}
type Writer = {fd: Int, pending: Str, job: ProcessHandle?, name: Str, command: Bool, fifo: Path?}
type MainInput = {index: Int, reader: Reader?, opened: Bool, finished: Bool, stdin_used: Bool}
pure no_reader() -> Reader { Reader(fd: -1, text: "", at: 0, done: true, tail: b"", pollable: false, job: null, name: "", command: false) }
pure no_writer() -> Writer { Writer(fd: -1, pending: "", job: null, name: "", command: false, fifo: null) }
# State touched rarely lives behind a one-element list: a record is copied
# whole whenever the machine is updated, a list only by reference.
type Cold = {calls: Int, main: MainInput, interactive: Bool, state: Int, generator: List[Int], front: Int, rear: Int, seed: Float, previous_seed: Float, temp_root: FsRoot?, temp_path: Path?, temp_count: Int, running_end: Bool, running_begin: Bool, sandbox: Bool}
type Machine = {
  expressions: List[Expression], statements: List[Statement], sites: List[Str], functions: Map[Function],
  values: Map[Scalar], arrays: Map[Table], scopes: List[Scope], fields: List[Scalar], record: Str,
  status: Int, flow: Flow, value: Scalar, depth: Int, site: Str,
  readers: Map[Reader], writers: Map[Writer], cold: List[Cold],
}
pure cold(m: Machine) -> Cold { m.cold[0] }
pure with_cold(machine: Machine, state: Cold) -> Machine {
  var m = machine
  m.cold = [state]
  m
}
type Located = {machine: Machine, place: Place}
type Keyed = {machine: Machine, key: Str}

pure get_scalar(m: Machine, name: Str) -> Scalar {
  if ! m.scopes.is_empty() {
    let scope = m.scopes[m.scopes.len() - 1]
    if name in scope.values { return scope.values.get(name) ?? Empty }
  }
  m.values.get(name) ?? Empty
}
pure variable_text(m: Machine, name: Str) -> Result[Str] { scalar_text(get_scalar(m, name), plain_text(get_scalar(m, "CONVFMT"))?) }
pure conversion_format(m: Machine) -> Result[Str] { plain_text(get_scalar(m, "CONVFMT")) }
pure output_format(m: Machine) -> Result[Str] { plain_text(get_scalar(m, "OFMT")) }

# The array a name denotes in the current activation: a parameter alias or the
# global array of that name. Untyped parameters are resolved by the caller.
pure array_name(m: Machine, name: Str) -> Str {
  if ! m.scopes.is_empty() {
    let scope = m.scopes[m.scopes.len() - 1]
    if name in scope.arrays { return scope.arrays.get(name) ?? name }
    if name in scope.aliases { return scope.aliases.get(name) ?? name }
  }
  name
}
# True when the name is a parameter of the current activation (so a global of
# the same name is hidden).
pure is_local(m: Machine, name: Str) -> Bool {
  if m.scopes.is_empty() { return false }
  let scope = m.scopes[m.scopes.len() - 1]
  name in scope.values or name in scope.arrays or name in scope.aliases
}

pure table_get(m: Machine, array: Str) -> Table { m.arrays.get(array) ?? Table(cells: {}, next: 0) }
pure array_size(m: Machine, array: Str) -> Int { table_get(m, array).cells.len() }
pure array_has(m: Machine, array: Str, key: Str) -> Bool { key in table_get(m, array).cells }
pure array_find(m: Machine, array: Str, key: Str) -> Scalar? {
  let cells = table_get(m, array).cells
  if key not in cells { return null }
  (cells.get(key) ?? Cell(value: Empty, seq: 0)).value
}
pure array_set(machine: Machine, array: Str, key: Str, value: Scalar) -> Machine {
  var m = machine
  var table = table_get(m, array)
  m.arrays = m.arrays.remove(array)
  var seq = table.next
  if key in table.cells { seq = (table.cells.get(key) ?? Cell(value: Empty, seq: 0)).seq } else { table.next += 1 }
  table.cells = table.cells.set(key, Cell(value: value, seq: seq))
  m.arrays = m.arrays.set(array, table)
  m
}
pure array_remove(machine: Machine, array: Str, key: Str) -> Machine {
  var m = machine
  var table = table_get(m, array)
  m.arrays = m.arrays.remove(array)
  table.cells = table.cells.remove(key)
  m.arrays = m.arrays.set(array, table)
  m
}
pure array_clear(machine: Machine, array: Str) -> Machine {
  var m = machine
  m.arrays = m.arrays.remove(array)
  m.arrays = m.arrays.set(array, Table(cells: {}, next: 0))
  m
}
# Subscripts in the order a for-in loop visits them.
pure array_keys(m: Machine, array: Str) -> List[Str] { table_get(m, array).cells.keys() }

pure result(machine: Machine, value: Scalar) -> Machine { var m = machine; m.value = value; m }

# ---- Diagnostics and low-level streams -----------------------------------

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
pure context_text(m: Machine) -> Str {
  let fnr = number(get_scalar(m, "FNR")) ?? 0.0
  if fnr > 0.0 {
    let name = plain_text(get_scalar(m, "FILENAME")) ?? ""
    let line = float_text(fnr, "%.6g") ?? "0"
    return f"(FILENAME={name} FNR={line}) "
  }
  ""
}
pure fatal(m: Machine, text: Str) -> Error {
  AwkError.Fatal(f"{m.site}: {context_text(m)}fatal: {text}")
}
pure bare_fatal(text: Str) -> Error { AwkError.Fatal(f"fatal: {text}") }

proc write_all(fd: Int, data: Bytes) -> Result[Unit] {
  var rest = data
  while ! rest.is_empty() {
    let count = unix.write_fd(fd, rest)?
    if count <= 0 { return error.fail("write failed") }
    rest = rest.slice(count, length: rest.len() - count)
  }
}
proc warn(m: Machine, text: Str) -> Result[Unit] {
  write_all(2, bytes.from_text(f"awk: {m.site}: {context_text(m)}warning: {text}\n"))?
}
proc warn_plain(text: Str) -> Result[Unit] {
  write_all(2, bytes.from_text(f"awk: warning: {text}\n"))?
}
proc flush_stdout(machine: Machine) -> Result[Machine] {
  match io.flush_stdout() {
    Ok(_) => Ok(machine)
    Err(failure) => Err(fatal(machine, f"print to \"standard output\" failed ({reason_of(failure)})"))
  }
}
proc flush_writer(machine: Machine, key: Str) -> Result[Machine] {
  var m = machine
  var writer = m.writers.get(key) ?? no_writer()
  if writer.fd >= 0 and ! writer.pending.is_empty() {
    let data = writer.pending
    writer.pending = ""
    m.writers = m.writers.set(key, writer)
    match write_all(writer.fd, bytes.from_text(data)) {
      Ok(_) => {}
      Err(failure) => { return Err(fatal(m, f"print to \"{writer.name}\" failed ({reason_of(failure)})")) }
    }
  }
  Ok(m)
}
# Everything the program wrote becomes visible: buffered files and pipes first,
# then standard output, as GNU awk does before starting a child process.
proc flush_all(machine: Machine) -> Result[Machine] {
  var m = machine
  for key in m.writers.keys() { m = flush_writer(m, key)? }
  flush_stdout(m)
}

# A child's exit status as awk numbers it: the exit code, or 256 plus the
# signal number.
pure status_code(status: Status) -> Int {
  if status.signaled() { return 256 + (status.signal_number() ?? 0) }
  status.exit_code() ?? 0
}

# A shell prefix that exports the changes the program made to ENVIRON, so
# children see them as they do under GNU awk.
proc environment_prefix(m: Machine) -> Str {
  if "ENVIRON" not in m.arrays { return "" }
  var prefix = ""
  let table = table_get(m, "ENVIRON")
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
type Located2 = {machine: Machine, path: Path}
proc fifo_path(machine: Machine) -> Result[Located2] {
  var m = machine
  var state = cold(m)
  if state.temp_root == null {
    let root = fs.tempdir()?
    state.temp_path = root.host_path()?
    state.temp_root = root
  }
  state.temp_count += 1
  let base = state.temp_path ?? fp"/tmp"
  let made = fp"{base}/c{state.temp_count}"
  m = with_cold(m, state)
  Ok({machine: m, path: made})
}

# Starts `sh -c command` with standard output into a FIFO this process reads.
proc open_command_reader(machine: Machine, command: Str) -> Result[Machine] {
  # GNU awk does not synchronize its own buffered output before starting an
  # input command; only output commands and system() flush first.
  var m = machine
  let made = fifo_path(m)?
  m = made.machine
  let pipe_path = made.path
  fs.mkfifo(pipe_path, 0o600)?
  let fd = unix.open_fd(pipe_path, nonblock: true)?
  let job = spawn run.status sh -c ${environment_prefix(m) + command} > $pipe_path ?
  m.readers = m.readers.set(f"|{command}", Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: true, job: job, name: command, command: true))
  Ok(m)
}
# Starts `sh -c command` with standard input from a FIFO this process writes.
proc open_command_writer(machine: Machine, command: Str) -> Result[Machine] {
  var m = flush_all(machine)?
  let made = fifo_path(m)?
  m = made.machine
  let pipe_path = made.path
  fs.mkfifo(pipe_path, 0o600)?
  # Holding the read end open lets the write end open at once; the child then
  # inherits the read side and the held descriptor is released.
  let hold = unix.open_fd(pipe_path, nonblock: true)?
  let fd = unix.open_fd(pipe_path, write: true)?
  let job = spawn run.status sh -c ${environment_prefix(m) + command} < $pipe_path ?
  unix.close_fd(hold)?
  m.writers = m.writers.set(f"|{command}", Writer(fd: fd, pending: "", job: job, name: command, command: true, fifo: pipe_path))
  Ok(m)
}

# ---- Reading records -----------------------------------------------------

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
type Chunk = {text: Str, tail: Bytes}
pure decode_chunk(carry: Bytes, fresh: Bytes) -> Result[Chunk] {
  let data = if carry.is_empty() { fresh } else { bytes.concat([carry, fresh]) }
  match data.utf8() {
    Ok(text) => Ok({text: text, tail: b""})
    Err(_) => {
      let keep = incomplete_tail(data)
      if keep > 0 {
        let head = data.slice(0, length: data.len() - keep)
        match head.utf8() {
          Ok(text) => { return Ok({text: text, tail: data.slice(data.len() - keep, length: keep)}) }
          Err(_) => {}
        }
      }
      Err(bare_fatal("unsupported: input is not valid UTF-8"))
    }
  }
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
      if ! r.tail.is_empty() { return Err(bare_fatal("unsupported: input is not valid UTF-8")) }
      return Ok(r)
    }
    let decoded = decode_chunk(r.tail, chunk)?
    r.tail = decoded.tail
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

# ---- Fields, variables and places ----------------------------------------

pure fold_case(m: Machine) -> Bool { truth(get_scalar(m, "IGNORECASE")) ?? false }

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

# The field separator in force: with RS = "" newline separates fields too.
pure field_separator(m: Machine) -> Result[Str] {
  var separator = variable_text(m, "FS")?
  if (variable_text(m, "RS")?).is_empty() and separator != " " and ! separator.is_empty() {
    if separator.count_chars() == 1 and separator in REGEX_SPECIAL { separator = "\\" + separator }
    if separator == "\\" { separator = "\\\\" }
    separator = f"({separator})|\n"
  }
  Ok(separator)
}

pure rebuild(machine: Machine) -> Result[Machine] {
  var m = machine
  let conversion = conversion_format(m)?
  m.record = [scalar_text(value, conversion)? for value in m.fields].join(variable_text(m, "OFS")?)
  m.values = m.values.set("NF", Numeric(m.fields.len().float()))
  Ok(m)
}
# Installs a new record: $0, its fields and NF.
pure set_record(machine: Machine, content: Str) -> Result[Machine] {
  var m = machine
  let separator = field_separator(m)?
  m.fields = split_fields(content, separator, false, fold_case(m))?
  m.record = content
  m.values = m.values.set("NF", Numeric(m.fields.len().float()))
  Ok(m)
}

# Checks that a name is not currently an array before a scalar use.
pure scalar_conflict(m: Machine, name: Str) -> Bool {
  array_name(m, name) in m.arrays and ! (is_local(m, name) and ! (name in m.scopes[m.scopes.len() - 1].aliases))
}

proc assign_variable(machine: Machine, name: Str, value: Scalar) -> Result[Machine] {
  var m = machine
  if scalar_conflict(m, name) { return Err(fatal(m, f"attempt to use array `{name}' in a scalar context")) }
  if name == "NF" {
    let count = integer(number(value)?)?
    if count < 0 { return Err(fatal(m, "NF set to negative value")) }
    if count > 10000000 { return Err(fatal(m, f"NF set to too large a value {count}")) }
    if count < m.fields.len() { m.fields = m.fields[..count] } else { while m.fields.len() < count { m.fields += [Input("", null)] } }
    return rebuild(m)
  }
  if ! m.scopes.is_empty() {
    var scope = m.scopes[m.scopes.len() - 1]
    if name in scope.values {
      scope.values = scope.values.set(name, value)
      m.scopes[m.scopes.len() - 1] = scope
      return Ok(m)
    }
  }
  if array_name(m, name) in m.arrays { return Err(fatal(m, f"attempt to use array `{name}' in a scalar context")) }
  m.values = m.values.set(name, value)
  Ok(m)
}

proc key(machine: Machine, expressions: List[Int]) -> Result[Keyed] {
  var m = machine
  var keys: List[Str] = []
  for id in expressions {
    m = evaluate(m, id)?
    if m.flow != Normal { return Ok({machine: m, key: ""}) }
    keys += [scalar_text(m.value, conversion_format(m)?)?]
  }
  Ok({machine: m, key: keys.join(variable_text(m, "SUBSEP")?)})
}
# The array a subscripted name refers to, or a fatal error when it is a scalar.
pure array_ref(m: Machine, name: Str) -> Result[Str] {
  let resolved = array_name(m, name)
  if is_local(m, name) {
    let scope = m.scopes[m.scopes.len() - 1]
    if name in scope.aliases {
      let current = scope.values.get(name) ?? Empty
      match current { Empty => { return Ok(resolved) }; _ => { return Err(fatal(m, f"attempt to use scalar `{name}' as an array")) } }
    }
    if name in scope.values { return Err(fatal(m, f"attempt to use scalar `{name}' as an array")) }
    return Ok(resolved)
  }
  if name in m.values { return Err(fatal(m, f"attempt to use scalar `{name}' as an array")) }
  Ok(resolved)
}
proc locate(machine: Machine, id: Int) -> Result[Located] {
  var m = machine
  match m.expressions[id] {
    Variable(name) => Ok({machine: m, place: Named(name)})
    Field(child) => {
      m = evaluate(m, child)?
      let index = integer(number(m.value)?)?
      if index < 0 { return Err(fatal(m, f"attempt to access field {index}")) }
      if index > 10000000 { return Err(fatal(m, f"attempt to access field {index}")) }
      Ok({machine: m, place: FieldPlace(index)})
    }
    Index(name, keys) => {
      let found = key(m, keys)?
      m = found.machine
      let array = array_ref(m, name)?
      Ok({machine: m, place: Element(array, found.key)})
    }
    _ => Err(fatal(m, "assignment requires variable, field, or array element"))
  }
}
pure read_place(m: Machine, place: Place) -> Scalar {
  match place {
    Named(name) => get_scalar(m, name)
    FieldPlace(index) => if index == 0 { Input(m.record, null) } else { m.fields.get(index - 1) ?? Input("", null) }
    Element(array, key) => array_find(m, array, key) ?? Empty
  }
}
proc write_place(machine: Machine, place: Place, value: Scalar) -> Result[Machine] {
  var m = machine
  match place {
    Named(name) => assign_variable(m, name, value)
    FieldPlace(index) => {
      if index == 0 { return set_record(m, scalar_text(value, conversion_format(m)?)?) }
      while m.fields.len() < index { m.fields += [Input("", null)] }
      m.fields[index - 1] = value
      rebuild(m)
    }
    Element(array, key) => Ok(array_set(m, array, key, value))
  }
}

# ---- Operators ----------------------------------------------------------

pure to_bit_operand(name: Str, position: Int, value: Float) -> Result[Float] {
  if value < 0.0 { return Err(AwkError.Fatal(f"fatal: {name}: argument {position} negative value {float_text(value, "%.6g") ?? ""} is not allowed")) }
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

# ---- Evaluation ---------------------------------------------------------

proc regex_source(machine: Machine, id: Int) -> Result[Keyed] {
  match machine.expressions[id] {
    RegexLiteral(pattern) => Ok({machine: machine, key: pattern})
    _ => { let m = evaluate(machine, id)?; Ok({machine: m, key: scalar_text(m.value, conversion_format(m)?)?}) }
  }
}
# Dynamic regular expressions fail at run time with the engine's message.
pure regexp_matches(m: Machine, pattern: Str, content: Str) -> Result[List[Match]] {
  match rx_matches(pattern, content, fold_case(m)) {
    Ok(hits) => Ok(hits)
    Err(failure) => Err(fatal(m, f"invalid regexp: {failure.message}: /{pattern}/"))
  }
}
pure matches_somewhere(m: Machine, pattern: Str, content: Str) -> Result[Bool] {
  Ok(! regexp_matches(m, pattern, content)?.is_empty())
}

proc evaluate(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  if m.flow != Normal { return Ok(m) }
  match m.expressions[id] {
    Number(n) => Ok(result(m, Numeric(n)))
    Text(content) => Ok(result(m, String(content)))
    RegexLiteral(pattern) => Ok(result(m, Numeric(if matches_somewhere(m, pattern, m.record)? { 1.0 } else { 0.0 })))
    Variable(name) => {
      if scalar_conflict(m, name) { return Err(fatal(m, f"attempt to use array `{name}' in a scalar context")) }
      Ok(result(m, get_scalar(m, name)))
    }
    Field(child) => {
      m = evaluate(m, child)?
      if m.flow != Normal { return Ok(m) }
      let index = integer(number(m.value)?)?
      if index < 0 or index > 10000000 { return Err(fatal(m, f"attempt to access field {index}")) }
      Ok(result(m, if index == 0 { input(m.record)? } else { m.fields.get(index - 1) ?? Input("", null) }))
    }
    Index(name, keys) => {
      let located = key(m, keys)?
      m = located.machine
      if m.flow != Normal { return Ok(m) }
      let array = array_ref(m, name)?
      let existing = array_find(m, array, located.key)
      if existing != null { return Ok(result(m, existing ?? Empty)) }
      m = array_set(m, array, located.key, Empty)
      Ok(result(m, Empty))
    }
    Unary(op, child) => {
      m = evaluate(m, child)?
      if m.flow != Normal { return Ok(m) }
      if op == "!" { return Ok(result(m, Numeric(if truth(m.value)? { 0.0 } else { 1.0 }))) }
      match m.value {
        Nan(positive) => { return Ok(result(m, if op == "-" { Nan(! positive) } else { m.value })) }
        _ => {}
      }
      let n = number(m.value)?
      if is_nan(n) { return Ok(result(m, if op == "-" { Nan(true) } else { Nan(false) })) }
      Ok(result(m, Numeric(if op == "-" { -n } else { n })))
    }
    Increment(target, delta, post) => {
      let located = locate(m, target)?
      m = located.machine
      if m.flow != Normal { return Ok(m) }
      let old = number(read_place(m, located.place))?
      let value = Numeric(old + delta)
      m = write_place(m, located.place, value)?
      Ok(result(m, if post { Numeric(old) } else { value }))
    }
    Assign(op, target, right) => {
      let located = locate(m, target)?
      m = evaluate(located.machine, right)?
      if m.flow != Normal { return Ok(m) }
      var value = m.value
      if op != "=" {
        let operator = op.byte_slice(0, length: 1)
        let current = number(read_place(m, located.place))?
        let operand = number(m.value)?
        if operator in ["/", "%"] and operand == 0.0 { return Err(fatal(m, f"division by zero attempted in `{op}'")) }
        value = Numeric(arith(operator, current, operand)?)
      } else {
        # Assigning a whole array's name is an error, caught when it was read.
        match value { Input(content, n) => { value = Input(content, n) }; _ => {} }
      }
      m = write_place(m, located.place, value)?
      Ok(result(m, value))
    }
    Conditional(condition, yes, no) => {
      m = evaluate(m, condition)?
      if m.flow != Normal { return Ok(m) }
      evaluate(m, if truth(m.value)? { yes } else { no })
    }
    Binary(op, left, right) => {
      m = evaluate(m, left)?
      if m.flow != Normal { return Ok(m) }
      let a = m.value
      if op in ["&&", "||"] {
        let yes = truth(a)?
        if (op == "&&" and ! yes) or (op == "||" and yes) { return Ok(result(m, Numeric(if yes { 1.0 } else { 0.0 }))) }
        m = evaluate(m, right)?
        if m.flow != Normal { return Ok(m) }
        return Ok(result(m, Numeric(if truth(m.value)? { 1.0 } else { 0.0 })))
      }
      if op in ["~", "!~"] {
        let pattern = regex_source(m, right)?
        m = pattern.machine
        if m.flow != Normal { return Ok(m) }
        let hit = matches_somewhere(m, pattern.key, scalar_text(a, conversion_format(m)?)?)?
        return Ok(result(m, Numeric(if hit != (op == "!~") { 1.0 } else { 0.0 })))
      }
      m = evaluate(m, right)?
      if m.flow != Normal { return Ok(m) }
      let b = m.value
      let conversion = conversion_format(m)?
      if op == "concat" { return Ok(result(m, String(scalar_text(a, conversion)? + scalar_text(b, conversion)?))) }
      if op in ["==", "!=", "<", ">", "<=", ">="] {
        let order = compare(a, b, conversion)?
        let yes = if op == "==" { order == 0 } else if op == "!=" { order != 0 } else if op == "<" { order < 0 } else if op == ">" { order > 0 } else if op == "<=" { order <= 0 } else { order >= 0 }
        # NaN is unordered: every comparison but != is false.
        if numeric(a) and numeric(b) {
          let x = number(a)?
          let y = number(b)?
          if is_nan(x) or is_nan(y) { return Ok(result(m, Numeric(if op == "!=" { 1.0 } else { 0.0 }))) }
        }
        return Ok(result(m, Numeric(if yes { 1.0 } else { 0.0 })))
      }
      let x = number(a)?
      let y = number(b)?
      if op == "/" and y == 0.0 { return Err(fatal(m, "division by zero attempted")) }
      if op == "%" and y == 0.0 { return Err(fatal(m, "division by zero attempted in `%'")) }
      Ok(result(m, Numeric(arith(op, x, y)?)))
    }
    Member(keys, array) => {
      let found = key(m, keys)?
      m = found.machine
      if m.flow != Normal { return Ok(m) }
      let resolved = array_ref(m, array)?
      Ok(result(m, Numeric(if array_has(m, resolved, found.key) { 1.0 } else { 0.0 })))
    }
    CommandInput(command, target) => {
      m = evaluate(m, command)?
      if m.flow != Normal { return Ok(m) }
      let text = scalar_text(m.value, conversion_format(m)?)?
      if text.is_empty() { return Err(fatal(m, "expression for `|' redirection has null string value")) }
      let reader_key = f"|{text}"
      if reader_key not in m.readers { m = open_command_reader(m, text)? }
      deliver_record(m, reader_key, target, false)
    }
    GetlineMain(target) => {
      let got = next_main_record(m)?
      m = got.machine
      if got.record == null { return Ok(result(m, Numeric(if got.failed { -1.0 } else { 0.0 }))) }
      let content = got.record ?? ""
      m = bump_counters(m, false)?
      m = store_record(m, content, got.terminator, target)?
      Ok(result(m, Numeric(1.0)))
    }
    GetlineFile(target, file) => {
      m = evaluate(m, file)?
      if m.flow != Normal { return Ok(m) }
      let name = scalar_text(m.value, conversion_format(m)?)?
      if name.is_empty() { return Err(fatal(m, "expression for `<' redirection has null string value")) }
      let reader_key = f"<{name}"
      if reader_key not in m.readers {
        match open_file_reader(m, name) {
          Ok(opened) => { m = opened }
          Err(_) => { return Ok(result(m, Numeric(-1.0))) }
        }
      }
      deliver_record(m, reader_key, target, false)
    }
    Call(name, args) => call(m, name, args)
  }
}
# Reads the next record of an open stream into $0 or the target variable.
proc deliver_record(machine: Machine, reader_key: Str, target: Int?, counts: Bool) -> Result[Machine] {
  var m = machine
  let reader = m.readers.get(reader_key) ?? no_reader()
  var got: Read = {reader: reader, record: null, terminator: ""}
  match read_record(reader, variable_text(m, "RS")?, fold_case(m)) {
    Ok(read) => { got = read }
    Err(failure) => { return Ok(result(m, Numeric(-1.0))) }
  }
  m.readers = m.readers.set(reader_key, got.reader)
  if got.record == null { return Ok(result(m, Numeric(0.0))) }
  m = store_record(m, got.record ?? "", got.terminator, target)?
  Ok(result(m, Numeric(1.0)))
}
proc bump_counters(machine: Machine, command: Bool) -> Result[Machine] {
  var m = machine
  m.values = m.values.set("NR", Numeric(number(get_scalar(m, "NR"))? + 1.0))
  if ! command { m.values = m.values.set("FNR", Numeric(number(get_scalar(m, "FNR"))? + 1.0)) }
  Ok(m)
}
proc store_record(machine: Machine, content: Str, terminator: Str, target: Int?) -> Result[Machine] {
  var m = machine
  m.values = m.values.set("RT", String(terminator))
  if target == null { return set_record(m, content) }
  let located = locate(m, target ?? 0)?
  write_place(located.machine, located.place, input(content)?)
}

# ---- Input files ----------------------------------------------------------

proc open_file_reader(machine: Machine, name: Str) -> Result[Machine] {
  var m = machine
  if name == "-" or name == "/dev/stdin" {
    if "<-" not in m.readers {
      m.readers = m.readers.set("<-", Reader(fd: 0, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: "-", command: false))
    }
    m.readers = m.readers.set(f"<{name}", m.readers.get("<-") ?? Reader(fd: 0, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: "-", command: false))
    return Ok(m)
  }
  let metadata = fs.stat(fp"{name}", follow_symlinks: true)?
  if metadata.kind == "dir" { return Err(error.failure("is a directory")) }
  let fd = unix.open_fd(fp"{name}")?
  m.readers = m.readers.set(f"<{name}", Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: name, command: false))
  Ok(m)
}
type MainRead = {machine: Machine, record: Str?, terminator: Str, failed: Bool}

# Whether an ARGV entry is a var=value assignment operand.
pure assignment_operand(argument: Str) -> Bool {
  let equals = argument.find("=")
  if equals == null { return false }
  binding_name(argument.byte_slice(0, length: equals ?? 0))
}
## True when the text is a valid awk variable name.
export pure binding_name(content: Str) -> Bool {
  if content.is_empty() or ! letter(content.byte_at(0) ?? 0) { return false }
  for at in range(1, content.byte_len()) { if ! letter(content.byte_at(at) ?? 0) and ! digit(content.byte_at(at) ?? 0) { return false } }
  true
}
proc apply_binding(machine: Machine, item: Str) -> Result[Machine] {
  let equals = item.find("=") ?? 0
  let name = item.byte_slice(0, length: equals)
  let decoded = escaped(bytes.from_text(item.byte_slice(equals + 1)), 0, -1)?
  for note in decoded.notes { warn_plain(note)? }
  assign_variable(machine, name, input(decoded.content)?)
}

# The next record of the main input, opening files named by ARGV on demand.
# ARGV and ARGC are consulted live, so the program can edit the operand list.
proc next_main_record(machine: Machine) -> Result[MainRead] {
  var m = machine
  var state = cold(m)
  loop {
    if state.main.finished { return Ok({machine: with_cold(m, state), record: null, terminator: "", failed: false}) }
    let reader = state.main.reader
    if reader != null {
      let source = reader ?? no_reader()
      var shared_key = ""
      if source.fd == 0 { shared_key = "<-" }
      var current = source
      if ! shared_key.is_empty() { current = m.readers.get(shared_key) ?? source }
      let got = read_record(current, variable_text(m, "RS")?, fold_case(m))?
      if ! shared_key.is_empty() { m.readers = m.readers.set(shared_key, got.reader) }
      state.main.reader = got.reader
      if got.record != null { return Ok({machine: with_cold(m, state), record: got.record, terminator: got.terminator, failed: false}) }
      if got.reader.fd > 0 { unix.close_fd(got.reader.fd)? }
      state.main.reader = null
      continue
    }
    # Choose the next operand.
    let count = integer(number(get_scalar(m, "ARGC"))?)?
    var opened = false
    while state.main.index < count {
      let index = state.main.index
      state.main.index += 1
      let entry = array_find(m, "ARGV", f"{index}")
      if entry == null { continue }
      let argument = plain_text(entry ?? Empty)?
      if argument.is_empty() { continue }
      if assignment_operand(argument) {
        m = with_cold(m, state)
        m = apply_binding(m, argument)?
        state = cold(m)
        continue
      }
      state.main.opened = true
      if argument == "-" or argument == "/dev/stdin" {
        m.values = m.values.set("FILENAME", String(argument))
        m.values = m.values.set("FNR", Numeric(0.0))
        if "<-" not in m.readers { m.readers = m.readers.set("<-", Reader(fd: 0, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: "-", command: false)) }
        state.main.reader = m.readers.get("<-") ?? no_reader()
        opened = true
        break
      }
      let metadata = fs.stat(fp"{argument}", follow_symlinks: true)
      match metadata {
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
          m.values = m.values.set("FILENAME", String(argument))
          m.values = m.values.set("FNR", Numeric(0.0))
          state.main.reader = Reader(fd: fd, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: argument, command: false)
          opened = true
        }
        Err(failure) => { return Err(bare_fatal(f"cannot open file `{argument}' for reading: {reason_of(failure)}")) }
      }
      break
    }
    if opened { continue }
    if ! state.main.opened and ! state.main.stdin_used {
      state.main.stdin_used = true
      state.main.opened = true
      m.values = m.values.set("FILENAME", String("-"))
      if "<-" not in m.readers { m.readers = m.readers.set("<-", Reader(fd: 0, text: "", at: 0, done: false, tail: b"", pollable: false, job: null, name: "-", command: false)) }
      state.main.reader = m.readers.get("<-") ?? no_reader()
      continue
    }
    state.main.finished = true
  }
  Ok({machine: with_cold(m, state), record: null, terminator: "", failed: false})
}

# ---- Functions ------------------------------------------------------------

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
  let changed = if upper { character.upper() } else { character.lower() }
  if changed.count_chars() == 1 { changed } else { character }
}

pure random_next(machine: Machine) -> Machine {
  var state = cold(machine)
  var generator = state.generator
  generator[state.front] = (generator[state.front] + generator[state.rear]) % 4294967296
  state.state = generator[state.front] / 2 % 2147483648
  state.front += 1
  if state.front >= 63 { state.front = 0; state.rear += 1 } else { state.rear += 1; if state.rear >= 63 { state.rear = 0 } }
  state.generator = generator
  with_cold(machine, state)
}
pure random_seed(machine: Machine, seed: Int) -> Machine {
  var state = cold(machine)
  var generator = [0 for _ in range(63)]
  var word = seed % 4294967296
  if word < 0 { word += 4294967296 }
  generator[0] = word
  for index in range(1, 63) {
    let previous = if generator[index - 1] >= 2147483648 { generator[index - 1] - 4294967296 } else { generator[index - 1] }
    let high = previous / 127773
    let low = previous % 127773
    var value = 16807 * low - 2836 * high
    if value < 0 { value += 2147483647 }
    generator[index] = value % 4294967296
  }
  state.generator = generator
  state.front = 1
  state.rear = 0
  var m = with_cold(machine, state)
  for _ in range(630) { m = random_next(m) }
  m
}

# User function calls. Scalars are passed by value; a bare variable name that
# is an array or was never given a value is passed by alias, so the callee may
# use it as an array and the caller sees the result.
proc call_user(machine: Machine, name: Str, args: List[Int]) -> Result[Machine] {
  var m = machine
  let function = m.functions.get(name)?
  var counters = cold(m)
  counters.calls += 1
  let call_id = counters.calls
  m = with_cold(m, counters)
  m.depth += 1
  if m.depth > 1000 { return Err(fatal(m, "function call nesting too deep")) }
  var scope = Scope(values: {}, arrays: {}, aliases: {}, site: m.site)
  for at in range(function.parameters.len()) {
    let parameter = function.parameters[at]
    if at >= args.len() {
      scope.values = scope.values.set(parameter, Empty)
      scope.aliases = scope.aliases.set(parameter, f"\0{call_id}:{parameter}")
      continue
    }
    var passed = false
    match m.expressions[args[at]] {
      Variable(variable) => {
        let resolved = array_name(m, variable)
        if resolved in m.arrays {
          scope.aliases = scope.aliases.set(parameter, resolved)
          scope.values = scope.values.set(parameter, Empty)
          passed = true
        } else if is_local(m, variable) {
          let current = m.scopes[m.scopes.len() - 1]
          if variable in current.aliases and (current.values.get(variable) ?? Empty) == Empty {
            scope.aliases = scope.aliases.set(parameter, current.aliases.get(variable) ?? variable)
            scope.values = scope.values.set(parameter, Empty)
            passed = true
          }
        } else if variable not in m.values {
          scope.aliases = scope.aliases.set(parameter, variable)
          scope.values = scope.values.set(parameter, Empty)
          passed = true
        }
      }
      _ => {}
    }
    if ! passed {
      m = evaluate(m, args[at])?
      if m.flow != Normal { return Ok(m) }
      scope.values = scope.values.set(parameter, m.value)
    }
  }
  # Surplus arguments have no parameter to bind, but their side effects still happen, in order.
  for at in range(args.len()) {
    if at >= function.parameters.len() {
      m = evaluate(m, args[at])?
      if m.flow != Normal { return Ok(m) }
    }
  }
  m.scopes += [scope]
  let saved_site = m.site
  m = execute_statement(m, function.body)?
  m.scopes = m.scopes[..m.scopes.len() - 1]
  for parameter in function.parameters { m.arrays = m.arrays.remove(f"\0{call_id}:{parameter}") }
  m.site = saved_site
  m.depth -= 1
  if m.flow == ReturnFunction { m.flow = Normal } else if m.flow == Normal { m.value = Empty } else if m.flow not in [ExitProgram, NextRecord, SkipFile] { return Err(fatal(m, "invalid control flow from function")) }
  Ok(m)
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
pure char_slice_start(content: Str, count: Int) -> Int {
  if count <= 0 { return 0 }
  var seen = 0
  var at = 0
  for character in content.split("") {
    if seen == count { break }
    seen += 1
    at += character.byte_len()
  }
  at
}
pure located_result(m: Machine, outcome: Result[Float]) -> Result[Float] {
  match outcome {
    Ok(value) => Ok(value)
    Err(failure) => Err(located(m, failure))
  }
}

proc call(machine: Machine, name: Str, args: List[Int]) -> Result[Machine] {
  var m = machine
  if name in m.functions { return call_user(m, name, args) }
  if name in ["sub", "gsub"] {
    if args.len() < 2 or args.len() > 3 { return Err(fatal(m, f"{name}: wrong number of arguments")) }
    let pattern = regex_source(m, args[0])?
    m = evaluate(pattern.machine, args[1])?
    if m.flow != Normal { return Ok(m) }
    let replacement = scalar_text(m.value, conversion_format(m)?)?
    var place: Place = FieldPlace(0)
    var assignable = true
    if args.len() == 3 {
      match m.expressions[args[2]] {
        Variable(_) => { let located = locate(m, args[2])?; m = located.machine; place = located.place }
        Field(_) => { let located = locate(m, args[2])?; m = located.machine; place = located.place }
        Index(_, _) => { let located = locate(m, args[2])?; m = located.machine; place = located.place }
        _ => {
          m = evaluate(m, args[2])?
          assignable = false
        }
      }
    }
    var content = ""
    if assignable { content = scalar_text(read_place(m, place), conversion_format(m)?)? } else { content = scalar_text(m.value, conversion_format(m)?)? }
    var out = ""
    var copied = 0
    var count = 0
    var previous_end = -1
    for hit in regexp_matches(m, pattern.key, content)? {
      if hit.start == hit.end and previous_end == hit.start { continue }
      out += content.byte_slice(copied, length: hit.start - copied) + substitute(replacement, content.byte_slice(hit.start, length: hit.end - hit.start))
      copied = hit.end
      previous_end = hit.end
      count += 1
      if name == "sub" { break }
    }
    out += content.byte_slice(copied)
    if count > 0 and assignable { m = write_place(m, place, String(out))? }
    return Ok(result(m, Numeric(count.float())))
  }
  if name == "split" {
    if args.len() < 2 or args.len() > 4 { return Err(fatal(m, "split: wrong number of arguments")) }
    m = evaluate(m, args[0])?
    if m.flow != Normal { return Ok(m) }
    let content = scalar_text(m.value, conversion_format(m)?)?
    var separator = variable_text(m, "FS")?
    var regex_mode = false
    if args.len() >= 3 {
      match m.expressions[args[2]] { RegexLiteral(_) => { regex_mode = true }; _ => {} }
      let pattern = regex_source(m, args[2])?
      m = pattern.machine
      separator = pattern.key
    }
    let array = match m.expressions[args[1]] {
      Variable(variable) => {
        match array_ref(m, variable) {
          Ok(resolved) => resolved
          Err(_) => { return Err(fatal(m, "split: second argument is not an array")) }
        }
      }
      _ => { return Err(fatal(m, "split: second argument is not an array")) }
    }
    if args.len() == 4 {
      let seps_array = match m.expressions[args[3]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, "split: fourth argument is not an array")) } }
      let split = split_with_separators(content, separator, regex_mode, fold_case(m))?
      m = array_clear(m, array)
      m = array_clear(m, seps_array)
      for at in range(split.fields.len()) { m = array_set(m, array, f"{at + 1}", input(split.fields[at])?) }
      for at in range(split.separators.len()) { m = array_set(m, seps_array, f"{at + 1}", String(split.separators[at])) }
      return Ok(result(m, Numeric(split.fields.len().float())))
    }
    let fields = split_fields(content, separator, regex_mode, fold_case(m))?
    m = array_clear(m, array)
    for at in range(fields.len()) { m = array_set(m, array, f"{at + 1}", fields[at]) }
    return Ok(result(m, Numeric(fields.len().float())))
  }
  if name == "match" {
    if args.len() < 2 or args.len() > 3 { return Err(fatal(m, "match: wrong number of arguments")) }
    m = evaluate(m, args[0])?
    if m.flow != Normal { return Ok(m) }
    let content = scalar_text(m.value, conversion_format(m)?)?
    let pattern = regex_source(m, args[1])?
    m = pattern.machine
    var hit: Match? = null
    let hits = regexp_matches(m, pattern.key, content)?
    if ! hits.is_empty() { hit = hits[0] }
    let start = if hit == null { 0 } else { content.byte_slice(0, length: (hit ?? Match(start: 0, end: 0)).start).count_chars() + 1 }
    let length = if hit == null { -1 } else { content.byte_slice((hit ?? Match(start: 0, end: 0)).start, length: (hit ?? Match(start: 0, end: 0)).end - (hit ?? Match(start: 0, end: 0)).start).count_chars() }
    m.values = m.values.set("RSTART", Numeric(start.float()))
    m.values = m.values.set("RLENGTH", Numeric(length.float()))
    if args.len() == 3 {
      let array = match m.expressions[args[2]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, "match: third argument is not an array")) } }
      m = array_clear(m, array)
      if hit != null {
        let caps = rx_captures(pattern.key, content, fold_case(m))?
        let separator = variable_text(m, "SUBSEP")?
        for at in range(caps.len()) {
          let item = caps[at]
          if item == null { continue }
          let span = item ?? Match(start: 0, end: 0)
          let text = content.byte_slice(span.start, length: span.end - span.start)
          let from = content.byte_slice(0, length: span.start).count_chars() + 1
          m = array_set(m, array, f"{at}", input(text)?)
          m = array_set(m, array, f"{at}{separator}start", Numeric(from.float()))
          m = array_set(m, array, f"{at}{separator}length", Numeric(text.count_chars().float()))
        }
      }
    }
    return Ok(result(m, Numeric(start.float())))
  }
  if name == "length" and args.len() == 1 {
    match m.expressions[args[0]] {
      Variable(variable) => {
        let array = array_name(m, variable)
        if array in m.arrays { return Ok(result(m, Numeric(array_size(m, array).float()))) }
        if is_local(m, variable) {
          let scope = m.scopes[m.scopes.len() - 1]
          if variable in scope.aliases and (scope.values.get(variable) ?? Empty) == Empty { return Ok(result(m, Numeric(0.0))) }
        }
      }
      _ => {}
    }
  }
  if name in ["typeof", "isarray"] and args.len() == 1 {
    match m.expressions[args[0]] {
      Variable(variable) => {
        let array = array_name(m, variable)
        let is_array = array in m.arrays
        var kind = "untyped"
        if is_array { kind = "array" } else if is_local(m, variable) {
          let scope = m.scopes[m.scopes.len() - 1]
          let current = scope.values.get(variable) ?? Empty
          kind = type_name(current)
        } else if variable in m.values { kind = type_name(m.values.get(variable) ?? Empty) }
        if name == "isarray" { return Ok(result(m, Numeric(if is_array { 1.0 } else { 0.0 }))) }
        return Ok(result(m, String(kind)))
      }
      Index(variable, keys) => {
        let located = key(m, keys)?
        m = located.machine
        let array = array_ref(m, variable)?
        let found = array_find(m, array, located.key)
        if name == "isarray" { return Ok(result(m, Numeric(0.0))) }
        return Ok(result(m, String(if found == null { "unassigned" } else { type_name(found ?? Empty) })))
      }
      _ => {}
    }
  }
  if name in ["asort", "asorti"] {
    if args.len() < 1 or args.len() > 3 { return Err(fatal(m, f"{name}: wrong number of arguments")) }
    let source = match m.expressions[args[0]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, f"{name}: first argument is not an array")) } }
    var destination = source
    if args.len() >= 2 {
      destination = match m.expressions[args[1]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, f"{name}: second argument is not an array")) } }
    }
    var how = if name == "asorti" { "@ind_str_asc" } else { "@val_type_asc" }
    if args.len() == 3 {
      m = evaluate(m, args[2])?
      how = scalar_text(m.value, conversion_format(m)?)?
    }
    let table = table_get(m, source)
    var items: List[SortItem] = []
    for subscript in table.cells.keys() {
      let held = (table.cells.get(subscript) ?? Cell(value: Empty, seq: 0)).value
      let shown = if name == "asorti" { input(subscript)? } else { held }
      items += [sort_item(shown, subscript, how)?]
    }
    let ordered = merge_sort(items, how.ends_with("_desc"))
    m = array_clear(m, destination)
    for at in range(ordered.len()) {
      let item = ordered[at]
      m = array_set(m, destination, f"{at + 1}", if name == "asorti" { String(item.text) } else { item.value })
    }
    return Ok(result(m, Numeric(ordered.len().float())))
  }
  if name == "gensub" {
    if args.len() < 3 or args.len() > 4 { return Err(fatal(m, "gensub: wrong number of arguments")) }
    let given = regex_source(m, args[0])?
    m = given.machine
    var rest: List[Scalar] = []
    for child in args[1..] {
      m = evaluate(m, child)?
      if m.flow != Normal { return Ok(m) }
      rest += [m.value]
    }
    let conversion = conversion_format(m)?
    var target = m.record
    if rest.len() == 3 { target = scalar_text(rest[2], conversion)? }
    let replaced = gensub(m, given.key, scalar_text(rest[0], conversion)?, rest[1], target)?
    return Ok(result(m, String(replaced)))
  }
  if name == "patsplit" {
    if args.len() < 2 or args.len() > 4 { return Err(fatal(m, "patsplit: wrong number of arguments")) }
    m = evaluate(m, args[0])?
    let content = scalar_text(m.value, conversion_format(m)?)?
    let array = match m.expressions[args[1]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, "patsplit: second argument is not an array")) } }
    var pattern = ""
    if args.len() >= 3 {
      let given = regex_source(m, args[2])?
      m = given.machine
      pattern = given.key
    } else { pattern = variable_text(m, "FPAT")? }
    if pattern.is_empty() { pattern = "[^[:space:]]+" }
    var seps_array = ""
    if args.len() == 4 { seps_array = match m.expressions[args[3]] { Variable(variable) => array_ref(m, variable)?; _ => { return Err(fatal(m, "patsplit: fourth argument is not an array")) } } }
    m = array_clear(m, array)
    if ! seps_array.is_empty() { m = array_clear(m, seps_array) }
    var count = 0
    var cursor = 0
    for hit in regexp_matches(m, pattern, content)? {
      if hit.end == hit.start { continue }
      count += 1
      if ! seps_array.is_empty() { m = array_set(m, seps_array, f"{count - 1}", String(content.byte_slice(cursor, length: hit.start - cursor))) }
      m = array_set(m, array, f"{count}", input(content.byte_slice(hit.start, length: hit.end - hit.start))?)
      cursor = hit.end
    }
    if ! seps_array.is_empty() { m = array_set(m, seps_array, f"{count}", String(content.byte_slice(cursor))) }
    return Ok(result(m, Numeric(count.float())))
  }
  var values: List[Scalar] = []
  for child in args {
    m = evaluate(m, child)?
    if m.flow != Normal { return Ok(m) }
    values += [m.value]
  }
  let conversion = conversion_format(m)?
  var value: Scalar = Empty
  if name == "length" {
    if args.len() > 1 { return Err(fatal(m, "length: wrong number of arguments")) }
    value = Numeric((if values.is_empty() { m.record } else { scalar_text(values[0], conversion)? }).count_chars().float())
  } else if name == "substr" {
    if values.len() < 2 or values.len() > 3 { return Err(fatal(m, "substr: wrong number of arguments")) }
    let content = scalar_text(values[0], conversion)?
    let pieces = content.split("")
    let given = truncate(number(values[1])?)
    let size = pieces.len()
    let start = if is_nan(given) or given < 1.0 { 0 } else if given > size.float() { size } else { integer(given - 1.0)? }
    var length = size
    if values.len() == 3 {
      let wanted = truncate(number(values[2])?)
      length = if is_nan(wanted) or wanted < 0.0 { 0 } else if wanted > size.float() { size } else { integer(wanted)? }
    }
    let end = if start + length > size { size } else { start + length }
    value = String(pieces[start..end].join(""))
  } else if name == "index" {
    if values.len() != 2 { return Err(fatal(m, "index: wrong number of arguments")) }
    let content = scalar_text(values[0], conversion)?
    let found = content.find(scalar_text(values[1], conversion)?)
    value = Numeric(if found == null { 0.0 } else { (content.byte_slice(0, length: found ?? 0).count_chars() + 1).float() })
  } else if name == "tolower" or name == "toupper" {
    if values.len() != 1 { return Err(fatal(m, f"{name}: wrong number of arguments")) }
    let text = scalar_text(values[0], conversion)?
    value = String(if name == "tolower" { lower_text(text) } else { upper_text(text) })
  } else if name == "sprintf" {
    if values.is_empty() { return Err(fatal(m, "sprintf: no arguments")) }
    match printf(scalar_text(values[0], conversion)?, values[1..], conversion) {
      Ok(rendered) => { value = String(rendered) }
      Err(failure) => { return Err(located(m, failure)) }
    }
  } else if name in ["int", "sqrt", "exp", "log", "sin", "cos"] {
    if values.len() != 1 { return Err(fatal(m, f"{name}: wrong number of arguments")) }
    let n = number(values[0])?
    var computed = 0.0
    if name == "int" { computed = truncate(n) } else if name == "sqrt" {
      if n < 0.0 { warn(m, f"sqrt: received negative argument {float_text(n, conversion)?}")? }
      computed = n.sqrt()
    } else if name == "log" {
      if n < 0.0 { warn(m, f"log: received negative argument {float_text(n, conversion)?}")? }
      computed = n.ln()
    } else if name == "exp" { computed = n.exp() } else if name == "sin" { computed = n.sin() } else { computed = n.cos() }
    value = Numeric(computed)
  } else if name == "atan2" {
    if values.len() != 2 { return Err(fatal(m, "atan2: wrong number of arguments")) }
    value = Numeric(number(values[0])?.atan2(number(values[1])?))
  } else if name in ["and", "or", "xor"] {
    if values.len() < 2 { return Err(fatal(m, f"fatal: {name}: called with less than two arguments")) }
    var accumulator = located_result(m, to_bit_operand(name, 1, number(values[0])?))?
    for at in range(1, values.len()) {
      let operand = located_result(m, to_bit_operand(name, at + 1, number(values[at])?))?
      accumulator = if name == "and" and false { accumulator } else { bit_step(name, accumulator, operand) }
    }
    value = Numeric(accumulator)
  } else if name in ["lshift", "rshift"] {
    if values.len() != 2 { return Err(fatal(m, f"{name}: wrong number of arguments")) }
    let a = located_result(m, to_bit_operand(name, 1, number(values[0])?))?
    let b = located_result(m, to_bit_operand(name, 2, number(values[1])?))?
    value = Numeric(if name == "lshift" { a * 2.0.pow(b) } else { truncate(a / 2.0.pow(b)) })
  } else if name == "compl" {
    if values.len() != 1 { return Err(fatal(m, "compl: wrong number of arguments")) }
    let a = located_result(m, to_bit_operand(name, 1, number(values[0])?))?
    value = Numeric(9007199254740991.0 - a)
  } else if name == "strtonum" {
    if values.len() != 1 { return Err(fatal(m, "strtonum: wrong number of arguments")) }
    value = Numeric(strtonum(scalar_text(values[0], conversion)?)?)
  } else if name == "systime" {
    value = Numeric((time.now() / 1000).float())
  } else if name == "strftime" {
    var format = "%a %b %e %H:%M:%S %Z %Y"
    if values.len() >= 1 { format = scalar_text(values[0], conversion)? }
    var stamp = time.now() / 1000
    if values.len() >= 2 { stamp = integer(truncate(number(values[1])?))? }
    var utc = false
    if values.len() >= 3 { utc = truth(values[2])? }
    value = String(time.format(stamp * 1000000000, format, utc)?)
  } else if name == "mktime" {
    if values.len() != 1 { return Err(fatal(m, "mktime: wrong number of arguments")) }
    value = Numeric(make_time(scalar_text(values[0], conversion)?)?.float())
  } else if name == "rand" {
    m = random_next(m)
    let high = cold(m).state.float()
    m = random_next(m)
    let low = cold(m).state.float()
    var draw = 0.5 + (high / 2147483648.0 + low) / 2147483648.0
    draw = draw - 0.5
    if draw == 1.0 { draw = 0.0 }
    value = Numeric(draw)
  } else if name == "srand" {
    var state = cold(m)
    let previous = state.seed
    var seed = 0.0
    if values.is_empty() { seed = 0.0 } else { seed = number(values[0])? }
    state.previous_seed = previous
    state.seed = seed
    m = with_cold(m, state)
    m = random_seed(m, integer(truncate(seed))?)
    value = Numeric(previous)
  } else if name == "system" {
    if values.len() != 1 { return Err(fatal(m, "system: wrong number of arguments")) }
    let command = scalar_text(values[0], conversion)?
    m = flush_all(m)?
    let status = run.status sh -c ${environment_prefix(m) + command}
    value = Numeric(status_code(status).float())
  } else if name == "close" {
    if values.is_empty() or values.len() > 2 { return Err(fatal(m, "close: wrong number of arguments")) }
    let target = scalar_text(values[0], conversion)?
    let closed = close_stream(m, target)?
    m = closed.machine
    value = Numeric(closed.code.float())
  } else if name == "fflush" {
    if values.len() > 1 { return Err(fatal(m, "fflush: wrong number of arguments")) }
    if values.is_empty() or (scalar_text(values[0], conversion)?).is_empty() {
      m = flush_all(m)?
      value = Numeric(0.0)
    } else {
      let target = scalar_text(values[0], conversion)?
      let writer_keys = [key for key in m.writers.keys() if key == f">{target}" or key == f"|{target}" or key == f">>{target}"]
      if writer_keys.is_empty() and target not in ["/dev/stdout", "/dev/stderr", "-"] {
        warn(m, f"fflush: `{target}' is not an open file, pipe or co-process")?
        value = Numeric(-1.0)
      } else {
        for key in writer_keys { m = flush_writer(m, key)? }
        if target in ["/dev/stdout", "-"] { m = flush_stdout(m)? }
        value = Numeric(0.0)
      }
    }
  } else if name == "typeof" {
    value = String(if values.len() != 1 { "unknown" } else { match values[0] { Empty => "untyped"; Numeric(_) => "number"; Nan(_) => "number"; String(_) => "string"; Input(_, n) => if n != null { "strnum" } else { "string" } } })
  } else if name == "isarray" {
    value = Numeric(0.0)
  } else {
    return Err(fatal(m, f"function `{name}' not defined"))
  }
  Ok(result(m, value))
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

# ---- gawk extensions -------------------------------------------------------

pure type_name(value: Scalar) -> Str {
  match value {
    Empty => "unassigned"
    Numeric(_) => "number"
    Nan(_) => "number"
    String(_) => "string"
    Input(_, n) => if n != null { "strnum" } else { "string" }
  }
}
type SortItem = {rank: Int, number: Float, text: Str, value: Scalar}
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
proc gensub(m: Machine, pattern: Str, replacement: Str, how: Scalar, target: Str) -> Result[Str] {
  let conversion = conversion_format(m)?
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
  for hit in regexp_matches(m, pattern, target)? {
    if hit.start == hit.end and previous_end == hit.start { continue }
    counted += 1
    previous_end = hit.end
    if ! global and counted != which { continue }
    let matched = target.byte_slice(hit.start, length: hit.end - hit.start)
    var groups: List[Str] = [matched]
    if "\\" in replacement {
      let caps = rx_captures(pattern, matched, fold_case(m))?
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

# ---- Closing streams ------------------------------------------------------

type Closed = {machine: Machine, code: Int}
# close(name): finishes a reader or writer with that name and returns its
# status: 0 for files, the command's exit status for commands, -1 when
# nothing by that name is open.
proc close_stream(machine: Machine, target: Str) -> Result[Closed] {
  var m = machine
  var code = -1
  var found = false
  for key in [f">{target}", f">>{target}", f"|{target}"] {
    if key not in m.writers { continue }
    m = flush_writer(m, key)?
    let writer = m.writers.get(key) ?? no_writer()
    m.writers = m.writers.remove(key)
    found = true
    if writer.fd >= 0 { unix.close_fd(writer.fd)? }
    code = 0
    if writer.command {
      m = flush_stdout(m)?
      if let handle = writer.job {
        let status = wait handle ?
        code = status_code(status)
      }
    }
  }
  for key in [f"<{target}", f"|{target}"] {
    if key not in m.readers { continue }
    let reader = m.readers.get(key) ?? no_reader()
    m.readers = m.readers.remove(key)
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
  Ok({machine: m, code: if found { code } else { -1 }})
}
# Everything still open is closed at exit: files and commands first (waiting
# for the commands), then standard output is flushed.
proc close_everything(machine: Machine) -> Result[Machine] {
  var m = machine
  for key in m.writers.keys() { m = flush_writer(m, key)? }
  for key in m.writers.keys() {
    let writer = m.writers.get(key) ?? no_writer()
    if writer.fd >= 0 { unix.close_fd(writer.fd)? }
    if writer.command {
      if let handle = writer.job { let _ = wait handle ? }
    }
  }
  m.writers = {}
  for key in m.readers.keys() {
    let reader = m.readers.get(key) ?? no_reader()
    if reader.fd > 0 { unix.close_fd(reader.fd)? }
    if reader.command {
      if let handle = reader.job { let _ = wait handle ? }
    }
  }
  m.readers = {}
  flush_stdout(m)
}

# ---- Statements -------------------------------------------------------------

proc execute_statement(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  if m.flow != Normal { return Ok(m) }
  m.site = m.sites[id]
  execute_inner(m, id)
}
proc write_stdout_text(machine: Machine, text: Str) -> Result[Machine] {
  match io.write_stdout(text) {
    Ok(_) => {}
    Err(failure) => { return Err(fatal(machine, f"print to \"standard output\" failed ({reason_of(failure)})")) }
  }
  if cold(machine).interactive { return flush_stdout(machine) }
  Ok(machine)
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
proc print_text(machine: Machine, text: Str, target: Int?, mode: Str) -> Result[Machine] {
  var m = machine
  if target == null {
    m = write_stdout_text(m, text)?
    return Ok(m)
  }
  m = evaluate(m, target ?? 0)?
  if m.flow != Normal { return Ok(m) }
  let destination = scalar_text(m.value, conversion_format(m)?)?
  if destination.is_empty() { return Err(fatal(m, f"expression for `{mode}' redirection has null string value")) }
  if mode != "|" and destination in ["/dev/stdout", "/dev/fd/1"] {
    m = write_stdout_text(m, text)?
    return Ok(m)
  }
  if mode != "|" and destination in ["/dev/stderr", "/dev/fd/2"] {
    write_all(2, bytes.from_text(text))?
    return Ok(m)
  }
  let key = f"{mode}{destination}"
  if key not in m.writers {
    if mode == "|" {
      m = open_command_writer(m, destination)?
    } else {
      m = flush_all(m)?
      let target = fp"{destination}"
      match open_output(target, mode == ">>") {
        Ok(fd) => { m.writers = m.writers.set(key, Writer(fd: fd, pending: "", job: null, name: destination, command: false, fifo: null)) }
        Err(failure) => { return Err(fatal(m, f"cannot redirect to `{destination}': {reason_of(failure)}")) }
      }
    }
  }
  var writer = m.writers.get(key) ?? no_writer()
  writer.pending += text
  m.writers = m.writers.set(key, writer)
  Ok(m)
}
proc execute_inner(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  match m.statements[id] {
    Block(body) => {
      for child in body { m = execute_statement(m, child)?; if m.flow != Normal { return Ok(m) } }
    }
    ExpressionStatement(child) => { m = evaluate(m, child)? }
    Print(args, formatted_output, target, mode) => {
      var values: List[Scalar] = []
      for child in args { m = evaluate(m, child)?; if m.flow != Normal { return Ok(m) }; values += [m.value] }
      let conversion = conversion_format(m)?
      var text = ""
      if formatted_output {
        if values.is_empty() { return Err(fatal(m, "printf: no arguments")) }
        match printf(scalar_text(values[0], conversion)?, values[1..], conversion) {
          Ok(rendered) => { text = rendered }
          Err(failure) => { return Err(located(m, failure)) }
        }
      } else {
        if values.is_empty() { text = m.record } else {
          let output = output_format(m)?
          var pieces: List[Str] = []
          for value in values { pieces += [scalar_text(value, output)?] }
          text = pieces.join(variable_text(m, "OFS")?)
        }
        text += variable_text(m, "ORS")?
      }
      m = print_text(m, text, target, mode)?
    }
    If(condition, yes, no) => {
      m = evaluate(m, condition)?
      if m.flow != Normal { return Ok(m) }
      if truth(m.value)? { m = execute_statement(m, yes)? } else if no != null { m = execute_statement(m, no ?? 0)? }
    }
    While(condition, body) => {
      loop {
        m = evaluate(m, condition)?
        if m.flow != Normal or ! truth(m.value)? { break }
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
      }
    }
    Do(body, condition) => {
      loop {
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
        m = evaluate(m, condition)?
        if m.flow != Normal or ! truth(m.value)? { break }
      }
    }
    For(initial, condition, step, body) => {
      if initial != null { m = evaluate(m, initial ?? 0)? }
      loop {
        if m.flow != Normal { break }
        if condition != null { m = evaluate(m, condition ?? 0)?; if m.flow != Normal or ! truth(m.value)? { break } }
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
        if step != null { m = evaluate(m, step ?? 0)? }
      }
    }
    Each(variable, array, body) => {
      let resolved = array_ref(m, array)?
      # The loop runs over a snapshot of the subscripts, as GNU awk does.
      for member in array_keys(m, resolved) {
        m = assign_variable(m, variable, input_or_string(member)?)?
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
      }
    }
    Delete(array, keys) => {
      let resolved = array_ref(m, array)?
      if keys != null {
        let found = key(m, keys ?? [])?
        m = found.machine
        if m.flow != Normal { return Ok(m) }
        m = array_remove(m, resolved, found.key)
      } else {
        m = array_clear(m, resolved)
      }
    }
    Next => {
      if cold(m).running_begin { return Err(fatal(m, "`next' cannot be called from a `BEGIN' rule")) }
      if cold(m).running_end { return Err(fatal(m, "`next' cannot be called from an `END' rule")) }
      m.flow = NextRecord
    }
    NextFile => {
      if cold(m).running_begin { return Err(fatal(m, "`nextfile' cannot be called from a `BEGIN' rule")) }
      if cold(m).running_end { return Err(fatal(m, "`nextfile' cannot be called from an `END' rule")) }
      m.flow = SkipFile
    }
    Break => { m.flow = BreakLoop }
    Continue => { m.flow = ContinueLoop }
    Exit(child) => {
      if child != null { m = evaluate(m, child ?? 0)?; if m.flow != Normal { return Ok(m) }; m.status = integer(number(m.value)?)? }
      m.flow = ExitProgram
    }
    Return(child) => {
      if child != null { m = evaluate(m, child ?? 0)?; if m.flow != Normal { return Ok(m) } } else { m.value = Empty }
      m.flow = ReturnFunction
    }
  }
  Ok(m)
}
# For-in loop variables hold the subscript as a string that still compares as
# a number when it looks like one.
pure input_or_string(member: Str) -> Result[Scalar] { Ok(String(member)) }
pure located(m: Machine, failure: Error) -> Error {
  AwkError.Fatal(f"{m.site}: {context_text(m)}{failure.message}")
}

# ---- Program execution ---------------------------------------------------

pure new_machine(program: Program, interactive: Bool, sandbox: Bool) -> Machine {
  let values: Map[Scalar] = {"FS": String(" "), "OFS": String(" "), "RS": String("\n"), "RT": String(""), "ORS": String("\n"), "SUBSEP": String("\u{1c}"), "OFMT": String("%.6g"), "CONVFMT": String("%.6g"), "NR": Numeric(0.0), "FNR": Numeric(0.0), "NF": Numeric(0.0), "RSTART": Numeric(0.0), "RLENGTH": Numeric(0.0), "FILENAME": String("")}
  let state = Cold(
    calls: 0, main: MainInput(index: 1, reader: null, opened: false, finished: false, stdin_used: false), interactive: interactive, state: 0,
    generator: [], front: 1, rear: 0, seed: 1.0, previous_seed: 0.0, temp_root: null, temp_path: null, temp_count: 0,
    running_end: false, running_begin: false, sandbox: sandbox,
  )
  var m = Machine(
    expressions: program.expressions, statements: program.statements, sites: program.site, functions: program.functions,
    values: values, arrays: {}, scopes: [], fields: [], record: "", status: 0, flow: Normal, value: Empty, depth: 0, site: "cmd. line:1",
    readers: {}, writers: {}, cold: [state],
  )
  m = random_seed(m, 1)
  m
}

type RuleRun = {machine: Machine, ranges: List[Bool]}
# Runs every rule whose pattern matches the current record.
proc run_rules(machine: Machine, rules: List[Rule], ranges: List[Bool]) -> Result[RuleRun] {
  var m = machine
  var active = ranges
  for at in range(rules.len()) {
    let rule = rules[at]
    m.site = rule.site
    var matched = active[at]
    if ! matched {
      if rule.pattern == null { matched = true } else {
        m = evaluate(m, rule.pattern ?? 0)?
        if m.flow != Normal { break }
        matched = truth(m.value)?
      }
    }
    if ! matched { continue }
    if rule.end != null {
      m = evaluate(m, rule.end ?? 0)?
      if m.flow != Normal { break }
      active[at] = ! truth(m.value)?
    }
    m = execute_statement(m, rule.action)?
    if m.flow in [NextRecord, SkipFile, ExitProgram] { break }
    if m.flow != Normal { return Err(fatal(m, "invalid control flow at top level")) }
  }
  Ok({machine: m, ranges: active})
}

# Runs a parsed program over ARGV-selected input. Returns the exit status.
proc run_program(program: Program, arguments: List[Str], variables: List[Str], interactive: Bool, sandbox: Bool) -> Result[Int] {
  var m = new_machine(program, interactive, sandbox)
  var argv: Map[Cell] = {"0": Cell(value: String("awk"), seq: 0)}
  for at in range(arguments.len()) { argv = argv.set(f"{at + 1}", Cell(value: input(arguments[at])?, seq: at + 1)) }
  m.arrays = m.arrays.set("ARGV", Table(cells: argv, next: arguments.len() + 1))
  m.values = m.values.set("ARGC", Numeric((arguments.len() + 1).float()))
  if program.environ {
    var environment: Map[Cell] = {}
    var count = 0
    for entry in env.list()? {
      environment = environment.set(entry.name, Cell(value: input(entry.value)?, seq: count))
      count += 1
    }
    m.arrays = m.arrays.set("ENVIRON", Table(cells: environment, next: count))
  }
  for item in variables { m = apply_binding(m, item)? }
  var state = cold(m)
  state.running_begin = true
  m = with_cold(m, state)
  for id in program.begin {
    m = execute_statement(m, id)?
    if m.flow == ExitProgram { break }
    if m.flow != Normal { return Err(fatal(m, "invalid control flow in BEGIN")) }
  }
  state = cold(m)
  state.running_begin = false
  m = with_cold(m, state)
  var ranges = [false for _ in program.rules]
  if m.flow != ExitProgram and (! program.rules.is_empty() or ! program.end.is_empty()) {
    loop {
      let got = next_main_record(m)?
      m = got.machine
      if got.record == null { break }
      m = bump_counters(m, false)?
      m = store_record(m, got.record ?? "", got.terminator, null)?
      let ran = run_rules(m, program.rules, ranges)?
      m = ran.machine
      ranges = ran.ranges
      if m.flow == ExitProgram { break }
      if m.flow == SkipFile {
        var main_state = cold(m)
        let reader = main_state.main.reader
        if reader != null and (reader ?? no_reader()).fd > 0 {
          unix.close_fd((reader ?? no_reader()).fd)?
        }
        main_state.main.reader = null
        m = with_cold(m, main_state)
      }
      m.flow = Normal
    }
  }
  m.flow = Normal
  state = cold(m)
  state.running_end = true
  m = with_cold(m, state)
  for id in program.end {
    m = execute_statement(m, id)?
    if m.flow == ExitProgram { break }
    if m.flow != Normal { return Err(fatal(m, "invalid control flow in END")) }
  }
  m = close_everything(m)?
  Ok((m.status % 256 + 256) % 256)
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
  let result = run_program(program, arguments, variables, unix.isatty(1), sandbox)
  result
}
