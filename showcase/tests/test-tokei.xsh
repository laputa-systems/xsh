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

  let output = run.text "xsh" "showcase/tokei.xsh" -- --json $root
  let data = json.decode(output)?
  assert json.get(data, ["BASH", "code"])?.require(Int)? == 1
  assert json.get(data, ["Shell", "blanks"])?.require(Int)? == 1
  assert json.get(data, ["JSON", "code"])?.require(Int)? == 1
  assert json.get(data, ["TOML", "comments"])?.require(Int)? == 1
  assert json.get(data, ["JavaScript", "comments"])?.require(Int)? == 2
  assert json.get(data, ["HTML", "children", "JavaScript"])?.require(List[Any])?.len() == 1
  assert json.get(data, ["HTML", "children", "JavaScript", 0, "stats", "code"])?.require(Int)? == 1
  assert json.get(data, ["Markdown", "children", "BASH", 0, "stats", "comments"])?.require(Int)? == 1
  assert json.get(data, ["Markdown", "children", "Shell", 0, "stats", "code"])?.require(Int)? == 1
  assert json.get(data, ["MDX", "comments"])?.require(Int)? == 4
  assert json.get(data, ["MDX", "blanks"])?.require(Int)? == 1
  assert json.get(data, ["MDX", "children"])?.require(Record)?.keys().is_empty()
  assert json.get(data, ["Rust", "children", "Markdown", 0, "stats", "blobs", "TOML", "code"])?.require(Int)? == 1
  assert json.get(data, ["Total", "code"])?.require(Int)? == 16
  assert json.get(data, ["Total", "comments"])?.require(Int)? == 23
  assert json.get(data, ["Total", "blanks"])?.require(Int)? == 5
  assert ".hidden" not in json.get(data, ["Total", "children", "JSON", 0, "name"])?.require(Str)?
  assert json.get(data, ["Rust", "reports"])?.require(List[Any])?.len() == 1
  let table = run.text "xsh" "showcase/tokei.xsh" -- $root

  # tokei-format table: heavy rules, capitalized header, embedded ("|-") child rows,
  # per-language "(Total)" subtotals, and the grand "Total".
  assert "Language" in table
  assert "━" in table
  assert "|- JavaScript" in table
  assert ! ("|- TSX" in table)
  assert "(Total)" in table
  assert "Total" in table
}

test test_tokei_small_fixture_selects_exact_files_and_preserves_table_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "tokei-small-fixture")?
  fp"{root}/src".mkdir()
  fp"{root}/ignored".mkdir()
  fp"{root}/.tokeignore".write("ignored\n")
  fp"{root}/src/app.py".write("print(1)\n")
  fp"{root}/src/interface.pyi".write("# stub\nvalue: int\n")
  fp"{root}/translations.pot".write("msgid \"hello\"\n")
  fp"{root}/script".write("#!/usr/bin/env python3\nprint(2)\n")
  fp"{root}/data.json".write("{\"ok\":true}\n")
  fp"{root}/ignored/skip.rs".write("fn ignored() {}\n")
  fp"{root}/.hidden.py".write("print(3)\n")
  fp"{root}/notes.xyz".write("not a supported source\n")
  fp"{root}/unknown".write("no recognized shebang\n")

  let json_output = run.text "xsh" "showcase/tokei.xsh" -- --json $root
  let data = json.decode(json_output)?
  let labels = json.get(data, ["Total", "children"])?.require(Record)?.keys() |> sort-by .
  assert labels == ["JSON", "PO File", "Python"]
  let selected = collect {
    for label in ["JSON", "PO File", "Python"] {
      let reports = json.get(data, ["Total", "children", label])?.require(List[Any])?
      for report in reports {
        let absolute = json.get(report, ["name"])?.require(Str)?
        yield fp"{absolute}".relative_to(root).display()
      }
    }
  } |> sort-by .
  assert selected == [
    "data.json",
    "script",
    "src/app.py",
    "src/interface.pyi",
    "translations.pot",
  ]

  let table = run.capture --bytes "xsh" "showcase/tokei.xsh" -- $root ?
  assert table.status.ok
  assert table.stdout == fp"showcase/tests/fixtures/tokei-small.txt".read_bytes()?
}

test test_tokei_json_encodes_stats_and_nested_blob_stats_exactly { |ctx|
  let root = test.temp_dir(ctx, name: "tokei-json-stats")?
  fp"{root}/README.md".write("""# Demo

```toml
name = "x"
# config comment
```
""")

  let output = run.text "xsh" "showcase/tokei.xsh" -- --json $root
  assert "\"stats\":{\"blanks\":1,\"blobs\":{\"TOML\":{\"blanks\":0,\"blobs\":{},\"code\":1,\"comments\":1}},\"code\":0,\"comments\":3}" in output
  assert "\"stats\":{\"blanks\":0,\"blobs\":{},\"code\":1,\"comments\":1}" in output
}

test test_tokei_slash_scanner_preserves_crlf_and_final_line { |ctx|
  let root = test.temp_dir(ctx, name: "tokei-slash-scanner")?
  fp"{root}/scan.rs".write("fn main() {}\r\n// comment\r\n\r\n/* block\r\ncomment\r\n*/\r\nlet x = 1;")

  let output = run.text "xsh" "showcase/tokei.xsh" -- --json $root
  let data = json.decode(output)?
  assert json.get(data, ["Rust", "code"])?.require(Int)? == 2
  assert json.get(data, ["Rust", "comments"])?.require(Int)? == 4
  assert json.get(data, ["Rust", "blanks"])?.require(Int)? == 1
}

test test_tokei_fails_when_selected_file_cannot_be_read { |ctx|
  let root = test.temp_dir(ctx, name: "tokei-unreadable")?
  let unreadable = fp"{root}/unreadable.rs"
  unreadable.write("fn main() {}\n")
  let prefix = if unix.id()?.euid == 0 {
    ctx.temp_root.chmod(0o755)?
    root.chmod(0o755)?
    ["setpriv", "--reuid=65534", "--regid=65534", "--clear-groups"]
  } else { [] }

  let json_args = [@prefix, "xsh", "showcase/tokei.xsh", "--", "--json", root.display()]
  let table_args = [@prefix, "xsh", "showcase/tokei.xsh", "--", root.display()]
  let readable_json = run.capture --bytes @json_args ?
  let readable_table = run.capture --bytes @table_args ?
  assert readable_json.status.ok, readable_json.stderr.utf8()?
  assert readable_table.status.ok, readable_table.stderr.utf8()?
  unreadable.chmod(0o000)?
  defer unreadable.chmod(0o600)
  let json_run = run.capture --bytes @json_args ?
  let table_run = run.capture --bytes @table_args ?
  assert ! json_run.status.ok
  assert ! table_run.status.ok
}
