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

proc test_env_overlays_blocks_lookup_and_path_mutation_affect_children(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "env-scope")?
  let tool = fp"${root}/env-scope-tool"

  tool.write("""#!/bin/sh
printf '%s|%s|%s|%s' "$CC" "$CFLAGS" "$DESTDIR" "$XSH_ENV_SCOPE"
""")?

  tool.chmod(0o755)?
  env.PATH.append(root)?
  test.ok(root in env.PATH)?

  env XSH_ENV_SCOPE=block DESTDIR=/tmp/xsh-env-scope HOME=$root {
    let dest = env.Str.DESTDIR?
    let dest_path = env.path("DESTDIR")?
    let fallback = env.get_or("XSH_ENV_SCOPE_MISSING", "fallback")?
    let empty = env.get_or("XSH_ENV_SCOPE_MISSING_EMPTY")?
    let truthy = env.bool("XSH_ENV_SCOPE", false)?
    let default_bool = env.bool("XSH_ENV_SCOPE_BOOL_MISSING")?
    let count = env.int("XSH_ENV_SCOPE_COUNT", 7)?
    let default_count = env.int("XSH_ENV_SCOPE_COUNT_MISSING")?
    let fallback_path = env.path("XSH_ENV_SCOPE_MISSING_PATH", root)?
    let entries = env.list()?
    let home = env.Path.HOME?
    let path_list = env.PathList.PATH?
    test.eq(dest, "/tmp/xsh-env-scope")?
    test.eq(dest_path.display(), "/tmp/xsh-env-scope")?
    test.eq(empty, "")?
    test.eq(default_bool, false)?
    test.eq(default_count, 0)?
    test.eq(home, root)?
    test.ok(root in path_list)?
    test.ok(entries |> any .name == "DESTDIR" and .value == "/tmp/xsh-env-scope")?
    test.eq(fallback, "fallback")?
    test.eq(truthy, false)?
    test.eq(count, 7)?
    test.eq(fallback_path, root)?
    let line = run.text CC=cc CFLAGS="-O2 -pipe" env-scope-tool ?
    test.eq(line, "cc|-O2 -pipe|/tmp/xsh-env-scope|block")?
  } ?

  let removed_path = env.PATH.pop()?
  test.eq(removed_path, root)?
  test.ok(root not in env.PATH)?
}

proc test_path_literals_method_sugar_and_expr_env_blocks(ctx: TestContext) [fs, process, env, error] {
  let root = test.temp_dir(ctx, name: "sugar")?
  let child_name = "child"
  let child = fp"${root}/${child_name}"
  root.mkdir()?

  env {
    HOME = root
    CHILD = child
    DIGEST = b"abc".sha256().hex()
    ENCODED = b"abc".base64()
    COUNT = 3
  } {
    let home = env.Path.HOME?
    let encoded = env.Str.ENCODED?
    let decoded = encoded.base64_decode()?

    let lines = """ alpha
beta """.trim().lines().collect()

    test.eq(home, root)?
    test.ok("child" in env.Path.CHILD?)
    test.eq(decoded, b"abc")?
    test.eq(lines[1], "beta")?
    test.eq(b"abc".compare(b"abd").byte, 3)?
    let line = run.text sh -c "printf '%s|%s|%s' \"\$HOME\" \"\$DIGEST\" \"\$COUNT\";" ?
    test.eq(line, f"${root.display()}|ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad|3")?
  } ?
}
