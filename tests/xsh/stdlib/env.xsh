proc test_env_functions_and_path_list(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "env")?
  let tool_dir = fp"${root}/bin"
  fs.mkdir(tool_dir)?
  let tool = fp"${tool_dir}/xsh-env-helper"

  fs.write(
    tool,
    """#!/bin/sh
printf '%s|%s|%s' "$XSH_STDLIB_ENV" "$DESTDIR" "$PATH"
""",
  )?

  fs.chmod(tool, 0o755)?

  env XSH_STDLIB_ENV=yes DESTDIR=/tmp/xsh-stdlib-env XSH_STDLIB_COUNT=7 XSH_STDLIB_BOOL=true XSH_STDLIB_PATH=$root {
    test.eq(env.get("XSH_STDLIB_ENV")?, "yes")?
    test.eq(env.get_or("XSH_STDLIB_MISSING", "fallback")?, "fallback")?
    test.eq(env.bool("XSH_STDLIB_BOOL", false)?, true)?
    test.eq(env.bool("XSH_STDLIB_MISSING_BOOL")?, false)?
    test.eq(env.int("XSH_STDLIB_COUNT", 0)?, 7)?
    test.eq(env.int("XSH_STDLIB_MISSING_INT")?, 0)?
    test.eq(env.path("XSH_STDLIB_PATH")?, root)?
    test.eq(env.path("XSH_STDLIB_MISSING_PATH", root)?, root)?
    test.ok(env.list()? |> any .name == "DESTDIR" and .value == "/tmp/xsh-stdlib-env")?
    env.PATH.prepend(tool_dir)?
    test.ok(tool_dir in env.path_list("PATH")?)?
    test.ok(env.path_list("PATH")?.contains(tool_dir))?
    let path_entries = env.path_entries("PATH")?
    test.ok(path_entries |> any .raw == tool_dir.display() and .path == tool_dir and ! .empty)?
    let extra_dir = fp"${tool_dir}/extra"
    env.PATH.append(extra_dir)?
    test.eq(env.PATH.pop()?, extra_dir)?
    test.eq(env.Path.XSH_STDLIB_PATH?, root)?
    test.eq(env.Str.DESTDIR?, "/tmp/xsh-stdlib-env")?
    let output = run.text xsh-env-helper ?
    test.contains(output, "yes|/tmp/xsh-stdlib-env|")?
  } ?

  env XSH_STDLIB_CUSTOM_PATH=f":${tool_dir.display()}::" {
    let entries = env.path_entries("XSH_STDLIB_CUSTOM_PATH")?
    test.eq(entries.len(), 4)?
    test.ok(entries[0].empty)?
    test.eq(entries[1].path, tool_dir)?
    test.ok(entries[2].empty)?
    test.ok(entries[3].empty)?
  } ?
}
