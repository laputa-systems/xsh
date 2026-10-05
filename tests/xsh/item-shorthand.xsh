type ShorthandHit = {url: Str, size: Int}

const shorthand_hits: List[ShorthandHit] = [
  {url: "/a", size: 120},
  {url: "/b/c", size: 4000},
  {url: "/a", size: 30},
]

error ShorthandError = refused

pure shorthand_budget(hit: ShorthandHit) -> Int {
  hit.url.byte_len() * 100
}

pure shorthand_refuse(reason: Str) -> Result[Int, ShorthandError] {
  Err(.refused(reason))
}

test test_implicit_item_reads_like_a_named_parameter {
  let hits = shorthand_hits
  let by_url = hits |> count { .url }
  assert by_url.get("/a") == Ok(2)
  assert by_url.get("/b/c") == Ok(1)
  assert (hits |> map .size + shorthand_budget(.)) == [320, 4400, 230]
  assert (hits |> where .size > shorthand_budget(.) |> map .url) == ["/b/c"]
  assert (hits |> sort-by .size |> map .size) == [30, 120, 4000]
  assert (hits |> group-by .url |> map .items.len()) == [2, 1]
  assert (hits |> unique-by .url |> count()) == 2
  assert (hits |> par-map(jobs: 2) .url.byte_len()) == [2, 4, 2]
  assert hits |> any .size == 30
  assert hits |> all .url.starts_with("/")
  assert (hits |> flat-map .url.split("/") |> where . != "") == ["a", "b", "c", "a"]
  let sizes = hits |> reduce-by(sum: true) { {key: .url, value: .size} }
  assert sizes.get("/a") == Ok(150)
  var seen = []
  hits |> each { seen += [.url] }
  assert seen == ["/a", "/b/c", "/a"]
}

test test_error_handler_takes_the_error_as_its_item {
  assert (shorthand_refuse("busy") ?? { .message.byte_len() }) == 4
  var handled = 0
  # A later statement cannot begin with `.name`, which continues the line
  # before it, so it reads the item through a binding.
  let loaded = Ok(7) ?? {
    handled += 1
    let message = .message
    message.byte_len()
  }
  assert loaded == 7
  assert handled == 0
}

test test_nested_callbacks_have_their_own_item {
  let hits = shorthand_hits
  # The inner stage block and the inner handler each read their own item.
  let segments = hits |> map { .url.split("/") |> where . != "" |> count() }
  assert segments == [1, 2, 1]
  let reasons = hits |> map shorthand_refuse(.url) ?? { .message.byte_len() + .message.byte_len() }
  assert reasons == [4, 8, 4]
  # Branch and `try` blocks inside a callback see its item.
  let labels = hits
    |> map {
      if .size < 50 {
        "small"
      } else {
        let outcome = try {
          if .url == "/a" { "a" } else { .url }
        }
        outcome ?? { |_| "" }
      }
    }
  assert labels == ["a", "/b/c", "small"]
}

test test_item_is_rejected_where_the_parameter_is_written_out { |ctx|
  for {source, code} in [
    {source: "let total = [1, 2] |> map { |x| x + . }\n", code: "check.stream-item"},
    {source: "let total = [1, 2] |> map { |_| . }\n", code: "check.stream-item"},
    {source: "let total = [1, 2] |> fold(0) { |acc| acc + . }\n", code: "check.stream-item"},
    {source: "let total = [1, 2] |> reduce(0) { . }\n", code: "check.stream-item"},
    {source: "let size = \"x\".parse_int() ?? { |failure| . }\n", code: "check.stream-item"},
    {source: "let item = .\n", code: "check.stream-item"},
    {source: "let size = \"x\".parse_int() ?? { 0 }\n", code: "check.fallback-block-params"},
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert code in output.stderr, f"{source}: {output.stderr}"
  }
}
