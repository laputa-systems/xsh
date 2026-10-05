type Captured = {status: Status, stdout: Str, stderr: Str}

# One way to start a command: where it runs and the paths it is given.
type Invocation = {dir: Path, paths: List[Str]}

# Every key of this config changes what a tool reports for some file of the
# project, so a command that read another config, or none, shows it.
const project_config = """exclude = ignored/**
module_path = lib
test_roots = checks

[format]
line-width = 40
exclude = generated/**

[lint]
prefer-inferred-pure-returns = true

[lint.prefer-inferred-pure-return]
exclude = legacy/**

[dead-code]
exclude = dead/**
"""

# Draws `lint.prefer-inferred-pure-return` where the rule is on.
const annotated_pure = """pure label(name: Str) -> Str {
  name.trim()
}

print label("ready")
"""

# Draws the opt-in lint and holds a list that is laid out for a 40 column line
# and would be joined at the default width.
const main_source = """const names = [
  "alpha",
  "beta",
  "gamma",
  "delta",
]

pure label(name: Str) -> Str {
  name.trim()
}

print label(names[0])
"""

# One line at the default width, which a 40 column config would break.
const wide_source = """const names = ["alpha", "beta", "gamma", "delta"]
print names[0]
"""

# A directory holding `project`, a project whose config is the only one that
# may govern its files, beside a config of its own whose settings name the
# project's files and must not reach them.
proc nested_project(ctx: TestContext) [fs, error] -> Result[Path] {
  let parent = test.temp_dir(ctx, name: "parent")?
  let files: Map[Str, Str] = {
    "xsht-config.ini": "[format]\nline-width = 200\n\n[lint.prefer-inferred-pure-return]\nexclude = project/**\n\n[dead-code]\nexclude = project/**\n",
    "project/xsht-config.ini": project_config,
    "project/lib/helper.xsh": "##! Nested config helper module.\n## Returns the configured helper value.\nexport pure value() -> Str {\n  \"ok\"\n}\n",
    "project/app/main.xsh": main_source,
    "project/app/greet.xsh": "use helper\n\nprint helper.value()\n",
    "project/ignored/bad.xsh": "let =\n",
    "project/legacy/old.xsh": annotated_pure,
    "project/dead/unused.xsh": "proc unused() {\n  print \"never\"\n}\n\nprint \"used\"\n",
    "project/generated/ugly.xsh": "print  \"ugly\"\n",
    "project/checks/helper-test.xsh": "use helper\n\ntest test_helper_value {\n  assert helper.value() == \"ok\"\n}\n",
  }
  for name in files.keys() {
    let file = fp"{parent}/{name}"
    file.parent().mkdir()
    file.write(files[name])
  }

  Ok(parent)
}

# Runs `xsht` with `arguments` in `dir`.
proc xsht(dir: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  cd (dir) {
    run.capture --text "xsht" @arguments
  }
}

# What a command printed on standard error apart from its closing timing line.
pure diagnostics(stderr: Str) -> Str {
  let kept = collect {
    for line in stderr.lines() {
      yield line unless "thread time by stage" in line
    }
  }

  kept.join("\n")
}

# The codes of the warnings and errors in `stderr`, in the order reported.
pure reported_codes(stderr: Str) -> List[Str] {
  let codes = collect {
    for line in stderr.lines() {
      let found = rx"^(?:warn|err|note)\[([^\]]+)\]".captures(line)
      yield found[1] when found.len() == 2
    }
  }

  codes
}

# Every way to name the whole project: no argument, `.`, a relative and an
# absolute directory, from the project, from the directory above it, and from
# a directory that has nothing to do with it.
proc whole_project_invocations(ctx: TestContext, parent: Path) [fs, error] -> Result[List[Invocation]] {
  let project = fp"{parent}/project"
  let elsewhere = test.temp_dir(ctx, name: "elsewhere")?
  Ok(
    [
      {dir: project, paths: []},
      {dir: project, paths: ["."]},
      {dir: project, paths: [project.display()]},
      {dir: parent, paths: []},
      {dir: parent, paths: ["."]},
      {dir: parent, paths: ["project"]},
      {dir: parent, paths: [project.display()]},
      {dir: elsewhere, paths: [project.display()]},
    ],
  )
}

# Every way to name one file of the project, given its path below the project.
proc file_invocations(ctx: TestContext, parent: Path, name: Str) [fs, error] -> Result[List[Invocation]] {
  let project = fp"{parent}/project"
  let elsewhere = test.temp_dir(ctx, name: "elsewhere")?
  Ok(
    [
      {dir: project, paths: [name]},
      {dir: parent, paths: [f"project/{name}"]},
      {dir: fp"{project}/lib", paths: [f"../{name}"]},
      {dir: elsewhere, paths: [fp"{project}/{name}".display()]},
    ],
  )
}

test test_lint_gives_each_file_its_nearest_config_from_every_directory { |ctx|
  let parent = nested_project(ctx)?
  for invocation in whole_project_invocations(ctx, parent)? {
    let linted = xsht(invocation.dir, ["lint", @invocation.paths])?
    let where = f"{invocation.dir} lint {invocation.paths.join(" ")}: {linted.stderr}"
    # `exclude` drops the file that does not parse, `module_path` resolves the
    # import, the `[lint]` key turns the opt-in rule on, its `[lint.RULE]`
    # section and `[dead-code]` keep the two other files quiet, and the
    # sections of the config above the project silence nothing in it.
    assert reported_codes(linted.stderr) == ["lint.prefer-inferred-pure-return"], where
    assert "app/main.xsh" in linted.stderr, where
    assert linted.status.exited_with(1), where
  }

  for invocation in file_invocations(ctx, parent, "app/main.xsh")? {
    let linted = xsht(invocation.dir, ["lint", @invocation.paths])?
    let where = f"{invocation.dir} lint {invocation.paths.join(" ")}: {linted.stderr}"
    assert reported_codes(linted.stderr) == ["lint.prefer-inferred-pure-return"], where
  }

  for name in ["legacy/old.xsh", "dead/unused.xsh"] {
    for invocation in file_invocations(ctx, parent, name)? {
      let linted = xsht(invocation.dir, ["lint", @invocation.paths])?
      let where = f"{invocation.dir} lint {invocation.paths.join(" ")}: {linted.stderr}"
      assert reported_codes(linted.stderr) == [], where
      assert linted.status.exited_with(0), where
    }
  }
}

test test_check_gives_each_file_its_nearest_config_from_every_directory { |ctx|
  let parent = nested_project(ctx)?
  # `module_path` resolves the import of `greet.xsh` and `exclude` drops the
  # file that does not parse.
  let invocations = whole_project_invocations(ctx, parent)? + file_invocations(ctx, parent, "app/greet.xsh")?
  for invocation in invocations {
    let checked = xsht(invocation.dir, ["check", @invocation.paths])?
    let where = f"{invocation.dir} check {invocation.paths.join(" ")}: {checked.stderr}"
    assert diagnostics(checked.stderr) == "", where
    assert checked.status.exited_with(0), where
  }
}

test test_fmt_check_gives_each_file_its_nearest_config_from_every_directory { |ctx|
  let parent = nested_project(ctx)?
  let invocations = whole_project_invocations(ctx, parent)? + file_invocations(ctx, parent, "app/main.xsh")?
  for invocation in invocations {
    # The project's `line-width` keeps the list broken, its `[format] exclude`
    # and `exclude` drop the two files that would fail, and the width of the
    # config above the project does not apply.
    let formatted = xsht(invocation.dir, ["fmt", "--check", @invocation.paths])?
    let where = f"{invocation.dir} fmt --check {invocation.paths.join(" ")}: {formatted.stdout}{formatted.stderr}"
    assert formatted.stdout == "", where
    assert formatted.stderr == "", where
    assert formatted.status.exited_with(0), where
  }

  # A file named on the command line is formatted whatever `[format] exclude`
  # says about it.
  for invocation in file_invocations(ctx, parent, "generated/ugly.xsh")? {
    let formatted = xsht(invocation.dir, ["fmt", "--check", @invocation.paths])?
    assert formatted.status.exited_with(1), formatted.stderr
    assert "ugly.xsh: needs formatting" in formatted.stdout, formatted.stdout
  }
}

# `test_roots` lists the test directories of one project, so `xsht test` reads
# it from the config in the current directory alone. Each test file it finds
# still loads with the module roots of its own nearest config.
test test_test_roots_come_from_the_config_in_the_current_directory { |ctx|
  let parent = nested_project(ctx)?
  let project = fp"{parent}/project"
  let listed = xsht(project, ["test", "--list"])?
  assert listed.status.exited_with(0), listed.stderr
  assert listed.stdout == "checks/helper-test.xsh::test_helper_value\n"
  let filtered = xsht(project, ["test", "--list", "checks/helper-test.xsh"])?
  assert filtered.stdout == listed.stdout
  let ran = xsht(project, ["test"])?
  assert ran.status.exited_with(0), ran.stdout + ran.stderr
  assert "1 passed" in ran.stdout, ran.stdout

  let from_parent = xsht(parent, ["test", "--list"])?
  assert from_parent.status.exited_with(0), from_parent.stderr
  assert from_parent.stdout == ""
}

# A file with no config above it takes the defaults: the config of the
# directory the command runs in governs the files of that directory's project
# and no others.
test test_a_file_outside_every_project_takes_the_defaults { |ctx|
  let strict = test.temp_dir(ctx, name: "strict")?
  let loose = test.temp_dir(ctx, name: "loose")?
  fp"{strict}/xsht-config.ini".write("[lint]\nprefer-inferred-pure-returns = true\n\n[format]\nline-width = 40\n")
  fp"{strict}/label.xsh".write(annotated_pure)
  fp"{loose}/label.xsh".write(annotated_pure)
  fp"{loose}/wide.xsh".write(wide_source)

  let outside = xsht(strict, ["lint", fp"{loose}/label.xsh".display()])?
  assert reported_codes(outside.stderr) == [], outside.stderr
  assert outside.status.exited_with(0), outside.stderr
  let inside = xsht(loose, ["lint", fp"{strict}/label.xsh".display()])?
  assert reported_codes(inside.stderr) == ["lint.prefer-inferred-pure-return"], inside.stderr

  let wide = xsht(strict, ["fmt", "--check", fp"{loose}/wide.xsh".display()])?
  assert wide.status.exited_with(0), wide.stdout + wide.stderr
}

# An exclusion covers what lies below the config that states it, a project
# nested there included, for a command started at or above that config. A
# command about the nested project sees the project as it sees itself.
test test_an_outer_exclude_covers_a_nested_project_only_from_outside { |ctx|
  let root = test.temp_dir(ctx, name: "outer")?
  let files: Map[Str, Str] = {
    "xsht-config.ini": "exclude = vendored/**\n",
    "main.xsh": "print \"ok\"\n",
    "vendored/xsht-config.ini": "",
    "vendored/bad.xsh": "let =\n",
  }
  for name in files.keys() {
    let file = fp"{root}/{name}"
    file.parent().mkdir()
    file.write(files[name])
  }

  for command in [["lint"], ["check"], ["fmt", "--check"]] {
    for paths in [[], ["."]] {
      let outside = xsht(root, [@command, @paths])?
      assert outside.status.exited_with(0), f"{command.join(" ")} {paths.join(" ")}: {outside.stderr}"
    }

    let named = xsht(root, [@command, "vendored"])?
    assert named.status.exited_with(2), f"{command.join(" ")} vendored: {named.stderr}"
    assert "bad.xsh" in named.stderr, named.stderr
    let inside = xsht(fp"{root}/vendored", command)?
    assert inside.status.exited_with(2), f"{command.join(" ")} in vendored: {inside.stderr}"
    assert "bad.xsh" in inside.stderr, inside.stderr
  }
}
