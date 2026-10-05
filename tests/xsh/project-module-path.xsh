# `xsh` resolves `use` through the project module path exactly as `xsht`
# does: beside the importing file, then `XSH_MODULE_PATH`, then the
# `module_path` of the nearest `xsht-config.ini` above the entry script.

const answers_module = """##! Shared answers.

## The shared answer.
export pure answer() -> Int {
  42
}
"""

const entry_script = """use shared.answers as answers

print answers.answer()
"""

# A module named `origin` whose one export says which root it was found in.
pure origin_module(root: Str) -> Str {
  f"""##! Reports the root it was loaded from.

## The root this copy lives in.
export const root = "{root}"
"""
}

proc project(ctx: TestContext, name: Str, config: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name:)?
  fp"{root}/lib/shared".mkdir()?
  fp"{root}/bin".mkdir()?
  fp"{root}/xsht-config.ini".write(config)?
  fp"{root}/lib/shared/answers.xsh".write(answers_module)?
  fp"{root}/bin/entry.xsh".write(entry_script)?
  root
}

test xsh_resolves_use_through_the_project_module_path { |ctx|
  let root = project(ctx, "project-roots", "module_path = lib\n")?
  let entry = fp"{root}/bin/entry.xsh"

  # No `XSH_MODULE_PATH` entry names the project; only its config does.
  let run_output = run.capture --text "xsh" $entry ?
  assert run_output.status.exited_with(0), run_output.stderr
  assert run_output.stdout == "42\n", run_output.stdout

  # The same entry checks with the same roots.
  let check_output = run.capture --text "xsht" check $entry ?
  assert check_output.status.exited_with(0), check_output.stderr
}

test xsh_reads_the_nearest_config_and_defaults_to_its_directory { |ctx|
  let root = project(ctx, "nearest-config", "module_path = missing\n")?
  # A nearer config without `module_path` names its own directory as the root.
  fp"{root}/bin/xsht-config.ini".write("test_roots = tests\n")?
  fp"{root}/bin/shared".mkdir()?
  fp"{root}/bin/nested".mkdir()?
  fp"{root}/bin/shared/answers.xsh".write(answers_module.replace("42", "7"))?
  fp"{root}/bin/nested/entry.xsh".write(entry_script)?

  let output = run.capture --text "xsh" fp"{root}/bin/nested/entry.xsh" ?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "7\n", output.stdout
}

test module_load_resolves_imports_through_the_entry_project_roots { |ctx|
  let root = project(ctx, "loaded-module-roots", "module_path = lib\n")?
  fp"{root}/plugins".mkdir()?
  fp"{root}/plugins/plugin.xsh".write("""##! A plugin importing through the project root.
use shared.answers as answers

## The answer resolved through the project root.
export let value: Int = answers.answer()
""")?
  fp"{root}/bin/load.xsh".write(f"""type AnswerPlugin = module {{
  export let value: Int
}}

print ${{module.load(p"{root}/plugins/plugin.xsh")?.require(AnswerPlugin)?.value}}
""")?

  let output = run.capture --text "xsh" fp"{root}/bin/load.xsh" ?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "42\n", output.stdout
}

test module_search_order_is_file_relative_then_environment_then_project { |ctx|
  let root = test.temp_dir(ctx, name: "search-order")?
  let from_env = test.temp_dir(ctx, name: "search-order-env")?
  fp"{root}/bin".mkdir()?
  fp"{root}/lib".mkdir()?
  fp"{root}/xsht-config.ini".write("module_path = lib\n")?
  fp"{root}/lib/origin.xsh".write(origin_module("project"))?
  fp"{from_env}/origin.xsh".write(origin_module("environment"))?
  let entry = fp"{root}/bin/entry.xsh"
  entry.write(r"""use origin

print ${origin.root}
""")?
  let env_root = from_env.display()

  let project_only = run.capture --text "xsh" $entry ?
  assert project_only.status.exited_with(0), project_only.stderr
  assert project_only.stdout == "project\n", project_only.stdout

  env XSH_MODULE_PATH=$env_root {
    let with_env = run.capture --text "xsh" $entry ?
    assert with_env.status.exited_with(0), with_env.stderr
    assert with_env.stdout == "environment\n", with_env.stdout

    fp"{root}/bin/origin.xsh".write(origin_module("file"))?
    let beside = run.capture --text "xsh" $entry ?
    assert beside.status.exited_with(0), beside.stderr
    assert beside.stdout == "file\n", beside.stdout
  }
}

test a_malformed_project_config_stops_the_run { |ctx|
  let root = project(ctx, "malformed-config", "module_path lib\n")?
  let entry = fp"{root}/bin/entry.xsh"
  # The entry needs nothing from the config; the run still refuses to start.
  entry.write("print started\n")?

  let undecodable = run.capture --text "xsh" $entry ?
  assert undecodable.status.exited_with(2), undecodable.stderr
  assert "xsh: invalid xsht-config.ini" in undecodable.stderr, undecodable.stderr
  assert undecodable.stdout == "", undecodable.stdout

  fp"{root}/xsht-config.ini".write("[module_path]\nroot = lib\n")?
  let section = run.capture --text "xsh" $entry ?
  assert section.status.exited_with(2), section.stderr
  assert "module_path: expected a list of directories" in section.stderr, section.stderr
  assert section.stdout == "", section.stdout

  let checked = run.capture --text "xsht" check $entry ?
  assert checked.status.exited_with(2), checked.stderr
  assert "module_path: expected a list of directories" in checked.stderr, checked.stderr
}

test a_script_without_a_project_config_has_no_project_roots { |ctx|
  let root = project(ctx, "no-config", "module_path = lib\n")?
  fp"{root}/xsht-config.ini".remove()?

  let output = run.capture --text "xsh" fp"{root}/bin/entry.xsh" ?
  assert !output.status.ok, output.stdout
  assert "failed to read module" in output.stderr, output.stderr
  assert "`module_path` in the project's xsht-config.ini" in output.stderr, output.stderr
}
