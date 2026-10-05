##! Renders `docs/user-tour.md` as one self-contained HTML page: inline styles
##! and script, no external requests, readable without JavaScript.
##!
##! The renderer understands exactly the Markdown the tour is written in:
##! headings, paragraphs, bullet lists (items may hold paragraphs and fenced
##! code), pipe tables, fenced code, code spans, and links. Anything else is
##! reported instead of being rendered wrongly. A `text` fence that follows an
##! `<!-- expected-output -->` marker is the output of the code before it and is
##! drawn as a console attached to that code.
##!
##! XSH code is colored by `xsht highlight`, so the page uses the lexer the
##! language itself uses. Shell and INI fences, which that lexer does not
##! cover, get small line-based colorers below.

## A Markdown construct the renderer does not support, or a highlighter failure.
export error TourHtmlError = Unsupported | Highlighter

type HeadingBlock = {level: Int, id: Str, markup: Str}

type TableBlock = {header: List[Str], rows: List[List[Str]]}

type CodeBlock = {lang: Str, source: Str}

enum Block {
    Head(HeadingBlock),
    Para(Str),
    Bullets(List[List[Block]]),
    Grid(TableBlock),
    Fence(CodeBlock),
    Console(Str),
}

type FenceText = {source: Str, next: Int}

type Items = {items: List[List[Str]], next: Int}

type Taken = {lines: List[Str], next: Int}

type HighlightRun = {kind: Str, text: Str}

# Where `xsht highlight` reads each block from.
type Highlighter = {xsht: Path, scratch: Path}

const output_marker = "<!-- expected-output -->"

const html_comment = rx"^<!--.*-->$"

const fence_open = rx"^```([a-z]*)$"

const fence_close = "```"

const heading_line = rx"^(#{1,6}) +(.+)$"

const block_start = rx"^(- |#{1,6} |\||<!--|```)"

const slug_gap = rx"[^a-z0-9]+"

const slug_edge = rx"^-+|-+$"

const table_edge = rx"^\||\|$"

const inline_token = rx"`[^`]+`|\[(?:[^\]`]|`[^`]*`)+\]\([^)\s]+\)"

const link_parts = rx"^\[(.+)\]\(([^)\s]+)\)$"

const fence_labels: Map[Str] = {xsh: "xsh", bash: "shell", ini: "ini", text: "text"}

# One alternative per shell lexeme; plain words are matched too so a word can
# be classified by where it stands in the command.
const shell_token = rx"""#.*$|"(?:[^"\\]|\\.)*"|'[^']*'|\$\{[^}]*\}|\$\(|\$[A-Za-z_][A-Za-z0-9_]*|-{1,2}[A-Za-z][A-Za-z0-9-]*|&&|\|\||[|;<>&]|[A-Za-z_./][A-Za-z0-9_./-]*"""

const shell_keywords = [
  "if",
  "then",
  "else",
  "elif",
  "fi",
  "for",
  "in",
  "do",
  "done",
  "while",
  "until",
  "case",
  "esac",
  "function",
  "local",
  "export",
  "set",
  "trap",
  "unset",
  "return",
]

# Keywords after which a command word follows.
const shell_command_openers = ["if", "then", "else", "elif", "while", "until", "do"]

const shell_separators = ["|", "&&", "||", ";", "$("]

const ini_comment = rx"^(\s*)([#;].*)$"

const ini_section = rx"^(\s*)(\[[^\]]+\])(\s*)$"

const ini_entry = rx"^(\s*)([A-Za-z0-9_.-]+)(\s*=\s*)(.*)$"

# Applies the saved or system color scheme before the first paint.
const theme_script = """
(function () {
  var root = document.documentElement;
  var theme = null;
  try { theme = localStorage.getItem('xsh-tour-theme'); } catch (error) {}
  if (theme !== 'light' && theme !== 'dark') {
    theme = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  root.dataset.theme = theme;
})();
"""

const page_script = """
(function () {
  var root = document.documentElement;
  var body = document.body;

  var toggle = document.querySelector('.theme-toggle');
  toggle.hidden = false;
  toggle.addEventListener('click', function () {
    var next = root.dataset.theme === 'dark' ? 'light' : 'dark';
    root.dataset.theme = next;
    try { localStorage.setItem('xsh-tour-theme', next); } catch (error) {}
  });

  var menu = document.querySelector('.toc-toggle');
  function setMenu(open) {
    body.classList.toggle('toc-open', open);
    menu.setAttribute('aria-expanded', open ? 'true' : 'false');
  }
  menu.addEventListener('click', function () { setMenu(!body.classList.contains('toc-open')); });
  document.querySelector('.toc').addEventListener('click', function (event) {
    if (event.target.closest('a')) { setMenu(false); }
  });
  document.addEventListener('keydown', function (event) {
    if (event.key === 'Escape') { setMenu(false); }
  });

  function copyText(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text);
    }
    return new Promise(function (resolve, reject) {
      var area = document.createElement('textarea');
      area.value = text;
      area.style.position = 'fixed';
      area.style.opacity = '0';
      body.appendChild(area);
      area.select();
      var copied = document.execCommand('copy');
      body.removeChild(area);
      if (copied) { resolve(); } else { reject(); }
    });
  }

  document.querySelectorAll('figure.code').forEach(function (figure) {
    var button = document.createElement('button');
    button.type = 'button';
    button.className = 'copy';
    button.textContent = 'Copy';
    button.addEventListener('click', function () {
      copyText(figure.querySelector('code').textContent).then(function () {
        button.textContent = 'Copied';
        button.classList.add('done');
        setTimeout(function () {
          button.textContent = 'Copy';
          button.classList.remove('done');
        }, 1600);
      });
    });
    figure.querySelector('figcaption').appendChild(button);
  });

  var links = {};
  document.querySelectorAll('.toc a').forEach(function (link) {
    links[link.getAttribute('href').slice(1)] = link;
  });
  var current = null;
  var observer = new IntersectionObserver(function (entries) {
    entries.forEach(function (entry) {
      var link = links[entry.target.id];
      if (!entry.isIntersecting || !link) { return; }
      if (current) { current.removeAttribute('aria-current'); }
      link.setAttribute('aria-current', 'true');
      current = link;
      link.scrollIntoView({ block: 'nearest' });
    });
  }, { rootMargin: '0px 0px -75% 0px' });
  document.querySelectorAll('main h2[id], main h3[id]').forEach(function (heading) {
    observer.observe(heading);
  });
})();
"""

const stylesheet = """
:root {
  color-scheme: light dark;
  --bg: light-dark(#ffffff, #1a1918);
  --ink: light-dark(#141413, #e8e6dc);
  --body: light-dark(#3d3d3a, #c9c7bc);
  --muted: light-dark(#73726c, #959388);
  --rule: light-dark(#e8e6dc, #302f2c);
  --accent: light-dark(#c4623f, #e08a68);
  --hover: light-dark(#f5f4ed, #262524);
  --code-bg: light-dark(#faf9f5, #211f1e);
  --inline-bg: light-dark(#f2f0e8, #292826);
  --hl-keyword: light-dark(#7b5cb8, #b79fe0);
  --hl-constant: light-dark(#35709b, #86b3d6);
  --hl-type: light-dark(#2e7f77, #7fc4bb);
  --hl-function: light-dark(#3b6fb6, #86aee0);
  --hl-property: light-dark(#8a6a2f, #cfae6e);
  --hl-variable: light-dark(#a35a2e, #d6a07a);
  --hl-string: light-dark(#3a7d44, #9cc794);
  --hl-path: light-dark(#8a6a2f, #cfae6e);
  --hl-regex: light-dark(#a8487a, #d896b8);
  --hl-number: light-dark(#b0602a, #d9a074);
  --hl-comment: light-dark(#8b8a82, #78776f);
  --hl-doc-comment: light-dark(#6f8260, #8fa282);
  --hl-operator: light-dark(#67665f, #a09f96);
  --hl-punctuation: light-dark(#8b8a82, #8b8a82);
  --hl-interpolation: light-dark(#a8487a, #d896b8);
  --sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
  --mono: ui-monospace, "SF Mono", SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
  --topbar: 3.5rem;
}

:root[data-theme="light"] { color-scheme: light; }
:root[data-theme="dark"] { color-scheme: dark; }

* { box-sizing: border-box; }

html {
  scroll-behavior: smooth;
  scroll-padding-top: calc(var(--topbar) + 1.5rem);
  -webkit-text-size-adjust: 100%;
}

@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--body);
  font: 1rem/1.7 var(--sans);
  -webkit-font-smoothing: antialiased;
}

a { color: var(--ink); text-decoration-color: var(--muted); text-decoration-thickness: 1px; text-underline-offset: 0.2em; }
a:hover { text-decoration-color: var(--accent); }

::selection { background: var(--inline-bg); }

.topbar {
  position: sticky;
  top: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  height: var(--topbar);
  padding: 0 1.25rem;
  background: var(--bg);
  border-bottom: 1px solid var(--rule);
}

.brand {
  display: inline-flex;
  align-items: baseline;
  gap: 0.5rem;
  margin-right: auto;
  color: var(--ink);
  font-weight: 600;
  text-decoration: none;
}

.brand span { color: var(--muted); font-weight: 400; }

.icon-button {
  display: inline-grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  padding: 0;
  border: 0;
  border-radius: 0.5rem;
  background: transparent;
  color: var(--muted);
  font: 1rem/1 var(--sans);
  cursor: pointer;
}

.icon-button:hover { background: var(--hover); color: var(--ink); }
.icon-button:focus-visible, .copy:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.toc-toggle { display: none; }

.theme-toggle span::before { content: "☾"; }
:root[data-theme="dark"] .theme-toggle span::before { content: "☀"; }

.layout {
  display: grid;
  grid-template-columns: 16rem minmax(0, 1fr);
  max-width: 76rem;
  margin: 0 auto;
}

.toc {
  position: sticky;
  top: var(--topbar);
  height: calc(100vh - var(--topbar));
  overflow-y: auto;
  padding: 1.5rem 1rem 3rem 1.25rem;
  font-size: 0.875rem;
  line-height: 1.4;
}

.toc ol { margin: 0; padding: 0; list-style: none; }

.toc a {
  display: block;
  padding: 0.3rem 0.65rem;
  border-radius: 0.4rem;
  color: var(--muted);
  text-decoration: none;
}

.toc ol ol { margin: 0.1rem 0 0.3rem 0.9rem; }
.toc a:hover { color: var(--ink); background: var(--hover); }
.toc a[aria-current] { color: var(--ink); background: var(--hover); font-weight: 500; }

main {
  min-width: 0;
  max-width: 48rem;
  margin: 0 auto;
  padding: 2.5rem 2rem 6rem;
}

.hero { padding: 0.5rem 0 0.25rem; }

h1 {
  margin: 0;
  color: var(--ink);
  font-size: 2.25rem;
  font-weight: 600;
  line-height: 1.2;
  letter-spacing: -0.02em;
}

h1 .anchor { color: inherit; text-decoration: none; }

.lede { margin: 1rem 0 1.25rem; font-size: 1.125rem; color: var(--muted); }
.lede p { margin: 0; }

h2, h3 { color: var(--ink); font-weight: 600; line-height: 1.3; letter-spacing: -0.01em; }
h2 { margin: 3rem 0 1rem; font-size: 1.5rem; }
h3 { margin: 2rem 0 0.75rem; font-size: 1.15rem; }

.anchor { color: inherit; text-decoration: none; }
.anchor::after { content: " #"; color: var(--muted); opacity: 0; transition: opacity 0.15s; }
h2:hover .anchor::after, h3:hover .anchor::after, .anchor:focus-visible::after { opacity: 0.8; }

p { margin: 0 0 1rem; }
ul { margin: 0 0 1.1rem; padding-left: 1.3rem; }
li { margin: 0.35rem 0; padding-left: 0.2rem; }
li::marker { color: var(--muted); }
li > p:last-child, li > figure:last-child, li > ul:last-child { margin-bottom: 0; }

code {
  font: 0.875em/1.4 var(--mono);
  font-variant-ligatures: none;
  padding: 0.1em 0.35em;
  border: 1px solid var(--rule);
  border-radius: 0.35rem;
  background: var(--inline-bg);
  color: var(--ink);
  word-break: break-word;
}

figure { margin: 1.25rem 0; }

figure.code, figure.console {
  border: 1px solid var(--rule);
  border-radius: 0.6rem;
  overflow: hidden;
  background: var(--code-bg);
  color: var(--ink);
}

figcaption {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 2.25rem;
  padding: 0.25rem 0.5rem 0.25rem 1rem;
  border-bottom: 1px solid var(--rule);
  color: var(--muted);
  font-size: 0.75rem;
}

.copy {
  padding: 0.25rem 0.55rem;
  border: 0;
  border-radius: 0.35rem;
  background: transparent;
  color: var(--muted);
  font: 0.75rem/1.2 var(--sans);
  cursor: pointer;
}

.copy:hover { background: var(--hover); color: var(--ink); }
.copy.done { color: var(--hl-string); }

pre {
  margin: 0;
  padding: 0.9rem 1rem;
  font: 0.85rem/1.65 var(--mono);
  font-variant-ligatures: none;
  tab-size: 2;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

pre code { padding: 0; border: 0; background: none; font: inherit; word-break: normal; white-space: inherit; overflow-wrap: inherit; }

figure.code:has(+ figure.console) { margin-bottom: 0; border-bottom-left-radius: 0; border-bottom-right-radius: 0; }
figure.code + figure.console { margin-top: 0; border-top: 0; border-top-left-radius: 0; border-top-right-radius: 0; background: var(--bg); }
figure.console figcaption { min-height: 1.9rem; }
figure.console pre { color: var(--body); }

.table-wrap { margin: 1.25rem 0; border: 1px solid var(--rule); border-radius: 0.6rem; }
table { width: 100%; border-collapse: collapse; font-size: 0.925rem; line-height: 1.5; }
th, td { padding: 0.55rem 0.9rem; text-align: left; vertical-align: top; border-bottom: 1px solid var(--rule); }
th { color: var(--ink); font-weight: 600; background: var(--code-bg); }
tr:last-child td { border-bottom: 0; }

.hl-keyword { color: var(--hl-keyword); }
.hl-constant { color: var(--hl-constant); }
.hl-type { color: var(--hl-type); }
.hl-function { color: var(--hl-function); }
.hl-property { color: var(--hl-property); }
.hl-variable { color: var(--hl-variable); }
.hl-string { color: var(--hl-string); }
.hl-path { color: var(--hl-path); }
.hl-regex { color: var(--hl-regex); }
.hl-number { color: var(--hl-number); }
.hl-comment { color: var(--hl-comment); }
.hl-doc-comment { color: var(--hl-doc-comment); }
.hl-operator { color: var(--hl-operator); }
.hl-punctuation { color: var(--hl-punctuation); }
.hl-interpolation { color: var(--hl-interpolation); }

.colophon { margin-top: 4rem; padding-top: 1.25rem; border-top: 1px solid var(--rule); color: var(--muted); font-size: 0.925rem; }

@media (max-width: 62rem) {
  .layout { display: block; }
  .toc-toggle { display: inline-grid; }
  .toc {
    position: fixed;
    inset: var(--topbar) auto 0 0;
    z-index: 15;
    width: min(18rem, 86vw);
    height: auto;
    background: var(--bg);
    border-right: 1px solid var(--rule);
    transform: translateX(-100%);
    visibility: hidden;
    transition: transform 0.2s ease, visibility 0.2s;
  }
  body.toc-open .toc { transform: none; visibility: visible; }
  main { padding: 1.5rem 1.25rem 5rem; }
}

@media (max-width: 40rem) {
  h1 { font-size: 1.85rem; }
  h2 { font-size: 1.35rem; }
}

@media print {
  .topbar, .toc, .copy { display: none; }
  .layout { display: block; }
  body { background: #fff; color: #000; font-size: 11pt; }
  figure, .table-wrap { break-inside: avoid; }
  main { max-width: none; padding: 0; }
}
"""

pure esc(text: Str) -> Str {
  text.replace("&", with: "&amp;").replace("<", with: "&lt;").replace(">", with: "&gt;")
}

pure attr(text: Str) -> Str {
  esc(text).replace("\"", with: "&quot;")
}

pure span(kind: Str, text: Str) -> Str {
  return esc(text) when kind == "plain"

  f"<span class=\"hl-{kind}\">{esc(text)}</span>"
}

# The slice of `text` between two byte offsets.
pure between(text: Str, start: Int, end: Int) -> Str {
  text.byte_slice(start, end - start)
}

pure slug(markup: Str) -> Str {
  slug_edge.replace(slug_gap.replace(markup.lower(), with: "-"), with: "")
}

# Prose markup with code spans and links turned into elements.
pure inline(text: Str) -> Str {
  var cursor = 0
  let parts = collect {
    for found in inline_token.find(text) {
      yield esc(between(text, cursor, found.start))
      if found.text.starts_with("`") {
        yield f"<code>{esc(between(found.text, 1, found.text.byte_len() - 1))}</code>"
      } else {
        let link = link_parts.captures(found.text)
        yield f"<a href=\"{attr(link[2])}\">{inline(link[1])}</a>"
      }

      cursor = found.end
    }

    yield esc(text.byte_slice(cursor))
  }
  parts.join("")
}

pure starts_block(line: Str) -> Bool {
  block_start.matches(line)
}

pure take_fence(lines: List[Str], start: Int) -> Result[FenceText] {
  var index = start + 1
  let body = collect {
    while index < lines.len() and lines[index] != fence_close {
      yield lines[index]
      index += 1
    }
  }

  guard index < lines.len() else {
    return Err(TourHtmlError.Unsupported(f"unterminated code fence {lines[start]}"))
  }

  FenceText(body.join("\n"), index + 1)
}

# The lines of one bullet list. A blank line stays inside the list only when
# the next non-blank line is another item or continues one.
pure take_list(lines: List[Str], start: Int) -> Items {
  var items = []
  var current = []
  var open = false
  var index = start
  while index < lines.len() {
    let line = lines[index]
    if line.starts_with("- ") {
      if open {
        items += [current]
      }

      current = [line.byte_slice(2)]
      open = true
      index += 1
    } else if line.starts_with("  ") {
      current += [line.byte_slice(2)]
      index += 1
    } else if line.trim() == "" {
      var peek = index + 1
      while peek < lines.len() and lines[peek].trim() == "" {
        peek += 1
      }

      if peek < lines.len() and (lines[peek].starts_with("- ") or lines[peek].starts_with("  ")) {
        current += [""]
        index += 1
      } else {
        break
      }
    } else {
      break
    }
  }

  if open {
    items += [current]
  }

  Items(items:, next: index)
}

pure table_cells(line: Str) -> List[Str] {
  [cell.trim() for cell in table_edge.replace(line.trim(), with: "").split("|")]
}

pure take_table(lines: List[Str], start: Int) -> Result[Taken] {
  var index = start
  while index < lines.len() and lines[index].starts_with("|") {
    index += 1
  }

  guard index - start >= 2 else {
    return Err(TourHtmlError.Unsupported(f"table without a delimiter row: {lines[start]}"))
  }

  Taken(lines[start..index], index)
}

pure take_paragraph(lines: List[Str], start: Int) -> Taken {
  var index = start
  while index < lines.len() and lines[index].trim() != "" and (index == start or ! starts_block(lines[index])) {
    index += 1
  }

  Taken(lines[start..index], index)
}

pure parse_blocks(lines: List[Str]) -> Result[List[Block]] {
  var blocks: List[Block] = []
  var output_next = false
  var index = 0
  while index < lines.len() {
    let line = lines[index]
    if line.trim() == "" {
      index += 1
    } else if line.trim() == output_marker {
      output_next = true
      index += 1
    } else if html_comment.matches(line.trim()) {
      index += 1
    } else if fence_open.matches(line) {
      let lang = fence_open.captures(line)[1]
      guard lang in fence_labels else {
        return Err(TourHtmlError.Unsupported(f"code fence language `{lang}` has no renderer"))
      }

      let fence = take_fence(lines, index)?
      if output_next and lang == "text" {
        blocks += [Console(fence.source)]
      } else {
        blocks += [Fence(CodeBlock(lang:, source: fence.source))]
      }

      output_next = false
      index = fence.next
    } else if heading_line.matches(line) {
      let parts = heading_line.captures(line)
      blocks += [Head(HeadingBlock(level: parts[1].byte_len(), id: slug(parts[2]), markup: parts[2]))]
      index += 1
    } else if line.starts_with("|") {
      let table = take_table(lines, index)?
      let header = table_cells(table.lines[0])
      let rows = collect {
        for row in table.lines[2..] {
          let cells = table_cells(row)
          guard cells.len() == header.len() else {
            return Err(
              TourHtmlError.Unsupported(f"table row has {cells.len()} cells, header has {header.len()}: {row}"),
            )
          }

          yield cells
        }
      }

      blocks += [Grid(TableBlock(header:, rows:))]
      index = table.next
    } else if line.starts_with("- ") {
      let list = take_list(lines, index)
      var items: List[List[Block]] = [parse_blocks(item)? for item in list.items]
      blocks += [Bullets(items)]
      index = list.next
    } else {
      let paragraph = take_paragraph(lines, index)
      blocks += [Para(paragraph.lines.join(" "))]
      index = paragraph.next
    }
  }

  blocks
}

# Colors one shell line. A word is a command at the start of a line and after
# a pipe, `&&`, `;`, `$(`, or a keyword such as `then`.
pure shell_line(line: Str) -> Str {
  var parts = []
  var cursor = 0
  var command = true
  for found in shell_token.find(line) {
    parts += [esc(between(line, cursor, found.start))]
    let text = found.text
    let assigned = line.byte_slice(found.end).starts_with("=")
    var kind = "plain"
    if text.starts_with("#") {
      kind = "comment"
    } else if text.starts_with("\"") or text.starts_with("'") {
      kind = "string"
      command = false
    } else if text.starts_with("$") and text != "$(" {
      kind = "variable"
      command = false
    } else if text in shell_separators {
      kind = "operator"
      command = true
    } else if text in ["<", ">"] {
      kind = "operator"
    } else if text.starts_with("-") {
      kind = "property"
    } else if text in shell_keywords {
      kind = "keyword"
      command = text in shell_command_openers
    } else if assigned {
      kind = "variable"
      command = false
    } else if command {
      kind = "function"
      command = false
    }

    parts += [span(kind, text)]
    cursor = found.end
  }

  parts += [esc(line.byte_slice(cursor))]
  parts.join("")
}

pure ini_line(line: Str) -> Str {
  if let [_, space, comment] = ini_comment.captures(line) {
    return f"{space}{span("comment", comment)}"
  }

  if let [_, space, name, tail] = ini_section.captures(line) {
    return f"{space}{span("keyword", name)}{tail}"
  }

  if let [_, space, key, equals, value] = ini_entry.captures(line) {
    return f"{space}{span("property", key)}{span("operator", equals)}{esc(value)}"
  }

  esc(line)
}

# Source colored by `xsht highlight`, which prints one `{kind, text}` run per
# line and whose runs concatenate back to the source.
proc xsh_html(hl: Highlighter, source: Str) [fs, process, error] -> Result[Str] {
  let file = fp"{hl.scratch}/block.xsh"
  file.write(source)
  let lines = run.text $hl.xsht highlight $file
  let parts = collect {
    for line in lines.lines() {
      let found = json.decode(line)?.require(HighlightRun)?
      yield span(found.kind, found.text)
    }
  }

  parts.join("")
}

proc code_html(hl: Highlighter, block: CodeBlock) [fs, process, error] -> Result[Str] {
  let colored = match block.lang {
    "xsh" => xsh_html(hl, block.source)?,
    "bash" => [shell_line(line) for line in block.source.lines()].join("\n"),
    "ini" => [ini_line(line) for line in block.source.lines()].join("\n"),
    else => esc(block.source),
  }

  let label = fence_labels[block.lang]
  f"<figure class=\"code code-{block.lang}\"><figcaption><span class=\"lang\">{label}</span></figcaption><pre><code>{colored}</code></pre></figure>"
}

pure table_html(table: TableBlock) -> Str {
  let head = [f"<th>{inline(cell)}</th>" for cell in table.header].join("")
  let rows = [
    "<tr>" + [f"<td>{inline(cell)}</td>" for cell in row].join("") + "</tr>"
    for row in table.rows
  ]
  f"<div class=\"table-wrap\"><table><thead><tr>{head}</tr></thead><tbody>{rows.join("")}</tbody></table></div>"
}

pure heading_html(heading: HeadingBlock) -> Str {
  let tag = f"h{heading.level}"
  f"<{tag} id=\"{heading.id}\"><a class=\"anchor\" href=\"#{heading.id}\">{inline(heading.markup)}</a></{tag}>"
}

# A single-paragraph item renders without a paragraph wrapper.
proc item_html(hl: Highlighter, item: List[Block]) [fs, process, error] -> Result[Str] {
  if item.len() == 1 {
    if let Para(text) = item[0] {
      return f"<li>{inline(text)}</li>"
    }
  }

  f"<li>{blocks_html(hl, item)?}</li>"
}

proc block_html(hl: Highlighter, block: Block) [fs, process, error] -> Result[Str] {
  match block {
    Head(heading) => heading_html(heading)
    Para(text) => f"<p>{inline(text)}</p>"
    Bullets(items) => {
      let rendered = [item_html(hl, item)? for item in items]
      f"<ul>{rendered.join("\n")}</ul>"
    }
    Grid(table) => table_html(table)
    Fence(code) => code_html(hl, code)?
    Console(text) => f"<figure class=\"console\"><figcaption><span class=\"lang\">output</span></figcaption><pre><code>{esc(text)}</code></pre></figure>"
  }
}

proc blocks_html(hl: Highlighter, blocks: List[Block]) [fs, process, error] -> Result[Str] {
  [block_html(hl, block)? for block in blocks].join("\n")
}

# The contents list: each `##` heading with its `###` headings nested.
pure toc_html(headings: List[HeadingBlock]) -> Str {
  var parts: List[Str] = ["<ol>"]
  var chapter_open = false
  var nested_open = false
  for heading in headings {
    let link = f"<a href=\"#{heading.id}\">{inline(heading.markup)}</a>"
    if heading.level == 2 {
      if nested_open {
        parts += ["</ol>"]
        nested_open = false
      }

      if chapter_open {
        parts += ["</li>"]
      }

      parts += [f"<li>{link}"]
      chapter_open = true
    } else {
      if ! nested_open {
        parts += ["<ol>"]
        nested_open = true
      }

      parts += [f"<li>{link}</li>"]
    }
  }

  if nested_open {
    parts += ["</ol>"]
  }

  if chapter_open {
    parts += ["</li>"]
  }

  parts += ["</ol>"]
  parts.join("\n")
}

pure plain_text(markup: Str) -> Str {
  markup.replace("`", with: "")
}

pure tour_title(first: Block) -> Result[HeadingBlock] {
  if let Head(heading) = first {
    return heading when heading.level == 1
  }

  Err(TourHtmlError.Unsupported("the tour must start with a `#` title"))
}

# The page body: contents list, title block, introduction, and one section per
# `##` heading.
proc page_html(hl: Highlighter, blocks: List[Block]) [fs, process, error] -> Result[Str] {
  guard ! blocks.is_empty() else {
    return Err(TourHtmlError.Unsupported("the tour is empty"))
  }

  let title = tour_title(blocks[0])?
  var headings: List[HeadingBlock] = []
  var seen = []
  var intro = []
  var chapters = []
  var in_chapter = false
  var lede = true
  for block in blocks[1..] {
    if let Head(heading) = block {
      guard heading.id not in seen else {
        return Err(TourHtmlError.Unsupported(f"two headings produce the anchor #{heading.id}"))
      }

      seen += [heading.id]
      headings += [heading]
      if heading.level == 2 {
        if in_chapter {
          chapters += ["</section>"]
        }

        chapters += ["<section class=\"chapter\">"]
        in_chapter = true
      }
    }

    let html = block_html(hl, block)?
    if in_chapter {
      chapters += [html]
    } else if lede and block is Para(_) {
      intro += [f"<div class=\"lede\">{html}</div>"]
      lede = false
    } else {
      intro += [html]
    }
  }

  if in_chapter {
    chapters += ["</section>"]
  }

  let name = plain_text(title.markup)
  let parts = [
    "<header class=\"topbar\">",
    "<button class=\"icon-button toc-toggle\" type=\"button\" aria-label=\"Contents\" aria-expanded=\"false\" aria-controls=\"toc\"><span aria-hidden=\"true\">☰</span></button>",
    "<a class=\"brand\" href=\"#top\">xsh<span>tour</span></a>",
    "<button class=\"icon-button theme-toggle\" type=\"button\" aria-label=\"Switch color theme\" hidden><span aria-hidden=\"true\"></span></button>",
    "</header>",
    "<div class=\"layout\">",
    "<nav class=\"toc\" id=\"toc\" aria-label=\"Contents\">",
    toc_html(headings),
    "</nav>",
    "<main id=\"top\">",
    f"<header class=\"hero\"><h1>{inline(title.markup)}</h1></header>",
    intro.join("\n"),
    chapters.join("\n"),
    "<footer class=\"colophon\"><p>That is the tour. The <a href=\"SPEC.md\">language specification</a> is the contract, and <code>xsht api</code> answers signature questions from the terminal.</p></footer>",
    "</main>",
    "</div>",
  ]
  f"<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"color-scheme\" content=\"light dark\">\n<title>{esc(name)}</title>\n<script>\n{theme_script}\n</script>\n<style>\n{stylesheet}\n</style>\n</head>\n<body>\n{parts.join("\n")}\n<script>\n{page_script}\n</script>\n</body>\n</html>\n"
}

## Renders the tour Markdown as a complete HTML document, coloring XSH code
## with `xsht highlight`.
export proc render(markdown: Str, xsht: Path) [fs, process, error] -> Result[Str, Error] {
  let blocks = parse_blocks(markdown.lines())?
  let scratch = fs.tempdir()?
  defer scratch.close()
  page_html(Highlighter(xsht:, scratch: scratch.host_path()?), blocks)?
}
