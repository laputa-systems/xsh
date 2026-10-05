test test_tokei_json_shape_counts_and_ignores { |ctx|
  let root = test.temp_dir(ctx, name: "tokei-root")?

  fp"{root}/.tokeignore".write("""ignored
""")

  fp"{root}/ignored".mkdir()

  fp"{root}/ignored/skip.rs".write("""fn skipped() {}
""")

  fp"{root}/.hidden.json".write("""{"skip":true}
""")

  fp"{root}/build.bash".write("""echo bash
# comment
""")

  fp"{root}/run.sh".write("""echo shell
# comment

""")

  fp"{root}/data.json".write("""{"ok":true}
""")

  fp"{root}/config.toml".write("""# comment
name = "demo"
""")

  fp"{root}/app.js".write("""// top
const x = "/* no */";
/* block */
""")

  fp"{root}/index.html".write("""<!-- note -->
<div>
<script>
  // c
  let x = 1;
</script>
</div>
""")

  fp"{root}/README.md".write("""Intro

```bash
echo hi
# nope
```

```shell
echo shell
```
""")

  fp"{root}/component.mdx".write("""Intro

```tsx
const x = 1;
```
""")

  fp"{root}/main.rs".write("""/// # Doc
/// 
/// ```toml
/// name = "nested"
/// # nested comment
/// ```
fn main() {
  println!("hi"); // inline
}
/* block
comment */
""")

  let output = run.text "xsh" "showcase/tokei.xsh" -- --json $root ?
  let data = json.decode(output)?
  assert data["BASH"]["code"].require(Int)? == 1
  assert data["Shell"]["blanks"].require(Int)? == 1
  assert data["JSON"]["code"].require(Int)? == 1
  assert data["TOML"]["comments"].require(Int)? == 1
  assert data["JavaScript"]["comments"].require(Int)? == 2
  assert data["HTML"]["children"]["JavaScript"].require(List[Any])?.len() == 1
  assert data["HTML"]["children"]["JavaScript"][0]["stats"]["code"].require(Int)? == 1
  assert data["Markdown"]["children"]["BASH"][0]["stats"]["comments"].require(Int)? == 1
  assert data["Markdown"]["children"]["Shell"][0]["stats"]["code"].require(Int)? == 1
  assert data["MDX"]["comments"].require(Int)? == 4
  assert data["MDX"]["blanks"].require(Int)? == 1
  assert data["MDX"]["children"].require(Record)?.keys().len() == 0
  assert data["Rust"]["children"]["Markdown"][0]["stats"]["blobs"]["TOML"]["code"].require(Int)? == 1
  assert data["Total"]["code"].require(Int)? == 16
  assert data["Total"]["comments"].require(Int)? == 23
  assert data["Total"]["blanks"].require(Int)? == 5
  assert ".hidden" not in data["Total"]["children"]["JSON"][0]["name"].require(Str)?
  assert data["Rust"]["reports"].require(List[Any])?.len() == 1
  let table = run.text "xsh" "showcase/tokei.xsh" -- $root ?

  # tokei-format table: heavy rules, capitalized header, embedded ("|-") child rows,
  # per-language "(Total)" subtotals, and the grand "Total".
  assert "Language" in table
  assert "━" in table
  assert "|- JavaScript" in table
  assert ! ("|- TSX" in table)
  assert "(Total)" in table
  assert "Total" in table
}
