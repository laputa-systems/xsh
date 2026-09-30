test cli_commands_constants_preserve_known_shape [fs, error] { |ctx|
  let output = test.run_script(ctx, r"""
const commands = {
  build: {positionals: ["root"], types: {root: "Path"}, rest: "raw", options: {jobs: {kind: "Int", default: 4}, verbose: "Bool", tag: "List[Str]", label: {kind: "Str", required: false}}},
  clean: {positionals: ["root"], types: {root: "Path"}, rest: "raw", options: {jobs: {kind: "Int", default: 4}, verbose: "Bool", tag: "List[Str]", label: {kind: "Str", required: false}}},
}
let parsed = cli.commands(["build", "workspace", "extra", "--jobs", "6", "--tag", "one"], commands)?
let root: Path = parsed.root
let raw: List[Str] = parsed.raw
let jobs: Int = parsed.jobs
let flag: Bool = parsed.verbose
let tags: List[Str] = parsed.tag
let label: Str? = parsed.label
print root.display() ${raw[0]} $jobs $flag ${tags[0]}
print (label == null)
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "workspace extra 6 false one\ntrue\n")?
}

test cli_commands_constants_reject_unreachable_invalid_descriptor [error] { |ctx|
  let output = test.run_script(ctx, r"""
const commands = {build: {positionals: ["root"], types: {root: "NotAType"}}}
if false { let _ = cli.commands(["build", "workspace"], commands) }
""")?
  test.ok(!output.success, output.stderr)?
  test.contains(output.stderr, "check.cli-descriptor")?
}

test cli_commands_constants_preserve_fallback_and_rootless_selection [fs, error] { |ctx|
  let output = test.run_script(ctx, r"""
const commands = {build: {aliases: ["compile"], positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
const fallback = {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true}
let fallback_args = cli.commands(["deploy", "target/demo", "extra"], "build", commands, fallback)?
let selected: Path = fallback_args.root
let action: Str = fallback_args.action
let raw: List[Str] = fallback_args.raw
print $action selected.name() ${raw[0]}
let alias = cli.commands(["compile", "workspace"], commands)?
let command: Str = alias.command
print $command
let rootless = cli.commands(["target/demo"], "build", commands, fallback)?
let root: Path = rootless.root
print root.name()
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "deploy demo extra\nbuild\ndemo\n")?
}

test cli_commands_constants_import_projection_and_named_spread [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "cli-command-descriptors")?
  fp"${root}/config.xsh".write(r"""##! Command descriptor configuration.
## Prepared command records.
export const descriptor = {commands: {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}}
""")?
  let output = test.run_script(ctx, r"""
use config as c
const commands = {...c.descriptor.commands, clean: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
const invocation = {argv: ["clean", "workspace", "extra"], commands: commands}
let parsed = cli.commands(...invocation)?
let root: Path = parsed.root
let rest: List[Str] = parsed.raw
print root.display() ${rest[0]}
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "workspace extra\n")?
}

test cli_commands_constants_keep_dynamic_validation_and_command_fields [fs, error] { |ctx|
  let output = test.run_script(ctx, r"""
proc descriptor() [] -> Record { {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}} }
type ParsedCommand = {command: Str, action: Str, root: Path, raw: List[Str]}
let parsed = cli.commands(["build", "workspace"], descriptor())?.require(ParsedCommand)?
print parsed.root.display()
const commands = {build: {positionals: ["root"], types: {root: "Path"}}, clean: {positionals: ["count"], types: {count: "Int"}}}
let clean = cli.commands(["clean", "3"], commands)?
print (clean.get("count")?.require(Int)?)
proc fallback() [] -> Record { print "fallback once"; {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw"} }
let dynamic_fallback = cli.commands(["deploy", "workspace"], "build", commands, fallback())?.require(ParsedCommand)?
print dynamic_fallback.root.display()
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "workspace\n3\nfallback once\nworkspace\n")?
}

test cli_dynamic_full_descriptor_preserves_outcome_envelope [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc descriptor() [] -> Record { {count: {kind: "Int", default: 2, deprecated: "use jobs"}} }
type ParsedValues = {count: Int}
let full = cli.parse_full(["--count", "4"], descriptor())?
let values = full.values.require(ParsedValues)?
let warnings: List[Str] = full.warnings
let source = full.sources.get("count")?.require(Str)?
print ${values.count} $source warnings.len()
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "4 argv 1\n")?
}

test cli_prepared_descriptors_check_annotated_result_after_refinement [fs, error] { |ctx|
  let output = test.run_script(ctx, r"""
type ParsedValues = {count: Int}
type CommandValues = {command: Str, action: Str, root: Path, raw: List[Str]}
const schema = {count: {kind: "Int", default: 2}}
const commands = {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
let parsed: ParsedValues = cli.parse([], schema)?
let applet_values: ParsedValues = cli.applet([], schema)?
let command: CommandValues = cli.commands(["build", "workspace"], commands)?
print ${parsed.count} ${applet_values.count} command.root.display()
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "2 2 workspace\n")?
  let mismatch = test.run_script(ctx, r"""
type IncorrectValues = {count: Str}
const schema = {count: {kind: "Int", default: 2}}
let parsed: IncorrectValues = cli.parse([], schema)?
""")?
  test.ok(!mismatch.success)?
  test.contains(mismatch.stderr, "check.type-mismatch")?
}
