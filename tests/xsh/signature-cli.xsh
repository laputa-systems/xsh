test test_signature_cli_binds_typed_positionals_options_and_rest { |ctx|
  let result = test.run_script(ctx, r"""
cli main(zlabel: Str, count: Int, jobs: Int = 4, verbose: Bool = false, tags: List[Str] = ["base"], ...files: List[Path]) [error] {
  let names = [file.display() for file in files].join(",")
  print $zlabel $count $jobs $verbose ${[f"$tag" for tag in tags].join(",")} $names
}
""", ["source", "3", "--jobs=8", "--verbose", "--tags", "one", "--tags=two", "first", "--", "-last"], {}, b"", "signature-bindings.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert (result.stdout) == ("source 3 8 true base,one,two first,-last\n")
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
      let assertion_condition = !rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert (rejected.stdout) == ("")
    {
      let assertion_condition = "usage:" in rejected.stderr
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_signature_cli_rejects_invalid_declaration_shapes { |ctx|
  for source in [
    "cli other() [] {}\n",
    "cli main() [] {}\ncli main() [] {}\n",
    "cli main() [] {}\nproc main() [] {}\n",
    "cli main() [] {}\nmain()\n",
    "export cli main() [] {}\n",
    "proc outer() [] { cli main() [] {} }\n",
    "cli main(help: Bool = false) [] {}\n",
    "cli main(rows: Record) [] {}\n",
    "proc default_jobs() [] -> Int { print \"DEFAULT-MARKER\"; 4 }\ncli main(jobs: Int = default_jobs()) [] {}\n",
  ] {
    let result = test.run_script(ctx, source, ["--help"], {}, b"", "signature-rejected.xsh")?
    {
      let assertion_condition = !result.success
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
  print ${root.display()} $worker_count ${[f"$tag" for tag in tags].join(",")} $verbose $delay
}
"""
  let defaults = test.run_script(ctx, source, ["nonexistent"], {}, b"", "signature-defaults.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = defaults
    assert assertion_condition, assertion_message
  }
  assert (defaults.stdout) == ("nonexistent 4 1,2 true 20ms\n")
  let supplied = test.run_script(ctx, source, ["nonexistent", "--worker-count=8", "--tags=3", "--verbose=false", "--delay=30ms"], {}, b"", "signature-aliases.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = supplied
    assert assertion_condition, assertion_message
  }
  assert (supplied.stdout) == ("nonexistent 8 1,2,3 false 30ms\n")
  let negative = test.run_script(ctx, source, ["nonexistent", "--worker-count=-1"], {}, b"", "signature-unsigned.xsh")?
  {
    let assertion_condition = !negative.success
    let assertion_message = negative.stderr
    assert assertion_condition, assertion_message
  }
  assert (negative.stdout) == ("")
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
  assert (result.stdout) == ("--help,-h,last\n")
  let empty = test.run_script(ctx, source, [], {}, b"", "signature-empty-rest.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = empty
    assert assertion_condition, assertion_message
  }
  assert (empty.stdout) == ("\n")
}
