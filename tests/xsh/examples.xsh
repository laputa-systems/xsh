proc test_idiom_subcommand_dispatch() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-subcommand-dispatch.xsh ?
  test.contains(output, "quick")?
  test.contains(output, "verbose")?
  test.contains(output, "error: unknown: unknown")?
}

proc test_idiom_enumerate() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-enumerate.xsh ?
  test.contains(output, "[keep]")?
  test.contains(output, "[dup ]")?
  test.contains(output, "1: line one")?
  test.contains(output, "2: line two")?
}

proc test_idiom_flat_map() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-flat-map.xsh ?
  test.contains(output, "the")?
  test.contains(output, "quick")?
  test.contains(output, "brown")?
  test.contains(output, "fox")?
}

proc test_idiom_any_all() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-any-all.xsh ?
  test.contains(output, "true true false")?
}

proc test_idiom_sort_by_desc() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-sort-by-desc.xsh ?
  test.contains(output, "large.txt")?
  test.contains(output, "cherry")?
}

proc test_idiom_building_maps() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-building-maps.xsh ?
  test.contains(output, "apple 3")?
  test.contains(output, "banana 2")?
  test.contains(output, "cherry 1")?
  test.contains(output, "33")?
  test.contains(output, "apple=3")?
}

proc test_idiom_match_result_loop() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-match-result-loop.xsh ?

  test.eq(
    output,
    """6
""",
  )?
}

proc test_idiom_dry_run() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-dry-run.xsh ?
  test.contains(output, "would keep: file-a.txt")?
  test.contains(output, "would drop: _hidden.txt")?
  test.contains(output, "dry run")?
  test.contains(output, "keep: file-a.txt")?
  test.contains(output, "drop: _hidden.txt")?

  test.contains(
    output,
    """2 kept  1 dropped
""",
  )?
}

proc test_idiom_reading_data() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-reading-data.xsh ?
  test.contains(output, "all fields present")?
  test.contains(output, "demo v1.0")?
}

proc test_idiom_typed_cli() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-typed-cli.xsh ?
  test.contains(output, "proc true 20")?
}

proc test_idiom_temp_dir() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-temp-dir.xsh ?

  test.eq(
    output,
    """hello
""",
  )?
}

proc test_idiom_install_files() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-install-files.xsh ?
  test.contains(output, "true")?
}

proc test_idiom_gitroot() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-gitroot.xsh ?
  test.contains(output, "repo root: /")?
  test.contains(output, "true")?
}

proc test_idiom_cache() [process, error] {
  let output: Str = run.text "xsh" examples/idiom-cache.xsh ?
  test.contains(output, "root: /")?
  test.contains(output, "hello, world")?
  test.contains(output, "hello, xsh")?
}
