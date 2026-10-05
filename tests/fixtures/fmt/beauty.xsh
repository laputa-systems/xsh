# Calls and method chains: keep authored argument breaks and break chains between calls.
let fmt_source_path = Path("source")
let fmt_normalized = "a long source name with slash / and dash - that keeps the chain readable".replace("/", with: "_").replace("-", with: "_")
let fmt_selected = "alpha,beta,gamma".split(",").get(0) ?? ""
let fmt_encoded = bytes.from_text(
  "generated source metadata that should remain grouped as one call argument",
)

# Named and spliced arguments: preserve authored breaks and positional expansion.
pure fmt_pair(first: Str, second: Str) -> Str { f"{first}/{second}" }
let fmt_named = process.command_argv(
  "echo",
  ["echo", "ready"],
  timeout: 1s,
)
let fmt_parts = ["left", "right"]
let fmt_spliced = fmt_pair(@fmt_parts)

# Collections and comprehensions: expand related siblings and nested records consistently, keep flat rows that fit.
let fmt_items = [{name: "one", value: "1", enabled: true}, {name: "two", value: "2", enabled: false}]
let fmt_source_shaped = [
  1,
  2,
]
let fmt_rows = [{name: "short"}, {name: "a deliberately long record value that forces every structurally similar sibling record to use the expanded shape"}]
let fmt_nested = [{meta: {name: "short"}}, {meta: {name: "another deliberately long nested record value that cannot fit on one line, so it expands with its parent collection"}}]
let fmt_table = [
  {name: "one", value: "1", enabled: true},
  {name: "two", value: "2", enabled: false},
]
let fmt_wide_record = {a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7}
let fmt_many_items = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
let fmt_filtered = [item.name for item in fmt_items if item.enabled]
let fmt_by_name = {item.name: item.value for item in fmt_items if item.enabled}

# Control-flow expressions: preserve authored multiline branches and match arms.
let fmt_user_name = "administrator"
let fmt_mode = "production"
let fmt_deployment = "rolling"
let fmt_choice = if fmt_user_name == "administrator" and fmt_mode == "production" and fmt_deployment == "rolling" { "allow" } else { "deny" }
let fmt_label = match Ok("ready") {
  Ok(value) => value
  _ => "fallback"
}
let fmt_authored = if true {
  "keep this authored branch shape"
} else {
  "drop this authored branch shape"
}
let fmt_nested_control = fmt_pair(
  first: if true { "yes" } else { "no" },
  second: match Ok("ready") {
    Ok(value) => value
    _ => "fallback"
  },
)

# Short blocks: a block the author wrote on one line stays there when it fits.
pure fmt_sign(value: Int) -> Int {
  if value < 0 { return -1 }
  if value == 0 { 0 } else { 1 }
}
let fmt_increments = [1, 2, 3] |> map { |value| value + 1 }
match fmt_increments.len() {
  0 => print "none"
  count => if count > 1 { print "many" } else { print "one" }
}
let fmt_spread = fmt_pair(...{first: if true {
  "yes"
} else {
  "no"
}}, second: "b")

# Pipelines, operators, and result propagation: keep authored stage and operator line breaks, stage bodies, and ? attached.
let fmt_doubled = [1, 2, 3]
|> map { |value|
value * 2
}
let fmt_total = [1, 2, 3] |> where . > 1 |> sum
let fmt_folded = [1, 2, 3] |> fold(0) { |total, value|
  let next = total + value
  next
}
let fmt_ready = fmt_items.len() > 0
  and fmt_parts.len() > 0
pure fmt_ordered(first: Int, second: Int) -> Bool {
  first > 0 and
    second > 0 and
    first < second
}
proc fmt_get() [error] -> Result[Str] { Ok("ready") }
proc fmt_use() [error] -> Result[Str] {
  let value = fmt_get(
  )?
  return Ok(value)
}

# Comments: keep leading, trailing, nested, block-ending, and fmt: skip comments in place.
# leading comment stays with the binding
let fmt_value=1 # trailing comment stays with the binding

proc fmt_main() {
  # explain the nested command shape
  let fmt_before=1
  # fmt: skip
  let fmt_skipped=1+2
  let fmt_after=3

  # an authored blank line before a comment stays

  # and so does one between a comment and its statement
  print $fmt_after
  # a comment that ends a block stays inside it
}
# begin example
let fmt_region = "region markers keep their place"
# end example
let fmt_delimited_list = [
  "first",
  "second",
] # keep with the completed collection
let fmt_delimited_call = fmt_pair(
  "left",
  "right",
) # keep with the completed call



let fmt_authored_gap = "one section after multiple blank lines"

# fmt: skip
let fmt_skipped_with_comment=1+2 # keep this complete statement

print "done"

# Literals and width boundaries: keep string escapes and quoted words, and do not split long literals, paths, or formatted strings.
let fmt_source_url = "https://downloads.example.test/releases/xsh/generated/source-index/2026/07/30/package-with-a-deliberately-unbreakable-name.tar.zst"
let fmt_source_path_literal = p"/var/lib/xsh/cache/generated/source-index/2026/07/30/package-with-a-deliberately-unbreakable-name.tar.zst"
let fmt_label_text = f"source: {fmt_source_path_literal.display()}"
let fmt_escaped_lines = "first\nsecond\n"
print "-j${fmt_items.len()}"
let fmt_predicate = "a generated predicate with a long explanatory literal that should not be split".starts_with("a generated")
