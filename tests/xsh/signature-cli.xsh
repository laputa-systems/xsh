test test_signature_cli_binds_typed_positionals_options_and_rest { |ctx|
  let result = test.run_script(
    ctx,
    r"""
cli main(zlabel: Str, count: Int, jobs: Int = 4, verbose: Bool = false, tags: List[Str] = ["base"], ...files: List[Path]) [error] {
  let names = [file.display() for file in files].join(",")
  print $zlabel $count $jobs $verbose ${[f"{tag}" for tag in tags].join(",")} $names
}
""",
    [
      "source",
      "3",
      "--jobs=8",
      "--verbose",
      "--tags",
      "one",
      "--tags=two",
      "first",
      "--",
      "-last",
    ],
    {},
    b"",
    "signature-bindings.xsh",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """source 3 8 true base,one,two first,-last
"""
}

test test_signature_cli_help_and_invalid_arguments_skip_initializers_and_body { |ctx|
  let source = """
##! A typed signature entry.
proc mark() [] -> Int { print "INITIALIZER-MARKER"; 1 }
let initialized = mark()
cli main(root: Path, jobs: Int = 4) [] { print "BODY-MARKER" }
"""
  let help = test.run_script(ctx, source, ["--help"], {}, b"", "signature-help.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = help
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "usage: signature-help.xsh-" in help.stdout
    let assertion_message = help.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "--jobs" in help.stdout
    let assertion_message = help.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "MARKER" not in help.stdout
    let assertion_message = help.stdout
    assert assertion_condition, assertion_message
  }
  for arguments in [[], ["root", "--unknown"], ["root", "--jobs", "invalid"], ["root", "--jobs=2", "--jobs=3"]] {
    let rejected = test.run_script(ctx, source, arguments, {}, b"", "signature-invalid.xsh")?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert rejected.stdout == ""
    {
      let assertion_condition = "usage:" in rejected.stderr
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_signature_cli_rejects_invalid_declaration_shapes { |ctx|
  for source in [
    """cli other() [] {}
""",
    """cli main() [] {}
cli main() [] {}
""",
    """cli main() [] {}
proc main() [] {}
""",
    """cli main() [] {}
main()
""",
    """export cli main() [] {}
""",
    """proc outer() [] { cli main() [] {} }
""",
    """cli main(help: Bool = false) [] {}
""",
    """cli main(rows: Record) [] {}
""",
    """proc default_jobs() [] -> Int { print "DEFAULT-MARKER"; 4 }
cli main(jobs: Int = default_jobs()) [] {}
""",
  ] {
    let result = test.run_script(ctx, source, ["--help"], {}, b"", "signature-rejected.xsh")?
    {
      let assertion_condition = ! result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    {
      let assertion_condition = "MARKER" not in result.stdout
      let assertion_message = result.stdout
      assert assertion_condition, assertion_message
    }
  }
}

test test_signature_cli_prepared_defaults_alias_parsers_and_kebab_options { |ctx|
  let source = r"""
type Count = UInt
type Tags = List[Int]
const DEFAULT_COUNT = 2 + 2
const DEFAULT_TAGS: Tags = [1, 2]
## Parse typed options without executing defaults.
cli main(root: Path, worker_count: Count = DEFAULT_COUNT, tags: Tags = DEFAULT_TAGS, verbose: Bool = true, delay: Duration = 20ms) [] {
  print ${root} $worker_count ${[f"{tag}" for tag in tags].join(",")} $verbose $delay
}
"""
  let defaults = test.run_script(ctx, source, ["nonexistent"], {}, b"", "signature-defaults.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = defaults
    assert assertion_condition, assertion_message
  }
  assert defaults.stdout == """nonexistent 4 1,2 true 20ms
"""
  let supplied = test.run_script(
    ctx,
    source,
    ["nonexistent", "--worker-count=8", "--tags=3", "--verbose=false", "--delay=30ms"],
    {},
    b"",
    "signature-aliases.xsh",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = supplied
    assert assertion_condition, assertion_message
  }
  assert supplied.stdout == """nonexistent 8 1,2,3 false 30ms
"""
  let negative = test.run_script(ctx, source, ["nonexistent", "--worker-count=-1"], {}, b"", "signature-unsigned.xsh")?
  {
    let assertion_condition = ! negative.success
    let assertion_message = negative.stderr
    assert assertion_condition, assertion_message
  }
  assert negative.stdout == ""
  let help = test.run_script(ctx, source, ["-h"], {}, b"", "signature-doc.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = help
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "Parse typed options" in help.stdout
    let assertion_message = help.stdout
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "UInt, default: 4" in help.stdout
    let assertion_message = help.stdout
    assert assertion_condition, assertion_message
  }
}

test test_signature_cli_rest_is_ordered_after_option_termination { |ctx|
  let source = r"""
cli main(...operands: List[Str]) [] { print ${operands.join(",")} }
"""
  let result = test.run_script(ctx, source, ["--", "--help", "-h", "last"], {}, b"", "signature-rest.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """--help,-h,last
"""
  let empty = test.run_script(ctx, source, [], {}, b"", "signature-empty-rest.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = empty
    assert assertion_condition, assertion_message
  }
  assert empty.stdout == "\n"
}

test test_signature_cli_entry_scripts_lint_together_in_a_project { |ctx|
  let root = test.temp_dir(ctx, name: "cli-project")?
  fp"{root}/xsht-config.ini".write("module_path = lib\n")
  fp"{root}/lib".mkdir()
  fp"{root}/bin".mkdir()
  fp"{root}/lib/greet.xsh".write(r"""##! Greetings.

## Builds a greeting.
export pure greeting(name: Str) -> Str {
  "hello " + name
}
""")
  for name in ["tool", "other"] {
    fp"{root}/bin/{name}.xsh".write(r"""use greet

cli main(name: Str = "world") [io] {
  print greet.greeting(name)
}
""")
  }

  let linted = cd (root) {
    run.capture --text "xsht" lint
  }?
  assert linted.status.exited_with(0), linted.stderr
  assert "check.cli-entry" not in linted.stderr, linted.stderr
}

# Argument parsing is a preflight: `--help` and rejected arguments end the run
# before any module-level initializer, of the entry script or of a module it
# imports, has had an effect.
test test_signature_cli_preflight_precedes_imported_and_entry_initializers { |ctx|
  let root = test.temp_dir(ctx, name: "signature-preflight")?
  let imported_marker = fp"{root}/imported-marker"
  let entry_marker = fp"{root}/entry-marker"
  fp"{root}/marker.xsh".write(
    r"""##! Initializer marker module.
## An unsigned worker count.
export type WorkerCount = UInt
proc initialize() [fs, error] -> Int { fs.write(p"IMPORTED_MARKER", "ran")?; 1 }
let initialized_marker = initialize()
## A callable exported value.
export pure value() -> Int { initialized_marker }
""".replace("IMPORTED_MARKER", with: imported_marker.display()),
  )
  let script = fp"{root}/entry.xsh"
  script.write(
    r"""##! A checked signature CLI.
use marker
proc initialize() [fs, error] -> Int { fs.write(p"ENTRY_MARKER", "ran")?; 1 }
let initialized = initialize()
cli main(root: Path, jobs: marker.WorkerCount = 4) [error] { print ${marker.value()} $initialized $jobs }
""".replace("ENTRY_MARKER", with: entry_marker.display()),
  )
  for {arguments, status} in [
    {arguments: ["--help"], status: 0},
    {arguments: [], status: 2},
    {arguments: ["operand", "--jobs=nope"], status: 2},
    {arguments: ["operand", "--jobs=-1"], status: 2},
  ] {
    let output = run.capture --text ${ctx.xsh_bin} $script @arguments
    assert output.status.exited_with(status), output.stderr
    assert ! imported_marker.exists()?
    assert ! entry_marker.exists()?
    let usage = if status == 0 { output.stdout } else { output.stderr }
    assert "usage:" in usage, usage
  }

  let output = run.capture --text ${ctx.xsh_bin} $script missing-path-is-allowed --jobs=8
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "1 1 8\n"
  assert imported_marker.exists()?
  assert entry_marker.exists()?
}

test test_signature_cli_preserves_entry_exit_status_and_errors { |ctx|
  for {body, status} in [{body: "exit 7", status: 7}, {body: "error.fail(\"entry failed\")?", status: 3}] {
    let output = test.run_script(ctx, f"cli main() [error] {{ {body} }}\n")?
    assert output.status == status, output.stderr
  }
}

# The accepted entry needs a `root` argument, so only the checker reads it.
test test_signature_cli_checks_body_without_registering_a_callable { |ctx|
  let file = test.temp_file(
    ctx,
    name: "entry.xsh",
    contents: bytes.from_text(
  r"""type Root = Path
cli main(root: Root, jobs: Int = 4, ...paths: List[Path]) [error] { guard jobs > 0 else { return error.fail("positive") }; print ${root.display()} ${paths.len()} }
""",
),
  )?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  assert "[check." not in checked.stderr, checked.stderr

  let _ = test.expect(
    ctx,
    "cli main() [] {}\nlet callable = main\nmain()\n",
    status: 2,
    stderr: ["[check.unresolved-name]"],
  )?
  let _ = test.expect(
    ctx,
    "cli main() [] { fs.read_text(p\"file\")? }\n",
    status: 2,
    stderr: ["[check.effect-violation]"],
  )?
}

test test_signature_cli_rejects_imported_entries_and_unprepared_defaults { |ctx|
  let root = test.temp_dir(ctx, name: "imported-entry")?
  fp"{root}/entry.xsh".write("##! Imported entry.\ncli main() [] {}\n")
  fp"{root}/main.xsh".write("use entry\n")
  let imported = run.capture --text "xsht" check fp"{root}/main.xsh"
  assert imported.status.exited_with(2), imported.stderr
  assert "[check.cli-entry]" in imported.stderr, imported.stderr

  for source in [
    "let jobs = 4\ncli main(jobs: Int = jobs) [] {}\n",
    "cli main(verbose: Bool = false, root: Path) [] {}\n",
    "type Count = UInt\ncli main(count: Count = -1) [] {}\n",
  ] {
    let _ = test.expect(ctx, source, status: 2, stderr: ["[check.cli-entry]"])?
  }
}
