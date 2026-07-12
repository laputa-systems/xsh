# curated formatter corpus
let source = p"."
let items = [{name: "one", enabled: true}, {name: "two", enabled: false}]
let source_shaped = [
  1,
  2,
]
let rows = [{name: "short"}, {name: "a deliberately long record value that forces its sibling to break too"}]
let nested = [{meta: {name: "short"}}, {meta: {name: "another deliberately long nested record value"}}]
let filtered = [item.name for item in items if item.enabled]
let by_name = {
  item.name: f"${item.name}"
  for item in items
  if item.enabled
}
let chain = source.display().replace("/", "_").replace("-", "_")
# fmt: skip
let skipped=1+2
