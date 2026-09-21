# The message of a failed string lookup, or the empty string when it succeeded.
# `test.error_kind` compares kinds only, so message parity is asserted through
# this.
pure str_failure(result: Result[Str]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

# The message of a failed integer lookup, or the empty string on success.
pure int_failure(result: Result[Int]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

proc test_env_get_or_yields_the_fallback_only_for_an_unset_name()  [env, error] {
  env {
    XSH_ENV_EMPTY = ""
    XSH_ENV_TEXT = "value"
    XSH_ENV_SPACED = "  keep  "
    XSH_ENV_UNICODE = "héllo"
  } {
    # An unset name yields the fallback, defaulted or explicit, and the
    # fallback is not evaluated when the name is set.
    test.eq(env.get_or("XSH_ENV_ABSENT")?, "")?
    test.eq(env.get_or("XSH_ENV_ABSENT", "fallback")?, "fallback")?
    test.eq(env.get_or("XSH_ENV_ABSENT", "")?, "")?

    # A present name yields its own value, byte for byte: no trimming, no
    # decoding, and no substitution of the fallback for an empty value.
    test.eq(env.get_or("XSH_ENV_EMPTY", "fallback")?, "")?
    test.eq(env.get_or("XSH_ENV_TEXT", "fallback")?, "value")?
    test.eq(env.get_or("XSH_ENV_SPACED", "fallback")?, "  keep  ")?
    test.eq(env.get_or("XSH_ENV_UNICODE", "fallback")?, "héllo")?

    # Key validation is the native one and is not a fallback case.
    test.error_kind(env.get_or(""), "env-name")?
    test.eq(
      str_failure(env.get_or("")),
      "environment names cannot be empty or contain NUL or `=`",
    )?
    test.error_kind(env.get_or("=bad", "fallback"), "env-name")?
    test.error_kind(env.get("XSH_ENV=A"), "env-name")?
  } ?
}

proc test_env_bool_accepts_only_the_baseline_spellings()  [env, error] {
  env {
    XSH_ENV_BOOL_ONE = "1"
    XSH_ENV_BOOL_TRUE = "true"
    XSH_ENV_BOOL_YES = "yes"
    XSH_ENV_BOOL_ON = "on"
    XSH_ENV_BOOL_MIXED = "  TRUE  "
    XSH_ENV_BOOL_ZERO = "0"
    XSH_ENV_BOOL_FALSE = "false"
    XSH_ENV_BOOL_NO = "no"
    XSH_ENV_BOOL_OFF = "off"
    XSH_ENV_BOOL_Y = "y"
    XSH_ENV_BOOL_T = "t"
    XSH_ENV_BOOL_TWO = "2"
    XSH_ENV_BOOL_EMPTY = ""
    XSH_ENV_BOOL_FRENCH = "vrai"
  } {
    # The four accepted spellings, plus one that differs only in case and
    # surrounding white space.
    test.eq(env.bool("XSH_ENV_BOOL_ONE")?, true)?
    test.eq(env.bool("XSH_ENV_BOOL_TRUE")?, true)?
    test.eq(env.bool("XSH_ENV_BOOL_YES")?, true)?
    test.eq(env.bool("XSH_ENV_BOOL_ON")?, true)?
    test.eq(env.bool("XSH_ENV_BOOL_MIXED")?, true)?

    # An unset name is the only case that yields the fallback.
    test.eq(env.bool("XSH_ENV_BOOL_ABSENT")?, false)?
    test.eq(env.bool("XSH_ENV_BOOL_ABSENT", true)?, true)?

    # Every other present value is false, not an error, and never the
    # fallback: a fallback of true is passed to prove it is not substituted.
    let rejected = [
      "XSH_ENV_BOOL_ZERO",
      "XSH_ENV_BOOL_FALSE",
      "XSH_ENV_BOOL_NO",
      "XSH_ENV_BOOL_OFF",
      "XSH_ENV_BOOL_Y",
      "XSH_ENV_BOOL_T",
      "XSH_ENV_BOOL_TWO",
      "XSH_ENV_BOOL_EMPTY",
      "XSH_ENV_BOOL_FRENCH",
    ]
    for name in rejected {
      test.eq(env.bool(name, true)?, false, f"${name} should not be true")?
    }

    test.error_kind(env.bool("", true), "env-name")?
  } ?
}

proc test_env_int_parses_the_baseline_grammar()  [env, error] {
  env {
    XSH_ENV_INT_ZERO = "0"
    XSH_ENV_INT_PLAIN = "42"
    XSH_ENV_INT_SPACED = "  42  "
    XSH_ENV_INT_TABBED = "\t7\n"
    XSH_ENV_INT_NBSP = "\u{00A0}42"
    XSH_ENV_INT_PLUS = "+42"
    XSH_ENV_INT_MINUS = "-42"
    XSH_ENV_INT_PADDED = " -00042 "
    XSH_ENV_INT_ZEROED = "007"
    XSH_ENV_INT_MANY_ZEROS = "000000000000000000000000000000000000000000042"
    XSH_ENV_INT_MAX = "9223372036854775807"
    XSH_ENV_INT_MAX_ZEROED = "0000009223372036854775807"
    XSH_ENV_INT_MIN = "-9223372036854775808"
    XSH_ENV_INT_MIN_ZEROED = "-0009223372036854775808"
  } {
    test.eq(env.int("XSH_ENV_INT_ZERO")?, 0)?
    test.eq(env.int("XSH_ENV_INT_PLAIN")?, 42)?
    test.eq(env.int("XSH_ENV_INT_SPACED")?, 42)?
    test.eq(env.int("XSH_ENV_INT_TABBED")?, 7)?
    test.eq(env.int("XSH_ENV_INT_NBSP")?, 42)?
    test.eq(env.int("XSH_ENV_INT_PLUS")?, 42)?
    test.eq(env.int("XSH_ENV_INT_MINUS")?, -42)?
    test.eq(env.int("XSH_ENV_INT_PADDED")?, -42)?
    test.eq(env.int("XSH_ENV_INT_ZEROED")?, 7)?
    test.eq(env.int("XSH_ENV_INT_MANY_ZEROS")?, 42)?
    test.eq(env.int("XSH_ENV_INT_MAX")?, 9223372036854775807)?
    test.eq(env.int("XSH_ENV_INT_MAX_ZEROED")?, 9223372036854775807)?
    # The negative bound cannot be written as a literal: the indexed IR rejects
    # the `-9223372036854775808` spelling, so it is built from its neighbour.
    test.eq(env.int("XSH_ENV_INT_MIN")?, -9223372036854775807 - 1)?
    test.eq(env.int("XSH_ENV_INT_MIN_ZEROED")?, -9223372036854775807 - 1)?

    # An unset name is the only case that yields the fallback.
    test.eq(env.int("XSH_ENV_INT_ABSENT")?, 0)?
    test.eq(env.int("XSH_ENV_INT_ABSENT", 7)?, 7)?

    test.error_kind(env.int("", 7), "env-name")?
  } ?
}

proc test_env_int_rejects_unparsable_and_out_of_range_text()  [env, error] {
  env {
    XSH_ENV_BAD_EMPTY = ""
    XSH_ENV_BAD_SPACES = "   "
    XSH_ENV_BAD_PLUS = "+"
    XSH_ENV_BAD_MINUS = "-"
    XSH_ENV_BAD_DOUBLE_SIGN = "--5"
    XSH_ENV_BAD_MIXED_SIGN = "+-5"
    XSH_ENV_BAD_UNDERSCORE = "1_000"
    XSH_ENV_BAD_HEX = "0x10"
    XSH_ENV_BAD_OCTAL = "0o10"
    XSH_ENV_BAD_FLOAT = "1.5"
    XSH_ENV_BAD_INNER_SPACE = "4 2"
    XSH_ENV_BAD_TRAILING = "42a"
    XSH_ENV_BAD_LEADING = "a42"
    XSH_ENV_BAD_UNICODE = "٤٢"
    XSH_ENV_BAD_OVER = "9223372036854775808"
    XSH_ENV_BAD_OVER_ZEROED = "09223372036854775808"
    XSH_ENV_BAD_UNDER = "-9223372036854775809"
  } {
    # Each of these is a present value, so the fallback of 7 is never used: it
    # is passed to prove that a failed conversion is reported rather than
    # silently defaulted, and that an empty value is not mistaken for an
    # unset one.
    let rejected = [
      "XSH_ENV_BAD_EMPTY",
      "XSH_ENV_BAD_SPACES",
      "XSH_ENV_BAD_PLUS",
      "XSH_ENV_BAD_MINUS",
      "XSH_ENV_BAD_DOUBLE_SIGN",
      "XSH_ENV_BAD_MIXED_SIGN",
      "XSH_ENV_BAD_UNDERSCORE",
      "XSH_ENV_BAD_HEX",
      "XSH_ENV_BAD_OCTAL",
      "XSH_ENV_BAD_FLOAT",
      "XSH_ENV_BAD_INNER_SPACE",
      "XSH_ENV_BAD_TRAILING",
      "XSH_ENV_BAD_LEADING",
      "XSH_ENV_BAD_UNICODE",
      "XSH_ENV_BAD_OVER",
      "XSH_ENV_BAD_OVER_ZEROED",
      "XSH_ENV_BAD_UNDER",
    ]
    for name in rejected {
      test.error_kind(env.int(name, 7), "env-int", f"${name} should be rejected")?
      test.eq(
        int_failure(env.int(name, 7)),
        "environment value is not an integer",
        f"${name} should report the baseline message",
      )?
    }
  } ?
}

proc test_env_conversions_read_the_scoped_overlay()  [env, error] {
  env XSH_ENV_OVERLAY=outer {
    test.eq(env.get_or("XSH_ENV_OVERLAY")?, "outer")?
    test.error_kind(env.int("XSH_ENV_OVERLAY", 7), "env-int")?

    env XSH_ENV_OVERLAY=inner XSH_ENV_OVERLAY_DIGITS=11 {
      test.eq(env.get_or("XSH_ENV_OVERLAY")?, "inner")?
      test.eq(env.int("XSH_ENV_OVERLAY_DIGITS", 7)?, 11)?
      test.eq(env.bool("XSH_ENV_OVERLAY_BOOL", true)?, true)?
      test.eq(env.get_or("XSH_ENV_OVERLAY_ABSENT", "fallback")?, "fallback")?

      env { XSH_ENV_OVERLAY_BOOL = "off" } {
        test.eq(env.bool("XSH_ENV_OVERLAY_BOOL", true)?, false)?
        test.eq(env.get_or("XSH_ENV_OVERLAY")?, "inner")?
      } ?
    } ?

    # The inner scopes are gone: the outer value is visible again, and the
    # inner-only name is unset again.
    test.eq(env.get_or("XSH_ENV_OVERLAY")?, "outer")?
    test.eq(env.int("XSH_ENV_OVERLAY_DIGITS", 7)?, 7)?
    test.eq(env.bool("XSH_ENV_OVERLAY_BOOL", false)?, false)?
  } ?
}

proc test_env_functions_and_path_list(ctx: TestContext)  [fs, process, env, error] {
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

proc test_env_overlays_blocks_lookup_and_path_mutation_affect_children(ctx: TestContext)  [fs, process, env, error] {
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

proc test_path_literals_method_sugar_and_expr_env_blocks(ctx: TestContext)  [fs, process, env, error] {
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
beta """.trim()
  .lines()
  .collect()

    test.eq(home, root)?
    test.ok("child" in env.Path.CHILD?)
    test.eq(decoded, b"abc")?
    test.eq(lines[1], "beta")?
    test.eq(b"abc".compare(b"abd").byte, 3)?
    let line = run.text sh -c "printf '%s|%s|%s' \"\$HOME\" \"\$DIGEST\" \"\$COUNT\";" ?
    test.eq(line, f"${root.display()}|ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad|3")?
  } ?
}
