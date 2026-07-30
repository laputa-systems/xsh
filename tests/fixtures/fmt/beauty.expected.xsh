# Calls and method chains: keep authored argument breaks and break chains between calls.
let fmt_source_path = Path("source")
let fmt_normalized = "a very long source name with slash / and dash - that keeps the chain readable".replace("/", "_")
  .replace("-", "_")
let fmt_selected = "alpha,beta,gamma".split(",").get(0, "")
let fmt_encoded = bytes.from_text(
  "generated source metadata that should remain grouped as one call argument",
)

# Collections and comprehensions: expand related siblings and nested records consistently.
let fmt_items = [{name: "one", value: "1", enabled: true}, {name: "two", value: "2", enabled: false}]
let fmt_source_shaped = [
  1,
  2,
]
let fmt_rows = [
  {
    name: "short",
  },
  {
    name: "a deliberately long record value that forces every structurally similar sibling record to use the expanded shape",
  },
]
let fmt_nested = [
  {
    meta: {
      name: "short",
    },
  },
  {
    meta: {
      name: "another deliberately long nested record value that should expand with its parent collection",
    },
  },
]
let fmt_filtered = [item.name for item in fmt_items if item.enabled]
let fmt_by_name = {item.name: item.value for item in fmt_items if item.enabled}

# Control-flow expressions: preserve authored multiline branches and match arms.
let fmt_user_name = "administrator"
let fmt_mode = "production"
let fmt_deployment = "rolling"
let fmt_choice = if fmt_user_name == "administrator" and fmt_mode == "production" and fmt_deployment == "rolling" {
  "allow"
} else {
  "deny"
}
let fmt_label = match Ok("ready") {
  Ok(value) => value,
  _ => "fallback",
}
let fmt_authored = if true {
  "keep this authored branch shape"
} else {
  "drop this authored branch shape"
}

# Comments: keep leading, trailing, nested, and fmt: skip comments attached.
# leading comment stays with the binding
let fmt_value = 1 # trailing comment stays with the binding

proc fmt_main() {
  # explain the nested command shape
  let fmt_before = 1

  # fmt: skip
  let fmt_skipped=1+2
  let fmt_after = 3
}

print "done"

# Width boundaries: do not split long literals, paths, or formatted strings.
let fmt_source_url = "https://downloads.example.test/releases/xsh/generated/source-index/2026/07/30/package-with-a-deliberately-unbreakable-name.tar.zst"
let fmt_source_path_literal = /var/lib/xsh/cache/generated/source-index/2026/07/30/package-with-a-deliberately-unbreakable-name.tar.zst
let fmt_label_text = f"source: ${fmt_source_path_literal.display()}"
let fmt_predicate = "a generated predicate with a long explanatory literal that should not be split".contains(
  "explanatory",
)
