test test_removed_compatibility_vocabulary_prevents_execution { |ctx|
  for source in [
    "print \"unreachable\"\nlet old = ARGV\n",
    "print \"unreachable\"\nlet old = \"é\".count_bytes()\n",
    "print \"unreachable\"\nlet old = fs.ls(p\".\")?\n",
    "print \"unreachable\"\nrun.builtin printf \"old\\n\" ?\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, source)?
    test.eq(output.stdout, "")?
    test.contains(output.stderr, "compatibility-vocabulary")?
  }
}

test test_canonical_compatibility_vocabulary_keeps_byte_and_child_contracts { |ctx|
  test.eq("é🍃".byte_len(), 6)?
  test.eq("é🍃".count_chars(), 2)?
  let root = test.temp_dir(ctx, name: "canonical-children")?
  fs.mkdir(fp"${root}/nested")?
  fs.write(fp"${root}/z.txt", "z")?
  fs.write(fp"${root}/a.txt", "a")?
  fs.write(fp"${root}/nested/child.txt", "child")?
  test.eq(fs.children(root)? |> map .name, ["a.txt", "nested", "z.txt"])?
  let absent = test.temp_path(ctx, name: "canonical-missing")
  test.ok(fs.children(absent) is Err(_))?
}

test test_compatibility_vocabulary_migration_preserves_comments_and_rechecks { |ctx|
  let root = test.temp_dir(ctx, name: "vocabulary-migration")?
  fs.write(fp"${root}/entry", "data")?
  let source = f"""# café ARGV fs.ls run.builtin count_bytes
let input = ARGV
let byte_count = "é🍃".count_bytes() # keep bytes
let children = fs.ls(p"${root.display()}", stat: false, ordered: true)? |> map .name
let capture = run.builtin.capture --text printf "%s" "external ARGV run.builtin fs.ls count_bytes" ?
let input_count = input.len()
let child_count = children.len()
print \$input_count \$byte_count \$child_count
print \$capture.stdout
"""
  let candidate = test.temp_file(ctx, name: "compatibility-migration.xsh", contents: bytes.from_text(source))?
  let diagnosed = run.capture --text "xsht" lint $candidate ?
  test.contains(diagnosed.stderr, "lint.compatibility-vocabulary", diagnosed.stderr)?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "# café ARGV fs.ls run.builtin count_bytes")?
  test.contains(fixed, "# keep bytes")?
  test.contains(fixed, "external ARGV run.builtin fs.ls count_bytes")?
  test.contains(fixed, "let input = args")?
  test.contains(fixed, "\"é🍃\".byte_len()")?
  test.contains(fixed, "fs.children(")?
  test.contains(fixed, "run.capture --text printf")?
  let output = test.run_script(ctx, fixed)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "0 6 1\nexternal ARGV run.builtin fs.ls count_bytes\n")?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(candidate.read_text()?, fixed)?
}

test test_compatibility_vocabulary_migration_preserves_shorthand_wire_keys { |ctx|
  let source = r"""pure count(ARGV: List[Str]) -> Int { ARGV.len() }
let record_value = {ARGV}
print ${record_value.ARGV.len()}
print $ARGV.len()
print ${count(ARGV:)}
"""
  let candidate = test.temp_file(ctx, name: "argv-shorthand-migration.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "{ARGV: args}")?
  test.contains(fixed, "print $args.len()")?
  let output = test.run_script(ctx, fixed)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "0\n0\n0\n")?
}

test test_compatibility_vocabulary_keeps_user_names_and_refuses_shadowed_targets { |ctx|
  let source = "let ARGV = [\"local\"]\nlet object = {count_bytes: 7}\nprint \${ARGV[0]} \${object.count_bytes}\nrun printf \"%s\\n\" ARGV run.builtin fs.ls count_bytes ?\n"
  let output = test.run_script(ctx, source)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "ARGV\nrun.builtin\nfs.ls\ncount_bytes\nlocal 7\n")?
  for rejected in [
    "proc inspect(args: List[Str]) [] { let old = ARGV }\ninspect([])\n",
    "let old = \"é\".count_bytes()\nlet unrelated = missing\n",
    "let old = fs.ls(1)?\n",
    "let old = fs.ls\n",
    "let old = \"é\".count_bytes(1)\n",
  ] {
    let candidate = test.temp_file(ctx, name: "compatibility-no-fix.xsh", contents: bytes.from_text(rejected))?
    let refused = run.capture --text "xsht" lint --fix $candidate ?
    test.ok(! refused.status.exited_with(0), refused.stderr)?
    test.eq(candidate.read_text()?, rejected)?
  }
}

test test_run_qualifier_migration_keeps_each_result_mode { |ctx|
  for source in [
    "run.builtin printf \"plain\\n\" ?\n",
    "let status = run.builtin.status false\nprint \${status.ok}\n",
    "let value = run.builtin.text printf \"text\\n\" ?\nprint $value\n",
    "let value = run.builtin.bytes printf \"bytes\" ?\nprint \${value.len()}\n",
    "let value = run.builtin.capture --text printf \"capture\" ?\nprint \${value.stdout}\n",
    "let value = run.builtin.capture --bytes printf \"capture\" ?\nprint \${value.stdout.len()}\n",
    "let output = run.builtin.stream --text printf \"stream\\n\" ?\nfor line in output { print $line }\n",
    "let output = run.builtin.stream --bytes printf \"stream\\n\" ?\nfor line in output { print \${line.len()} }\n",
  ] {
    let candidate = test.temp_file(ctx, name: "run-qualifier-migration.xsh", contents: bytes.from_text(source))?
    let applied = run.capture --text "xsht" lint --fix $candidate ?
    test.ok(applied.status.exited_with(0), applied.stderr)?
    let fixed = candidate.read_text()?
    test.eq(fixed, source.replace("run.builtin", "run"))?
    let actual = test.run_script(ctx, fixed)?
    let expected = test.run_script(ctx, source.replace("run.builtin", "run"))?
    test.ok(actual.success, actual.stderr)?
    test.eq(actual.stdout, expected.stdout)?
    test.eq(actual.status, expected.status)?
  }
}

test test_compatibility_vocabulary_public_inventory_is_canonical { |ctx|
  let removed = run.capture --text "xsht" api --format jsonl --strict api:fs.ls method:Str.count_bytes ?
  test.ok(! removed.status.exited_with(0), removed.stdout)?
  test.contains(removed.stdout, "\"status\":\"missing\"")?
  let canonical = run.capture --text "xsht" api --format jsonl --strict api:fs.children method:Str.byte_len ?
  test.ok(canonical.status.exited_with(0), canonical.stderr)?
  test.contains(canonical.stdout, "\"status\":\"exact\"")?
}

test test_compatibility_vocabulary_migration_keeps_imported_user_methods { |ctx|
  let root = test.temp_dir(ctx, name: "compatibility-import")?
  let library = fp"${root}/custom.xsh"
  library.write("##! Custom fixture.\n## Returns a user-defined count.\nexport pure count_bytes() -> Int { 7 }\n")?
  let source = "use custom\nprint \${custom.count_bytes()}\nlet value: Str? = \"é\"\nlet byte_count = value?.count_bytes()\nprint \${byte_count == 2}\n"
  let candidate = fp"${root}/entry.xsh"
  candidate.write(source)?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "custom.count_bytes()")?
  test.contains(fixed, "value?.byte_len()")?
  let output = run.capture --text "xsh" $candidate ?
  test.ok(output.status.exited_with(0), output.stderr)?
  test.eq(output.stdout, "7\ntrue\n")?
}

test test_compatibility_vocabulary_keeps_environment_names_and_serialized_keys { |ctx|
  let output = test.run_script(ctx, r"""run ARGV="kept" printenv ARGV ?
let object = {ARGV: "wire", count_bytes: 7}
print $object.ARGV
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "kept\nwire\n")?
}

test test_compatibility_vocabulary_trace_has_no_removed_dispatch_names { |ctx|
  let traced = test.run_xsht_trace(ctx, r"""let byte_count = "é".byte_len()
let children = fs.children(p".")?
let child_count = children |> count()
print $byte_count $child_count
""", ["--trace", "--raw", "--trace-format", "jsonl"])?
  test.ok(traced.success, traced.stderr)?
  test.contains(traced.stderr, "stream.count")?
  test.contains(traced.stderr, "core.print")?
  test.ok(! traced.stderr.contains("module.fs.ls"))?
  test.ok(! traced.stderr.contains("method.Str.count_bytes"))?
}

test test_compatibility_vocabulary_migration_rechecks_inferred_pures_and_pipeline_receivers { |ctx|
  let source = r"""pure byte_count(value: Str) { value.count_bytes() }
let sizes = ["a", "é"] |> map .count_bytes()
let captured = run.builtin.text printf "capture" ?
let second_size = sizes[1]
let direct_size = byte_count("é")
print $direct_size $second_size $captured
"""
  let candidate = test.temp_file(ctx, name: "compatibility-inference-migration.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.contains(fixed, "value.byte_len()")?
  test.contains(fixed, "map .byte_len()")?
  test.contains(fixed, "run.text printf")?
  let output = test.run_script(ctx, fixed)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "2 2 capture\n")?
}

test test_checked_compatibility_migration_preserves_literal_bytes_and_line_endings { |ctx|
  let source = "# café ARGV count_bytes\r\nlet width = \"é🍃\".count_bytes() # keep\r\nprint $width\r\n"
  let candidate = test.temp_file(ctx, name: "checked-vocabulary-layout.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.eq(fixed, source.replace(".count_bytes()", ".byte_len()"))?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(candidate.read_text()?, fixed)?
  let output = test.run_script(ctx, fixed)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "6\n")?
}
