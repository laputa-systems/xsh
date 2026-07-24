proc test_tokei_json_shape_counts_and_ignores(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "tokei-root")?

  fp"${root}/.tokeignore".write("""ignored
""")?

  fp"${root}/ignored".mkdir()?

  fp"${root}/ignored/skip.rs".write("""fn skipped() {}
""")?

  fp"${root}/.hidden.json".write("""{"skip":true}
""")?

  fp"${root}/build.bash".write("""echo bash
# comment
""")?

  fp"${root}/run.sh".write("""echo shell
# comment

""")?

  fp"${root}/data.json".write("""{"ok":true}
""")?

  fp"${root}/config.toml".write("""# comment
name = "demo"
""")?

  fp"${root}/app.js".write("""// top
const x = "/* no */";
/* block */
""")?

  fp"${root}/index.html".write("""<!-- note -->
<div>
<script>
  // c
  let x = 1;
</script>
</div>
""")?

  fp"${root}/README.md".write("""Intro

```bash
echo hi
# nope
```

```shell
echo shell
```
""")?

  fp"${root}/component.mdx".write("""Intro

```tsx
const x = 1;
```
""")?

  fp"${root}/main.rs".write("""/// # Doc
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
""")?

  let output = run.text "xsh" "showcase/tokei.xsh" -- --json $root ?
  let data = json.decode(output)?
  test.eq(data["BASH"]["code"], 1)?
  test.eq(data["Shell"]["blanks"], 1)?
  test.eq(data["JSON"]["code"], 1)?
  test.eq(data["TOML"]["comments"], 1)?
  test.eq(data["JavaScript"]["comments"], 2)?
  test.eq(data["HTML"]["children"]["JavaScript"][0]["stats"]["code"], 1)?
  test.eq(data["Markdown"]["children"]["BASH"][0]["stats"]["comments"], 1)?
  test.eq(data["Markdown"]["children"]["Shell"][0]["stats"]["code"], 1)?
  test.eq(data["MDX"]["comments"], 4)?
  test.eq(data["MDX"]["blanks"], 1)?
  test.eq(data["MDX"]["children"].keys().len(), 0)?
  test.eq(data["Rust"]["children"]["Markdown"][0]["stats"]["blobs"]["TOML"]["code"], 1)?
  test.eq(data["Total"]["code"], 16)?
  test.eq(data["Total"]["comments"], 23)?
  test.eq(data["Total"]["blanks"], 5)?
  test.ok(! data["Total"]["children"]["JSON"][0]["name"].contains(".hidden"))?
  test.eq(data["Rust"]["reports"].len(), 1)?
  let table = run.text "xsh" "showcase/tokei.xsh" -- $root ?

  # tokei-format table: heavy rules, capitalized header, embedded ("|-") child rows,
  # per-language "(Total)" subtotals, and the grand "Total".
  test.ok("Language" in table)?
  test.ok("\u{2501}" in table)?
  test.ok("|- JavaScript" in table)?
  test.ok(! ("|- TSX" in table))?
  test.ok("(Total)" in table)?
  test.ok("Total" in table)?
}
