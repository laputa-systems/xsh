test test_expression_selection_runs_subjects_and_selected_operands_once { |ctx|
  let output = test.expect(
    ctx,
    r"""enum Choice { Chosen(Int), Other }
error OrderError = Failed(message: Str)
proc number(label: Str, value: Int) [io] -> Int { print $label; return value }
proc flag(label: Str, value: Bool) [io] -> Bool { print $label; return value }
proc text(label: Str, value: Str) [io] -> Str { print $label; return value }
proc choice(label: Str, value: Choice) [io] -> Choice { print $label; return value }
proc outcome(label: Str, success: Bool) [io] -> Result[Int, OrderError] {
  print $label
  if success { return Ok(4) }
  return Err(OrderError.Failed(message: "missing"))
}

let chain = number("chain:left", 1) < number("chain:middle", 2) <= number("chain:right", 2)
print f"chain={chain}"
let stopped = number("stop:left", 3) < number("stop:middle", 2) < number("stop:late", 1 / 0)
print f"stopped={stopped}"

let branch = if flag("if:first", false) {
  number("if:wrong", 0)
} else if flag("if:second", true) {
  number("if:selected", 7)
} else {
  number("if:else", 0)
}
print f"if={branch}"

let patterned = if let Chosen(value) = choice("pattern:subject", Chosen(8)) {
  number("pattern:selected", value)
} else {
  number("pattern:else", 0)
}
print f"pattern={patterned}"

let matched = match choice("match:subject", Chosen(9)) {
  Other if flag("match:unselected-guard", true) => number("match:wrong", 0)
  Chosen(value) if flag("match:declined-guard", false) => number("match:declined", value)
  Chosen(value) if flag("match:selected-guard", true) => number("match:selected", value)
  _ => number("match:else", 0)
}
print f"match={matched}"

let string_match = match text("str-match:subject", "ready") {
  "missing" => number("str-match:wrong", 0)
  "ready" => number("str-match:selected", 10)
  else => number("str-match:else", 0)
}
print f"str-match={string_match}"

let tag_match = match choice("tag-match:subject", Chosen(11)) {
  Other => number("tag-match:wrong", 0)
  Chosen(_) => number("tag-match:selected", 11)
  else => number("tag-match:else", 0)
}
print f"tag-match={tag_match}"

let kept = outcome("fallback:ok", true) ?? { |_| number("fallback:wrong", 0) }
let recovered = outcome("fallback:err", false) ?? { |failure|
  print f"fallback:handler:{failure.message}"
  number("fallback:selected", 12)
}
print f"fallback={kept},{recovered}"
""",
    status: 0,
  )?
  assert output.stdout == """chain:left
chain:middle
chain:right
chain=true
stop:left
stop:middle
stopped=false
if:first
if:second
if:selected
if=7
pattern:subject
pattern:selected
pattern=8
match:subject
match:declined-guard
match:selected-guard
match:selected
match=9
str-match:subject
str-match:selected
str-match=10
tag-match:subject
tag-match:selected
tag-match=11
fallback:ok
fallback:err
fallback:handler:missing
fallback:selected
fallback=4,12
"""
}

test test_comprehensions_and_pipeline_visit_sources_and_items_in_order { |ctx|
  let output = test.expect(
    ctx,
    r"""proc source(label: Str) [io] -> List[Int] {
  print $label
  return [1, 2, 3]
}
proc keep(label: Str, value: Int) [io] -> Bool {
  print f"{label}:{value}"
  return value != 2
}
proc project(label: Str, value: Int) [io] -> Int {
  print f"{label}:{value}"
  return value * 10
}
proc key(label: Str, value: Int) [io] -> Str {
  print f"{label}:{value}"
  return f"item-{value}"
}

let list = [project("list:value", value) for value in source("list:source") if keep("list:filter", value)]
let mapped = {[key("map:key", value)]: project("map:value", value) for value in source("map:source") if keep("map:filter", value)}
let pipeline = source("pipeline:source") |> map { |value| project("pipeline:map", value) }
print f"list={list[0]},{list[1]} map={mapped["item-1"]},{mapped["item-3"]} pipeline={pipeline[0]},{pipeline[1]},{pipeline[2]}"
""",
    status: 0,
  )?
  assert output.stdout == """list:source
list:filter:1
list:value:1
list:filter:2
list:filter:3
list:value:3
map:source
map:filter:1
map:key:1
map:value:1
map:filter:2
map:filter:3
map:key:3
map:value:3
pipeline:source
pipeline:map:1
pipeline:map:2
pipeline:map:3
list=10,30 map=10,30 pipeline=10,20,30
"""
}

test test_calls_methods_and_module_operations_preserve_argument_order { |ctx|
  let output = test.expect(
    ctx,
    r"""type OrderedPair = proc(left: Int, right: Int) [io] -> Int
proc integer(label: Str, value: Int) [io] -> Int { print $label; return value }
proc string(label: Str, value: Str) [io] -> Str { print $label; return value }
proc add(left: Int, right: Int) [io] -> Int { print "call:body"; return left + right }
pure pure_add(left: Int, right: Int) -> Int { left + right }
proc receiver() [io] -> Str { print "method:receiver"; return "abc" }
proc json_value() [io] -> Any { print "module:value"; return {value: "x"} }
proc pretty_flag() [io] -> Bool { print "module:pretty"; return false }
proc descend(depth: Int) [io] -> Int {
  print f"self:body:{depth}"
  if depth == 0 { return 0 }
  return descend(integer("self:argument", depth - 1)) + 1
}

let direct = add(integer("call:first", 1), integer("call:second", 2))
let alias = add
let dynamic = alias(integer("dynamic:first", 3), integer("dynamic:second", 4))
let typed: OrderedPair = add
let selected = typed(right: integer("typed:right", 6), left: integer("typed:left", 5))
let erased: Proc = typed
let erased_result = erased.call(integer("proc-result:first", 7), integer("proc-result:second", 8))?.require(Int)?
let pure_result = pure_add(20, 22)
let recursion = descend(2)
let replaced = receiver().replace(string("method:old", "a"), with: string("method:new", "z"))
let encoded = json.encode(json_value(), pretty: pretty_flag())?
print f"calls={direct},{dynamic},{selected},{erased_result},{pure_result},{recursion}"
print f"method={replaced} module={encoded.is_empty()}"
""",
    status: 0,
  )?
  assert output.stdout == """call:first
call:second
call:body
dynamic:first
dynamic:second
call:body
typed:right
typed:left
call:body
proc-result:first
proc-result:second
call:body
self:body:2
self:argument
self:body:1
self:argument
self:body:0
method:receiver
method:old
method:new
module:value
module:pretty
calls=3,7,11,15,42,2
method=zbc module=false
"""
  let pure_failure = test.run_script(
    ctx,
    r"""pure add(left: Int, right: Int) -> Int { left + right }
let value = add(1 / 0, "not-an-int".parse_int()?)
print $value
""",
  )?
  assert ! pure_failure.success, pure_failure.stderr
  assert "division-by-zero" in pure_failure.stderr, pure_failure.stderr
  assert "invalid digit" not in pure_failure.stderr, pure_failure.stderr
}

test test_external_call_evaluates_each_argument_once { |ctx|
  let root = test.temp_dir(ctx, name: "operand-order-external")?
  let implementation = fp"{root}/renderer.xsh"
  implementation.write(r"""##! Template renderer used to check external-call argument order.
proc mark_template() [io] -> Str {
  print "external:template"
  return "{{.name}}"
}

proc mark_vars() [io] -> Map[Str] {
  print "external:vars"
  return {name: "web"}
}

## Renders values returned by the two marked arguments.
export proc render() [io] -> Result[Str, Error] {
  template.render(mark_template(), mark_vars())
}
""")
  let output = test.expect(
    ctx,
    f"""type Renderer = module {{
  export proc render() [io] -> Result[Str, Error]
}}

let loaded = module.load(p"{implementation}")?.require(Renderer)?
print ${{loaded.render()?}}
""",
    status: 0,
  )?
  assert output.stdout == """external:template
external:vars
web
"""
}

test test_value_blocks_contexts_loop_retry_try_fail_and_abort_keep_payload_order { |ctx|
  let output = test.expect(
    ctx,
    r"""error RetryError = Again(message: Str)
proc text(label: Str, value: Str) [io] -> Str { print $label; return value }
proc number(label: Str, value: Int) [io] -> Int { print $label; return value }
proc delay(label: Str) [io] -> Duration { print $label; return 0ms }

let block = {
  print "value-block:statement"
  number("value-block:tail", 5)
}
print f"value-block={block}"

let scoped = env ({OPERAND_ORDER_SCOPE: text("scope:input", "inside")}) {
  print "scope:body:first"
  print "scope:body:second"
  e"OPERAND_ORDER_SCOPE"?
}?
print f"scope={scoped}"

let contextual = try {
  ctx text("context:label", "loading") {
    print "context:body"
    error.fail("broken")?
  }
}
match contextual {
  Err(error) => print f"context:error:{error.message}"
  Ok(_) => print "context:wrong"
}

let looped = loop {
  print "expression-loop:body"
  break number("expression-loop:break", 6)
}
print f"expression-loop={looped}"

var attempts = 0
let retried = retry [delay("retry:delay:first"), delay("retry:delay:second")] {
  attempts += 1
  print f"retry:attempt:{attempts}"
  if attempts == 1 { Err(RetryError.Again(message: "again"))? }
  7
}?
print f"retry={retried}"

proc capture() -> Result[Int, Error] {
  let captured: Result[Int, Error] = try {
    print "try:capture:first"
    print "try:capture:second"
    fail text("try:fail-message", "captured")
    number("try:unreachable", 0)
  }
  captured
}
let captured = capture()
match captured {
  Err(error) => print f"try:captured:{error.message}"
  Ok(_) => print "try:wrong"
}
""",
    status: 0,
  )?
  assert output.stdout == """value-block:statement
value-block:tail
value-block=5
scope:input
scope:body:first
scope:body:second
scope=inside
context:label
context:body
context:error:broken (ctx: loading)
expression-loop:body
expression-loop:break
expression-loop=6
retry:delay:first
retry:delay:second
retry:attempt:1
retry:attempt:2
retry=7
try:capture:first
try:capture:second
try:fail-message
try:captured:captured
"""

  let aborted = test.expect(
    ctx,
    r"""proc status() [io] -> Int { print "abort:status"; return 23 }
exit status()
""",
    status: 23,
  )?
  assert aborted.stdout == """abort:status
"""
}

test test_guard_with_and_statement_branches_evaluate_selected_payloads_once { |ctx|
  let output = test.expect(
    ctx,
    r"""enum Choice { Chosen(Int), Other }
error ParseError = Failed(message: Str)
proc parsed(label: Str, succeeds: Bool) [io] -> Result[Int, ParseError] {
  print $label
  if succeeds { return Ok(4) }
  return Err(ParseError.Failed(message: "missing"))
}
proc flag(label: Str, value: Bool) [io] -> Bool { print $label; return value }
proc choice(label: Str, value: Choice) [io] -> Choice { print $label; return value }
proc text(label: Str, value: Str) [io] -> Str { print $label; return value }

proc guarded() [io] {
  guard let value = parsed("guard:subject", false) else { |failure|
    print f"guard:else:{failure.message}"
    return
  }
  print f"guard:wrong:{value}"
}
guarded()

with first = parsed("with:first", true), second = parsed("with:second", false), third = parsed("with:skipped", true) {
  print f"with:wrong:{first},{second},{third}"
} else { |failure| print f"with:else:{failure.message}" }

with value = parsed("with:body-binding", true) {
  print "with:body:first"
  print "with:body:second"
} else { |failure| print f"with:body-error:{failure.message}" }

if flag("if:dynamic-condition", true) { print "if:dynamic-body" } else { print "if:wrong" }
let enabled = true
if enabled { print "if-bool:body" } else { print "if-bool:wrong" }

var dynamic_checks = 0
proc dynamic_condition() [io] -> Bool {
  dynamic_checks += 1
  print f"while:condition:{dynamic_checks}"
  return dynamic_checks < 3
}
while dynamic_condition() { print "while:body" }

var bool_count = 0
while bool_count < 2 {
  print f"while-bool:body:{bool_count}"
  bool_count += 1
}

if let Chosen(value) = choice("pattern-if:subject", Other) {
  print f"pattern-if:wrong:{value}"
} else {
  print "pattern-if:else"
}

var pattern_checks = 0
proc pattern_condition() [io] -> Result[Int, ParseError] {
  print f"pattern-while:condition:{pattern_checks}"
  if pattern_checks < 2 { return Ok(pattern_checks) }
  return Err(ParseError.Failed(message: "done"))
}
while let Ok(value) = pattern_condition() {
  print f"pattern-while:body:{value}"
  pattern_checks += 1
}

match choice("match:subject", Chosen(8)) {
  Other if flag("match:unselected-guard", true) => print "match:wrong"
  Chosen(value) if flag("match:declined-guard", false) => print f"match:declined:{value}"
  Chosen(value) if flag("match:selected-guard", true) => print f"match:selected:{value}"
  _ => print "match:else"
}
match text("str-match:subject", "ready") {
  "missing" => print "str-match:wrong"
  "ready" => print "str-match:selected"
  else => print "str-match:else"
}
match choice("tag-match:subject", Chosen(1)) {
  Other => print "tag-match:wrong"
  Chosen(_) => print "tag-match:selected"
  else => print "tag-match:else"
}

proc values() [io] -> List[Int] { print "for:source"; return [2, 3] }
for value in values() { print f"for:item:{value}" }
type Entry = {name: Str, value: Int}
proc entries() [io] -> List[Entry] {
  print "for-record:source"
  return [{name: "a", value: 4}, {name: "b", value: 5}]
}
for {name, value} in entries() { print f"for-record:{name}:{value}" }
proc lines() [io] -> Str { print "for-lines:source"; return "a\nb\n" }
for line in lines().lines() { print f"for-lines:{line}" }

var loop_count = 0
loop {
  loop_count += 1
  print f"statement-loop:{loop_count}"
  break when loop_count == 2
}

proc returned() [io, error] -> Int {
  defer { print "return:cleanup" }
  return parsed("return:value", true)?
}
print f"return={returned()}"

stream yielded() [io, error] -> Stream[Int] {
  yield parsed("yield:value", true)?
}
for value in yielded() { print f"yield:item:{value}" }

proc delegated_values() [io] -> List[Int] { print "yield-delegate:source"; return [6, 7] }
stream delegated() [io] -> Stream[Int] { yield @delegated_values() }
for value in delegated() { print f"yield-delegate:item:{value}" }

{
  defer { print "defer:first" }
  defer { print "defer:second" }
  print "defer:body"
}
""",
    status: 0,
  )?
  assert output.stdout == """guard:subject
guard:else:missing
with:first
with:second
with:else:missing
with:body-binding
with:body:first
with:body:second
if:dynamic-condition
if:dynamic-body
if-bool:body
while:condition:1
while:body
while:condition:2
while:body
while:condition:3
while-bool:body:0
while-bool:body:1
pattern-if:subject
pattern-if:else
pattern-while:condition:0
pattern-while:body:0
pattern-while:condition:1
pattern-while:body:1
pattern-while:condition:2
match:subject
match:declined-guard
match:selected-guard
match:selected:8
str-match:subject
str-match:selected
tag-match:subject
tag-match:selected
for:source
for:item:2
for:item:3
for-record:source
for-record:a:4
for-record:b:5
for-lines:source
for-lines:a
for-lines:b
statement-loop:1
statement-loop:2
return:value
return:cleanup
return=4
yield:value
yield:item:4
yield-delegate:source
yield-delegate:item:6
yield-delegate:item:7
defer:body
defer:second
defer:first
"""
}

test test_default_parameters_run_only_for_omitted_arguments_in_parameter_order { |ctx|
  let output = test.expect(
    ctx,
    r"""proc mark(label: Str, value: Int) [io] -> Int { print $label; return value }
proc combine(left = mark("default:left", 1), right = mark("default:right", 2)) [io] -> Int {
  print "default:body"
  return left + right
}
print f"defaulted={combine()}"
print f"supplied={combine(right: mark("supplied:right", 4), left: mark("supplied:left", 3))}"
""",
    status: 0,
  )?
  assert output.stdout == """default:left
default:right
default:body
defaulted=3
supplied:right
supplied:left
default:body
supplied=7
"""
}
