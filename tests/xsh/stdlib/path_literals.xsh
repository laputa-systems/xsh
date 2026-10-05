type Layout = {root: Path = "/srv", label: Str}

const install_prefix: Path = "/usr/local"
const search_roots: List[Path] = ["/a", "/b"]

pure leaf(target: Path) -> Str {
  target.name()
}

pure fixed_path() -> Path {
  "ret/urn"
}

pure under(target: Path = "/default/dir") -> Str {
  target.name()
}

pure kind(target: Path) -> Str {
  match target {
    "etc/passwd" => "accounts"
    "etc/hosts" | "etc/resolv.conf" => "network"
    else => "other"
  }
}

test test_string_literal_is_a_path_where_a_path_is_expected {
  let bound: Path = "a/b"
  assert bound == p"a/b"
  assert leaf("c/d") == "d"
  assert fixed_path() == p"ret/urn"
  assert under() == "dir"
  assert install_prefix == /usr/local
  assert search_roots == [/a, /b]

  let many: List[Path] = ["x/1", "x/2"]
  assert many[1].name() == "2"
  let maybe: Path? = "opt/ional"
  assert (maybe ?? "fall/back") == p"opt/ional"
  let absent: Path? = null
  assert (absent ?? "fall/back") == p"fall/back"

  let layout = Layout(label: "x")
  assert layout.root == /srv
  let explicit = Layout(root: "/opt", label: "y")
  assert explicit.root.name() == "opt"
}

test test_string_literal_reaches_standard_path_parameters_as_a_path { |ctx|
  let root = test.temp_dir(ctx, name: "path-literal-args")?
  let file = fp"{root}/file.txt"
  file.write("x")
  cd $root {
    assert p"file.txt".exists()?
    assert ! p"absent.txt".exists()?
    p"file.txt".copy(to: "copy.txt")
    assert p"copy.txt".read_text()? == "x"
    p"written.txt".write("text stays text")
    assert p"written.txt".read_text()? == "text stays text"
  }
  assert p"a/b".strip_prefix("a")? == p"b"
  assert p"a/b".relative_to("a") == p"b"
  assert p"a/b".starts_with("a")
  assert ! p"a/b".ends_with("a")
}

test test_string_literal_compares_and_matches_as_a_path {
  let target = p"etc/hosts"
  assert target == "etc/hosts"
  assert "etc/hosts" == target
  assert target != "etc/host"
  assert ! (target != "etc/hosts")
  assert target is "etc/hosts"
  assert kind(p"etc/passwd") == "accounts"
  assert kind(target) == "network"
  assert kind(p"etc/resolv.conf") == "network"
  assert kind(p"etc") == "other"

  # The literal is the bytes written, so a path that only displays the same
  # is a different path.
  let raw = b"bad\xffname" as Path
  assert raw.display() == "bad�name"
  assert raw != "bad�name"
}

test test_string_literal_is_a_member_and_a_key_among_paths {
  let seen = [p"etc/hosts", p"etc/passwd"]
  assert "etc/hosts" in seen
  assert "etc/shadow" not in seen

  var sizes: Map[Path, Int] = {"etc/hosts": 3}
  sizes["etc/passwd"] = 5
  assert sizes[p"etc/hosts"] == 3
  assert sizes["etc/passwd"] == 5
  assert "etc/hosts" in sizes
  assert "etc/shadow" not in sizes
  assert (sizes.get("etc/passwd") ?? 0) == 5
  assert sizes.keys() == [p"etc/hosts", p"etc/passwd"]

  # Membership in a Path stays text containment.
  assert "lib" in p"usr/lib/libz.so"
}

test test_only_a_literal_becomes_a_path { |ctx|
  for statement in [
    "let bound: Path = text",
    "let built: Path = f\"{text}/x\"",
    "let name = leaf(text)",
    "let same = target == text",
    "let member = text in [target]",
    "let nul: Path = \"a\\0b\"",
    "let digest = hash.sha256(\"image.bin\")",
    "let found = text in env.PATH",
  ] {
    let rejected = test.run_script(
      ctx,
      """pure leaf(target: Path) -> Str {
  target.name()
}

let text = "etc"
let target = p"etc"
""" + statement + "\n",
    )?
    assert rejected.status == 2, statement
    assert "check." in rejected.stderr, rejected.stderr
  }

  # Where nothing in particular is expected, a literal is text.
  let free = "a/b"
  assert free.count_chars() == 3
  let dynamic: Any = "a/b"
  assert dynamic is Str
}

test test_path_display_equality_lint_drops_the_conversion { |ctx|
  let source = r"""proc show(target: Path) {
  let repo = target.display() == "/repo/x"
  let other = "/repo/y" != target.display()
  let lossy = target.display() == "bad\u{fffd}name"
  print $repo $other $lossy
}

show(/repo/x)
show(/repo/y)
show(Path.parse_bytes(b"bad\xffname")?)
"""
  let candidate = test.temp_file(ctx, name: "path-display-equality.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.path-display-equality --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?

  # A replacement-character literal equals the display of a path with other
  # bytes, so that comparison keeps its conversion.
  assert fixed == r"""proc show(target: Path) {
  let repo = target == "/repo/x"
  let other = "/repo/y" != target
  let lossy = target.display() == "bad\u{fffd}name"
  print $repo $other $lossy
}

show(/repo/x)
show(/repo/y)
show(Path.parse_bytes(b"bad\xffname")?)
"""
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = after
  assert succeeded, failure_details
  assert after.stdout == before.stdout
  assert after.stdout == """true true false
false false false
false true true
"""
  let repeated = run.capture --text "xsht" lint --only lint.path-display-equality $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.path-display-equality" not in repeated.stderr
}
