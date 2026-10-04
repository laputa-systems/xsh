use tour_html

const tour = """# A Tour of `XSH`

First paragraph, the lede.

Second paragraph with `code`, a [link](SPEC.md), and a <tag> & ampersand.

## Values

```xsh
let name = "db-01"
print f"host {name}"
```

<!-- expected-output -->

```text
host db-01
```

| Form | Result |
|---|---|
| `run x` | `Status` |

- plain item
- item with code:

  ```bash
  rm $file  # gone
  ```

### Sub heading

## Config

```ini
# comment
[section]
key = value
```
"""

pure xsht(ctx: TestContext) -> Path {
  fp"{ctx.xsh_bin.parent()}/xsht"
}

proc render(ctx: TestContext, markdown: Str) [fs, process, error] -> Result[Str] {
  tour_html.render(markdown, xsht(ctx))
}

proc rejection(ctx: TestContext, markdown: Str) [fs, process, error] -> Str {
  match render(ctx, markdown) {
    Ok(_) => "rendered"
    Err(error) => error.message
  }
}

test test_tour_html_is_one_self_contained_page { |ctx|
  let html = render(ctx, tour)?
  assert html.starts_with("<!DOCTYPE html>"), "doctype"
  assert "<title>A Tour of XSH</title>" in html, "rendered page"
  assert "<style>" in html and "<script>" in html, "rendered page"
  assert "http://" not in html and "https://" not in html, "the page must not reach for the network"
  assert "src=" not in html, "the page must not load external resources"
  assert "<link " not in html, "the page must not load external stylesheets"
}

test test_tour_html_numbers_chapters_and_links_the_contents { |ctx|
  let html = render(ctx, tour)?
  assert """<li><a href="#values">Values</a>
<ol>
<li><a href="#sub-heading">Sub heading</a></li>
</ol>
</li>
<li><a href="#config">Config</a>
</li>""" in html, "contents list"
  assert """<h2 id="values"><a class="anchor" href="#values">Values</a></h2>""" in html, "rendered page"
  assert """<h3 id="sub-heading">""" in html, "rendered page"
  assert html.split("<section class=\"chapter\">").len() == 3, "two chapters"
}

test test_tour_html_renders_prose_markup_and_escapes_text { |ctx|
  let html = render(ctx, tour)?
  assert """<div class="lede"><p>First paragraph, the lede.</p></div>""" in html, "rendered page"
  assert """<code>code</code>, a <a href="SPEC.md">link</a>, and a &lt;tag&gt; &amp; ampersand.""" in html, "rendered page"
  assert "<th>Form</th><th>Result</th>" in html, "rendered page"
  assert "<td><code>run x</code></td><td><code>Status</code></td>" in html, "rendered page"
  assert "<li>plain item</li>" in html, "rendered page"
}

test test_tour_html_colors_xsh_with_the_language_lexer_and_attaches_output { |ctx|
  let html = render(ctx, tour)?
  assert """<span class="hl-keyword">let</span>""" in html, "rendered page"
  assert """<span class="hl-string">"db-01"</span>""" in html, "rendered page"
  assert """<span class="hl-interpolation">{</span>""" in html, "rendered page"
  assert """</figure>
<figure class="console">""" in html, "rendered page"
  assert "host db-01" in html, "rendered page"
}

test test_tour_html_colors_shell_and_ini_fences { |ctx|
  let html = render(ctx, tour)?
  assert """<span class="hl-function">rm</span> <span class="hl-variable">$file</span>  <span class="hl-comment"># gone</span>""" in html, "rendered page"
  assert """<span class="hl-comment"># comment</span>
<span class="hl-keyword">[section]</span>
<span class="hl-property">key</span><span class="hl-operator"> = </span>value""" in html, "rendered page"
}

test test_tour_html_refuses_markdown_it_cannot_render { |ctx|
  assert "unterminated code fence ```xsh" in rejection(ctx, "# T\n\n```xsh\nlet x = 1\n"), "an open fence must fail"
  assert "code fence language `rust` has no renderer" in rejection(ctx, "# T\n\n```rust\nfn main() {}\n```\n")
  assert "table row has 1 cells, header has 2" in rejection(ctx, "# T\n\n| a | b |\n|---|---|\n| only |\n")
  assert "table without a delimiter row" in rejection(ctx, "# T\n\n| a | b |\n")
  assert "two headings produce the anchor #same" in rejection(ctx, "# T\n\n## Same\n\n## same\n")
  assert "must start with a `#` title" in rejection(ctx, "## Not a title\n")
  assert "the tour is empty" in rejection(ctx, "")
}
