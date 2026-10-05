test test_removed_compatibility_vocabulary_prevents_execution { |ctx|
  for source in [
    """print "unreachable"
let old = ARGV
""",
    """print "unreachable"
let old = "é".count_bytes()
""",
    """print "unreachable"
let old = fs.ls(p".")?
""",
    """print "unreachable"
run.builtin printf "old\\n" ?
""",
  ] {
    let output = test.run_script(ctx, source)?
    let rejected = ! output.success
    let rejected_source = source
    assert rejected, rejected_source
    assert output.stdout == ""
    assert "compatibility-vocabulary" in output.stderr
  }
}

test test_canonical_compatibility_vocabulary_keeps_byte_and_child_contracts { |ctx|
  assert "é🍃".byte_len() == 6
  assert "é🍃".count_chars() == 2
  let root = test.temp_dir(ctx, name: "canonical-children")?
  fs.mkdir(fp"{root}/nested")
  fs.write(fp"{root}/z.txt", "z")
  fs.write(fp"{root}/a.txt", "a")
  fs.write(fp"{root}/nested/child.txt", "child")
  assert (fs.children(root)? |> map .name) == ["a.txt", "nested", "z.txt"]
  let absent = test.temp_path(ctx, name: "canonical-missing")
  assert fs.children(absent) is Err(_)
}

test test_compatibility_vocabulary_migration_preserves_comments_and_rechecks { |ctx|
  let root = test.temp_dir(ctx, name: "vocabulary-migration")?
  fs.write(fp"{root}/entry", "data")
  let source = f"""# café ARGV fs.ls run.builtin count_bytes
let input = ARGV
let byte_count = "é🍃".count_bytes() # keep bytes
let children = fs.ls(p"{root}", stat: false, ordered: true)? |> map .name
let capture = run.builtin.capture --text printf "%s" "external ARGV run.builtin fs.ls count_bytes" ?
let input_count = input.len()
let child_count = children.len()
print \$input_count \$byte_count \$child_count
print \$capture.stdout
"""
  let candidate = test.temp_file(ctx, name: "compatibility-migration.xsh", contents: bytes.from_text(source))?
  let diagnosed = run.capture --text "xsht" lint $candidate ?
  let diagnosed_migration = "lint.compatibility-vocabulary" in diagnosed.stderr
  let diagnosis_details = diagnosed.stderr
  assert diagnosed_migration, diagnosis_details
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert "# café ARGV fs.ls run.builtin count_bytes" in fixed
  assert "# keep bytes" in fixed
  assert "external ARGV run.builtin fs.ls count_bytes" in fixed
  assert "let input = args" in fixed
  assert "\"é🍃\".byte_len()" in fixed
  assert "fs.children(" in fixed
  assert "run.capture --text printf" in fixed
  let output = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """0 6 1
external ARGV run.builtin fs.ls count_bytes
"""
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  let repeated_succeeded = repeated.status.exited_with(0)
  let repeated_details = repeated.stderr
  assert repeated_succeeded, repeated_details
  assert candidate.read_text()? == fixed
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
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert "{ARGV: args}" in fixed
  assert r"print $args.len()" in fixed
  let output = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """0
0
0
"""
}

test test_compatibility_vocabulary_keeps_user_names_and_refuses_shadowed_targets { |ctx|
  let source = """let ARGV = ["local"]
let object = {count_bytes: 7}
print \${ARGV[0]} \${object.count_bytes}
run printf "%s\\n" ARGV run.builtin fs.ls count_bytes ?
"""
  let output = test.run_script(ctx, source)?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """local 7
ARGV
run.builtin
fs.ls
count_bytes
"""
  for rejected in [
    """proc inspect(args: List[Str]) [] { let old = ARGV }
inspect([])
""",
    """let old = "é".count_bytes()
let unrelated = missing
""",
    """let old = fs.ls(1)?
""",
    """let old = fs.ls
""",
    """let old = "é".count_bytes(1)
""",
  ] {
    let candidate = test.temp_file(ctx, name: "compatibility-no-fix.xsh", contents: bytes.from_text(rejected))?
    let refused = run.capture --text "xsht" lint --fix $candidate ?
    let declined = ! refused.status.exited_with(0)
    let refusal_details = refused.stderr
    assert declined, refusal_details
    assert candidate.read_text()? == rejected
  }
}

test test_run_qualifier_migration_keeps_each_result_mode { |ctx|
  for source in [
    """run.builtin printf "plain\\n" ?
""",
    """let status = run.builtin.status false
print \${status.ok}
""",
    """let value = run.builtin.text printf "text\\n" ?
print $value
""",
    """let value = run.builtin.bytes printf "bytes" ?
print \${value.len()}
""",
    """let value = run.builtin.capture --text printf "capture" ?
print \${value.stdout}
""",
    """let value = run.builtin.capture --bytes printf "capture" ?
print \${value.stdout.len()}
""",
    """let output = run.builtin.stream --text printf "stream\\n" ?
for line in output { print $line }
""",
    """let output = run.builtin.stream --bytes printf "stream\\n" ?
for line in output { print \${line.len()} }
""",
  ] {
    let candidate = test.temp_file(ctx, name: "run-qualifier-migration.xsh", contents: bytes.from_text(source))?
    let applied = run.capture --text "xsht" lint --fix $candidate ?
    let applied_succeeded = applied.status.exited_with(0)
    let applied_details = applied.stderr
    assert applied_succeeded, applied_details
    let fixed = candidate.read_text()?
    assert fixed == source.replace("run.builtin", "run")
    let actual = test.run_script(ctx, fixed)?
    let expected = test.run_script(ctx, source.replace("run.builtin", "run"))?
    let {success: succeeded, stderr: failure_details, ..} = actual
    assert succeeded, failure_details
    assert actual.stdout == expected.stdout
    assert actual.status == expected.status
  }
}

test test_compatibility_vocabulary_public_inventory_is_canonical {
  let removed = run.capture --text "xsht" api --format jsonl --strict api:fs.ls method:Str.count_bytes ?
  let removed_absent = ! removed.status.exited_with(0)
  let removed_details = removed.stdout
  assert removed_absent, removed_details
  assert "\"status\":\"missing\"" in removed.stdout
  let canonical = run.capture --text "xsht" api --format jsonl --strict api:fs.children method:Str.byte_len ?
  let canonical_succeeded = canonical.status.exited_with(0)
  let canonical_details = canonical.stderr
  assert canonical_succeeded, canonical_details
  assert "\"status\":\"exact\"" in canonical.stdout
}

test test_compatibility_vocabulary_migration_keeps_imported_user_methods { |ctx|
  let root = test.temp_dir(ctx, name: "compatibility-import")?
  let library = fp"{root}/custom.xsh"
  library.write("""##! Custom fixture.
## Returns a user-defined count.
export pure count_bytes() -> Int { 7 }
""")
  let source = """use custom
print \${custom.count_bytes()}
let value: Str? = "é"
let byte_count = value?.count_bytes()
print \${byte_count == 2}
"""
  let candidate = fp"{root}/entry.xsh"
  candidate.write(source)
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert "custom.count_bytes()" in fixed
  assert "value?.byte_len()" in fixed
  let output = run.capture --text "xsh" $candidate ?
  let succeeded = output.status.exited_with(0)
  let failure_details = output.stderr
  assert succeeded, failure_details
  assert output.stdout == """7
true
"""
}

test test_compatibility_vocabulary_keeps_environment_names_and_serialized_keys { |ctx|
  let output = test.run_script(
    ctx,
    r"""run ARGV="kept" printenv ARGV ?
let object = {ARGV: "wire", count_bytes: 7}
print $object.ARGV
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """kept
wire
"""
}

test test_compatibility_vocabulary_trace_has_no_removed_dispatch_names { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    r"""let byte_count = "é".byte_len()
let children = fs.children(p".")?
let child_count = children |> count()
print $byte_count $child_count
""",
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?
  let {success: succeeded, stderr: failure_details, ..} = traced
  assert succeeded, failure_details
  assert "stream.count" in traced.stderr
  assert "core.print" in traced.stderr
  assert "module.fs.ls" not in traced.stderr
  assert "method.Str.count_bytes" not in traced.stderr
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
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert "value.byte_len()" in fixed
  assert "map .byte_len()" in fixed
  assert "run.text printf" in fixed
  let output = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """2 2 capture
"""
}

test test_checked_compatibility_migration_preserves_literal_bytes_and_line_endings { |ctx|
  let source = """# café ARGV count_bytes\r
let width = "é🍃".count_bytes() # keep\r
print $width\r
"""
  let candidate = test.temp_file(ctx, name: "checked-vocabulary-layout.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?
  assert fixed == source.replace(".count_bytes()", ".byte_len()")
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  let repeated_succeeded = repeated.status.exited_with(0)
  let repeated_details = repeated.stderr
  assert repeated_succeeded, repeated_details
  assert candidate.read_text()? == fixed
  let output = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """6
"""
}
