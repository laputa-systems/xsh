# Coverage for the opt-in GNU `getopt_long` mode of `cli.applet`, `cli.parse`,
# and `cli.parse_full`, selected by a `gnu` record in the schema. Native
# implementation: `src/modules/cli/gnu.rs`. The default policies are covered by
# `cli.xsh` and must not change.
#
# Parsing results are read in-process. Diagnostics, exit statuses, and the
# environment inputs (`POSIXLY_CORRECT`, `XSH_EXECUTION_PHRASE`) need the
# outer usage boundary, so those cases run a nested script; `SCHEMA_SOURCE`
# repeats `GNU` for that.
const GNU = {
  gnu: {
    prog: "demo",
    status: 2,
    unsupported: {"--selinux": "SELinux is not available", "-Z": "contexts are not available"},
  },
  all: {form: "-a --all", default: false},
  almost_all: {form: "-A --almost-all", default: false},
  human: {form: "-h --human-readable", default: false},
  verbose: {form: "-v --verbose", default: false},
  oneline: {form: "-1", default: false, conflicts: ["long_format"]},
  long_format: {form: "-l", default: false, conflicts: ["oneline"]},
  time: {form: "-t --time", default: false},
  time_style: {form: "--time-style STYLE"},
  lines: {form: "-n --lines N", kind: "Int", default: 10, numeric: true},
  size: {form: "-s --size SIZE"},
  color: {form: "--color --colour[=WHEN]", default: "never", optional_default: "always"},
  extra: {form: "-x", optional_value: true, optional_default: "dflt"},
  ignore: {form: "-I --ignore PATTERN", repeated: true},
  help: {form: "--help", default: false, stop: true},
  version: {form: "--version", default: false, stop: true},
  paths: {form: "...FILE"},
}

const SCHEMA_SOURCE = """{
  gnu: {
    prog: "demo",
    status: 2,
    unsupported: {"--selinux": "SELinux is not available", "-Z": "contexts are not available"},
  },
  all: {form: "-a --all", default: false},
  almost_all: {form: "-A --almost-all", default: false},
  verbose: {form: "-v --verbose", default: false},
  lines: {form: "-n --lines N", kind: "Int", default: 10},
  help: {form: "--help", default: false, stop: true},
  paths: {form: "...FILE"},
}"""

pure dynamic_cli_schema(schema: Record) -> Record {
  schema
}

type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs a nested applet whose whole body is `cli.applet(argv, schema)` followed
# by printing the parsed record as JSON.
proc run_applet(ctx: TestContext, schema: Str, args: List[Str], environment: Record) [error] -> Result[Ran] {
  let output = test.run_script(
    ctx,
    "use cli\n\nproc main(...argv: List[Str]) [io, error] {\n  let o = cli.applet(argv, " + schema + ")?\n  print json.encode(o)?\n}\n",
    args,
    environment,
  )?
  Ok({status: output.status, stdout: output.stdout, stderr: output.stderr})
}

# A usage error: nothing on stdout, a one-message diagnostic on stderr, the
# schema's exit status, and no traceback.
proc assert_diagnostic(ctx: TestContext, args: List[Str], message: Str, status: Int) [error] {
  let ran = run_applet(ctx, SCHEMA_SOURCE, args, {XSH_EXECUTION_PHRASE: ""})?
  assert ran.stderr == f"demo: {message}\nTry 'demo --help' for more information.\n"
  assert ran.status == status
  assert ran.stdout == ""
}

pure failure_message(result: Result[Record]) -> Str {
  match result {
    Ok(_) => ""
    Err(failure) => failure.message
  }
}

test test_cli_gnu_bundles_short_options_and_attaches_values {
  let bundle = cli.applet(["-aAv"], GNU)?
  assert bundle.all and bundle.almost_all and bundle.verbose
  assert bundle.lines == 10 and bundle.paths == []
  assert ! bundle.human and ! bundle.help

  # `-h` is an ordinary option, not help.
  assert cli.applet(["-h"], GNU)?.human

  assert cli.applet(["-n5"], GNU)?.lines == 5
  assert cli.applet(["-n", "7"], GNU)?.lines == 7
  assert cli.applet(["--lines=3"], GNU)?.lines == 3
  assert cli.applet(["--lines", "4"], GNU)?.lines == 4

  # A value ends the bundle: the rest of the word is the value.
  let mixed = cli.applet(["-van3", "-sfoo"], GNU)?
  assert mixed.verbose and mixed.all and mixed.lines == 3 and mixed.size == "foo"

  # A required value is the next word even when it looks like an option.
  assert cli.applet(["-n", "-5"], GNU)?.lines == -5
  assert cli.applet(["-n-5"], GNU)?.lines == -5
  assert cli.applet(["--size", "--all"], GNU)?.size == "--all"
  assert cli.applet(["--size="], GNU)?.size == ""

  let terminated = cli.applet(["-v", "--", "-a", "--all", "-"], GNU)?
  assert terminated.verbose and ! terminated.all
  assert terminated.paths == ["-a", "--all", "-"]
  assert cli.applet(["-", "x"], GNU)?.paths == ["-", "x"]
}

test test_cli_gnu_optional_values_attach_only {
  assert cli.applet([], GNU)?.color == "never"
  assert cli.applet(["--color"], GNU)?.color == "always"
  assert cli.applet(["--color=auto"], GNU)?.color == "auto"
  assert cli.applet(["--colour=never"], GNU)?.color == "never"

  # The next word is an operand, never the value.
  let separate = cli.applet(["--color", "auto"], GNU)?
  assert separate.color == "always" and separate.paths == ["auto"]

  assert cli.applet(["-x"], GNU)?.extra == "dflt"
  assert cli.applet(["-xfoo"], GNU)?.extra == "foo"
  let after = cli.applet(["-vx", "foo"], GNU)?
  assert after.verbose and after.extra == "dflt" and after.paths == ["foo"]

  # A forced flag of another kind is a switch that may carry an attached value.
  let forced = dynamic_cli_schema({gnu: {}, mode: {form: "--mode", kind: "Str", flag: true}})
  assert cli.applet(["--mode"], forced)?.get("mode")? == true
  assert cli.applet(["--mode=fast"], forced)?.get("mode")? == "fast"
}

test test_cli_gnu_abbreviates_long_options {
  assert cli.applet(["--alm"], GNU)?.almost_all
  assert cli.applet(["--verb"], GNU)?.verbose
  assert cli.applet(["--lin=4"], GNU)?.lines == 4
  assert cli.applet(["--time-s=iso"], GNU)?.time_style == "iso"

  # An exact name beats a longer candidate that shares its prefix.
  let exact = cli.applet(["--time"], GNU)?
  assert exact.time and exact.time_style == null

  # Aliases of one option are not ambiguous with each other.
  assert cli.applet(["--col"], GNU)?.color == "always"
  assert cli.applet(["--colo=auto"], GNU)?.color == "auto"

  # Abbreviations only match as written: underscores are not hyphens.
  assert failure_message(cli.applet(["--almost_all"], dynamic_cli_schema(GNU))) != ""
}

test test_cli_gnu_repeated_options_are_legal {
  let all = cli.applet(["-a", "-a", "--all"], GNU)?
  assert all.all
  assert cli.applet(["-n1", "-n", "2", "--lines=3"], GNU)?.lines == 3
  assert cli.applet(["-s", "x", "-s", "y"], GNU)?.size == "y"
  assert cli.applet(["-I", "a", "--ignore=b", "-Ic"], GNU)?.ignore == ["a", "b", "c"]
  assert cli.applet([], GNU)?.ignore == []

  # Conflicting options reset one another: the last one given wins.
  let long_wins = cli.applet(["-1", "-l"], GNU)?
  assert long_wins.long_format and ! long_wins.oneline
  let one_wins = cli.applet(["-l", "-1"], GNU)?
  assert one_wins.oneline and ! one_wins.long_format
}

test test_cli_gnu_permutes_operands_by_default {
  let mixed = cli.applet(["a", "-v", "b", "--all", "c"], GNU)?
  assert mixed.paths == ["a", "b", "c"] and mixed.verbose and mixed.all
  let after_terminator = cli.applet(["a", "--", "-v", "b"], GNU)?
  assert after_terminator.paths == ["a", "-v", "b"] and ! after_terminator.verbose
}

test test_cli_gnu_permutation_ends_at_the_first_operand_when_disabled { |ctx|
  let small = """{gnu: {prog: "demo"}, verbose: {form: "-v --verbose", default: false}, paths: {form: "...FILE"}}"""
  let permuted = run_applet(ctx, small, ["a", "-v"], {})?
  assert permuted.status == 0
  assert permuted.stdout == """{"paths":["a"],"verbose":true}
"""

  let posix = run_applet(ctx, small, ["-v", "a", "-v", "--", "-v"], {POSIXLY_CORRECT: "1"})?
  assert posix.stdout == """{"paths":["a","-v","--","-v"],"verbose":true}
"""

  # The variable only needs to be defined.
  let empty = run_applet(ctx, small, ["a", "-v"], {POSIXLY_CORRECT: ""})?
  assert empty.stdout == """{"paths":["a","-v"],"verbose":false}
"""

  let stops = """{gnu: {prog: "demo", permute: false}, verbose: {form: "-v --verbose", default: false}, paths: {form: "...FILE"}}"""
  let plus = run_applet(ctx, stops, ["-v", "cmd", "-v", "--verbose"], {})?
  assert plus.stdout == """{"paths":["cmd","-v","--verbose"],"verbose":true}
"""
}

test test_cli_gnu_numeric_options_read_digit_runs {
  assert cli.applet(["-5"], GNU)?.lines == 5
  assert cli.applet(["-25"], GNU)?.lines == 25
  let bundled = cli.applet(["-37v"], GNU)?
  assert bundled.lines == 37 and bundled.verbose
  let inside = cli.applet(["-v5a"], GNU)?
  assert inside.lines == 5 and inside.verbose and inside.all

  # A declared short option for the digit wins over the numeric option.
  let one = cli.applet(["-1"], GNU)?
  assert one.oneline and one.lines == 10
}

test test_cli_gnu_stop_options_end_parsing_successfully {
  let help = cli.applet(["--help", "--bogus", "-q"], GNU)?
  assert help.help and ! help.version
  let version = cli.applet(["-v", "--vers", "--bogus", "operand"], GNU)?
  assert version.version and version.verbose and version.paths == []
  assert cli.applet(["-a", "--help"], GNU)?.all
}

test test_cli_gnu_diagnoses_like_getopt { |ctx|
  assert_diagnostic(ctx, ["-q"], "invalid option -- 'q'", 2)?
  assert_diagnostic(ctx, ["-aq"], "invalid option -- 'q'", 2)?
  assert_diagnostic(ctx, ["--foo"], "unrecognized option '--foo'", 2)?
  assert_diagnostic(ctx, ["--foo=bar"], "unrecognized option '--foo=bar'", 2)?
  assert_diagnostic(ctx, ["-n"], "option requires an argument -- 'n'", 2)?
  assert_diagnostic(ctx, ["-an"], "option requires an argument -- 'n'", 2)?
  assert_diagnostic(ctx, ["--lines"], "option '--lines' requires an argument", 2)?
  assert_diagnostic(ctx, ["--li"], "option '--lines' requires an argument", 2)?
  assert_diagnostic(ctx, ["--all=1"], "option '--all' doesn't allow an argument", 2)?
  assert_diagnostic(ctx, ["--al"], "option '--al' is ambiguous; possibilities: '--all' '--almost-all'", 2)?
  assert_diagnostic(ctx, ["--al=3"], "option '--al=3' is ambiguous; possibilities: '--all' '--almost-all'", 2)?
  assert_diagnostic(ctx, ["--lines=abc"], "invalid argument 'abc' for '--lines'", 2)?
  assert_diagnostic(ctx, ["-nabc"], "invalid argument 'abc' for '--lines'", 2)?

  # The first problem in command-line order is the one reported, and an
  # earlier stop option hides everything after it.
  assert_diagnostic(ctx, ["-q", "--bogus"], "invalid option -- 'q'", 2)?
  assert_diagnostic(ctx, ["--bogus", "-q"], "unrecognized option '--bogus'", 2)?
  assert_diagnostic(ctx, ["--lines=x", "--bogus"], "invalid argument 'x' for '--lines'", 2)?
  assert_diagnostic(ctx, ["-q", "--help"], "invalid option -- 'q'", 2)?
  let help = run_applet(ctx, SCHEMA_SOURCE, ["--help", "-q"], {})?
  assert help.status == 0 and help.stderr == ""
}

test test_cli_gnu_unsupported_options_fail_by_name { |ctx|
  assert_diagnostic(ctx, ["--selinux"], "option '--selinux' is not supported: SELinux is not available", 2)?
  assert_diagnostic(ctx, ["--selin"], "option '--selinux' is not supported: SELinux is not available", 2)?
  assert_diagnostic(ctx, ["--selinux=x"], "option '--selinux' is not supported: SELinux is not available", 2)?
  assert_diagnostic(ctx, ["-vZ"], "option '-Z' is not supported: contexts are not available", 2)?
  assert_diagnostic(ctx, ["-a", "--selinux", "--bogus"], "option '--selinux' is not supported: SELinux is not available", 2)?
}

test test_cli_gnu_reports_operand_errors_with_the_configured_status { |ctx|
  let one = """{gnu: {prog: "demo", status: 125}, help: {form: "--help", default: false, stop: true}, one: {form: "ONE"}}"""
  let extra = run_applet(ctx, one, ["a", "b"], {XSH_EXECUTION_PHRASE: ""})?
  assert extra.status == 125
  assert extra.stderr == """demo: extra operand 'b'
Try 'demo --help' for more information.
"""
  let missing = run_applet(ctx, one, [], {XSH_EXECUTION_PHRASE: ""})?
  assert missing.status == 125
  assert missing.stderr == """demo: missing operand
Try 'demo --help' for more information.
"""
  let stopped = run_applet(ctx, one, ["--help"], {})?
  assert stopped.status == 0 and stopped.stderr == ""
  assert stopped.stdout == """{"help":true}
"""
}

test test_cli_gnu_names_the_program_and_phrase { |ctx|
  # A multicall dispatcher supplies the phrase; the prefix stays the program.
  let phrase = run_applet(ctx, SCHEMA_SOURCE, ["-q"], {XSH_EXECUTION_PHRASE: "/opt/adapter ls"})?
  assert phrase.status == 2
  assert phrase.stderr == """demo: invalid option -- 'q'
Try '/opt/adapter ls --help' for more information.
"""
}

test test_cli_gnu_program_defaults_to_the_invoked_name { |ctx|
  # Aliases are symlinks and the kernel passes the symlink path, so the
  # default program is the name the script was invoked as, with the default
  # exit status 1.
  let root = test.temp_dir(ctx, name: "cli-gnu-invoked")?
  fp"{root}/real.xsh".write_atomic("""use cli

proc main(...argv: List[Str]) [io, error] {
  let _ = cli.applet(argv, {gnu: {}, verbose: {form: "-v", default: false}})?
}
""")?
  fs.symlink(fp"{root}/real.xsh", fp"{root}/dir")?
  let link = fp"{root}/dir"
  let output = run.capture --text --accept=[1] "xsh" $link "-z" ?
  assert output.status.exited_with(1)
  assert output.stderr == """dir: invalid option -- 'z'
Try 'dir --help' for more information.
"""
}

test test_cli_gnu_has_no_automatic_help { |ctx|
  let small = """{gnu: {prog: "demo"}, verbose: {form: "-v", default: false}}"""
  for flag in ["--help", "-h"] {
    let ran = run_applet(ctx, small, [flag], {XSH_EXECUTION_PHRASE: ""})?
    assert ran.status == 1
    assert ran.stdout == ""
    assert "usage" not in ran.stderr
  }
  let unrecognized = run_applet(ctx, small, ["--help"], {XSH_EXECUTION_PHRASE: ""})?
  assert unrecognized.stderr == """demo: unrecognized option '--help'
Try 'demo --help' for more information.
"""
}

test test_cli_gnu_is_available_to_parse_and_parse_full {
  let repeated = cli.parse(["-a", "-a", "--alm"], GNU)?
  assert repeated.all and repeated.almost_all
  assert "option '--al' is ambiguous" in failure_message(cli.parse(["--al"], GNU))
  let full = cli.parse_full(["-n5", "-n", "6", "x"], GNU)?
  assert full.values.lines == 6
  assert full.values.paths == ["x"]
  assert full.sources.get("lines")? == "argv"
  assert cli.parse(["-vn", "2"], GNU)?.lines == 2
}

test test_cli_gnu_prepared_and_dynamic_schemas_agree {
  let dynamic = dynamic_cli_schema(GNU)
  let args = ["-an5", "--al=x"]
  assert failure_message(cli.applet(args, dynamic)) == failure_message(cli.applet(args, GNU))
  assert "option '--al=x' is ambiguous" in failure_message(cli.applet(args, dynamic))

  let ok = ["-an5", "--alm", "f"]
  assert json.encode(cli.applet(ok, dynamic)?)? == json.encode(cli.applet(ok, GNU)?)?
}

test test_cli_gnu_rejects_malformed_configuration {
  assert "unknown `gnu` field `bogus`" in failure_message(cli.applet([], dynamic_cli_schema({gnu: {bogus: 1}})))
  assert "`gnu` field `status` has an invalid Int value" in failure_message(cli.applet([], dynamic_cli_schema({gnu: {status: 256}})))
  assert "`gnu` must be Record" in failure_message(cli.applet([], dynamic_cli_schema({gnu: true})))
  assert "must be `--long` or `-s`" in failure_message(cli.applet([], dynamic_cli_schema({gnu: {unsupported: {selinux: "x"}}})))
  assert "must be Str" in failure_message(cli.applet([], dynamic_cli_schema({gnu: {unsupported: {"--x": 1}}})))
}
