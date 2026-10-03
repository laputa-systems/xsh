##! Embedded implementation of the public `template` module.
# A small text template engine in the style of Go's text/template. The public
# contract lives in the standard API registry and
# `docs/snippets/template-contract.md`; this module owns parsing and rendering.
#
# A template is parsed completely before anything renders, so a syntax error
# anywhere is reported even when the branch that holds it would not run. The
# parsed program is a flat node table: block nodes record the index of their
# next branch (`links`) and of their closing `{{end}}` (`closes`), and rendering
# walks index ranges of that table. Records cannot be recursive, so the table
# stands in for a syntax tree.
#
# Templates have no access to XSH functions, the environment, or the host. The
# only callable names are the fixed functions in `call_function`.

# The failures `template.render` reports.
#
# `kind` keeps the stable spellings `template-syntax` and `template-render`
# visible to callers; the message starts with `template:LINE:COLUMN:`, where the
# line and column are 1-based and the column counts characters.
error TemplateError = Syntax(kind: Str, message: Str) | Render(kind: Str, message: Str)

# One lexical token inside an action.
type Tok = {kind: Str, text: Str, num: Int, pos: Int}

# One command argument. `kind` is `field`, `var`, `str`, `int`, `bool`,
# `null`, or `func`; `text` holds a string literal, variable, or function name.
type Arg = {kind: Str, text: Str, fields: List[Str], num: Int, pos: Int}

# One pipeline stage: a function call or a single operand.
type Cmd = {args: List[Arg], pos: Int}

# A pipeline and the variables a `range` or `with` action declares for it.
type Pipe = {decl: List[Str], cmds: List[Cmd], pos: Int}

# The lexed contents of one action and where the action ends.
type Action = {toks: List[Tok], close: Int, trim_right: Bool}

# The parsed node table. Parallel lists are indexed by node.
type Program = {
  source: Str,
  kinds: List[Str],
  texts: List[Str],
  pipes: List[Pipe],
  links: List[Int],
  closes: List[Int],
  positions: List[Int],
  defines: Map[Int],
}

# Nested `{{template}}` calls beyond this depth are reported as runaway
# recursion rather than exhausting the interpreter.
const MAX_TEMPLATE_DEPTH = 64

# The 1-based `LINE:COLUMN` of a byte offset; the column counts characters.
pure position_text(source: Str, offset: Int) -> Str {
  let lines = source.byte_slice(0, offset).split("\n")
  let line = lines.len()
  let column = lines[line - 1].count_chars() + 1
  return f"${line}:${column}"
}

pure syntax_error(source: Str, offset: Int, message: Str) -> TemplateError {
  return TemplateError.Syntax(
    kind: "template-syntax",
    message: f"template:${position_text(source, offset)}: ${message}",
  )
}

pure render_error(source: Str, offset: Int, message: Str) -> TemplateError {
  return TemplateError.Render(
    kind: "template-render",
    message: f"template:${position_text(source, offset)}: ${message}",
  )
}

pure is_space(byte: Int) -> Bool {
  return byte == 32 or byte == 9 or byte == 10 or byte == 13
}

pure is_digit(byte: Int) -> Bool {
  return byte >= 48 and byte <= 57
}

pure is_ident_byte(byte: Int) -> Bool {
  return byte == 95 or is_digit(byte) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122)
}

pure byte_of(source: Str, index: Int) -> Int {
  return source.byte_at(index) ?? -1
}

# `text` without trailing ASCII white space.
pure trim_end(text: Str) -> Str {
  var end = text.byte_len()
  while end > 0 and is_space(byte_of(text, end - 1)) {
    end -= 1
  }
  return text.byte_slice(0, end)
}

# `text` without leading ASCII white space.
pure trim_start(text: Str) -> Str {
  let len = text.byte_len()
  var start = 0
  while start < len and is_space(byte_of(text, start)) {
    start += 1
  }
  return text.byte_slice(start, len - start)
}

pure tok(kind: Str, text: Str, pos: Int) -> Tok {
  return {kind: kind, text: text, num: 0, pos: pos}
}

# Read a double-quoted string literal starting at its opening quote.
#
# Returns the decoded text and the offset just past the closing quote. The
# escapes are `\\`, `\"`, `\n`, `\t`, and `\r`.
pure lex_string(source: Str, open: Int) -> Result[Tok] {
  let n = source.byte_len()
  var parts: List[Str] = []
  var segment = open + 1
  var index = open + 1
  while index < n {
    let byte = byte_of(source, index)
    if byte == 34 {
      parts += [source.byte_slice(segment, index - segment)]
      return Ok({kind: "str", text: parts.join(""), num: index + 1, pos: open})
    }
    if byte == 10 {
      return Err(syntax_error(source, open, "unterminated string literal"))
    }
    if byte == 92 {
      parts += [source.byte_slice(segment, index - segment)]
      let escaped = byte_of(source, index + 1)
      if escaped == 92 {
        parts += ["\\"]
      } else if escaped == 34 {
        parts += ["\""]
      } else if escaped == 110 {
        parts += ["\n"]
      } else if escaped == 116 {
        parts += ["\t"]
      } else if escaped == 114 {
        parts += ["\r"]
      } else {
        return Err(syntax_error(source, index, "unknown escape in string literal"))
      }
      index += 2
      segment = index
    } else {
      index += 1
    }
  }
  return Err(syntax_error(source, open, "unterminated string literal"))
}

# Lex the inside of one action, from `start` up to and including its `}}`.
#
# `open` is the offset of the action's `{{`, used to report an action that
# never closes. A ` -}}` ending sets `trim_right`. Quoted strings may contain
# `}}`; nothing else inside an action may.
pure lex_action(source: Str, open: Int, start: Int) -> Result[Action] {
  let n = source.byte_len()
  var toks: List[Tok] = []
  var index = start
  while index < n {
    let byte = byte_of(source, index)
    if is_space(byte) {
      if byte_of(source, index + 1) == 45 and byte_of(source, index + 2) == 125 and byte_of(source, index + 3) == 125 {
        return Ok({toks: toks, close: index + 4, trim_right: true})
      }
      index += 1
    } else if byte == 125 and byte_of(source, index + 1) == 125 {
      return Ok({toks: toks, close: index + 2, trim_right: false})
    } else if byte == 46 or byte == 36 {
      # A field chain `.a.b`, a lone `.`, or a variable `$name.a.b` / `$`.
      var end = index + 1
      if byte == 36 {
        while end < n and is_ident_byte(byte_of(source, end)) {
          end += 1
        }
      } else {
        end = index
      }
      var lone_dot = false
      while !lone_dot and end < n and byte_of(source, end) == 46 {
        var segment_end = end + 1
        while segment_end < n and is_ident_byte(byte_of(source, segment_end)) {
          segment_end += 1
        }
        if segment_end == end + 1 {
          if byte == 46 and end == index {
            lone_dot = true
          } else {
            return Err(syntax_error(source, end, "expected a field name after `.`"))
          }
        }
        end = segment_end
      }
      let kind = if byte == 46 { "field" } else { "var" }
      toks += [tok(kind, source.byte_slice(index, end - index), index)]
      index = end
    } else if byte == 34 {
      let literal = lex_string(source, index)?
      toks += [{kind: "str", text: literal.text, num: 0, pos: index}]
      index = literal.num
    } else if byte == 96 {
      let close = source.find("`", index + 1)
      if close == null {
        return Err(syntax_error(source, index, "unterminated raw string literal"))
      }
      let end = close ?? n
      toks += [tok("str", source.byte_slice(index + 1, end - index - 1), index)]
      index = end + 1
    } else if is_digit(byte) or (byte == 45 and is_digit(byte_of(source, index + 1))) {
      var end = index + 1
      while end < n and is_digit(byte_of(source, end)) {
        end += 1
      }
      let digits = source.byte_slice(index, end - index)
      let negative = byte == 45
      let magnitude = if negative { digits.byte_slice(1) } else { digits }
      match magnitude.parse_int_decimal() {
        Ok(value) => {
          let number = if negative { 0 - value } else { value }
          toks += [{kind: "int", text: digits, num: number, pos: index}]
        }
        Err(_) => return Err(syntax_error(source, index, f"invalid number `${digits}`"))
      }
      index = end
    } else if is_ident_byte(byte) {
      var end = index + 1
      while end < n and is_ident_byte(byte_of(source, end)) {
        end += 1
      }
      toks += [tok("ident", source.byte_slice(index, end - index), index)]
      index = end
    } else if byte == 124 {
      toks += [tok("pipe", "|", index)]
      index += 1
    } else if byte == 44 {
      toks += [tok("comma", ",", index)]
      index += 1
    } else if byte == 58 and byte_of(source, index + 1) == 61 {
      toks += [tok("assign", ":=", index)]
      index += 2
    } else if byte == 40 or byte == 41 {
      return Err(syntax_error(source, index, "parentheses are not supported"))
    } else {
      return Err(syntax_error(source, index, f"unexpected character `${source.byte_slice(index, 1)}` in action"))
    }
  }
  return Err(syntax_error(source, open, "unclosed action; expected `}}`"))
}

# The fixed function table: the number of arguments each function takes,
# counting a piped value, or -1 for a name that is not a function.
pure function_arity(name: Str) -> Int {
  match name {
    "len" | "upper" | "lower" | "trim" | "quote" | "json" | "not" => return 1
    "default" | "join" | "eq" | "ne" | "and" | "or" => return 2
    _ => return -1
  }
}

# Convert one token into a command argument.
pure arg_from_tok(source: Str, token: Tok) -> Result[Arg] {
  if token.kind == "field" {
    let fields = if token.text == "." { [] } else { token.text.byte_slice(1).split(".") }
    return Ok({kind: "field", text: "", fields: fields, num: 0, pos: token.pos})
  }
  if token.kind == "var" {
    let parts = token.text.split(".")
    return Ok({kind: "var", text: parts[0], fields: parts[1..], num: 0, pos: token.pos})
  }
  if token.kind == "str" {
    return Ok({kind: "str", text: token.text, fields: [], num: 0, pos: token.pos})
  }
  if token.kind == "int" {
    return Ok({kind: "int", text: "", fields: [], num: token.num, pos: token.pos})
  }
  if token.kind == "ident" {
    match token.text {
      "true" => return Ok({kind: "bool", text: "", fields: [], num: 1, pos: token.pos})
      "false" => return Ok({kind: "bool", text: "", fields: [], num: 0, pos: token.pos})
      "null" => return Ok({kind: "null", text: "", fields: [], num: 0, pos: token.pos})
      _ => {}
    }
    if function_arity(token.text) < 0 {
      return Err(syntax_error(source, token.pos, f"unknown function `${token.text}`"))
    }
    return Ok({kind: "func", text: token.text, fields: [], num: 0, pos: token.pos})
  }
  return Err(syntax_error(source, token.pos, f"unexpected `${token.text}`"))
}

# Parse the tokens from `start` as a pipeline.
#
# With `allow_decl`, a leading `$v :=` or `$i, $v :=` declares the variables a
# `range` or `with` action binds.
pure parse_pipe(source: Str, toks: List[Tok], start: Int, allow_decl: Bool, owner: Str, pos: Int) -> Result[Pipe] {
  let count = toks.len()
  var index = start
  var decl: List[Str] = []
  var has_assign = false
  var scan = start
  while scan < count {
    if toks[scan].kind == "assign" {
      has_assign = true
    }
    scan += 1
  }
  if has_assign {
    if !allow_decl {
      return Err(syntax_error(source, pos, "variables can only be declared by `range` and `with`"))
    }
    var expect_name = true
    var done = false
    while !done {
      if index >= count {
        return Err(syntax_error(source, pos, "malformed variable declaration"))
      }
      let token = toks[index]
      if expect_name {
        if token.kind != "var" or token.text == "$" or "." in token.text {
          return Err(syntax_error(source, token.pos, "expected a variable name such as `$item`"))
        }
        decl += [token.text]
        expect_name = false
      } else if token.kind == "comma" and decl.len() == 1 {
        expect_name = true
      } else if token.kind == "assign" {
        done = true
      } else {
        return Err(syntax_error(source, token.pos, "malformed variable declaration"))
      }
      index += 1
    }
    if decl.len() == 2 and owner != "range" {
      return Err(syntax_error(source, pos, "only `range` declares two variables"))
    }
  }
  var cmds: List[Cmd] = []
  var args: List[Arg] = []
  var cmd_pos = if index < count { toks[index].pos } else { pos }
  while index < count {
    let token = toks[index]
    if token.kind == "pipe" {
      if args.len() == 0 {
        return Err(syntax_error(source, token.pos, "missing command before `|`"))
      }
      cmds += [{args: args, pos: cmd_pos}]
      args = []
      cmd_pos = if index + 1 < count { toks[index + 1].pos } else { token.pos }
    } else {
      args += [arg_from_tok(source, token)?]
    }
    index += 1
  }
  if args.len() == 0 {
    if cmds.len() == 0 {
      return Err(syntax_error(source, pos, f"missing value for `${owner}`"))
    }
    return Err(syntax_error(source, cmd_pos, "missing command after `|`"))
  }
  cmds += [{args: args, pos: cmd_pos}]
  var stage = 0
  while stage < cmds.len() {
    let cmd = cmds[stage]
    let head = cmd.args[0]
    if head.kind == "func" {
      let supplied = cmd.args.len() - 1 + (if stage > 0 { 1 } else { 0 })
      let wanted = function_arity(head.text)
      if supplied != wanted {
        return Err(
          syntax_error(source, head.pos, f"`${head.text}` takes ${wanted} argument(s), found ${supplied}"),
        )
      }
      var extra = 1
      while extra < cmd.args.len() {
        if cmd.args[extra].kind == "func" {
          return Err(
            syntax_error(source, cmd.args[extra].pos, f"function `${cmd.args[extra].text}` cannot be an argument"),
          )
        }
        extra += 1
      }
    } else if cmd.args.len() > 1 {
      return Err(syntax_error(source, cmd.args[1].pos, "only functions take arguments"))
    } else if stage > 0 {
      return Err(syntax_error(source, head.pos, "only functions can receive a piped value"))
    }
    stage += 1
  }
  return Ok({decl: decl, cmds: cmds, pos: pos})
}

pure empty_pipe(pos: Int) -> Pipe {
  return {decl: [], cmds: [], pos: pos}
}

# Parse a complete template into its node table.
pure parse(source: Str) -> Result[Program] {
  let n = source.byte_len()
  var kinds: List[Str] = []
  var texts: List[Str] = []
  var pipes: List[Pipe] = []
  var links: List[Int] = []
  var closes: List[Int] = []
  var positions: List[Int] = []
  var defines: Map[Int] = {}
  # `openers` holds each open block's first node; `branches` its latest branch.
  var openers: List[Int] = []
  var branches: List[Int] = []
  var calls: List[Int] = []
  var pos = 0
  var trim_next = false
  var finished = false
  while !finished {
    let found = source.find("{{", pos)
    let text_end = found ?? n
    var text = source.byte_slice(pos, text_end - pos)
    if trim_next {
      text = trim_start(text)
    }
    if found == null {
      finished = true
    }
    let open = found ?? n
    var start = open + 2
    if !finished and byte_of(source, start) == 45 and is_space(byte_of(source, start + 1)) {
      text = trim_end(text)
      start += 2
    }
    if text.byte_len() > 0 {
      kinds += ["text"]
      texts += [text]
      pipes += [empty_pipe(pos)]
      links += [-1]
      closes += [-1]
      positions += [pos]
    }
    if !finished {
      var body = start
      while body < n and is_space(byte_of(source, body)) {
        body += 1
      }
      if byte_of(source, body) == 47 and byte_of(source, body + 1) == 42 {
        let comment_end = source.find("*/", body + 2)
        if comment_end == null {
          return Err(syntax_error(source, open, "unclosed comment; expected `*/}}`"))
        }
        var after = (comment_end ?? n) + 2
        while after < n and is_space(byte_of(source, after)) {
          after += 1
        }
        trim_next = false
        if byte_of(source, after) == 45 and byte_of(source, after + 1) == 125 and is_space(byte_of(source, after - 1)) {
          trim_next = true
          after += 1
        }
        if byte_of(source, after) != 125 or byte_of(source, after + 1) != 125 {
          return Err(syntax_error(source, open, "comment must end with `*/}}`"))
        }
        pos = after + 2
      } else {
        let action = lex_action(source, open, start)?
        pos = action.close
        trim_next = action.trim_right
        let toks = action.toks
        if toks.len() == 0 {
          return Err(syntax_error(source, open, "empty action"))
        }
        let keyword = if toks[0].kind == "ident" { toks[0].text } else { "" }
        let index = kinds.len()
        if keyword == "if" or keyword == "range" or keyword == "with" {
          let pipe = parse_pipe(source, toks, 1, keyword != "if", keyword, open)?
          kinds += [keyword]
          texts += [""]
          pipes += [pipe]
          links += [-1]
          closes += [-1]
          positions += [open]
          openers += [index]
          branches += [index]
        } else if keyword == "else" {
          if branches.len() == 0 {
            return Err(syntax_error(source, open, "unexpected `{{else}}`"))
          }
          let latest = branches[branches.len() - 1]
          let owner = kinds[openers[openers.len() - 1]]
          if kinds[latest] == "else" {
            return Err(syntax_error(source, open, "`{{else}}` after the final `{{else}}`"))
          }
          if owner == "define" {
            return Err(syntax_error(source, open, "unexpected `{{else}}` in `define`"))
          }
          var kind = "else"
          var pipe = empty_pipe(open)
          if toks.len() > 1 {
            if toks[1].kind == "ident" and toks[1].text == "if" and owner == "if" {
              kind = "elif"
              pipe = parse_pipe(source, toks, 2, false, "else if", open)?
            } else {
              return Err(syntax_error(source, toks[1].pos, "unexpected tokens after `else`"))
            }
          }
          links[latest] = index
          kinds += [kind]
          texts += [""]
          pipes += [pipe]
          links += [-1]
          closes += [-1]
          positions += [open]
          branches[branches.len() - 1] = index
        } else if keyword == "end" {
          if openers.len() == 0 {
            return Err(syntax_error(source, open, "unexpected `{{end}}`"))
          }
          if toks.len() > 1 {
            return Err(syntax_error(source, toks[1].pos, "unexpected tokens after `end`"))
          }
          let opener = openers[openers.len() - 1]
          links[branches[branches.len() - 1]] = index
          var node = opener
          while node != index {
            closes[node] = index
            node = links[node]
          }
          kinds += ["end"]
          texts += [""]
          pipes += [empty_pipe(open)]
          links += [-1]
          closes += [index]
          positions += [open]
          openers = openers[..-1]
          branches = branches[..-1]
        } else if keyword == "define" {
          if openers.len() > 0 {
            return Err(syntax_error(source, open, "`define` must appear at the top level"))
          }
          if toks.len() != 2 or toks[1].kind != "str" {
            return Err(syntax_error(source, open, "`define` takes one quoted template name"))
          }
          let name = toks[1].text
          if name in defines {
            return Err(syntax_error(source, toks[1].pos, f"template `${name}` is already defined"))
          }
          defines[name] = index
          kinds += ["define"]
          texts += [name]
          pipes += [empty_pipe(open)]
          links += [-1]
          closes += [-1]
          positions += [open]
          openers += [index]
          branches += [index]
        } else if keyword == "template" {
          if toks.len() < 2 or toks[1].kind != "str" {
            return Err(syntax_error(source, open, "`template` takes a quoted template name"))
          }
          let pipe = if toks.len() > 2 {
            parse_pipe(source, toks, 2, false, "template", open)?
          } else {
            empty_pipe(open)
          }
          kinds += ["template"]
          texts += [toks[1].text]
          pipes += [pipe]
          links += [-1]
          closes += [-1]
          positions += [toks[1].pos]
          calls += [index]
        } else {
          let pipe = parse_pipe(source, toks, 0, false, "action", open)?
          kinds += ["emit"]
          texts += [""]
          pipes += [pipe]
          links += [-1]
          closes += [-1]
          positions += [open]
        }
      }
    }
  }
  if openers.len() > 0 {
    let opener = openers[openers.len() - 1]
    return Err(
      syntax_error(source, positions[opener], f"unclosed `{{${kinds[opener]}}}`; expected `{{end}}`"),
    )
  }
  for call in calls {
    if !(texts[call] in defines) {
      return Err(syntax_error(source, positions[call], f"no template named `${texts[call]}`"))
    }
  }
  return Ok(
    {
      source: source,
      kinds: kinds,
      texts: texts,
      pipes: pipes,
      links: links,
      closes: closes,
      positions: positions,
      defines: defines,
    },
  )
}

# The runtime type a template error message names.
pure type_label(value: Any) -> Str {
  match value {
    _ is Null => return "null"
    _ is Bool => return "Bool"
    _ is Int => return "Int"
    _ is Float => return "Float"
    _ is Str => return "Str"
    _ is List[Any] => return "List"
    _ is Map[Any] => return "Map"
    _ => return "value"
  }
}

# Template truthiness: false, null, 0, 0.0, and empty Str, List, Map, and
# record values are false; every other value is true.
pure truthy(value: Any) -> Bool {
  match value {
    _ is Null => return false
    flag is Bool => return flag
    number is Int => return number != 0
    real is Float => return real != 0.0
    text is Str => return text.byte_len() > 0
    items is List[Any] => return items.len() > 0
    entries is Map[Any] => return entries.len() > 0
    _ => return true
  }
}

# The text an emitted value renders as. Only scalars render directly.
pure scalar_text(source: Str, offset: Int, value: Any) -> Result[Str] {
  match value {
    text is Str => return Ok(text)
    number is Int => return Ok(f"${number}")
    real is Float => return Ok(f"${real}")
    flag is Bool => return Ok(f"${flag}")
    _ is Null => return Err(render_error(source, offset, "cannot render null; use `default` to supply a value"))
    _ => return Err(
      render_error(source, offset, f"cannot render a ${type_label(value)} directly; use `join` or `json`"),
    )
  }
}

# Follow `fields` from `value`. A missing field is an error, never empty text.
pure walk(source: Str, offset: Int, value: Any, fields: List[Str]) -> Result[Any] {
  var current = value
  for key in fields {
    match current {
      entries is Map[Any] => {
        if !(key in entries) {
          return Err(render_error(source, offset, f"missing field `${key}`"))
        }
        current = entries.get(key)?
      }
      _ => return Err(
        render_error(source, offset, f"cannot read field `${key}` of ${type_label(current)}"),
      )
    }
  }
  return Ok(current)
}

pure eval_arg(source: Str, arg: Arg, dot: Any, vars: Map[Any]) -> Result[Any] {
  match arg.kind {
    "field" => return walk(source, arg.pos, dot, arg.fields)
    "var" => {
      if !(arg.text in vars) {
        return Err(render_error(source, arg.pos, f"undefined variable `${arg.text}`"))
      }
      return walk(source, arg.pos, vars.get(arg.text)?, arg.fields)
    }
    "str" => return Ok(arg.text)
    "int" => return Ok(arg.num)
    "bool" => return Ok(arg.num == 1)
    _ => return Ok(null)
  }
}

# Whether two values are equal for `eq` and `ne`.
#
# Both values must be scalars of the same type, except that null may be
# compared with anything and equals only null.
pure template_equal(source: Str, offset: Int, left: Any, right: Any) -> Result[Bool] {
  if left is Null or right is Null {
    return Ok(left is Null and right is Null)
  }
  let left_type = type_label(left)
  if left_type == "List" or left_type == "Map" or left_type != type_label(right) {
    return Err(
      render_error(source, offset, f"cannot compare ${left_type} with ${type_label(right)}"),
    )
  }
  return Ok(left == right)
}

pure call_function(source: Str, name: Str, offset: Int, args: List[Any]) -> Result[Any] {
  match name {
    "len" => {
      match args[0] {
        text is Str => return Ok(text.count_chars())
        items is List[Any] => return Ok(items.len())
        entries is Map[Any] => return Ok(entries.len())
        _ => return Err(render_error(source, offset, f"`len` expects Str, List, or Map, found ${type_label(args[0])}"))
      }
    }
    "upper" | "lower" | "trim" => {
      match args[0] {
        text is Str => {
          if name == "upper" {
            return Ok(text.upper())
          }
          if name == "lower" {
            return Ok(text.lower())
          }
          return Ok(text.trim())
        }
        _ => return Err(render_error(source, offset, f"`${name}` expects Str, found ${type_label(args[0])}"))
      }
    }
    "join" => {
      if !(args[0] is Str) {
        return Err(render_error(source, offset, f"`join` separator must be Str, found ${type_label(args[0])}"))
      }
      match args[1] {
        items is List[Any] => {
          let separator = args[0].require(Str)?
          var parts: List[Str] = []
          for item in items {
            parts += [scalar_text(source, offset, item)?]
          }
          return Ok(parts.join(separator))
        }
        _ => return Err(render_error(source, offset, f"`join` expects a List, found ${type_label(args[1])}"))
      }
    }
    "default" => {
      if truthy(args[1]) {
        return Ok(args[1])
      }
      return Ok(args[0])
    }
    "quote" => {
      let text = scalar_text(source, offset, args[0])?
      return Ok(json.encode(text)?)
    }
    "json" => {
      match json.encode(args[0]) {
        Ok(text) => return Ok(text)
        Err(failure) => return Err(render_error(source, offset, f"`json` failed: ${failure.message}"))
      }
    }
    "not" => return Ok(!truthy(args[0]))
    "and" => {
      if truthy(args[0]) {
        return Ok(args[1])
      }
      return Ok(args[0])
    }
    "or" => {
      if truthy(args[0]) {
        return Ok(args[0])
      }
      return Ok(args[1])
    }
    "eq" => return Ok(template_equal(source, offset, args[0], args[1])?)
    _ => return Ok(!template_equal(source, offset, args[0], args[1])?)
  }
}

pure eval_pipe(source: Str, pipe: Pipe, dot: Any, vars: Map[Any]) -> Result[Any] {
  var value: Any = null
  var stage = 0
  for cmd in pipe.cmds {
    let head = cmd.args[0]
    if head.kind == "func" {
      var args: List[Any] = []
      var index = 1
      while index < cmd.args.len() {
        args += [eval_arg(source, cmd.args[index], dot, vars)?]
        index += 1
      }
      if stage > 0 {
        args += [value]
      }
      value = call_function(source, head.text, head.pos, args)?
    } else {
      value = eval_arg(source, head, dot, vars)?
    }
    stage += 1
  }
  return Ok(value)
}

# Render nodes `start` up to but not including `stop`.
pure render_nodes(program: Program, start: Int, stop: Int, dot: Any, vars: Map[Any], depth: Int) -> Result[Str] {
  var parts: List[Str] = []
  var index = start
  while index < stop {
    let kind = program.kinds[index]
    if kind == "text" {
      parts += [program.texts[index]]
      index += 1
    } else if kind == "emit" {
      let value = eval_pipe(program.source, program.pipes[index], dot, vars)?
      parts += [scalar_text(program.source, program.positions[index], value)?]
      index += 1
    } else if kind == "if" {
      var branch = index
      var deciding = true
      while deciding {
        let next = program.links[branch]
        if program.kinds[branch] == "else" {
          parts += [render_nodes(program, branch + 1, next, dot, vars, depth)?]
          deciding = false
        } else if truthy(eval_pipe(program.source, program.pipes[branch], dot, vars)?) {
          parts += [render_nodes(program, branch + 1, next, dot, vars, depth)?]
          deciding = false
        } else if program.kinds[next] == "end" {
          deciding = false
        } else {
          branch = next
        }
      }
      index = program.closes[index] + 1
    } else if kind == "with" {
      let pipe = program.pipes[index]
      let value = eval_pipe(program.source, pipe, dot, vars)?
      let next = program.links[index]
      if truthy(value) {
        var scope = vars
        if pipe.decl.len() == 1 {
          scope[pipe.decl[0]] = value
        }
        parts += [render_nodes(program, index + 1, next, value, scope, depth)?]
      } else if program.kinds[next] == "else" {
        parts += [render_nodes(program, next + 1, program.links[next], dot, vars, depth)?]
      }
      index = program.closes[index] + 1
    } else if kind == "range" {
      parts += [render_range(program, index, dot, vars, depth)?]
      index = program.closes[index] + 1
    } else if kind == "template" {
      if depth >= MAX_TEMPLATE_DEPTH {
        return Err(
          render_error(program.source, program.positions[index], f"template calls nest deeper than ${MAX_TEMPLATE_DEPTH} levels"),
        )
      }
      let pipe = program.pipes[index]
      let data = if pipe.cmds.len() > 0 { eval_pipe(program.source, pipe, dot, vars)? } else { null }
      let define = program.defines.get(program.texts[index])?
      var scope: Map[Any] = {}
      scope["$"] = data
      parts += [render_nodes(program, define + 1, program.links[define], data, scope, depth + 1)?]
      index += 1
    } else if kind == "define" {
      index = program.closes[index] + 1
    } else {
      index += 1
    }
  }
  return Ok(parts.join(""))
}

# Render one `range` block: its body once per item, or its `else` branch when
# there are no items. Lists iterate in order; maps and records iterate in
# their canonical sorted key order.
pure render_range(program: Program, index: Int, dot: Any, vars: Map[Any], depth: Int) -> Result[Str] {
  let pipe = program.pipes[index]
  let value = eval_pipe(program.source, pipe, dot, vars)?
  let next = program.links[index]
  var keys: List[Any] = []
  var items: List[Any] = []
  match value {
    _ is Null => {}
    list is List[Any] => {
      var position = 0
      for item in list {
        keys += [position]
        items += [item]
        position += 1
      }
    }
    entries is Map[Any] => {
      for key in entries.keys() {
        keys += [key]
        items += [entries.get(key)?]
      }
    }
    _ => return Err(
      render_error(program.source, program.positions[index], f"cannot range over ${type_label(value)}"),
    )
  }
  if items.len() == 0 {
    if program.kinds[next] == "else" {
      return render_nodes(program, next + 1, program.links[next], dot, vars, depth)
    }
    return Ok("")
  }
  var parts: List[Str] = []
  var position = 0
  while position < items.len() {
    var scope = vars
    if pipe.decl.len() == 1 {
      scope[pipe.decl[0]] = items[position]
    } else if pipe.decl.len() == 2 {
      scope[pipe.decl[0]] = keys[position]
      scope[pipe.decl[1]] = items[position]
    }
    parts += [render_nodes(program, index + 1, next, items[position], scope, depth)?]
    position += 1
  }
  return Ok(parts.join(""))
}

## Render a template against data.
##
## The template is parsed completely before rendering, so a syntax error is
## reported with kind `template-syntax` even in a branch that would not run.
## Rendering failures, such as a missing field or a value that cannot be
## rendered, report kind `template-render`. Both messages begin with
## `template:LINE:COLUMN:`.
export pure render(source: Str, data: Any) -> Result[Str] {
  let program = parse(source)?
  var vars: Map[Any] = {}
  vars["$"] = data
  return render_nodes(program, 0, program.kinds.len(), data, vars, 0)
}
