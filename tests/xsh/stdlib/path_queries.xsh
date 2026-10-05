test test_path_starts_with_and_ends_with_compare_whole_components {
  let library = /usr/lib/libz.so
  assert library.starts_with(/usr)
  assert library.starts_with(/usr/lib)
  assert library.starts_with(library)
  assert ! library.starts_with(/us)
  assert ! library.starts_with(p"usr")
  assert library.display().starts_with("/us")
  assert library.ends_with(p"libz.so")
  assert library.ends_with(p"lib/libz.so")
  assert library.ends_with(library)
  assert ! library.ends_with(p"z.so")
  assert ! library.ends_with(/lib/libz.so)
  assert ! library.ends_with(p"so")

  # A root prefix is the absolute-path test.
  assert library.starts_with(/)
  assert ! p"usr/lib".starts_with(/)
  assert ! p"".starts_with(/)

  # Components are read as strip_prefix reads them.
  assert p"lib//x/".starts_with(p"lib/")
  assert p"lib".starts_with(p"lib")
  assert p"lib/./x".ends_with(p"lib/x")
  assert ! p"./lib/x".starts_with(p"lib")
  assert p"a/../b".starts_with(p"a/..")
  assert agrees_with_strip_prefix(/a/b, /a)
  assert agrees_with_strip_prefix(/a/b, /b)
  assert agrees_with_strip_prefix(p"a", p"a/b")
  assert agrees_with_strip_prefix(p"a/b", p"a/b")
  assert agrees_with_strip_prefix(p"a//b/", p"a/")
}

pure agrees_with_strip_prefix(whole: Path, prefix: Path) -> Bool {
  let stripped = whole.strip_prefix(prefix) is Ok(_)
  whole.starts_with(prefix) == stripped
}

test test_path_component_queries_keep_native_bytes {
  # Display text would merge these two names into one replacement character.
  let raw = Path.parse_bytes(b"dir/bad\xffname")?
  assert raw.ends_with(Path.parse_bytes(b"bad\xffname")?)
  assert ! raw.ends_with(Path.parse_bytes(b"bad\xfename")?)
  assert raw.starts_with(p"dir")
}

test test_path_component_queries_on_a_dynamic_receiver {
  let library: Any = /usr/lib/libz.so
  assert library.starts_with(/usr).require(Bool)?
  assert ! library.starts_with(/us).require(Bool)?
  assert library.ends_with(p"lib/libz.so").require(Bool)?
  assert ! library.ends_with(p"z.so").require(Bool)?
  let text: Any = "/usr/lib"
  assert text.starts_with("/us").require(Bool)?
}

test test_path_starts_with_requires_a_path_argument { |ctx|
  let checked = test.run_script(
    ctx,
    r"""
let text = "usr"
print (p"usr/lib".starts_with(text))
""",
  )?
  assert ! checked.success
  assert "check.type-mismatch" in checked.stderr
}

test test_path_text_query_lint_rewrites_only_the_root_prefix_test { |ctx|
  let source = r"""proc show(target: Path) {
  let rooted = target.display().starts_with("/")
  let library = target.display().starts_with("lib/")
  let object = target.display().ends_with(".o")
  print $rooted $library $object
}

show(p"/lib/a.o")
show(p"lib/a.o")
show(p"lib")
show(p"")
"""
  let candidate = test.temp_file(ctx, name: "path-text-query.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.path-text-query --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?

  # Text prefixes longer than the root and text suffixes are not component
  # tests (`lib/` is no text prefix of `lib`), so they keep `.display()`.
  assert fixed == r"""proc show(target: Path) {
  let rooted = target.starts_with(p"/")
  let library = target.display().starts_with("lib/")
  let object = target.display().ends_with(".o")
  print $rooted $library $object
}

show(p"/lib/a.o")
show(p"lib/a.o")
show(p"lib")
show(p"")
"""
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = after
  assert succeeded, failure_details
  assert after.stdout == before.stdout
  assert after.stdout == """true false true
false true true
false false false
false false false
"""
  let repeated = run.capture --text "xsht" lint --only lint.path-text-query $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.path-text-query" not in repeated.stderr
}

test test_path_components_split_as_the_component_queries_read {
  assert p"/usr/lib/libz.so".components() == [/, p"usr", p"lib", p"libz.so"]
  assert p"usr/lib".components() == [p"usr", p"lib"]
  assert p"/".components() == [/]
  assert p"".components() == []

  # Repeated and trailing separators and an inner `.` are not components.
  assert p"/usr//lib/".components() == [/, p"usr", p"lib"]
  assert p"a/./b".components() == [p"a", p"b"]
  assert p"./a/../b".components() == [p".", p"a", p"..", p"b"]

  # Text splitting answers a different question.
  assert p"/usr//lib/".display().split("/") == ["", "usr", "", "lib", ""]

  # A leading run of the components is exactly a component prefix.
  let whole = /srv/data/set/a.bin
  let parts = whole.components()
  assert whole.starts_with(fp"{parts[0]}{parts[1]}/{parts[2]}")
  assert parts[parts.len() - 1].display() == whole.name()

  # Each component keeps its native bytes.
  let raw = Path.parse_bytes(b"dir/bad\xffname")?
  assert raw.components() == [p"dir", Path.parse_bytes(b"bad\xffname")?]
  assert raw.components()[1] != Path.parse_bytes(b"bad\xfename")?
}
