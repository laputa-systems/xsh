#!/usr/bin/env -S xsh --
# Tokei-shaped source counter for a deliberately small language set.
# Usage: xsh showcase/tokei.xsh -- [ROOT]
# Example: xsh showcase/tokei.xsh -- /Users/josh/d/tokei
#
# The tokei journey (why this file looks the way it does)
# -------------------------------------------------------
# This started as a benchmark to prove XSH could do real text-scanning work at a
# serious scale (counting all of Sentry, ~18k files / ~5M lines) and became the
# forcing function for most of the lowered-IR work. Milestones:
#
# 1. Scanner throughput. The per-file `count_*` scanners drove the lowered IR from
#    "never fires on the hot path" to fully active: lowering `map.empty()`/`bytes.concat`,
#    block-scoped re-bindings, borrowed `for line in text.lines()`, byte predicates,
#    and finally strongly-connected-component co-lowering so the mutually-recursive
#    cluster (count_markdown <-> count_slash_language -> count_html, dispatched by
#    count_language) could lower atomically. Net: the default table path went from
#    ~2.3x slower than native release tokei to ~1.3x FASTER. See docs/FRONTEND.md.
#
# 2. Output format: stable, tokei-like, and tested byte-for-byte against this
#    script's own saved output for the same corpus/options. Native tokei is not
#    the byte oracle. The default table keeps the embedded "|- Child" breakdown,
#    per-language "(Total)" rows, heavy/light rules (tui glyphs), fixed column
#    right-edges, and tokei-like row order (sorted by tokei's *internal*
#    LanguageType name, child-bearing languages last). The per-(parent,child)
#    breakdown is aggregated in-stream via `par-map |> flat-map |> reduce-by`.
#
# 3. Counts: file selection is exact, line classification is a deliberate
#    approximation. We closed file selection to Δfiles=0 vs tokei cheaply (`.pyi`/
#    `.pot` extensions + `#!`-shebang detection). Getting line-level code/comment/
#    blank counts byte-for-byte, though, needs each language's own string/comment
#    tokenizer (Python `"""`, JS/TS template literals, HTML `<style>`->CSS, blanks
#    inside block comments, regex literals, MDX prose, ...). Prototyping those
#    reached ~0.12% of total lines, but char-level scanning of every string/comment
#    -bearing line made the interpreter ~1.65x SLOWER than native -- forfeiting the
#    speed win for diminishing, long-tail accuracy. We chose to keep the speed lead
#    and leave the shared approximate counters (count_hash_language /
#    count_slash_plain / single-pass count_html) in place. So: matches tokei on file
#    selection and table format, stays faster than native, and the remaining count
#    gap (~0.18% of lines) is an intentional stopping point, not a bug to chase.
type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Scan = {stats: Stats, deep: Stats}

type FileReport = {stats: Stats, name: Str}

type Language =
    LangUnknown
  | LangBash
  | LangCss
  | LangDockerfile
  | LangForgeConfig
  | LangHtml
  | LangIni
  | LangJavaScript
  | LangJson
  | LangLess
  | LangLua
  | LangMakefile
  | LangMarkdown
  | LangMdx
  | LangModelica
  | LangPlainText
  | LangPoFile
  | LangPython
  | LangReStructuredText
  | LangRust
  | LangShell
  | LangSvg
  | LangTempl
  | LangToml
  | LangTsx
  | LangTypeScript
  | LangXml
  | LangYaml

type SummaryTotals = {
  files: Int,
  blanks: Int,
  code: Int,
  comments: Int,
  total_blanks: Int,
  total_code: Int,
  total_comments: Int,
}

type SummaryRow = {
  key: Str,
  files: Int,
  blanks: Int,
  code: Int,
  comments: Int,
  total_blanks: Int,
  total_code: Int,
  total_comments: Int,
}

type Opts = {root: Path, json: Bool}

pure zero_stats() -> Stats {
  return {blanks: 0, code: 0, comments: 0, blobs: map.empty()}
}

pure has_stats(stats: Stats) -> Bool {
  return stats.blanks > 0 or stats.code > 0 or stats.comments > 0 or stats.blobs.len() > 0
}

pure add_stats(a: Stats, b: Stats) -> Stats {
  return {blanks: a.blanks + b.blanks, code: a.code + b.code, comments: a.comments + b.comments, blobs: map.empty()}
}

pure with_blobs(stats: Stats, blobs: Map[Any]) -> Stats {
  return {blanks: stats.blanks, code: stats.code, comments: stats.comments, blobs}
}

# Total stats of an embedded blob: its own counts plus, recursively, the counts of
# anything nested inside it. tokei's "|- Child" rows show this deep total (e.g. a
# Rust doc-comment Markdown blob includes the code of a TOML fence nested in it).
pure blob_deep(stats: Stats) -> Stats {
  var blanks = stats.blanks
  var code = stats.code
  var comments = stats.comments

  for key in stats.blobs.keys() {
    let nested = stats.blobs.get(key, zero_stats()).require(Stats) ?? zero_stats()
    let deep = blob_deep(nested)
    blanks += deep.blanks
    code += deep.code
    comments += deep.comments
  }

  return {blanks, code, comments, blobs: map.empty()}
}

pure source_exts() -> List[Str] {
  return [
    "",
    "json",
    "py",
    "pyi",
    "tsx",
    "ts",
    "cts",
    "mts",
    "po",
    "pot",
    "html",
    "htm",
    "svg",
    "mdx",
    "md",
    "markdown",
    "less",
    "js",
    "cjs",
    "mjs",
    "yaml",
    "yml",
    "css",
    "cfg",
    "ini",
    "lua",
    "mo",
    "rs",
    "rst",
    "sh",
    "templ",
    "tmpl",
    "toml",
    "xml",
    "bash",
    "txt",
  ]
}

pure language_label(language: Language) -> Str {
  match language {
    LangBash => "BASH"
    LangCss => "CSS"
    LangDockerfile => "Dockerfile"
    LangForgeConfig => "Forge Config"
    LangHtml => "HTML"
    LangIni => "INI"
    LangJavaScript => "JavaScript"
    LangJson => "JSON"
    LangLess => "LESS"
    LangLua => "Lua"
    LangMakefile => "Makefile"
    LangMarkdown => "Markdown"
    LangMdx => "MDX"
    LangModelica => "Modelica"
    LangPlainText => "Plain Text"
    LangPoFile => "PO File"
    LangPython => "Python"
    LangReStructuredText => "ReStructuredText"
    LangRust => "Rust"
    LangShell => "Shell"
    LangSvg => "SVG"
    LangTempl => "Templ"
    LangToml => "TOML"
    LangTsx => "TSX"
    LangTypeScript => "TypeScript"
    LangXml => "XML"
    LangYaml => "YAML"
    LangUnknown => ""
  }
}

pure languages() -> List[Language] {
  return [
    LangBash,
    LangCss,
    LangDockerfile,
    LangForgeConfig,
    LangHtml,
    LangIni,
    LangJson,
    LangJavaScript,
    LangLess,
    LangLua,
    LangMdx,
    LangMakefile,
    LangMarkdown,
    LangModelica,
    LangPoFile,
    LangPlainText,
    LangPython,
    LangReStructuredText,
    LangRust,
    LangSvg,
    LangShell,
    LangToml,
    LangTsx,
    LangTempl,
    LangTypeScript,
    LangXml,
    LangYaml,
  ]
}

# Languages in tokei's table order: ascending by tokei's internal `LanguageType`
# variant name (e.g. Plain Text sorts as `Text`, Shell as `Sh`), which differs
# from the display-label order. Used so the default table matches tokei row-for-row.
pure sorted_languages() -> List[Language] {
  return [
    LangBash,
    LangCss,
    LangDockerfile,
    LangForgeConfig,
    LangHtml,
    LangIni,
    LangJavaScript,
    LangJson,
    LangLess,
    LangLua,
    LangMakefile,
    LangMarkdown,
    LangMdx,
    LangModelica,
    LangPoFile,
    LangPython,
    LangReStructuredText,
    LangRust,
    LangShell,
    LangSvg,
    LangTempl,
    LangPlainText,
    LangToml,
    LangTsx,
    LangTypeScript,
    LangXml,
    LangYaml,
  ]
}

# A horizontal rule of `n` copies of `glyph` (tokei uses 81-wide heavy/light rules).
pure rule(glyph: Str, n: Int) -> Str {
  var out = ""
  var i = 0

  while i < n {
    out = f"${out}${glyph}"
    i += 1
  }

  return out
}

# One tokei table row: language name left-aligned from column 1, then the five
# numeric columns right-aligned at fixed right edges (28, 41, 54, 67, 80), matching
# tokei byte-for-byte. The five values are pre-formatted strings so callers can pass
# "" for the blank Files cell of a `(Total)` row.
pure fmt_row(name: Str, files: Str, lines: Str, code: Str, comments: Str, blanks: Str) -> Str {
  var line = f" ${name}"
  line = f"${line}${tui.left_pad(files, 28 - line.byte_len())}"
  line = f"${line}${tui.left_pad(lines, 41 - line.byte_len())}"
  line = f"${line}${tui.left_pad(code, 54 - line.byte_len())}"
  line = f"${line}${tui.left_pad(comments, 67 - line.byte_len())}"
  line = f"${line}${tui.left_pad(blanks, 80 - line.byte_len())}"
  return line
}

pure lang_for_name_ext(name_raw: Str, ext_raw: Str) -> Language {
  let ext = ext_raw.lower()

  match ext {
    "bash" => return LangBash
    "tsx" => return LangTsx
    e if e == "py" or e == "pyi" => return LangPython
    "json" => return LangJson
    e if e == "ts" or e == "cts" or e == "mts" => return LangTypeScript
    e if e == "po" or e == "pot" => return LangPoFile
    e if e == "html" or e == "htm" => return LangHtml
    "svg" => return LangSvg
    "mdx" => return LangMdx
    e if e == "md" or e == "markdown" => return LangMarkdown
    "less" => return LangLess
    e if e == "js" or e == "cjs" or e == "mjs" => return LangJavaScript
    e if e == "yaml" or e == "yml" => return LangYaml
    "css" => return LangCss
    "cfg" => return LangForgeConfig
    "ini" => return LangIni
    "lua" => return LangLua
    "mo" => return LangModelica
    "rs" => return LangRust
    "rst" => return LangReStructuredText
    "sh" => return LangShell
    e if e == "templ" or e == "tmpl" => return LangTempl
    "toml" => return LangToml
    "xml" => return LangXml
    "txt" => return LangPlainText
    _ => {}
  }

  let name = name_raw.lower()

  return match name {
    "dockerfile" => LangDockerfile,
    "makefile" => LangMakefile,
    n if n == "post-checkout" or n == "post-merge" or n == "upload_snapshots" => LangBash,
    _ => LangUnknown,
  }
}

# Detect the language of an extension-less file from its `#!` shebang, like tokei
# does when the path gives no hint. Only the first line is inspected (the loop
# returns immediately), and only interpreters that map to a language in this
# showcase's set are recognized.
pure lang_for_shebang(text: Bytes) -> Language {
  for line in text.lines() {
    let first = (line.utf8() ?? "").lower()

    if ! first.starts_with("#!") {
      return LangUnknown
    }

    if "python" in first {
      return LangPython
    }

    if "node" in first {
      return LangJavaScript
    }

    if "bash" in first {
      return LangBash
    }

    if "/sh" in first or " sh" in first {
      return LangShell
    }

    return LangUnknown
  }

  return LangUnknown
}

pure lang_for_fence(line: Str) -> Language {
  let lower = line.trim().lower()

  if lower.starts_with("```bash") or lower.starts_with("````bash") {
    return LangBash
  }

  if lower.starts_with("```shell") or lower.starts_with("````shell") {
    return LangShell
  }

  if lower.starts_with("```json") or lower.starts_with("````json") {
    return LangJson
  }

  if lower.starts_with("```toml") or lower.starts_with("````toml") {
    return LangToml
  }

  if lower.starts_with("```html") or lower.starts_with("````html") {
    return LangHtml
  }

  if lower.starts_with("```javascript") or lower.starts_with("````javascript") {
    return LangJavaScript
  }

  if lower.starts_with("```js") or lower.starts_with("````js") {
    return LangJavaScript
  }

  if lower.starts_with("```python") or lower.starts_with("````python") {
    return LangPython
  }

  if lower.starts_with("```py") or lower.starts_with("````py") {
    return LangPython
  }

  if lower.starts_with("```tsx") or lower.starts_with("````tsx") {
    return LangTsx
  }

  if lower.starts_with("```typescript") or lower.starts_with("````typescript") {
    return LangTypeScript
  }

  if lower.starts_with("```ts") or lower.starts_with("````ts") {
    return LangTypeScript
  }

  if lower.starts_with("```markdown") or lower.starts_with("````markdown") {
    return LangMarkdown
  }

  if lower.starts_with("```md") or lower.starts_with("````md") {
    return LangMarkdown
  }

  if lower.starts_with("```rust") or lower.starts_with("````rust") {
    return LangRust
  }

  return LangUnknown
}

pure set_blob(blobs: Map[Any], language: Str, stats: Stats) -> Map[Any] {
  if has_stats(stats) {
    return blobs.set(language, stats)
  }

  return blobs
}

pure join_lines(lines: List[Bytes]) -> Bytes {
  var parts: List[Bytes] = []
  var first = true

  for line in lines {
    if ! first {
      parts = parts.push(b"\n")
    }

    parts = parts.push(line)
    first = false
  }

  return bytes.concat(parts)
}

pure count_hash_unindented(text: Bytes) -> Scan {
  var blanks = 0
  var comments = 0

  for line in text.lines() {
    if line == b"" {
      blanks += 1
    } else if line.starts_with(b"#") {
      comments += 1
    }
  }

  let code = text.count_lines() - blanks - comments
  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_hash_language(text: Bytes) -> Scan {
  if ! text.contains(b"#") {
    return count_code_text(text)
  }

  if ! text.starts_with(b" ") and ! text.starts_with(b"\t") and ! (b"\n " in text) and ! (b"\n\t" in text) {
    return count_hash_unindented(text)
  }

  var blanks = 0
  var comments = 0

  for line in text.lines() {
    if line.trim() == b"" {
      blanks += 1
    } else if line.trim().starts_with(b"#") {
      comments += 1
    }
  }

  let code = text.count_lines() - blanks - comments
  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_json(text: Bytes) -> Scan {
  if ! text.starts_with(b"\n") and ! (b"\n\n" in text) {
    let stats = {blanks: 0, code: text.count_lines(), comments: 0, blobs: map.empty()}
    return {stats, deep: stats}
  }

  var blanks = 0

  for line in text.lines() {
    if line.trim() == b"" {
      blanks += 1
    }
  }

  let code = text.count_lines() - blanks
  let stats = {blanks, code, comments: 0, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_markdown_prose(text: Bytes) -> Stats {
  var blanks = 0

  for line in text.lines() {
    if line.trim() == b"" {
      blanks += 1
    }
  }

  let comments = text.count_lines() - blanks
  return {blanks, code: 0, comments, blobs: map.empty()}
}

pure count_plain_text(text: Bytes) -> Scan {
  let stats = count_markdown_prose(text)
  return {stats, deep: stats}
}

pure count_code_text(text: Bytes) -> Scan {
  var blanks = 0

  for line in text.lines() {
    if line.trim() == b"" {
      blanks += 1
    }
  }

  let code = text.count_lines() - blanks
  let stats = {blanks, code, comments: 0, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_lua(text: Bytes) -> Scan {
  var blanks = 0
  var code = 0
  var comments = 0
  var in_block = false

  for line in text.lines() {
    let trimmed = line.trim()

    if in_block {
      comments += 1

      if b"]]" in trimmed {
        in_block = false
      }
    } else if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"--[[") {
      comments += 1

      if ! (b"]]" in trimmed) {
        in_block = true
      }
    } else if trimmed.starts_with(b"--") {
      comments += 1
    } else {
      code += 1
    }
  }

  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure rust_fence_contains_doc_fence(text: Bytes) -> Bool {
  for line in text.lines() {
    let trimmed = line.trim()

    if trimmed.starts_with(b"//! ```") or trimmed.starts_with(b"/// ```") {
      return true
    }
  }

  return false
}

pure count_slash_plain(text: Bytes) -> Scan {
  if ! (b"/" in text) {
    return count_code_text(text)
  }

  if ! (b"//" in text) and ! (b"/*" in text) {
    return count_code_text(text)
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var block_depth = 0

  for line in text.lines() {
    if block_depth == 0 and ! (b"/" in line) {
      if line.trim() == b"" {
        blanks += 1
      } else {
        code += 1
      }
    } else if block_depth == 0 and ! (b"//" in line) and ! (b"/*" in line) {
      code += 1
    } else {
      let trimmed = line.trim()

      if trimmed == b"" and block_depth == 0 {
        blanks += 1
      } else if block_depth == 0 and trimmed.starts_with(b"//") {
        comments += 1
      } else if block_depth == 0 and ! trimmed.starts_with(b"/") and (! (b"/*" in line) or b"*/" in line) {
        code += 1
      } else if block_depth == 0 and trimmed.starts_with(b"/*") and ! (b"*/" in line) {
        comments += 1
        block_depth = 1
      } else if block_depth > 0 and trimmed == b"" {
        blanks += 1
      } else if block_depth > 0 and ! (b"*/" in line) {
        comments += 1
      } else if block_depth > 0 and trimmed.ends_with(b"*/") {
        comments += 1
        block_depth -= 1
      } else {
        let line_len = line.len()
        var index = 0
        var code_seen = false
        var comment_seen = false
        var in_string = false
        var string_delim = -1
        var escaped = false

        while index < line_len {
          let ch = line.byte_at(index)
          let next = line.byte_at(index + 1)

          if block_depth > 0 {
            comment_seen = true

            if ch == 42 and next == 47 {
              block_depth -= 1
              index += 2
            } else {
              index += 1
            }
          } else if in_string {
            code_seen = true

            if escaped {
              escaped = false
            } else if ch == 92 {
              escaped = true
            } else if ch == string_delim {
              in_string = false
            }

            index += 1
          } else if ch == 34 or ch == 39 or ch == 96 {
            code_seen = true
            in_string = true
            string_delim = ch
            index += 1
          } else if ch == 47 and next == 47 {
            comment_seen = true
            index = line_len
          } else if ch == 47 and next == 42 {
            comment_seen = true
            block_depth = 1
            index += 2
          } else {
            if ch != 32 and ch != 9 {
              code_seen = true
            }

            index += 1
          }
        }

        if code_seen {
          code += 1
        } else if comment_seen {
          comments += 1
        } else {
          blanks += 1
        }
      }
    }
  }

  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_slash_language(text: Bytes, nested: Bool, collect_doc_markdown: Bool) -> Scan {
  if ! (b"/" in text) {
    return count_code_text(text)
  }

  if ! (b"//" in text) and ! (b"/*" in text) {
    return count_code_text(text)
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var doc_lines: List[Bytes] = []
  var block_depth = 0

  for line in text.lines() {
    if block_depth == 0 and ! (b"/" in line) {
      if line.trim() == b"" {
        blanks += 1
      } else {
        code += 1
      }
    } else if block_depth == 0 and ! (b"//" in line) and ! (b"/*" in line) {
      code += 1
    } else {
      let trimmed = line.trim()

      if collect_doc_markdown and block_depth == 0 and (trimmed.starts_with(b"///") or trimmed.starts_with(b"//!")) {
        doc_lines = doc_lines.push(trimmed.slice(3))
      } else if trimmed == b"" and block_depth == 0 {
        blanks += 1
      } else if block_depth == 0 and trimmed.starts_with(b"//") {
        comments += 1
      } else if block_depth == 0 and ! trimmed.starts_with(b"/") and (! (b"/*" in line) or b"*/" in line) {
        code += 1
      } else if block_depth == 0 and trimmed.starts_with(b"/*") and ! (b"*/" in line) {
        comments += 1
        block_depth = 1
      } else if block_depth > 0 and trimmed == b"" {
        blanks += 1
      } else if block_depth > 0 and ! (b"*/" in line) and (! nested or ! (b"/*" in line)) {
        comments += 1
      } else if block_depth > 0 and trimmed.ends_with(b"*/") and (! nested or ! (b"/*" in line)) {
        comments += 1
        block_depth -= 1
      } else {
        let line_len = line.len()
        var index = 0
        var code_seen = false
        var comment_seen = false
        var in_string = false
        var string_delim = -1
        var escaped = false

        while index < line_len {
          let ch = line.byte_at(index)
          let next = line.byte_at(index + 1)

          if block_depth > 0 {
            comment_seen = true

            if ch == 47 and next == 42 and nested {
              block_depth += 1
              index += 2
            } else if ch == 42 and next == 47 {
              block_depth -= 1
              index += 2
            } else {
              index += 1
            }
          } else if in_string {
            code_seen = true

            if escaped {
              escaped = false
            } else if ch == 92 {
              escaped = true
            } else if ch == string_delim {
              in_string = false
            }

            code_seen = true
            in_string = true
            string_delim = ch
            index += 1
          } else if ch == 47 and next == 47 {
            comment_seen = true
            index = line_len
          } else if ch == 47 and next == 42 {
            comment_seen = true
            block_depth = 1
            index += 2
          } else {
            if ch != 32 and ch != 9 {
              code_seen = true
            }

            index += 1
          }
        }

        if code_seen {
          code += 1
        } else if comment_seen {
          comments += 1
        } else {
          blanks += 1
        }
      }
    }
  }

  var markdown = zero_stats()
  let stats = {blanks, code, comments, blobs: map.empty()}
  var deep = stats

  if doc_lines.len() > 0 {
    let md_scan = count_markdown(join_lines(doc_lines))
    markdown = md_scan.stats
    deep = add_stats(deep, md_scan.deep)
  }

  var blobs: Map[Any] = {}
  blobs = set_blob(blobs, language_label(LangMarkdown), markdown)
  return {stats: with_blobs(stats, blobs), deep}
}

pure count_markdown(text: Bytes) -> Scan {
  if ! (b"```" in text) {
    let stats = count_markdown_prose(text)
    return {stats, deep: stats}
  }

  var blanks = 0
  var comments = 0
  var deep = zero_stats()
  var in_fence = false
  var fence_marker = b""
  var fence_lang = LangUnknown
  var fence_lines: List[Bytes] = []
  var bash = zero_stats()
  var html = zero_stats()
  var json_stats = zero_stats()
  var javascript = zero_stats()
  var markdown = zero_stats()
  var python = zero_stats()
  var rust = zero_stats()
  var shell = zero_stats()
  var tsx = zero_stats()
  var typescript = zero_stats()
  var toml = zero_stats()

  for line in text.lines() {
    let trimmed = line.trim()

    if in_fence {
      if trimmed.starts_with(fence_marker) {
        comments += 1
        let body = join_lines(fence_lines)

        match fence_lang {
          LangBash => {
            let scan = count_hash_language(body)
            bash = add_stats(bash, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangHtml => {
            let scan = count_html(body, true)
            html = add_stats(html, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangJson => {
            let scan = count_json(body)
            json_stats = add_stats(json_stats, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangJavaScript => {
            let scan = count_slash_plain(body)
            javascript = add_stats(javascript, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangMarkdown => {
            let scan = count_markdown(body)
            markdown = add_stats(markdown, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangPython => {
            let scan = count_hash_language(body)
            python = add_stats(python, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangRust => {
            if rust_fence_contains_doc_fence(body) {
              let prose = count_markdown_prose(body)
              blanks += prose.blanks
              comments += prose.comments
            } else {
              let scan = count_slash_language(body, true, true)
              rust = add_stats(rust, scan.stats)
              deep = add_stats(deep, scan.deep)
            }
          }
          LangShell => {
            let scan = count_hash_language(body)
            shell = add_stats(shell, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangTsx => {
            let scan = count_slash_plain(body)
            tsx = add_stats(tsx, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangTypeScript => {
            let scan = count_slash_plain(body)
            typescript = add_stats(typescript, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          LangToml => {
            let scan = count_hash_language(body)
            toml = add_stats(toml, scan.stats)
            deep = add_stats(deep, scan.deep)
          }
          _ => {}
        }

        in_fence = false
        fence_marker = b""
        fence_lang = LangUnknown
        fence_lines = []
      } else {
        fence_lines = fence_lines.push(line)
      }
    } else if trimmed.starts_with(b"```") {
      comments += 1
      let lang = lang_for_fence(trimmed.utf8() ?? "")

      if lang != LangUnknown {
        in_fence = true
        fence_marker = if trimmed.starts_with(b"````") { b"````" } else { b"```" }
        fence_lang = lang
        fence_lines = []
      }
    } else if trimmed == b"" {
      blanks += 1
    } else {
      comments += 1
    }
  }

  if in_fence and fence_lang != LangUnknown {
    let body = join_lines(fence_lines)

    match fence_lang {
      LangBash => {
        let scan = count_hash_language(body)
        bash = add_stats(bash, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangHtml => {
        let scan = count_html(body, true)
        html = add_stats(html, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangJson => {
        let scan = count_json(body)
        json_stats = add_stats(json_stats, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangJavaScript => {
        let scan = count_slash_plain(body)
        javascript = add_stats(javascript, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangMarkdown => {
        let scan = count_markdown(body)
        markdown = add_stats(markdown, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangPython => {
        let scan = count_hash_language(body)
        python = add_stats(python, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangRust => {
        if rust_fence_contains_doc_fence(body) {
          let prose = count_markdown_prose(body)
          blanks += prose.blanks
          comments += prose.comments
        } else {
          let scan = count_slash_language(body, true, true)
          rust = add_stats(rust, scan.stats)
          deep = add_stats(deep, scan.deep)
        }
      }
      LangShell => {
        let scan = count_hash_language(body)
        shell = add_stats(shell, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangTsx => {
        let scan = count_slash_plain(body)
        tsx = add_stats(tsx, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangTypeScript => {
        let scan = count_slash_plain(body)
        typescript = add_stats(typescript, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      LangToml => {
        let scan = count_hash_language(body)
        toml = add_stats(toml, scan.stats)
        deep = add_stats(deep, scan.deep)
      }
      _ => {}
    }
  }

  var blobs: Map[Any] = {}
  blobs = set_blob(blobs, language_label(LangBash), bash)
  blobs = set_blob(blobs, language_label(LangHtml), html)
  blobs = set_blob(blobs, language_label(LangJson), json_stats)
  blobs = set_blob(blobs, language_label(LangJavaScript), javascript)
  blobs = set_blob(blobs, language_label(LangMarkdown), markdown)
  blobs = set_blob(blobs, language_label(LangPython), python)
  blobs = set_blob(blobs, language_label(LangRust), rust)
  blobs = set_blob(blobs, language_label(LangShell), shell)
  blobs = set_blob(blobs, language_label(LangToml), toml)
  blobs = set_blob(blobs, language_label(LangTsx), tsx)
  blobs = set_blob(blobs, language_label(LangTypeScript), typescript)
  let stats = {blanks, code: 0, comments, blobs: map.empty()}
  let top = with_blobs(stats, blobs)
  var total_deep = stats
  total_deep.blanks += deep.blanks
  total_deep.code += deep.code
  total_deep.comments += deep.comments
  return {stats: top, deep: total_deep}
}

# `embed` controls whether `<script>`/`<style>` blocks are extracted as JavaScript/CSS
# children (true for HTML) or counted as ordinary markup code (false for SVG/XML, which
# tokei reports with no children).
pure count_html(text: Bytes, embed: Bool) -> Scan {
  if ! (b"<!--" in text) {
    let lower_text = text.lower()

    if ! embed or ! (b"<script" in lower_text) and ! (b"<style" in lower_text) {
      return count_code_text(text)
    }
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var deep = zero_stats()
  var in_comment = false
  var in_script = false
  var in_style = false
  var script_lines: List[Bytes] = []
  var style_lines: List[Bytes] = []
  var css = zero_stats()
  var javascript = zero_stats()

  for line in text.lines() {
    let trimmed = line.trim()

    if in_script {
      let lower = trimmed.lower()

      if lower.starts_with(b"</script") {
        let scan = count_slash_plain(join_lines(script_lines))
        javascript = add_stats(javascript, scan.stats)
        deep = add_stats(deep, scan.deep)
        code += 1
        script_lines = []
        in_script = false
      } else {
        script_lines = script_lines.push(line)
      }
    } else if in_style {
      let lower = trimmed.lower()

      if lower.starts_with(b"</style") {
        let scan = count_slash_plain(join_lines(style_lines))
        css = add_stats(css, scan.stats)
        deep = add_stats(deep, scan.deep)
        code += 1
        style_lines = []
        in_style = false
      } else {
        style_lines = style_lines.push(line)
      }
    } else if in_comment {
      comments += 1

      if b"-->" in trimmed {
        in_comment = false
      }
    } else if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"<!--") {
      comments += 1

      if ! (b"-->" in trimmed) {
        in_comment = true
      }
    } else {
      code += 1
      let lower = trimmed.lower()

      if embed and lower.starts_with(b"<script") and ! (b"</script" in lower) {
        in_script = true
        script_lines = []
      } else if embed and lower.starts_with(b"<style") and ! (b"</style" in lower) {
        in_style = true
        style_lines = []
      }
    }
  }

  if in_script {
    let scan = count_slash_plain(join_lines(script_lines))
    javascript = add_stats(javascript, scan.stats)
    deep = add_stats(deep, scan.deep)
  }

  if in_style {
    let scan = count_slash_plain(join_lines(style_lines))
    css = add_stats(css, scan.stats)
    deep = add_stats(deep, scan.deep)
  }

  var blobs: Map[Any] = {}
  blobs = set_blob(blobs, language_label(LangCss), css)
  blobs = set_blob(blobs, language_label(LangJavaScript), javascript)
  let stats = {blanks, code, comments, blobs: map.empty()}
  let top = with_blobs(stats, blobs)
  var total_deep = stats
  total_deep.blanks += deep.blanks
  total_deep.code += deep.code
  total_deep.comments += deep.comments
  return {stats: top, deep: total_deep}
}

pure count_language(language: Language, text: Bytes) -> Scan {
  let empty: Scan = {stats: zero_stats(), deep: zero_stats()}

  match language {
    LangTsx => count_slash_plain(text)
    LangPython => count_hash_language(text)
    LangJson => count_json(text)
    LangTypeScript => count_slash_plain(text)
    LangPoFile => count_hash_language(text)
    LangHtml => count_html(text, true)
    LangSvg => count_html(text, false)
    LangMdx => count_plain_text(text)
    LangMarkdown => count_markdown(text)
    LangLess => count_slash_plain(text)
    LangJavaScript => count_slash_plain(text)
    LangYaml => count_hash_language(text)
    LangBash => count_hash_language(text)
    LangCss => count_slash_plain(text)
    LangDockerfile => count_hash_language(text)
    LangForgeConfig => count_hash_language(text)
    LangIni => count_hash_language(text)
    LangLua => count_lua(text)
    LangMakefile => count_hash_language(text)
    LangModelica => count_slash_language(text, true, false)
    LangPlainText => count_plain_text(text)
    LangReStructuredText => count_code_text(text)
    LangRust => count_slash_language(text, true, true)
    LangShell => count_hash_language(text)
    LangTempl => count_slash_plain(text)
    LangToml => count_hash_language(text)
    LangXml => count_html(text, false)
    _ => empty
  }
}

pure hidden_relative(rel: Str) -> Bool {
  return rel.starts_with(".") or "/." in rel
}

pure ignored_by_patterns(rel: Str, patterns: List[Str]) -> Bool {
  for raw in patterns {
    let pattern = raw.trim()

    if pattern != "" and ! pattern.starts_with("#") {
      if rel == pattern or rel.starts_with(f"${pattern}/") {
        return true
      }
    }
  }

  return false
}

proc ignored_patterns(root: Path) [fs, error] -> Result[List[Str]] {
  let ignore_file = fp"${root}/.tokeignore"

  if ignore_file.exists()? {
    return ignore_file.lines()?.collect()
  }

  []
}

proc blob_stats(report: FileReport, language: Str) [error] -> Result[Stats] {
  return report.stats.blobs.get(language)?.require(Stats)?
}

proc children_from_reports(reports: List[FileReport]) [error] -> Result[Map[Any]] {
  var children: Map[List[FileReport]] = {}

  for report in reports {
    for language in report.stats.blobs.keys() {
      children = children.push(language, {stats: blob_stats(report, language)?, name: report.name})
    }
  }

  return children
}

proc json_main(root: Path, ignore_patterns: List[Str]) [fs, error] {
  let aggregates = fs.files(root, stat: false, exts: source_exts())?
    |> map { |entry|
      let language = lang_for_name_ext(entry.name, entry.ext)
      let rel = entry.path.relative_to(root).display()
      let keep = (language != LangUnknown or entry.ext == "") and ! hidden_relative(rel) and ! ignored_by_patterns(
        rel,
        ignore_patterns,
      )
      {language, keep, path: entry.path}
    }
    |> where .keep
    |> par-map { |candidate|
      var text = candidate.path.read_bytes() ?? b""
      let language = if candidate.language == LangUnknown { lang_for_shebang(text) } else { candidate.language }
      let scan = match language {
        LangTsx => count_slash_plain(text),
        LangPython => count_hash_language(text),
        LangJson => count_json(text),
        LangTypeScript => count_slash_plain(text),
        LangPoFile => count_hash_language(text),
        LangHtml => count_html(text, true),
        LangSvg => count_html(text, false),
        LangMdx => count_plain_text(text),
        LangMarkdown => count_markdown(text),
        LangLess => count_slash_plain(text),
        LangJavaScript => count_slash_plain(text),
        LangYaml => count_hash_language(text),
        LangBash => count_hash_language(text),
        LangCss => count_slash_plain(text),
        LangDockerfile => count_hash_language(text),
        LangForgeConfig => count_hash_language(text),
        LangIni => count_hash_language(text),
        LangLua => count_lua(text),
        LangMakefile => count_hash_language(text),
        LangModelica => count_slash_language(text, true, false),
        LangPlainText => count_plain_text(text),
        LangReStructuredText => count_code_text(text),
        LangRust => count_slash_language(text, true, true),
        LangShell => count_hash_language(text),
        LangTempl => count_slash_plain(text),
        LangToml => count_hash_language(text),
        LangXml => count_html(text, false),
        _ => count_language(language, text),
      }
      text = b""
      {language, report: {stats: scan.stats, name: candidate.path.display()}, deep: scan.deep}
    }
    |> where .language != LangUnknown
    |> reduce-by --sum { |scanned|
      {
        key: language_label(scanned.language),
        value: {
          blanks: scanned.report.stats.blanks,
          code: scanned.report.stats.code,
          comments: scanned.report.stats.comments,
          total_blanks: scanned.deep.blanks,
          total_code: scanned.deep.code,
          total_comments: scanned.deep.comments,
          reports: [
            scanned.report,
          ],
        },
      }
    }

  var output: Map[Any] = {}
  var total_children: Map[Any] = {}
  var total_blanks = 0
  var total_code = 0
  var total_comments = 0

  for language in languages() {
    let label = language_label(language)
    let aggregate = aggregates.get(
      label,
      {
        blanks: 0,
        code: 0,
        comments: 0,
        total_blanks: 0,
        total_code: 0,
        total_comments: 0,
        reports: [],
      },
    )
    let reports = aggregate.reports
    continue when reports.len() == 0
    let children = children_from_reports(reports)?
    output[label] = {
      blanks: aggregate.blanks,
      code: aggregate.code,
      comments: aggregate.comments,
      reports,
      children,
      inaccurate: false,
    }
    total_children[label] = reports
    total_blanks += aggregate.total_blanks
    total_code += aggregate.total_code
    total_comments += aggregate.total_comments
  }

  output["Total"] = {
    blanks: total_blanks,
    code: total_code,
    comments: total_comments,
    reports: [],
    children: total_children,
    inaccurate: false,
  }
  print (json.encode(output)?)
}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(argv, {root: {form: "ROOT", default: p"."}, json: {form: "--json", default: false}})?
  let root = opts.root.resolve()?
  let ignore_patterns = ignored_patterns(root)?

  if ! opts.json {
    let zero_summary: SummaryTotals = {
      files: 0,
      blanks: 0,
      code: 0,
      comments: 0,
      total_blanks: 0,
      total_code: 0,
      total_comments: 0,
    }

    let summary = fs.files(root, stat: false, exts: source_exts())?
      |> map { |entry|
        let language = lang_for_name_ext(entry.name, entry.ext)
        let rel = entry.path.relative_to(root).display()

        # Keep extension-less files even when the name gives no language, so par-map
        # can read their shebang; everything else must already resolve to a language.
        let keep = (language != LangUnknown or entry.ext == "") and ! hidden_relative(rel) and ! ignored_by_patterns(
          rel,
          ignore_patterns,
        )

        {language, keep, path: entry.path, rel}
      }
      |> where .keep
      |> par-map { |candidate|
        var text = candidate.path.read_bytes() ?? b""
        let language = if candidate.language == LangUnknown { lang_for_shebang(text) } else { candidate.language }

        let scan = match language {
          LangTsx => count_slash_plain(text),
          LangPython => count_hash_language(text),
          LangJson => count_json(text),
          LangTypeScript => count_slash_plain(text),
          LangPoFile => count_hash_language(text),
          LangHtml => count_html(text, true),
          LangSvg => count_html(text, false),
          LangMdx => count_plain_text(text),
          LangMarkdown => count_markdown(text),
          LangLess => count_slash_plain(text),
          LangJavaScript => count_slash_plain(text),
          LangYaml => count_hash_language(text),
          LangBash => count_hash_language(text),
          LangCss => count_slash_plain(text),
          LangDockerfile => count_hash_language(text),
          LangForgeConfig => count_hash_language(text),
          LangIni => count_hash_language(text),
          LangLua => count_lua(text),
          LangMakefile => count_hash_language(text),
          LangModelica => count_slash_language(text, true, false),
          LangPlainText => count_plain_text(text),
          LangReStructuredText => count_code_text(text),
          LangRust => count_slash_language(text, true, true),
          LangShell => count_hash_language(text),
          LangTempl => count_slash_plain(text),
          LangToml => count_hash_language(text),
          LangXml => count_html(text, false),
          _ => count_language(language, text),
        }

        text = b""
        var out: List[SummaryRow] = []

        if language != LangUnknown {
          # One record for the file's own language, plus one per embedded ("blob")
          # language so the per-(parent, child) breakdown aggregates in the same
          # fused reduce-by. Child records are keyed `parent\tchild`; the grand-total
          # and per-language rows read only the plain (parent) keys, so children
          # never double-count. `blobs` only ever holds non-zero embedded stats.
          let label = language_label(language)

          out = out.push({
            key: label,
            files: 1,
            blanks: scan.stats.blanks,
            code: scan.stats.code,
            comments: scan.stats.comments,
            total_blanks: scan.deep.blanks,
            total_code: scan.deep.code,
            total_comments: scan.deep.comments,
          })

          let has_child_blobs = match language {
            LangHtml => true,
            LangMarkdown => true,
            LangMdx => true,
            LangRust => true,
            LangSvg => true,
            LangXml => true,
            _ => false,
          }

          if has_child_blobs {
            for child in scan.stats.blobs.keys() {
              let blob = scan.stats.blobs.get(child, zero_stats()).require(Stats) ?? zero_stats()
              let cs = blob_deep(blob)

              out = out.push({
                key: f"${label}\t${child}",
                files: 1,
                blanks: cs.blanks,
                code: cs.code,
                comments: cs.comments,
                total_blanks: 0,
                total_code: 0,
                total_comments: 0,
              })
            }
          }
        }

        out
      }
      |> flat-map { |rows|
        rows
      }
      |> reduce-by --sum { |item|
        {
          key: item.key,
          value: {
            files: item.files,
            blanks: item.blanks,
            code: item.code,
            comments: item.comments,
            total_blanks: item.total_blanks,
            total_code: item.total_code,
            total_comments: item.total_comments,
          },
        }
      }

    var no_child: List[Str] = []
    var child_blocks: List[List[Str]] = []
    var total_files = 0
    var total_blanks = 0
    var total_code = 0
    var total_comments = 0

    for language in sorted_languages() {
      let label = language_label(language)
      let totals = summary.get(label, zero_summary)
      continue when totals.files == 0
      total_files += totals.files
      total_blanks += totals.total_blanks
      total_code += totals.total_code
      total_comments += totals.total_comments
      var child_rows: List[Str] = []

      for child in sorted_languages() {
        let clabel = language_label(child)
        let cagg = summary.get(f"${label}\t${clabel}", zero_summary)
        continue when cagg.files == 0
        let clines = cagg.blanks + cagg.code + cagg.comments

        child_rows = child_rows.push(
          fmt_row(
            f"|- ${clabel}",
            f"${cagg.files}",
            f"${clines}",
            f"${cagg.code}",
            f"${cagg.comments}",
            f"${cagg.blanks}",
          ),
        )
      }

      let lines = totals.blanks + totals.code + totals.comments

      let lang_row = fmt_row(
        label,
        f"${totals.files}",
        f"${lines}",
        f"${totals.code}",
        f"${totals.comments}",
        f"${totals.blanks}",
      )

      if child_rows.len() == 0 {
        no_child = no_child.push(lang_row)
      } else {
        let deep_lines = totals.total_blanks + totals.total_code + totals.total_comments

        let subtotal = fmt_row(
          "(Total)",
          "",
          f"${deep_lines}",
          f"${totals.total_code}",
          f"${totals.total_comments}",
          f"${totals.total_blanks}",
        )

        var block = [lang_row]
        block = block.extend(child_rows)
        block = block.push(subtotal)
        child_blocks = child_blocks.push(block)
      }
    }

    let heavy = rule("\u{2501}", 81)
    let light = rule("\u{2500}", 81)
    print $heavy
    print fmt_row("Language", "Files", "Lines", "Code", "Comments", "Blanks")
    print $heavy

    for row in no_child {
      print $row
    }

    var printed_any = no_child.len() > 0

    for block in child_blocks {
      if printed_any {
        print $light
      }

      for row in block {
        print $row
      }

      printed_any = true
    }

    let grand_lines = total_blanks + total_code + total_comments
    print $heavy

    print fmt_row(
      "Total",
      f"${total_files}",
      f"${grand_lines}",
      f"${total_code}",
      f"${total_comments}",
      f"${total_blanks}",
    )

    print $heavy
    return
  }

  return json_main(root, ignore_patterns)?
}
