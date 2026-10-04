const constant_options = {
  jobs: {
    kind: "Int",
    form: "-j --jobs N",
    default: 4,
    positive: true,
  },
  root: {
    kind: "Path",
    form: "ROOT",
  },
  verbose: "Bool",
  tag: {
    kind: "Str",
    repeated: true,
  },
  zoptional: {
    kind: "Int",
    form: "OPTIONAL",
    required: false,
  },
}

test cli_constants_retain_the_inline_descriptor_shape {
  let options = cli.parse(["workspace", "--jobs", "6", "--tag", "one"], constant_options)?
  let {jobs, root, verbose, tag: tags, ..} = options
  let optional: Int? = options.zoptional
  assert jobs == 6
  assert root == p"workspace"
  assert verbose == false
  assert tags == ["one"]
  assert optional == null
  let full = cli.parse_full(["workspace"], constant_options)?
  let default_jobs = full.values.jobs
  assert default_jobs == 4
  assert full.sources.get("jobs")? == "default"
  assert full.warnings == []
  let parsed_applet = cli.applet(["workspace", "-j2", "-j3"], constant_options)?
  let last_jobs = parsed_applet.jobs
  assert last_jobs == 3
}

test cli_constants_reject_invalid_known_descriptors_during_checking { |ctx|
  let rejected = test.run_script(
    ctx,
    """const schema = {count: {kind: "Nope"}}
let _ = cli.parse([], schema)
""",
  )?
  assert rejected.success == false
  assert "check.cli-descriptor" in rejected.stderr
  assert "unsupported option type `Nope`" in rejected.stderr
}

test cli_constants_import_projection_and_composition_keep_types { |ctx|
  let root = test.temp_dir(ctx, name: "cli-constant-module")?
  fp"{root}/config.xsh".write_atomic(r"""##! CLI configuration.
## Prepared descriptor fields.
export const descriptors = {schema: {jobs: {default: 4}, root: {kind: "Path", required: true}}}
""")?
  let output = test.run_script(
    ctx,
    r"""use config as c
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
print ${selected_root}
print $flag
print ${tags.len()}
print $default_jobs
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """7
workspace
false
0
4
"""
}

pure dynamic_options(value: Record) -> Record {
  value
}

test cli_constants_dynamic_descriptors_keep_runtime_validation {
  let dynamic = cli.parse(["--jobs", "8"], dynamic_options({jobs: {kind: "Int"}}))?
  let jobs = dynamic.get("jobs")?.require(Int)?
  assert jobs == 8
  test.error_kind(cli.parse([], dynamic_options({count: {kind: "Nope"}})), "cli-parse")?
}

test cli_constants_invalid_imported_composition_reports_original_descriptor { |ctx|
  let root = test.temp_dir(ctx, name: "cli-invalid-module")?
  fp"{root}/invalid_config.xsh".write_atomic(r"""##! Invalid descriptor fixture.
## Known malformed option.
export const descriptors = {count: {kind: "Nope"}}
""")?
  let rejected = test.run_script(
    ctx,
    r"""use invalid_config as c
const schema = {...c.descriptors, verbose: "Bool"}
let _ = cli.parse([], schema)
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert rejected.success == false
  assert "check.cli-descriptor" in rejected.stderr
  assert "invalid_config.xsh:3:" in rejected.stderr
  assert "unsupported option type `Nope`" in rejected.stderr
}

test cli_constants_named_arguments_evaluate_once_in_source_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""const schema = {jobs: {kind: "Int", default: 4}}
proc operands() [io] -> List[Str] { print "argv"; [] }
proc label() [io] -> Str { print "command"; "demo" }
proc descriptor() [io] -> Record { print "schema"; schema }
let prepared = cli.parse(schema: schema, argv: operands(), command: label())?
print ${prepared.jobs}
let dynamic = cli.parse(schema: descriptor(), argv: operands(), command: label())?
print ${dynamic.get("jobs")?.require(Int)?}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """argv
command
4
schema
argv
command
4
"""
}

test cli_constants_forced_non_bool_flags_keep_dynamic_values {
  const schema = {switch: {kind: "Int", flag: true}, many: {kind: "Str", flag: true, repeated: true}}
  let unvalued = cli.parse(["--switch", "--many"], schema)?
  assert unvalued.switch.require(Bool)? == true
  assert unvalued.many[0].require(Bool)? == true
  let valued = cli.parse(["--switch=3", "--many=text"], schema)?
  assert valued.switch.require(Int)? == 3
  assert valued.many[0].require(Str)? == "text"
}

test cli_constants_and_inline_descriptors_establish_the_same_field_types { |ctx|
  let descriptor = r"""{
  jobs: {kind: "Int", form: "-j --jobs N", default: 4, positive: true},
  root: {kind: "Path", form: "ROOT"},
}
"""
  let accepted = test.run_script(
    ctx,
    "const option_schema = " + descriptor + "let inline = cli.parse([\"work\"], " + descriptor + """)?
""" + r"""let options = cli.parse(["work", "-j", "3"], option_schema)?
let jobs: Int = options.jobs
let root: Path = options.root
let inline_jobs: Int = inline.jobs
let inline_root: Path = inline.root
print $jobs ${root} $inline_jobs ${inline_root}
""",
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == """3 work 4 work
"""
  for field in ["jobs", "root"] {
    let rejected = test.run_script(
      ctx,
      "const option_schema = " + descriptor + """let options = cli.parse(["work"], option_schema)?
let wrong: Str = options.""" + field + "\n",
    )?
    assert ! rejected.success, field
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }

  let runtime = test.run_script(
    ctx,
    "let option_schema = " + descriptor + """let options = cli.parse(["work"], option_schema)?
let jobs: Int = options.jobs
""",
  )?
  assert ! runtime.success, runtime.stdout
  assert "check.dynamic-boundary" in runtime.stderr, runtime.stderr
}
