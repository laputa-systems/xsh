# Common mistakes from shell, Python, and XSH itself. Each snippet must get
# exactly one diagnostic, with the expected code, a message that names the
# real cause, and, where the repair is mechanical, a fix hint. A second
# diagnostic would be noise the newcomer has to read past.
type Mistake = {name: Str, source: Str, code: Str, cause: Str, fix: Str}

proc assert_one_diagnostic(ctx: TestContext, mistake: Mistake) [fs, process, error] {
  let file = test.temp_file(ctx, name: mistake.name + ".xsh", contents: bytes.from_text(mistake.source))?
  let checked = run.capture --text "xsht" check $file ?
  let report = mistake.name + ":\n" + checked.stderr
  assert ! checked.status.exited_with(0), report
  let headers = [line for line in checked.stderr.lines() if line.starts_with("err[") or line.starts_with("warn[")]
  assert headers.len() == 1, report
  assert headers[0].starts_with("err[" + mistake.code + "]"), report
  assert mistake.cause in checked.stderr, report
  if mistake.fix != "" {
    let hints = [line for line in checked.stderr.lines() if line.starts_with("help: ") and mistake.fix in line]
    assert hints.len() == 1, report
  }
}

test test_known_diagnostic_gaps_report_one_precise_cause { |ctx|
  for mistake in [
    {
      name: "detached_else",
      source: r"""let x = 1
if x > 0 {
  print yes
}
else {
  print no
}
""",
      code: "parse.detached-else",
      cause: "on the same line as the `}`",
      fix: "join `else`",
    },
    {
      name: "detached_value_else",
      source: r"""let x = 1
let label = if x > 0 {
  "yes"
}
else {
  "no"
}
print $label
""",
      code: "parse.detached-else",
      cause: "on the same line as the `}`",
      fix: "join `else`",
    },
    {
      name: "unit_displayed",
      source: r"""proc greet() [io] { print hi }
let greeted = greet()?
print ${greeted}
""",
      code: "check.display-conversion",
      cause: "value of type `Unit`",
      fix: "",
    },
    {
      name: "unit_command_argument",
      source: r"""proc greet() [io] { print hi }
let greeted = greet()?
run echo ${greeted}
""",
      code: "check.argv-conversion",
      cause: "value of type `Unit`",
      fix: "",
    },
    {
      name: "value_match_missing_variant",
      source: r"""enum Color { Red, Green, Blue }
let color = Red
let name = match color {
  Red => "red"
  Green => "green"
}
print $name
""",
      code: "check.match-value-exhaustive",
      cause: "missing variant(s) `Blue`",
      fix: "",
    },
  ] {
    assert_one_diagnostic(ctx, mistake)?
  }
}

test test_shell_habits_name_the_xsh_spelling { |ctx|
  for mistake in [
    {
      name: "shell_test_brace",
      source: r"""if [ -f x ] {
  print yes
}
""",
      code: "parse.foreign-syntax",
      cause: "shell test syntax",
      fix: "",
    },
    {
      name: "shell_test_then",
      source: r"""if [ -f x ]; then
  print yes
fi
""",
      code: "parse.foreign-syntax",
      cause: "shell test syntax",
      fix: "",
    },
    {
      name: "shell_double_bracket_test",
      source: r"""let x = "a"
if [[ -n $x ]] {
  print yes
}
""",
      code: "parse.foreign-syntax",
      cause: "shell test syntax",
      fix: "",
    },
    {
      # A nested list or comprehension also starts with `[[`; it is a
      # non-Bool condition, not shell syntax.
      name: "comprehension_condition_is_not_shell_syntax",
      source: r"""let xs = [1]
if [[x] for x in xs] {
  print yes
}
""",
      code: "check.if-condition",
      cause: "condition must be Bool",
      fix: "",
    },
    {
      name: "shell_while_do",
      source: r"""var count = 0
while [ $count -lt 3 ]; do
  count += 1
done
""",
      code: "parse.foreign-syntax",
      cause: "shell test syntax",
      fix: "",
    },
    {
      name: "assignment_without_let",
      source: r"""count=1
print $count
""",
      code: "check.undefined-name",
      cause: "declare it with `let` or `var`",
      fix: "-> let",
    },
    {
      name: "command_substitution",
      source: r"""let listing = $(ls)
print $listing
""",
      code: "lex.unexpected-character",
      cause: "command substitution is shell syntax",
      fix: "",
    },
    {
      name: "double_ampersand",
      source: r"""let a = true
if a && false { print yes }
""",
      code: "parse.unsupported-boolean-operator",
      cause: "word forms 'and'",
      fix: "-> and",
    },
    {
      name: "double_pipe",
      source: r"""let a = true
if a || false { print yes }
""",
      code: "parse.unsupported-boolean-operator",
      cause: "word forms 'or'",
      fix: "-> or",
    },
    {
      name: "function_keyword",
      source: r"""function greet() {
  print hi
}
""",
      code: "parse.foreign-syntax",
      cause: "not `function`",
      fix: "-> proc",
    },
    {
      name: "local_declaration",
      source: r"""proc show() [io] {
  local count=1
  print $count
}
show()
""",
      code: "parse.foreign-syntax",
      cause: "not `local`",
      fix: "-> let",
    },
    {
      name: "export_assignment",
      source: r"""export CC=clang
""",
      code: "parse.export-target",
      cause: "env NAME=value",
      fix: "",
    },
    {
      name: "source_script",
      source: r"""source env.sh
""",
      code: "check.unresolved-proc-command",
      cause: "does not source shell scripts",
      fix: "",
    },
    {
      name: "echo_command",
      source: r"""echo hello
""",
      code: "check.unresolved-proc-command",
      cause: "print text with `print`",
      fix: "-> print",
    },
    {
      name: "elif_keyword",
      source: r"""let x = 1
if x > 0 {
  print positive
} elif x < 0 {
  print negative
}
""",
      code: "parse.foreign-syntax",
      cause: "`else if`",
      fix: "-> else if",
    },
    {
      name: "environment_variable_name",
      source: r"""print $HOME
""",
      code: "check.unresolved-name",
      cause: "env.Str.HOME?",
      fix: "",
    },
    {
      name: "increment_operator",
      source: r"""var count = 0
count++
print $count
""",
      code: "parse.foreign-syntax",
      cause: "no `++` operator",
      fix: "+= 1",
    },
    {
      name: "single_quoted_text",
      source: r"""let greeting = 'hello'
print $greeting
""",
      code: "lex.unexpected-character",
      cause: "single quotes do not delimit strings",
      fix: "-> \"hello\"",
    },
    {
      name: "cd_without_block",
      source: r"""cd /tmp
""",
      code: "parse.expected-token",
      cause: "only for its block",
      fix: "",
    },
  ] {
    assert_one_diagnostic(ctx, mistake)?
  }
}

test test_python_habits_name_the_xsh_spelling { |ctx|
  for mistake in [
    {
      name: "print_call",
      source: r"""let x = 1
print(x)
""",
      code: "check.unresolved-call",
      cause: "`print` is a command",
      fix: "-> print (x)",
    },
    {
      name: "len_call",
      source: r"""let items = [1, 2]
print ${len(items)}
""",
      code: "check.unresolved-call",
      cause: "sizes are methods",
      fix: "-> items.len()",
    },
    {
      name: "capitalized_true",
      source: r"""let enabled = True
print $enabled
""",
      code: "check.unresolved-name",
      cause: "unresolved name `True`",
      fix: "-> true",
    },
    {
      name: "none_literal",
      source: r"""let limit: Int? = None
print ${limit ?? 0}
""",
      code: "check.unresolved-name",
      cause: "unresolved name `None`",
      fix: "-> null",
    },
    {
      name: "list_append",
      source: r"""var items = [1]
items.append(2)
print ${items.len()}
""",
      code: "check.unknown-method",
      cause: "items += [value]",
      fix: "",
    },
    {
      name: "discarded_push",
      source: r"""var items = [1]
items.push(2)
print ${items.len()}
""",
      code: "check.non-tail-expression",
      cause: "`.push` returns a new list",
      fix: "",
    },
    {
      name: "ternary_operator",
      source: r"""let verbose = true
let level = verbose ? 2 : 1
print $level
""",
      code: "parse.foreign-syntax",
      cause: "no `? :` conditional operator",
      fix: "",
    },
    {
      name: "try_catch",
      source: r"""try {
  print trying
} catch failure {
  print failed
}
""",
      code: "parse.foreign-syntax",
      cause: "XSH has no `catch`",
      fix: "",
    },
    {
      name: "def_keyword",
      source: r"""def greet():
  print hi
""",
      code: "parse.foreign-syntax",
      cause: "not `def`",
      fix: "-> proc",
    },
    {
      name: "untyped_parameters",
      source: r"""proc add(a, b) { a + b }
""",
      code: "parse.expected-param-type",
      cause: "write `a: Type`",
      fix: "",
    },
    {
      name: "colon_block",
      source: r"""let x = 1
if x > 0:
  print yes
""",
      code: "parse.expected-token",
      cause: "expected `{` to start block",
      fix: "",
    },
  ] {
    assert_one_diagnostic(ctx, mistake)?
  }
}

test test_xsh_typing_mistakes_name_the_real_cause { |ctx|
  for mistake in [
    {
      name: "fallible_bool_condition",
      source: r"""if fs.exists(p"/tmp") { print yes }
""",
      code: "check.if-condition",
      cause: "found Result[Bool, Error]",
      fix: "-> ?",
    },
    {
      name: "fallible_int_condition",
      source: r"""if "4".parse_int() { print yes }
""",
      code: "check.if-condition",
      cause: "found Result[Int, Error]",
      fix: "",
    },
    {
      name: "fallible_assert_condition",
      source: r"""assert "4".parse_int()
""",
      code: "check.assert-condition",
      cause: "found Result[Int, Error]",
      fix: "",
    },
    {
      name: "missing_propagation",
      source: r"""let count: Int = "4".parse_int()
print $count
""",
      code: "check.type-mismatch",
      cause: "expected Int, found Result[Int, Error]",
      fix: "",
    },
    {
      name: "missing_propagation_in_arithmetic",
      source: r"""let next = "4".parse_int() + 1
print $next
""",
      code: "check.operator-type",
      cause: "unwrap the Result with `?` or `??`",
      fix: "",
    },
    {
      name: "value_if_without_else",
      source: r"""let flag = true
let value = if flag { 1 }
print $value
""",
      code: "parse.if-expression-else",
      cause: "require an `else` branch",
      fix: "",
    },
    {
      name: "let_reassigned",
      source: r"""let count = 1
count = 2
print $count
""",
      code: "check.assign-let",
      cause: "declare with `var`",
      fix: "",
    },
    {
      name: "parameter_reassigned",
      source: r"""proc next(count: Int) -> Int {
  count = count + 1
  count
}
""",
      code: "check.assign-let",
      cause: "cannot assign to `count`",
      fix: "",
    },
    {
      name: "field_typo",
      source: r"""let entry = {name: "a", size: 1}
print ${entry.nmae}
""",
      code: "check.unknown-field",
      cause: "did you mean `name`?",
      fix: "",
    },
    {
      name: "module_function_typo",
      source: r"""let present = fs.exits(p"/tmp")?
print $present
""",
      code: "check.unknown-module-api",
      cause: "did you mean `fs.exists`?",
      fix: "",
    },
    {
      name: "method_typo",
      source: r"""print ${"a".uper()}
""",
      code: "check.unknown-method",
      cause: "`upper()`",
      fix: "",
    },
    {
      name: "missing_argument",
      source: r"""proc add(a: Int, b: Int) -> Int { a + b }
print ${add(1)}
""",
      code: "check.arity",
      cause: "expected 2 arguments, found 1",
      fix: "",
    },
    {
      name: "ignored_result",
      source: r"""proc parse() [error] -> Result[Int] { 1 }
parse()
""",
      code: "check.ignored-result",
      cause: "handle it with `?` or `??`",
      fix: "",
    },
    {
      name: "string_repetition",
      source: r"""let rule = "-" * 3
print $rule
""",
      code: "check.operator-type",
      cause: "`*` is not defined for Str",
      fix: "",
    },
    {
      name: "path_concatenation",
      source: r"""let target = p"/tmp" + "/x"
print $target
""",
      code: "check.operator-type",
      cause: "operators never join paths",
      fix: "",
    },
    {
      name: "path_compound_append",
      source: r"""var target = p"/tmp"
target += "/x"
print $target
""",
      code: "check.operator-type",
      cause: "operators never join paths",
      fix: "",
    },
    {
      name: "path_compared_to_text",
      source: r"""let root = p"/tmp"
if root == "/tmp" { print yes }
""",
      code: "check.type-mismatch",
      cause: "expected Path, found Str",
      fix: "",
    },
    {
      name: "bool_statement",
      source: r"""let count = 1
count == 1
""",
      code: "check.bool-statement",
      cause: "not an assertion",
      fix: "insert `assert`",
    },
    {
      name: "dollar_in_expression",
      source: r"""let count = 1
let next = $count + 1
print $next
""",
      code: "parse.expected-expression",
      cause: "use `count` here",
      fix: "",
    },
    {
      name: "redeclared_var",
      source: r"""var count = 1
var count = 2
print $count
""",
      code: "check.duplicate-name",
      cause: "duplicate name in scope",
      fix: "",
    },
    {
      name: "status_compared_to_int",
      source: r"""let status = run.status ls
if status == 0 { print ok }
""",
      code: "check.type-mismatch",
      cause: "expected Status, found Int",
      fix: "",
    },
  ] {
    assert_one_diagnostic(ctx, mistake)?
  }
}
