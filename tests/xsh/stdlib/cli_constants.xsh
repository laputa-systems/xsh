const constant_options = {
  jobs: {kind: "Int", form: "-j --jobs N", default: 4, positive: true},
  root: {kind: "Path", form: "ROOT"},
  verbose: "Bool",
  tag: {kind: "Str", repeated: true},
  zoptional: {kind: "Int", form: "OPTIONAL", required: false},
}

test cli_constants_retain_the_inline_descriptor_shape [fs, error] {
  let options = cli.parse(["workspace", "--jobs", "6", "--tag", "one"], constant_options)?
  let {jobs, root, verbose, tag: tags, ..} = options
  let optional: Int? = options.zoptional
  (jobs) == (6)
  (root) == (p"workspace")
  (verbose) == (false)
  (tags) == (["one"])
  (optional) == (null)
  let full = cli.parse_full(["workspace"], constant_options)?
  let default_jobs = full.values.jobs
  (default_jobs) == (4)
  (full.sources.get("jobs")?) == ("default")
  (full.warnings) == ([])
  let parsed_applet = cli.applet(["workspace", "-j2", "-j3"], constant_options)?
  let last_jobs = parsed_applet.jobs
  (last_jobs) == (3)
}


test cli_constants_reject_invalid_known_descriptors_during_checking [error] { |ctx|
  let rejected = test.run_script(ctx, "const schema = {count: {kind: \"Nope\"}}\nlet _ = cli.parse([], schema)\n")?
  (rejected.success) == (false)
  ("check.cli-descriptor" in rejected.stderr)
  ("unsupported option type `Nope`" in rejected.stderr)
}


test cli_constants_import_projection_and_composition_keep_types [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "cli-constant-module")?
  fp"${root}/config.xsh".write_atomic(r"""##! CLI configuration.
## Prepared descriptor fields.
export const descriptors = {schema: {jobs: {default: 4}, root: {kind: "Path", required: true}}}
""")?
  let output = test.run_script(ctx, r"""use config as c
const base = c.descriptors.schema
const descriptors = {...base, verbose: "Bool", tag: "List[Str]"}
let parsed = cli.parse(["--root", "workspace", "--jobs", "7"], {...base, verbose: "Bool", tag: "List[Str]"})?
let prepared = cli.parse_full(["--root", "workspace"], descriptors)?
let jobs: Int = parsed.jobs
let selected_root: Path = parsed.root
let flag: Bool = parsed.verbose
let tags: List[Str] = parsed.tag
let default_jobs: Int = prepared.values.jobs
print $jobs
print ${selected_root.display()}
print $flag
print ${tags.len()}
print $default_jobs
""", [], {XSH_MODULE_PATH: root.display()})?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("7\nworkspace\nfalse\n0\n4\n")
}

pure dynamic_options(value: Record) -> Record { value }

test cli_constants_dynamic_descriptors_keep_runtime_validation [fs, error] {
  let dynamic = cli.parse(["--jobs", "8"], dynamic_options({jobs: {kind: "Int"}}))?
  let jobs = dynamic.get("jobs")?.require(Int)?
  (jobs) == (8)
  test.error_kind(cli.parse([], dynamic_options({count: {kind: "Nope"}})), "cli-parse")?
}


test cli_constants_invalid_imported_composition_reports_original_descriptor [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "cli-invalid-module")?
  fp"${root}/invalid_config.xsh".write_atomic(r"""##! Invalid descriptor fixture.
## Known malformed option.
export const descriptors = {count: {kind: "Nope"}}
""")?
  let rejected = test.run_script(ctx, r"""use invalid_config as c
const schema = {...c.descriptors, verbose: "Bool"}
let _ = cli.parse([], schema)
""", [], {XSH_MODULE_PATH: root.display()})?
  (rejected.success) == (false)
  ("check.cli-descriptor" in rejected.stderr)
  ("invalid_config.xsh:3:" in rejected.stderr)
  ("unsupported option type `Nope`" in rejected.stderr)
}

test cli_constants_named_arguments_evaluate_once_in_source_order [error] { |ctx|
  let output = test.run_script(ctx, r"""const schema = {jobs: {kind: "Int", default: 4}}
proc operands() [io] -> List[Str] { print "argv"; [] }
proc label() [io] -> Str { print "command"; "demo" }
proc descriptor() [io] -> Record { print "schema"; schema }
let prepared = cli.parse(schema: schema, argv: operands(), command: label())?
print ${prepared.jobs}
let dynamic = cli.parse(schema: descriptor(), argv: operands(), command: label())?
print ${dynamic.get("jobs")?.require(Int)?}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("argv\ncommand\n4\nschema\nargv\ncommand\n4\n")
}


test cli_constants_forced_non_bool_flags_keep_dynamic_values [fs, error] {
  const schema = {switch: {kind: "Int", flag: true}, many: {kind: "Str", flag: true, repeated: true}}
  let unvalued = cli.parse(["--switch", "--many"], schema)?
  (unvalued.switch.require(Bool)?) == (true)
  (unvalued.many[0].require(Bool)?) == (true)
  let valued = cli.parse(["--switch=3", "--many=text"], schema)?
  (valued.switch.require(Int)?) == (3)
  (valued.many[0].require(Str)?) == ("text")
}
