##! Indexed AWK syntax and evaluation implemented in XSH.

error AwkError = Syntax : Usage | Unsupported : InvalidArgument | Runtime : InvalidArgument

enum Token { TokWord(Str), TokNumber(Float), TokText(Str), TokRegex(Str), TokOp(Str), TokEnd }
enum Expression {
  Number(Float), Text(Str), RegexLiteral(Str), Variable(Str), Field(Int), Index(Str, List[Int]), Call(Str, List[Int]),
  Unary(Str, Int), Binary(Str, Int, Int), Assign(Str, Int, Int), Increment(Int, Float, Bool), Conditional(Int, Int, Int),
}
enum Statement {
  Block(List[Int]), ExpressionStatement(Int), Print(List[Int], Bool), If(Int, Int, Int?), While(Int, Int), Do(Int, Int),
  For(Int?, Int?, Int?, Int), Each(Str, Str, Int), Delete(Str, List[Int]?), Next, Break, Continue, Exit(Int?), Return(Int?),
}
type Rule = {pattern: Int?, end: Int?, action: Int}
type Function = {parameters: List[Str], body: Int}
# Child ids address immutable expression and statement arenas, so recursive syntax
# never requires recursive XSH record types.
type Program = {expressions: List[Expression], statements: List[Statement], begin: List[Int], end: List[Int], rules: List[Rule], functions: Map[Function]}
type Parser = {tokens: List[Token], at: Int, program: Program, printing: Bool, depth: Int}
type Parsed[T] = {parser: Parser, value: T}
type Escaped = {content: Str, at: Int}

pure syntax(message: Str) -> Error { AwkError.Syntax(f"syntax: {message}") }
pure runtime(message: Str) -> Error { AwkError.Runtime(f"runtime: {message}") }
pure unsupported(message: Str) -> Error { AwkError.Unsupported(f"unsupported: {message}") }
pure digit(byte: Int) -> Bool { byte >= 48 and byte <= 57 }
pure letter(byte: Int) -> Bool { byte == 95 or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) }

pure escaped(source: Bytes, start: Int, quote: Int) -> Result[Escaped] {
  var at = start
  var out: List[Int] = []
  while at < source.len() {
    let byte = source.byte_at(at) ?? 0
    at += 1
    if byte == quote and quote >= 0 { return Ok({content: bytes.from_ints(out)?.utf8()?, at: at}) }
    if byte != 92 { out += [byte]; continue }
    if at >= source.len() { return Err(syntax("unterminated escape")) }
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
      out += [number]
    } else if next == 10 { continue } else if next == 110 { out += [10] } else if next == 116 { out += [9] } else if next == 114 { out += [13] } else if next == 98 { out += [8] } else if next == 102 { out += [12] } else if next == 118 { out += [11] } else if next == 97 { out += [7] } else if quote == 47 and next != 47 { out += [92, next] } else { out += [next] }
  }
  if quote < 0 { Ok({content: bytes.from_ints(out)?.utf8()?, at: at}) } else { Err(syntax("unterminated string or regex")) }
}

pure lex(content: Str) -> Result[List[Token]] {
  let source = bytes.from_text(content)
  var tokens: List[Token] = []
  var at = 0
  var operand = true
  var parentheses = 0
  var brackets = 0
  while at < source.len() {
    let byte = source.byte_at(at) ?? 0
    if byte == 92 and source.byte_at(at + 1) == 10 { at += 2; continue }
    if byte == 35 { while at < source.len() and source.byte_at(at) != 10 { at += 1 }; continue }
    if byte == 32 or byte == 9 or byte == 13 or (byte == 10 and (parentheses > 0 or brackets > 0)) { at += 1; continue }
    if byte == 34 or (byte == 47 and operand and source.byte_at(at + 1) != 61) {
      let found = escaped(source, at + 1, byte)?
      tokens += [if byte == 47 { TokRegex(found.content) } else { TokText(found.content) }]
      at = found.at
      operand = false
      continue
    }
    if digit(byte) or (byte == 46 and digit(source.byte_at(at + 1) ?? 0)) {
      let start = at
      at += 1
      while digit(source.byte_at(at) ?? 0) or source.byte_at(at) == 46 { at += 1 }
      if source.byte_at(at) == 101 or source.byte_at(at) == 69 {
        at += 1
        if source.byte_at(at) == 43 or source.byte_at(at) == 45 { at += 1 }
        while digit(source.byte_at(at) ?? 0) { at += 1 }
      }
      tokens += [TokNumber(source[start..at].utf8()?.parse_float()?)]
      operand = false
      continue
    }
    if letter(byte) {
      let start = at
      at += 1
      while letter(source.byte_at(at) ?? 0) or digit(source.byte_at(at) ?? 0) { at += 1 }
      let word = source[start..at].utf8()?
      tokens += [TokWord(word)]
      operand = word in ["print", "printf", "return", "exit", "in", "delete"]
      continue
    }
    let pair = source.slice(at, length: if at + 1 < source.len() { 2 } else { 1 }).utf8()?
    var op = pair
    if pair in ["++", "--", "==", "!=", "<=", ">=", "&&", "||", "!~", "+=", "-=", "*=", "/=", "%=", "^="] { at += 2 } else {
      op = source[at..at + 1].utf8()?
      if op not in ["{", "}", "(", ")", "[", "]", ",", "$", ";", "\n", "?", ":", "+", "-", "*", "/", "%", "^", "!", "<", ">", "=", "~", "|"] { return Err(syntax(f"unexpected character {op}")) }
      at += 1
    }
    if op == "(" { parentheses += 1 }
    if op == ")" and parentheses > 0 { parentheses -= 1 }
    if op == "[" { brackets += 1 }
    if op == "]" and brackets > 0 { brackets -= 1 }
    operand = op not in [")", "]", "++", "--"]
    tokens += [TokOp(op)]
  }
  Ok(tokens + [TokEnd])
}

pure spelling(token: Token) -> Str { match token { TokWord(word) => word; TokOp(op) => op; _ => "" } }
pure peek(p: Parser) -> Token { p.tokens[p.at] }
pure is_at(p: Parser, content: Str) -> Bool { spelling(peek(p)) == content }
pure advance(parser: Parser) -> Parser { var p = parser; p.at += 1; p }
pure require(parser: Parser, content: Str) -> Result[Parser] {
  if is_at(parser, content) { Ok(advance(parser)) } else { Err(syntax(f"expected {content}")) }
}
pure separators(parser: Parser) -> Parser {
  var p = parser
  while is_at(p, ";") or is_at(p, "\n") { p.at += 1 }
  p
}
pure name(parser: Parser) -> Result[Parsed[Str]] {
  match peek(parser) { TokWord(word) => Ok({parser: advance(parser), value: word}); _ => Err(syntax("expected identifier")) }
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
  {parser: p, value: id}
}
pure terminated(p: Parser) -> Bool { peek(p) == TokEnd or spelling(peek(p)) in [";", "\n", "}"] }
pure expression_list(parser: Parser, end: Str) -> Result[Parsed[List[Int]]] {
  var p = parser
  var ids: List[Int] = []
  if is_at(p, end) { return Ok({parser: advance(p), value: ids}) }
  loop {
    let parsed = expression(p, 0)?
    p = parsed.parser
    ids += [parsed.value]
    if is_at(p, end) { return Ok({parser: advance(p), value: ids}) }
    p = require(p, ",")?
  }
  Err(syntax("unterminated expression list"))
}

const PRECEDENCE: Map[Int] = {"=": 1, "+=": 1, "-=": 1, "*=": 1, "/=": 1, "%=": 1, "^=": 1, "||": 3, "&&": 4, "in": 5, "~": 6, "!~": 6, "==": 7, "!=": 7, "<": 7, ">": 7, "<=": 7, ">=": 7, "+": 9, "-": 9, "*": 10, "/": 10, "%": 10, "^": 12}

pure expression(parser: Parser, minimum: Int) -> Result[Parsed[Int]] {
  var p = parser
  if p.depth >= 128 { return Err(syntax("expression nesting limit exceeded")) }
  p.depth += 1
  let token = peek(p)
  p = advance(p)
  var first: Parsed[Int] = {parser: p, value: 0}
  match token {
    TokNumber(number) => { first = expression_node(p, Number(number)) }
    TokText(content) => { first = expression_node(p, Text(content)) }
    TokRegex(pattern) => { let _ = regex.find_bytes(pattern, b"", extended: true)?; first = expression_node(p, RegexLiteral(pattern)) }
    TokWord(word) => {
      if word in ["getline", "system", "close", "fflush"] { return Err(unsupported(word)) }
      if is_at(p, "(") {
        let args = expression_list(advance(p), ")")?
        first = expression_node(args.parser, Call(word, args.value))
      } else if is_at(p, "[") {
        let args = expression_list(advance(p), "]")?
        first = expression_node(args.parser, Index(word, args.value))
      } else { first = expression_node(p, Variable(word)) }
    }
    TokOp(op) => {
      if op == "(" {
        let printing = p.printing
        p.printing = false
        first = expression(p, 0)?
        first.parser = require(first.parser, ")")?
        first.parser.printing = printing
      } else if op == "$" {
        let child = expression(p, 14)?
        first = expression_node(child.parser, Field(child.value))
      } else if op in ["!", "+", "-"] {
        let child = expression(p, 11)?
        first = expression_node(child.parser, Unary(op, child.value))
      } else if op in ["++", "--"] {
        let child = expression(p, 13)?
        first = expression_node(child.parser, Increment(child.value, if op == "++" { 1.0 } else { -1.0 }, false))
      } else { return Err(syntax("expected expression")) }
    }
    _ => { return Err(syntax("expected expression")) }
  }
  p = first.parser
  var left = first.value
  var operators = 0
  loop {
    let op = spelling(peek(p))
    if minimum <= 13 and op in ["++", "--"] {
      let next = expression_node(advance(p), Increment(left, if op == "++" { 1.0 } else { -1.0 }, true))
      p = next.parser
      left = next.value
      continue
    }
    if minimum <= 2 and op == "?" {
      let yes = expression(advance(p), 0)?
      let no = expression(require(yes.parser, ":")?, 2)?
      let next = expression_node(no.parser, Conditional(left, yes.value, no.value))
      p = next.parser
      left = next.value
      continue
    }
    if p.printing and op == ">" { break }
    let known = PRECEDENCE.get(op) ?? 0
    let concat = known == 0 and (match peek(p) { TokNumber(_) => true; TokText(_) => true; TokRegex(_) => true; TokWord(word) => word not in ["else", "in", "while"]; TokOp(symbol) => symbol in ["$", "("]; _ => false })
    let power = if concat { 8 } else { known }
    if power == 0 or power < minimum { break }
    operators += 1
    if operators > 128 { return Err(syntax("expression operator limit exceeded")) }
    let operator = if concat { "concat" } else { op }
    let right = expression(if concat { p } else { advance(p) }, if power == 1 or power == 12 { power } else { power + 1 })?
    let next = expression_node(right.parser, if power == 1 { Assign(operator, left, right.value) } else { Binary(operator, left, right.value) })
    p = next.parser
    left = next.value
  }
  p.depth -= 1
  Ok({parser: p, value: left})
}

pure block(parser: Parser) -> Result[Parsed[Int]] {
  var p = separators(require(parser, "{")?)
  var body: List[Int] = []
  while ! is_at(p, "}") {
    if peek(p) == TokEnd { return Err(syntax("unterminated action")) }
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
pure statement(parser: Parser) -> Result[Parsed[Int]] {
  var p = parser
  if p.depth >= 128 { return Err(syntax("statement nesting limit exceeded")) }
  p.depth += 1
  let next = statement_inner(p)?
  p = next.parser
  p.depth -= 1
  Ok({parser: p, value: next.value})
}
pure statement_inner(parser: Parser) -> Result[Parsed[Int]] {
  var p = parser
  while is_at(p, "\n") { p = advance(p) }
  if is_at(p, ";") { return Ok(statement_node(advance(p), Block([]))) }
  if is_at(p, "{") { return block(p) }
  let word = spelling(peek(p))
  if word == "if" {
    let condition = expression(require(advance(p), "(")?, 0)?
    let yes = statement(require(condition.parser, ")")?)?
    p = separators(yes.parser)
    var no: Int? = null
    if is_at(p, "else") { let branch = statement(advance(p))?; p = branch.parser; no = branch.value } else { p = yes.parser }
    return Ok(statement_node(p, If(condition.value, yes.value, no)))
  }
  if word == "while" {
    let condition = expression(require(advance(p), "(")?, 0)?
    let body = statement(require(condition.parser, ")")?)?
    return Ok(statement_node(body.parser, While(condition.value, body.value)))
  }
  if word == "do" {
    let body = statement(advance(p))?
    p = require(separators(body.parser), "while")?
    let condition = expression(require(p, "(")?, 0)?
    return Ok(statement_node(require(condition.parser, ")")?, Do(body.value, condition.value)))
  }
  if word == "for" {
    p = require(advance(p), "(")?
    if spelling(p.tokens.get(p.at + 1) ?? TokEnd) == "in" {
      let key = name(p)?
      let array = name(require(key.parser, "in")?)?
      let body = statement(require(array.parser, ")")?)?
      return Ok(statement_node(body.parser, Each(key.value, array.value, body.value)))
    }
    let initial = optional_expression(p, ";")?
    let condition = optional_expression(initial.parser, ";")?
    let step = optional_expression(condition.parser, ")")?
    let body = statement(step.parser)?
    return Ok(statement_node(body.parser, For(initial.value, condition.value, step.value, body.value)))
  }
  if word == "next" { return Ok(statement_node(advance(p), Next)) }
  if word == "break" { return Ok(statement_node(advance(p), Break)) }
  if word == "continue" { return Ok(statement_node(advance(p), Continue)) }
  if word in ["nextfile", "getline"] { return Err(unsupported(word)) }
  if word == "delete" {
    let target = name(advance(p))?
    p = target.parser
    var keys: List[Int]? = null
    if is_at(p, "[") { let next = expression_list(advance(p), "]")?; p = next.parser; keys = next.value }
    return Ok(statement_node(p, Delete(target.value, keys)))
  }
  if word in ["exit", "return"] {
    p = advance(p)
    var value: Int? = null
    if ! terminated(p) { let next = expression(p, 0)?; p = next.parser; value = next.value }
    return Ok(statement_node(p, if word == "exit" { Exit(value) } else { Return(value) }))
  }
  if word in ["print", "printf"] {
    p = advance(p)
    var args: List[Int] = []
    if word == "printf" and is_at(p, "(") {
      let next = expression_list(advance(p), ")")?
      p = next.parser
      args = next.value
    } else if ! terminated(p) {
      loop {
        p.printing = true
        let next = expression(p, 0)?
        p = next.parser
        p.printing = false
        args += [next.value]
        if ! is_at(p, ",") { break }
        p = advance(p)
      }
    }
    if is_at(p, "|") or is_at(p, ">") { return Err(unsupported("output redirection")) }
    return Ok(statement_node(p, Print(args, word == "printf")))
  }
  let next = expression(p, 0)?
  Ok(statement_node(next.parser, ExpressionStatement(next.value)))
}

pure parse(source: Str) -> Result[Program] {
  var p = Parser(tokens: lex(source)?, at: 0, program: Program(expressions: [], statements: [], begin: [], end: [], rules: [], functions: {}), printing: false, depth: 0)
  p = separators(p)
  while peek(p) != TokEnd {
    let word = spelling(peek(p))
    if word == "function" {
      let function_name = name(advance(p))?
      p = require(function_name.parser, "(")?
      var parameters: List[Str] = []
      if ! is_at(p, ")") {
        loop {
          let parameter = name(p)?
          p = parameter.parser
          parameters += [parameter.value]
          if is_at(p, ")") { break }
          p = require(p, ",")?
        }
      }
      let body = block(separators(require(p, ")")?))?
      p = body.parser
      if function_name.value in p.program.functions { return Err(syntax(f"duplicate function {function_name.value}")) }
      p.program.functions = p.program.functions.set(function_name.value, Function(parameters: parameters, body: body.value))
    } else if word in ["BEGIN", "END"] {
      let body = block(separators(advance(p)))?
      p = body.parser
      if word == "BEGIN" { p.program.begin += [body.value] } else { p.program.end += [body.value] }
    } else {
      var pattern: Int? = null
      var end: Int? = null
      if ! is_at(p, "{") { let next = expression(p, 0)?; p = next.parser; pattern = next.value }
      if is_at(p, ",") { let next = expression(advance(p), 0)?; p = next.parser; end = next.value }
      let action = if is_at(p, "{") { block(p)? } else { statement_node(p, Print([], false)) }
      p = action.parser
      p.program.rules += [Rule(pattern: pattern, end: end, action: action.value)]
    }
    p = separators(p)
  }
  Ok(p.program)
}

# Input preserves spelling and numeric classification; string literals keep string
# comparison and truth semantics even when their spelling is a decimal number.
enum Scalar { Empty, Numeric(Float), String(Str), Input(Str, Float?) }
enum Flow { Normal, NextRecord, BreakLoop, ContinueLoop, ExitProgram, ReturnFunction }
enum Place { Named(Str), FieldPlace(Int), Element(Str, Str) }
type Scope = {values: Map[Scalar], arrays: Map[Str]}
type Machine = {program: Program, values: Map[Scalar], arrays: Map[Map[Scalar]], scopes: List[Scope], fields: List[Scalar], record: Str, output: Str, status: Int, flow: Flow, value: Scalar, steps: Int, depth: Int}
type Located = {machine: Machine, place: Place}
type Keyed = {machine: Machine, key: Str}
type Format = {conversion: Str, width: Int, precision: Int?, left: Bool, plus: Bool, space: Bool, zero: Bool, alternate: Bool}
type ParsedFormat = {format: Format, at: Int, argument: Int}

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
pure input(content: Str) -> Result[Scalar] {
  let plain = trimmed(content)
  let end = decimal_end(plain)
  Ok(Input(content, if end != null and end == plain.byte_len() { plain.parse_float()? } else { null }))
}
pure numeric_prefix(content: Str) -> Result[Float] {
  let plain = trimmed(content)
  let end = decimal_end(plain)
  Ok(if end == null { 0.0 } else { plain.byte_slice(0, length: end).parse_float()? })
}
pure number(value: Scalar) -> Result[Float] {
  match value { Empty => Ok(0.0); Numeric(n) => Ok(n); String(content) => numeric_prefix(content); Input(content, n) => if n != null { Ok(n) } else { numeric_prefix(content) } }
}
pure numeric(value: Scalar) -> Bool { match value { Empty => true; Numeric(_) => true; Input(_, n) => n != null; _ => false } }
pure truth(value: Scalar) -> Result[Bool] {
  if numeric(value) { return Ok(number(value)? != 0.0) }
  Ok(match value { String(content) => ! content.is_empty(); Input(content, _) => ! content.is_empty(); _ => false })
}
pure integer(value: Float) -> Result[Int] { if value < 0.0 { value.ceil() } else { value.floor() } }
pure plain_text(value: Scalar) -> Result[Str] {
  match value {
    Empty => Ok("")
    String(content) => Ok(content)
    Input(content, _) => Ok(content)
    Numeric(n) => {
      if n >= -9223372036854775808.0 and n < 9223372036854775808.0 {
        let whole = integer(n)?
        if whole.float() == n { return Ok(f"{whole}") }
      }
      n.format_number("g", 6)
    }
  }
}
# Stars consume arguments before the value; numeric rendering receives only the
# conversion and precision after XSH has interpreted flags, width, and padding.
pure parse_format(format: Str, start: Int, values: List[Scalar], argument: Int) -> Result[ParsedFormat] {
  var at = start
  var arg = argument
  var spec = Format(conversion: "", width: 0, precision: null, left: false, plus: false, space: false, zero: false, alternate: false)
  while at < format.byte_len() {
    let flag = format.byte_slice(at, length: 1)
    if flag == "-" { spec.left = true } else if flag == "+" { spec.plus = true } else if flag == " " { spec.space = true } else if flag == "0" { spec.zero = true } else if flag == "#" { spec.alternate = true } else { break }
    at += 1
  }
  if format.byte_at(at) == 42 {
    if arg >= values.len() { return Err(runtime("not enough printf arguments")) }
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
      if arg >= values.len() { return Err(runtime("not enough printf arguments")) }
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
  if at >= format.byte_len() { return Err(runtime("unterminated printf conversion")) }
  spec.conversion = format.byte_slice(at, length: 1)
  if spec.conversion not in ["s", "c", "d", "i", "o", "u", "x", "X", "f", "F", "e", "E", "g", "G"] { return Err(unsupported(f"printf conversion %{spec.conversion}")) }
  if spec.width > 1000000 or (spec.precision ?? 0) > 1000000 { return Err(runtime("printf width or precision exceeds limit")) }
  Ok({format: spec, at: at + 1, argument: arg})
}
pure padding(char: Str, count: Int) -> Str { [char for _ in range(count)].join("") }
pure scalar_text(value: Scalar, conversion: Str) -> Result[Str] {
  if numeric(value) {
    match value {
      Numeric(n) => {
        if n >= -9223372036854775808.0 and n < 9223372036854775808.0 {
          let whole = integer(n)?
          if whole.float() == n { return Ok(f"{whole}") }
        }
        let parsed = parse_format(conversion, 1, [], 0)?
        if ! conversion.starts_with("%") or parsed.at != conversion.byte_len() or parsed.format.conversion in ["s", "c"] { return Err(runtime("invalid numeric conversion format")) }
        return formatted(value, parsed.format, "%.6g")
      }
      _ => {}
    }
  }
  plain_text(value)
}
pure formatted(value: Scalar, spec: Format, conversion: Str) -> Result[Str] {
  let code = spec.conversion
  var result = ""
  var prefix = ""
  if code == "s" {
    result = scalar_text(value, conversion)?
    if spec.precision != null and result.byte_len() > spec.precision { result = result.byte_slice(0, length: spec.precision) }
  } else if code == "c" {
    if numeric(value) { result = bytes.from_ints([integer(number(value)?)? % 256])?.utf8()? } else { let content = scalar_text(value, conversion)?; result = if content.is_empty() { "\0" } else { content.byte_slice(0, length: 1) } }
  } else {
    let n = number(value)?
    let signed = code in ["d", "i", "f", "F", "e", "E", "g", "G"]
    let precision = spec.precision ?? (if code in ["d", "i", "o", "u", "x", "X"] { 1 } else { 6 })
    result = n.format_number(code, precision)?
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
  let count = spec.width - prefix.byte_len() - result.byte_len()
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
    at += 1
    if format.byte_at(at) == 37 { out += "%"; at += 1; continue }
    let parsed = parse_format(format, at, values, argument)?
    argument = parsed.argument
    at = parsed.at
    if argument >= values.len() { return Err(runtime("not enough printf arguments")) }
    out += formatted(values[argument], parsed.format, conversion)?
    argument += 1
  }
  Ok(out)
}

type Match = {start: Int, end: Int}
pure find(pattern: Str, content: Str, offset = 0) -> Result[Match?] {
  let hits = regex.find_bytes(pattern, bytes.from_text(content), extended: true)?
  for hit in hits { if hit.start >= offset { return Ok(hit) } }
  Ok(null)
}
pure slice(content: Str, start: Int, end: Int) -> Result[Str] {
  match bytes.from_text(content)[start..end].utf8() {
    Ok(piece) => Ok(piece)
    Err(_) => Err(unsupported("POSIX regex match splits a UTF-8 scalar in the current locale"))
  }
}
pure split_fields(content: Str, separator: Str) -> Result[List[Scalar]] {
  if content.is_empty() { return Ok([]) }
  if separator == " " { return Ok([input(piece)? for piece in content.replace("\n", with: " ").replace("\t", with: " ").split(" ") if ! piece.is_empty()]) }
  if separator.is_empty() { return Ok([input(piece)? for piece in content.split("")]) }
  if separator.count_chars() == 1 { return Ok([input(piece)? for piece in content.split(separator)]) }
  var fields: List[Scalar] = []
  var start = 0
  for hit in regex.find_bytes(separator, bytes.from_text(content), extended: true)? {
    if hit.start == hit.end { continue }
    fields += [input(slice(content, start, hit.start)?)?]
    start = hit.end
  }
  Ok(fields + [input(slice(content, start, content.byte_len())?)?])
}
pure get(m: Machine, name: Str) -> Scalar {
  var at = m.scopes.len() - 1
  while at >= 0 {
    if name in m.scopes[at].values { return m.scopes[at].values.get(name) ?? Empty }
    at -= 1
  }
  m.values.get(name) ?? Empty
}
pure variable_text(m: Machine, name: Str) -> Result[Str] { scalar_text(get(m, name), plain_text(get(m, "CONVFMT"))?) }
pure array_name(m: Machine, name: Str) -> Str {
  var at = m.scopes.len() - 1
  while at >= 0 {
    if name in m.scopes[at].arrays { return m.scopes[at].arrays.get(name) ?? name }
    at -= 1
  }
  name
}
pure result(machine: Machine, value: Scalar) -> Machine { var m = machine; m.value = value; m }
pure rebuild(machine: Machine) -> Result[Machine] {
  var m = machine
  let conversion = plain_text(get(m, "CONVFMT"))?
  m.record = [scalar_text(value, conversion)? for value in m.fields].join(variable_text(m, "OFS")?)
  m.values = m.values.set("NF", Numeric(m.fields.len().float()))
  Ok(m)
}
pure assign_variable(machine: Machine, name: Str, value: Scalar) -> Result[Machine] {
  var m = machine
  if name in ["CONVFMT", "OFMT"] {
    let format = plain_text(value)?
    let parsed = parse_format(format, 1, [], 0)?
    if ! format.starts_with("%") or parsed.at != format.byte_len() or parsed.format.conversion in ["s", "c"] { return Err(runtime("invalid numeric conversion format")) }
  }
  if name == "NF" {
    let count = integer(number(value)?)?
    if count < 0 or count > 1000000 { return Err(runtime("invalid NF")) }
    if count < m.fields.len() { m.fields = m.fields[..count] } else { while m.fields.len() < count { m.fields += [Empty] } }
    return rebuild(m)
  }
  var at = m.scopes.len() - 1
  while at >= 0 {
    if name in m.scopes[at].values {
      var scope = m.scopes[at]
      scope.values = scope.values.set(name, value)
      m.scopes[at] = scope
      return Ok(m)
    }
    at -= 1
  }
  if array_name(m, name) in m.arrays { return Err(runtime(f"array {name} used as a scalar")) }
  m.values = m.values.set(name, value)
  Ok(m)
}
pure record(machine: Machine, content: Str) -> Result[Machine] {
  var m = machine
  var separator = variable_text(m, "FS")?
  if (variable_text(m, "RS")?).is_empty() and separator != " " and ! separator.is_empty() {
    if separator.count_chars() == 1 and separator in [".", "[", "]", "(", ")", "*", "+", "?", "{", "}", "|", "^", "$", "\\"] { separator = "\\" + separator }
    separator = f"({separator})|\n"
  }
  m.fields = split_fields(content, separator)?
  m.record = content
  m.values = m.values.set("NF", Numeric(m.fields.len().float()))
  Ok(m)
}
pure key(machine: Machine, expressions: List[Int]) -> Result[Keyed] {
  var m = machine
  var keys: List[Str] = []
  for id in expressions {
    m = evaluate(m, id)?
    if m.flow != Normal { return Ok({machine: m, key: ""}) }
    keys += [scalar_text(m.value, variable_text(m, "CONVFMT")?)?]
  }
  Ok({machine: m, key: keys.join(variable_text(m, "SUBSEP")?)})
}
pure locate(machine: Machine, id: Int) -> Result[Located] {
  var m = machine
  match m.program.expressions[id] {
    Variable(name) => Ok({machine: m, place: Named(name)})
    Field(child) => {
      m = evaluate(m, child)?
      let index = integer(number(m.value)?)?
      if index < 0 or index > 1000000 { return Err(runtime("invalid field index")) }
      Ok({machine: m, place: FieldPlace(index)})
    }
    Index(name, keys) => {
      let found = key(m, keys)?
      m = found.machine
      let array = array_name(m, name)
      if array in m.values { return Err(runtime(f"scalar {array} used as an array")) }
      Ok({machine: m, place: Element(array, found.key)})
    }
    _ => Err(syntax("assignment requires variable, field, or array element"))
  }
}
pure read_place(m: Machine, place: Place) -> Scalar {
  match place {
    Named(name) => get(m, name)
    FieldPlace(index) => if index == 0 { Input(m.record, null) } else { m.fields.get(index - 1) ?? Empty }
    Element(array, key) => (m.arrays.get(array) ?? {}).get(key) ?? Empty
  }
}
pure write_place(machine: Machine, place: Place, value: Scalar) -> Result[Machine] {
  var m = machine
  match place {
    Named(name) => assign_variable(m, name, value)
    FieldPlace(index) => {
      if index == 0 { return record(m, scalar_text(value, variable_text(m, "CONVFMT")?)?) }
      while m.fields.len() < index { m.fields += [Empty] }
      m.fields[index - 1] = value
      rebuild(m)
    }
    Element(array, key) => {
      let values: Map[Scalar] = m.arrays.get(array) ?? {}
      m.arrays = m.arrays.set(array, values.set(key, value))
      Ok(m)
    }
  }
}
pure binary(op: Str, left: Scalar, right: Scalar, conversion: Str) -> Result[Scalar] {
  if op == "concat" { return Ok(String(scalar_text(left, conversion)? + scalar_text(right, conversion)?)) }
  if op in ["==", "!=", "<", ">", "<=", ">="] {
    var order = 0
    if numeric(left) and numeric(right) {
      let a = number(left)?
      let b = number(right)?
      order = if a < b { -1 } else if a > b { 1 } else { 0 }
    } else {
      let a = scalar_text(left, conversion)?
      let b = scalar_text(right, conversion)?
      order = if a < b { -1 } else if a > b { 1 } else { 0 }
    }
    let yes = if op == "==" { order == 0 } else if op == "!=" { order != 0 } else if op == "<" { order < 0 } else if op == ">" { order > 0 } else if op == "<=" { order <= 0 } else { order >= 0 }
    return Ok(Numeric(if yes { 1.0 } else { 0.0 }))
  }
  let a = number(left)?
  let b = number(right)?
  if op in ["/", "%"] and b == 0.0 { return Err(runtime("division by zero")) }
  let value = if op == "+" { a + b } else if op == "-" { a - b } else if op == "*" { a * b } else if op == "/" { a / b } else if op == "%" { a - integer(a / b)?.float() * b } else if op == "^" { a.pow(b) } else { return Err(unsupported(f"operator {op}")) }
  Ok(Numeric(value))
}
pure regex_value(machine: Machine, id: Int) -> Result[Keyed] {
  match machine.program.expressions[id] {
    RegexLiteral(pattern) => Ok({machine: machine, key: pattern})
    _ => { let m = evaluate(machine, id)?; Ok({machine: m, key: scalar_text(m.value, variable_text(m, "CONVFMT")?)?}) }
  }
}
pure tick(machine: Machine) -> Result[Machine] {
  var m = machine
  m.steps += 1
  if m.steps > 50000000 { return Err(runtime("execution step limit exceeded")) }
  Ok(m)
}
pure evaluate(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  if m.flow != Normal { return Ok(m) }
  if m.depth >= 256 { return Err(runtime("execution nesting limit exceeded")) }
  m.depth += 1
  m = evaluate_inner(tick(m)?, id)?
  m.depth -= 1
  Ok(m)
}
pure evaluate_inner(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  match m.program.expressions[id] {
    Number(n) => Ok(result(m, Numeric(n)))
    Text(content) => Ok(result(m, String(content)))
    RegexLiteral(pattern) => Ok(result(m, Numeric(if find(pattern, m.record)? != null { 1.0 } else { 0.0 })))
    Variable(name) => {
      if name == "ENVIRON" { return Err(unsupported("ENVIRON requires explicit environment input")) }
      if array_name(m, name) in m.arrays { return Err(runtime(f"array {name} used as scalar")) }
      Ok(result(m, get(m, name)))
    }
    Field(child) => {
      m = evaluate(m, child)?
      let index = integer(number(m.value)?)?
      if index < 0 { return Err(runtime("negative field index")) }
      Ok(result(m, if index == 0 { input(m.record)? } else { m.fields.get(index - 1) ?? Empty }))
    }
    Index(name, keys) => {
      if name == "ENVIRON" { return Err(unsupported("ENVIRON requires explicit environment input")) }
      let located = key(m, keys)?
      m = located.machine
      let array = array_name(m, name)
      if array in m.values { return Err(runtime(f"scalar {array} used as an array")) }
      let values: Map[Scalar] = m.arrays.get(array) ?? {}
      let value = values.get(located.key) ?? Empty
      m.arrays = m.arrays.set(array, values.set(located.key, value))
      Ok(result(m, value))
    }
    Unary(op, child) => {
      m = evaluate(m, child)?
      Ok(result(m, Numeric(if op == "!" { if truth(m.value)? { 0.0 } else { 1.0 } } else if op == "-" { -number(m.value)? } else { number(m.value)? })))
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
      let value = if op == "=" { m.value } else { binary(op.byte_slice(0, length: 1), read_place(m, located.place), m.value, variable_text(m, "CONVFMT")?)? }
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
        return Ok(result(m, Numeric(if truth(m.value)? { 1.0 } else { 0.0 })))
      }
      if op in ["~", "!~"] {
        let pattern = regex_value(m, right)?
        m = pattern.machine
        let hit = find(pattern.key, scalar_text(a, variable_text(m, "CONVFMT")?)?)? != null
        return Ok(result(m, Numeric(if hit != (op == "!~") { 1.0 } else { 0.0 })))
      }
      if op == "in" {
        let array = match m.program.expressions[right] { Variable(name) => array_name(m, name); _ => { return Err(syntax("in requires array name")) } }
        let member = scalar_text(a, variable_text(m, "CONVFMT")?)? in (m.arrays.get(array) ?? {})
        return Ok(result(m, Numeric(if member { 1.0 } else { 0.0 })))
      }
      m = evaluate(m, right)?
      if m.flow != Normal { return Ok(m) }
      Ok(result(m, binary(op, a, m.value, variable_text(m, "CONVFMT")?)?))
    }
    Call(name, args) => call(m, name, args)
  }
}

pure execute_statement(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  if m.flow != Normal { return Ok(m) }
  if m.depth >= 256 { return Err(runtime("execution nesting limit exceeded")) }
  m.depth += 1
  m = execute_inner(tick(m)?, id)?
  m.depth -= 1
  Ok(m)
}
pure execute_inner(machine: Machine, id: Int) -> Result[Machine] {
  var m = machine
  match m.program.statements[id] {
    Block(body) => {
      for child in body { m = execute_statement(m, child)?; if m.flow != Normal { return Ok(m) } }
    }
    ExpressionStatement(child) => { m = evaluate(m, child)? }
    Print(args, formatted_output) => {
      var values: List[Scalar] = []
      for child in args { m = evaluate(m, child)?; if m.flow != Normal { return Ok(m) }; values += [m.value] }
      let conversion = variable_text(m, "CONVFMT")?
      if formatted_output {
        if values.is_empty() { return Err(syntax("printf requires a format")) }
        m.output += printf(scalar_text(values[0], conversion)?, values[1..], conversion)?
      } else {
        if values.is_empty() { m.output += m.record } else {
          var pieces: List[Str] = []
          for value in values {
            match value {
              Numeric(n) => {
                pieces += [scalar_text(value, variable_text(m, "OFMT")?)?]
              }
              _ => { pieces += [scalar_text(value, conversion)?] }
            }
          }
          m.output += pieces.join(variable_text(m, "OFS")?)
        }
        m.output += variable_text(m, "ORS")?
      }
    }
    If(condition, yes, no) => {
      m = evaluate(m, condition)?
      if m.flow != Normal { return Ok(m) }
      if truth(m.value)? { m = execute_statement(m, yes)? } else if no != null { m = execute_statement(m, no)? }
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
      if initial != null { m = evaluate(m, initial)? }
      loop {
        if m.flow != Normal { break }
        if condition != null { m = evaluate(m, condition)?; if m.flow != Normal or ! truth(m.value)? { break } }
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
        if step != null { m = evaluate(m, step)? }
      }
    }
    Each(name, array, body) => {
      if array == "ENVIRON" { return Err(unsupported("ENVIRON requires explicit environment input")) }
      let resolved = array_name(m, array)
      let values: Map[Scalar] = m.arrays.get(resolved) ?? {}
      m.arrays = m.arrays.set(resolved, values)
      for member in values.keys() {
        m = assign_variable(m, name, String(member))?
        m = execute_statement(m, body)?
        if m.flow == BreakLoop { m.flow = Normal; break }
        if m.flow == ContinueLoop { m.flow = Normal }
        if m.flow != Normal { break }
      }
    }
    Delete(name, keys) => {
      let resolved = array_name(m, name)
      var values: Map[Scalar] = m.arrays.get(resolved) ?? {}
      if keys != null { let found = key(m, keys)?; m = found.machine; values = values.remove(found.key) } else { values = {} }
      m.arrays = m.arrays.set(resolved, values)
    }
    Next => { m.flow = NextRecord }
    Break => { m.flow = BreakLoop }
    Continue => { m.flow = ContinueLoop }
    Exit(child) => {
      if child != null { m = evaluate(m, child)?; if m.flow != Normal { return Ok(m) }; m.status = integer(number(m.value)?)? }
      m.flow = ExitProgram
    }
    Return(child) => {
      if child != null { m = evaluate(m, child)?; if m.flow != Normal { return Ok(m) } } else { m.value = Empty }
      m.flow = ReturnFunction
    }
  }
  Ok(m)
}
pure expression_arrays(program: Program, id: Int) -> List[Str] {
  var arrays: List[Str] = []
  var children: List[Int] = []
  match program.expressions[id] {
    Index(name, keys) => { arrays += [name]; children = keys }
    Call(name, args) => {
      if name == "split" and args.len() >= 2 { match program.expressions[args[1]] { Variable(array) => { arrays += [array] }; _ => {} } }
      children = args
    }
    Binary(op, a, b) => {
      if op == "in" { match program.expressions[b] { Variable(name) => { arrays += [name] }; _ => {} } }
      children = [a, b]
    }
    Assign(_, a, b) => { children = [a, b] }
    Field(child) => { children = [child] }
    Unary(_, child) => { children = [child] }
    Increment(child, _, _) => { children = [child] }
    Conditional(a, b, c) => { children = [a, b, c] }
    _ => {}
  }
  for child in children { arrays += expression_arrays(program, child) }
  arrays
}
pure statement_arrays(program: Program, id: Int) -> List[Str] {
  var arrays: List[Str] = []
  var expressions: List[Int] = []
  var children: List[Int] = []
  match program.statements[id] {
    Block(body) => { children = body }
    ExpressionStatement(child) => { expressions = [child] }
    Print(args, _) => { expressions = args }
    If(condition, yes, no) => { expressions = [condition]; children = [yes]; if no != null { children += [no] } }
    While(condition, body) => { expressions = [condition]; children = [body] }
    Do(body, condition) => { expressions = [condition]; children = [body] }
    For(a, b, c, body) => { if a != null { expressions += [a] }; if b != null { expressions += [b] }; if c != null { expressions += [c] }; children = [body] }
    Each(_, array, body) => { arrays += [array]; children = [body] }
    Delete(array, keys) => { arrays += [array]; if keys != null { expressions = keys } }
    Exit(child) => { if child != null { expressions = [child] } }
    Return(child) => { if child != null { expressions = [child] } }
    _ => {}
  }
  for child in children { arrays += statement_arrays(program, child) }
  for child in expressions { arrays += expression_arrays(program, child) }
  arrays
}
pure substitute(replacement: Str, matched: Str) -> Str {
  var out = ""
  var at = 0
  while at < replacement.byte_len() {
    let byte = replacement.byte_at(at) ?? 0
    if byte == 38 { out += matched; at += 1 } else if byte == 92 {
      at += 1
      if at >= replacement.byte_len() { out += "\\"; break }
      let next = replacement.byte_slice(at, length: 1)
      if next not in ["&", "\\"] { out += "\\" }
      out += next
      at += 1
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
# Scopes carry scalar parameters by value and array parameters by name. Next and
# exit leave their flow marker intact while return is consumed at this boundary.
pure call(machine: Machine, name: Str, args: List[Int]) -> Result[Machine] {
  var m = machine
  if name in m.program.functions {
    let function = m.program.functions.get(name)?
    if args.len() > function.parameters.len() { return Err(runtime(f"too many arguments to {name}")) }
    let inferred = statement_arrays(m.program, function.body)
    var scope = Scope(values: {}, arrays: {})
    var locals: List[Str] = []
    for at in range(function.parameters.len()) {
      let parameter = function.parameters[at]
      var array: Str? = null
      if at < args.len() {
        match m.program.expressions[args[at]] {
          Variable(variable) => {
            let resolved = array_name(m, variable)
            if parameter in inferred or resolved in m.arrays {
              if resolved in m.values { return Err(runtime(f"scalar {resolved} passed as array")) }
              for outer in m.scopes { if resolved in outer.values { return Err(unsupported("forwarding an array through an uninferred function parameter")) } }
              array = resolved
            }
          }
          _ => { if parameter in inferred { return Err(runtime("array argument must name an array")) } }
        }
      } else if parameter in inferred {
        let local = f"\0{m.scopes.len()}:{parameter}"
        array = local
        locals += [local]
      }
      if array != null {
        m.arrays = m.arrays.set(array, m.arrays.get(array) ?? {})
        scope.arrays = scope.arrays.set(parameter, array)
      } else {
        if at < args.len() { m = evaluate(m, args[at])?; if m.flow != Normal { return Ok(m) } } else { m.value = Empty }
        scope.values = scope.values.set(parameter, m.value)
      }
    }
    m.scopes += [scope]
    m = execute_statement(m, function.body)?
    m.scopes = m.scopes[..m.scopes.len() - 1]
    for local in locals { m.arrays = m.arrays.remove(local) }
    if m.flow == ReturnFunction { m.flow = Normal } else if m.flow == Normal { m.value = Empty } else if m.flow not in [ExitProgram, NextRecord] { return Err(runtime("invalid control flow from function")) }
    return Ok(m)
  }
  if name in ["sub", "gsub"] {
    if args.len() < 2 or args.len() > 3 { return Err(runtime(f"{name} expects two or three arguments")) }
    let pattern = regex_value(m, args[0])?
    m = evaluate(pattern.machine, args[1])?
    if m.flow != Normal { return Ok(m) }
    let replacement = scalar_text(m.value, variable_text(m, "CONVFMT")?)?
    var place: Place = FieldPlace(0)
    if args.len() == 3 { let located = locate(m, args[2])?; m = located.machine; place = located.place }
    let content = scalar_text(read_place(m, place), variable_text(m, "CONVFMT")?)?
    var out = ""
    var copied = 0
    var count = 0
    var previous_end = -1
    for hit in regex.find_bytes(pattern.key, bytes.from_text(content), extended: true)? {
      if hit.start == hit.end and previous_end == hit.start { continue }
      out += slice(content, copied, hit.start)? + substitute(replacement, slice(content, hit.start, hit.end)?)
      copied = hit.end
      previous_end = hit.end
      count += 1
      if name == "sub" { break }
    }
    out += slice(content, copied, content.byte_len())?
    if count > 0 { m = write_place(m, place, String(out))? }
    return Ok(result(m, Numeric(count.float())))
  }
  if name == "split" {
    if args.len() < 2 or args.len() > 3 { return Err(runtime("split expects two or three arguments")) }
    m = evaluate(m, args[0])?
    let content = scalar_text(m.value, variable_text(m, "CONVFMT")?)?
    var separator = variable_text(m, "FS")?
    if args.len() == 3 { let pattern = regex_value(m, args[2])?; m = pattern.machine; separator = pattern.key }
    let array = match m.program.expressions[args[1]] { Variable(variable) => array_name(m, variable); _ => { return Err(runtime("split requires array name")) } }
    if array in m.values { return Err(runtime("split target is a scalar")) }
    let fields = split_fields(content, separator)?
    var values: Map[Scalar] = {}
    for at in range(fields.len()) { values = values.set(f"{at + 1}", fields[at]) }
    m.arrays = m.arrays.set(array, values)
    return Ok(result(m, Numeric(fields.len().float())))
  }
  if name == "match" {
    if args.len() != 2 { return Err(runtime("match expects two arguments")) }
    m = evaluate(m, args[0])?
    let content = scalar_text(m.value, variable_text(m, "CONVFMT")?)?
    let pattern = regex_value(m, args[1])?
    m = pattern.machine
    let hit = find(pattern.key, content)?
    let start = if hit == null { 0 } else { slice(content, 0, hit.start)?.count_chars() + 1 }
    let length = if hit == null { -1 } else { slice(content, hit.start, hit.end)?.count_chars() }
    m = assign_variable(m, "RSTART", Numeric(start.float()))?
    m = assign_variable(m, "RLENGTH", Numeric(length.float()))?
    return Ok(result(m, Numeric(start.float())))
  }
  let minimum = if name == "length" { 0 } else if name in ["substr", "index", "atan2"] { 2 } else if name in ["sprintf", "int", "sqrt", "exp", "log", "sin", "cos", "tolower", "toupper"] { 1 } else { return Err(unsupported(f"function {name}")) }
  let maximum = if name == "length" { 1 } else if name == "substr" { 3 } else if name == "sprintf" { 1000000 } else { minimum }
  if args.len() < minimum or args.len() > maximum { return Err(runtime(f"wrong argument count for {name}")) }
  var values: List[Scalar] = []
  for child in args { m = evaluate(m, child)?; if m.flow != Normal { return Ok(m) }; values += [m.value] }
  let conversion = variable_text(m, "CONVFMT")?
  var value: Scalar = Empty
  if name == "length" { value = Numeric((if values.is_empty() { m.record } else { scalar_text(values[0], conversion)? }).count_chars().float()) } else if name == "substr" {
    let content = scalar_text(values[0], conversion)?
    let pieces = content.split("")
    let given = integer(number(values[1])?)?
    let start = if given < 1 { 0 } else if given > pieces.len() { pieces.len() } else { given - 1 }
    let length = if values.len() == 3 { integer(number(values[2])?)? } else { pieces.len() }
    let end = if length < 0 { start } else if start + length > pieces.len() { pieces.len() } else { start + length }
    value = String(pieces[start..end].join(""))
  } else if name == "index" {
    let content = scalar_text(values[0], conversion)?
    let found = content.find(scalar_text(values[1], conversion)?)
    value = Numeric(if found == null { 0.0 } else { (slice(content, 0, found)?.count_chars() + 1).float() })
  } else if name == "tolower" { value = String((scalar_text(values[0], conversion)?).lower()) } else if name == "toupper" { value = String((scalar_text(values[0], conversion)?).upper()) } else if name == "sprintf" { value = String(printf(scalar_text(values[0], conversion)?, values[1..], conversion)?) } else {
    let n = number(values[0])?
    let computed = if name == "int" { integer(n)?.float() } else if name == "sqrt" { n.sqrt() } else if name == "exp" { n.exp() } else if name == "log" { n.ln() } else if name == "sin" { n.sin() } else if name == "cos" { n.cos() } else { n.atan2(number(values[1])?) }
    value = Numeric(computed)
  }
  Ok(result(m, value))
}

type ReadRecord = {content: Str?, position: Int, terminator: Str}
# GNU awk treats one-character RS values literally and longer values as regexes.
pure record_at(content: Str, position: Int, separator: Str) -> Result[ReadRecord] {
  if position == content.byte_len() + 1 { return Ok({content: "", position: position + 1, terminator: ""}) }
  if position > content.byte_len() { return Ok({content: null, position: position, terminator: ""}) }
  var at = position
  if at >= content.byte_len() { return Ok({content: null, position: at, terminator: ""}) }
  if separator.is_empty() {
    while content.byte_at(at) == 10 { at += 1 }
    if at >= content.byte_len() { return Ok({content: null, position: at, terminator: ""}) }
    let blank = content.find("\n\n", at)
    if blank == null {
      var end = content.byte_len()
      while end > at and content.byte_at(end - 1) == 10 { end -= 1 }
      return Ok({content: content.byte_slice(at, length: end - at), position: content.byte_len(), terminator: ""})
    }
    let piece = content.byte_slice(at, length: blank - at)
    at = blank + 2
    while content.byte_at(at) == 10 { at += 1 }
    return Ok({content: piece, position: at, terminator: content.byte_slice(blank, length: at - blank)})
  }
  if separator.count_chars() == 1 {
    let found = content.find(separator, at)
    if found == null { return Ok({content: content.byte_slice(at), position: content.byte_len(), terminator: ""}) }
    return Ok({content: content.byte_slice(at, length: found - at), position: found + separator.byte_len(), terminator: separator})
  }
  for hit in regex.find_bytes(separator, bytes.from_text(content), extended: true)? {
    if hit.start < at or hit.end == hit.start { continue }
    let terminator = content.byte_slice(hit.start, length: hit.end - hit.start)
    return Ok({content: content.byte_slice(at, length: hit.start - at), position: if hit.end == content.byte_len() { hit.end + 1 } else { hit.end }, terminator: terminator})
  }
  Ok({content: content.byte_slice(at), position: content.byte_len(), terminator: ""})
}
pure binding_name(content: Str) -> Bool {
  if content.is_empty() or ! letter(content.byte_at(0) ?? 0) { return false }
  for at in range(1, content.byte_len()) { if ! letter(content.byte_at(at) ?? 0) and ! digit(content.byte_at(at) ?? 0) { return false } }
  true
}
pure binding(machine: Machine, item: Str) -> Result[Machine] {
  let equals = item.find("=")
  if equals == null { return Err(syntax("variable binding requires name=value")) }
  let name = item.byte_slice(0, length: equals)
  if ! binding_name(name) { return Err(syntax(f"invalid variable name {name}")) }
  let decoded = escaped(bytes.from_text(item.byte_slice(equals + 1)), 0, -1)?
  assign_variable(machine, name, input(decoded.content)?)
}
# AWK consults the live ARGV array between files, so program edits select later inputs.
pure next_argv(machine: Machine, after: Int, count: Int) -> Result[Int?] {
  let arguments: Map[Scalar] = machine.arrays.get("ARGV") ?? {}
  var next: Int? = null
  for key in arguments.keys() {
    let index = key.parse_int_decimal() ?? -1
    if index <= after or index >= count { continue }
    if (plain_text(arguments.get(key) ?? Empty)?).is_empty() { continue }
    if next == null or index < (next ?? index) { next = index }
  }
  Ok(next)
}
pure new_machine(program: Program, separator: Str) -> Machine {
  let values: Map[Scalar] = {"FS": String(separator), "OFS": String(" "), "RS": String("\n"), "RT": String(""), "ORS": String("\n"), "SUBSEP": String("\u{1c}"), "OFMT": String("%.6g"), "CONVFMT": String("%.6g"), "NR": Numeric(0.0), "FNR": Numeric(0.0), "NF": Numeric(0.0), "RSTART": Numeric(0.0), "RLENGTH": Numeric(0.0)}
  Machine(program: program, values: values, arrays: {}, scopes: [], fields: [], record: "", output: "", status: 0, flow: Normal, value: Empty, steps: 0, depth: 0)
}

## Collected output and conventional byte-sized process exit status.
export type Output = {stdout: Str, status: Int}

## Parse and execute a program over named inputs; variable bindings precede BEGIN.
## File operands containing name=value are applied between inputs, after BEGIN.
export pure execute(source: Str, inputs: List[Str], names: List[Str], variables: List[Str] = [], field_separator = " ") -> Result[Output, Error] {
  if inputs.len() != names.len() { return Err(runtime("input names and contents must have equal lengths")) }
  let program = parse(source)?
  let separator = escaped(bytes.from_text(field_separator), 0, -1)?
  var m = new_machine(program, separator.content)
  for item in variables { m = binding(m, item)? }
  m = assign_variable(m, "ARGC", Numeric((inputs.len() + 1).float()))?
  var argv: Map[Scalar] = {"0": String("awk")}
  for at in range(names.len()) { argv = argv.set(f"{at + 1}", String(names[at])) }
  m.arrays = m.arrays.set("ARGV", argv)
  for id in program.begin {
    m = execute_statement(m, id)?
    if m.flow == ExitProgram { break }
    if m.flow != Normal { return Err(runtime("invalid control flow in BEGIN")) }
  }
  var ranges = [false for _ in program.rules]
  if m.flow != ExitProgram {
    var after = 0
    loop {
      let count = integer(number(get(m, "ARGC"))?)?
      let selected = next_argv(m, after, count)?
      if selected == null { break }
      let index = selected ?? 0
      after = index
      let argument = plain_text((m.arrays.get("ARGV") ?? {}).get(f"{index}") ?? Empty)?
      let equals = argument.find("=")
      if equals != null and binding_name(argument.byte_slice(0, length: equals)) {
        m = binding(m, argument)?
        continue
      }
      var input: Int? = null
      for at in range(names.len()) { if names[at] == argument { input = at; break } }
      if input == null { return Err(unsupported("changing ARGV input requires streamed file ownership")) }
      m = assign_variable(m, "FILENAME", String(argument))?
      m = assign_variable(m, "FNR", Numeric(0.0))?
      var position = 0
      loop {
        if m.flow == ExitProgram { break }
        if m.flow == NextRecord { m.flow = Normal }
        let next = record_at(inputs[input ?? 0], position, variable_text(m, "RS")?)?
        position = next.position
        if next.content == null { break }
        m = record(m, next.content)?
        m = assign_variable(m, "RT", String(next.terminator))?
        m = assign_variable(m, "NR", Numeric(number(get(m, "NR"))? + 1.0))?
        m = assign_variable(m, "FNR", Numeric(number(get(m, "FNR"))? + 1.0))?
        for at in range(program.rules.len()) {
          let rule = program.rules[at]
          var matched = ranges[at]
          if ! matched {
            if rule.pattern == null { matched = true } else { m = evaluate(m, rule.pattern)?; if m.flow != Normal { break }; matched = truth(m.value)? }
          }
          if ! matched { continue }
          if rule.end != null { m = evaluate(m, rule.end)?; if m.flow != Normal { break }; ranges[at] = ! truth(m.value)? }
          m = execute_statement(m, rule.action)?
          if m.flow in [NextRecord, ExitProgram] { break }
          if m.flow != Normal { return Err(runtime("invalid control flow at top level")) }
        }
      }
      if m.flow == ExitProgram { break }
    }
  }
  m.flow = Normal
  for id in program.end {
    m = execute_statement(m, id)?
    if m.flow == ExitProgram { break }
    if m.flow != Normal { return Err(runtime("invalid control flow in END")) }
  }
  Ok(Output(stdout: m.output, status: (m.status % 256 + 256) % 256))
}
