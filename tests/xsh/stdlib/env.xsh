# The message of a failed string lookup, or the empty string when it succeeded.
# `test.error_kind` compares kinds only, so message parity is asserted through
# this.
pure str_failure(result: Result[Str]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

# The message of a failed integer lookup, or the empty string on success.
pure int_failure(result: Result[Int]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

test test_env_get_or_yields_the_fallback_only_for_an_unset_name {
  env ({
    XSH_ENV_EMPTY: "",
    XSH_ENV_TEXT: "value",
    XSH_ENV_SPACED: "  keep  ",
    XSH_ENV_UNICODE: "h\u{e9}llo",
  }) {
    # An unset name yields the fallback, defaulted or explicit, and the
    # fallback is not evaluated when the name is set.
    assert env.get_or("XSH_ENV_ABSENT")? == ""
    assert env.get_or("XSH_ENV_ABSENT", "fallback")? == "fallback"
    assert env.get_or("XSH_ENV_ABSENT", "")? == ""

    # A present name yields its own value, byte for byte: no trimming, no
    # decoding, and no substitution of the fallback for an empty value.
    assert env.get_or("XSH_ENV_EMPTY", "fallback")? == ""
    assert env.get_or("XSH_ENV_TEXT", "fallback")? == "value"
    assert env.get_or("XSH_ENV_SPACED", "fallback")? == "  keep  "
    assert env.get_or("XSH_ENV_UNICODE", "fallback")? == "h\u{e9}llo"

    # Key validation is the native one and is not a fallback case.
    test.error_kind(env.get_or(""), "env-name")
    assert str_failure(env.get_or("")) == "environment names cannot be empty or contain NUL or `=`"
    test.error_kind(env.get_or("=bad", "fallback"), "env-name")
    test.error_kind(env.get("XSH_ENV=A"), "env-name")
  } ?
}

test test_env_bool_accepts_only_the_baseline_spellings {
  env ({
    XSH_ENV_BOOL_ONE: "1",
    XSH_ENV_BOOL_TRUE: "true",
    XSH_ENV_BOOL_YES: "yes",
    XSH_ENV_BOOL_ON: "on",
    XSH_ENV_BOOL_MIXED: "  TRUE  ",
    XSH_ENV_BOOL_ZERO: "0",
    XSH_ENV_BOOL_FALSE: "false",
    XSH_ENV_BOOL_NO: "no",
    XSH_ENV_BOOL_OFF: "off",
    XSH_ENV_BOOL_Y: "y",
    XSH_ENV_BOOL_T: "t",
    XSH_ENV_BOOL_TWO: "2",
    XSH_ENV_BOOL_EMPTY: "",
    XSH_ENV_BOOL_FRENCH: "vrai",
  }) {
    # The four accepted spellings, plus one that differs only in case and
    # surrounding white space.
    assert env.bool("XSH_ENV_BOOL_ONE")? == true
    assert env.bool("XSH_ENV_BOOL_TRUE")? == true
    assert env.bool("XSH_ENV_BOOL_YES")? == true
    assert env.bool("XSH_ENV_BOOL_ON")? == true
    assert env.bool("XSH_ENV_BOOL_MIXED")? == true

    # An unset name is the only case that yields the fallback.
    assert env.bool("XSH_ENV_BOOL_ABSENT")? == false
    assert env.bool("XSH_ENV_BOOL_ABSENT", true)? == true

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
      {
        let assertion_actual = env.bool(name, true)?
        let assertion_expected = false
        let assertion_message = f"{name} should not be true"
        assert assertion_actual == assertion_expected, assertion_message
      }
    }

    test.error_kind(env.bool("", true), "env-name")
  } ?
}

test test_env_int_parses_the_baseline_grammar {
  env ({
    XSH_ENV_INT_ZERO: "0",
    XSH_ENV_INT_PLAIN: "42",
    XSH_ENV_INT_SPACED: "  42  ",
    XSH_ENV_INT_TABBED: """	7
""",
    XSH_ENV_INT_NBSP: "\u{a0}42",
    XSH_ENV_INT_PLUS: "+42",
    XSH_ENV_INT_MINUS: "-42",
    XSH_ENV_INT_PADDED: " -00042 ",
    XSH_ENV_INT_ZEROED: "007",
    XSH_ENV_INT_MANY_ZEROS: "000000000000000000000000000000000000000000042",
    XSH_ENV_INT_MAX: "9223372036854775807",
    XSH_ENV_INT_MAX_ZEROED: "0000009223372036854775807",
    XSH_ENV_INT_MIN: "-9223372036854775808",
    XSH_ENV_INT_MIN_ZEROED: "-0009223372036854775808",
  }) {
    assert env.int("XSH_ENV_INT_ZERO")? == 0
    assert env.int("XSH_ENV_INT_PLAIN")? == 42
    assert env.int("XSH_ENV_INT_SPACED")? == 42
    assert env.int("XSH_ENV_INT_TABBED")? == 7
    assert env.int("XSH_ENV_INT_NBSP")? == 42
    assert env.int("XSH_ENV_INT_PLUS")? == 42
    assert env.int("XSH_ENV_INT_MINUS")? == -42
    assert env.int("XSH_ENV_INT_PADDED")? == -42
    assert env.int("XSH_ENV_INT_ZEROED")? == 7
    assert env.int("XSH_ENV_INT_MANY_ZEROS")? == 42
    assert env.int("XSH_ENV_INT_MAX")? == 9223372036854775807
    assert env.int("XSH_ENV_INT_MAX_ZEROED")? == 9223372036854775807

    # The negative bound cannot be written as a literal: the indexed IR rejects
    # the `-9223372036854775808` spelling, so it is built from its neighbour.
    assert env.int("XSH_ENV_INT_MIN")? == -9223372036854775807 - 1
    assert env.int("XSH_ENV_INT_MIN_ZEROED")? == -9223372036854775807 - 1

    # An unset name is the only case that yields the fallback.
    assert env.int("XSH_ENV_INT_ABSENT")? == 0
    assert env.int("XSH_ENV_INT_ABSENT", 7)? == 7

    test.error_kind(env.int("", 7), "env-name")
  } ?
}

test test_env_int_rejects_unparsable_and_out_of_range_text {
  env ({
    XSH_ENV_BAD_EMPTY: "",
    XSH_ENV_BAD_SPACES: "   ",
    XSH_ENV_BAD_PLUS: "+",
    XSH_ENV_BAD_MINUS: "-",
    XSH_ENV_BAD_DOUBLE_SIGN: "--5",
    XSH_ENV_BAD_MIXED_SIGN: "+-5",
    XSH_ENV_BAD_UNDERSCORE: "1_000",
    XSH_ENV_BAD_HEX: "0x10",
    XSH_ENV_BAD_OCTAL: "0o10",
    XSH_ENV_BAD_FLOAT: "1.5",
    XSH_ENV_BAD_INNER_SPACE: "4 2",
    XSH_ENV_BAD_TRAILING: "42a",
    XSH_ENV_BAD_LEADING: "a42",
    XSH_ENV_BAD_UNICODE: "\u{664}\u{662}",
    XSH_ENV_BAD_OVER: "9223372036854775808",
    XSH_ENV_BAD_OVER_ZEROED: "09223372036854775808",
    XSH_ENV_BAD_UNDER: "-9223372036854775809",
  }) {
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
      test.error_kind(env.int(name, 7), "env-int", f"{name} should be rejected")
      {
        let assertion_actual = int_failure(env.int(name, 7))
        let assertion_expected = "environment value is not an integer"
        let assertion_message = f"{name} should report the baseline message"
        assert assertion_actual == assertion_expected, assertion_message
      }
    }
  } ?
}

test test_env_conversions_read_the_scoped_overlay {
  env XSH_ENV_OVERLAY=outer {
    assert env.get_or("XSH_ENV_OVERLAY")? == "outer"
    test.error_kind(env.int("XSH_ENV_OVERLAY", 7), "env-int")

    env XSH_ENV_OVERLAY=inner XSH_ENV_OVERLAY_DIGITS=11 {
      assert env.get_or("XSH_ENV_OVERLAY")? == "inner"
      assert env.int("XSH_ENV_OVERLAY_DIGITS", 7)? == 11
      assert env.bool("XSH_ENV_OVERLAY_BOOL", true)? == true
      assert env.get_or("XSH_ENV_OVERLAY_ABSENT", "fallback")? == "fallback"

      env ({XSH_ENV_OVERLAY_BOOL: "off"}) {
        assert env.bool("XSH_ENV_OVERLAY_BOOL", true)? == false
        assert env.get_or("XSH_ENV_OVERLAY")? == "inner"
      }
    }

    # The inner scopes are gone: the outer value is visible again, and the
    # inner-only name is unset again.
    assert env.get_or("XSH_ENV_OVERLAY")? == "outer"
    assert env.int("XSH_ENV_OVERLAY_DIGITS", 7)? == 7
    assert env.bool("XSH_ENV_OVERLAY_BOOL", false)? == false
  }
}

test test_env_functions_and_path_list { |ctx|
  let root = test.temp_dir(ctx, name: "env")?
  let tool_dir = fp"{root}/bin"
  tool_dir.mkdir()
  let tool = fp"{tool_dir}/xsh-env-helper"

  tool.write(
    """#!/bin/sh
printf '%s|%s|%s' "$XSH_STDLIB_ENV" "$DESTDIR" "$PATH"
""",
    mode: 0o755,
  )

  env XSH_STDLIB_ENV=yes DESTDIR=/tmp/xsh-stdlib-env XSH_STDLIB_COUNT=7 XSH_STDLIB_BOOL=true XSH_STDLIB_PATH=$root {
    assert e"XSH_STDLIB_ENV"? == "yes"
    assert env.get_or("XSH_STDLIB_MISSING", "fallback")? == "fallback"
    assert env.bool("XSH_STDLIB_BOOL", false)? == true
    assert env.bool("XSH_STDLIB_MISSING_BOOL")? == false
    assert env.int("XSH_STDLIB_COUNT", 0)? == 7
    assert env.int("XSH_STDLIB_MISSING_INT")? == 0
    assert env.path("XSH_STDLIB_PATH")? == root
    assert env.path("XSH_STDLIB_MISSING_PATH", root)? == root
    assert env.list()? |> any .name == "DESTDIR" and .value == "/tmp/xsh-stdlib-env"
    env.PATH.prepend(tool_dir)
    assert tool_dir in env.path_list("PATH")?
    assert tool_dir in env.path_list("PATH")?
    let path_entries = env.path_entries("PATH")?
    assert path_entries |> any .raw == tool_dir.display() and .path == tool_dir and ! .empty
    let extra_dir = fp"{tool_dir}/extra"
    env.PATH.append(extra_dir)
    assert env.PATH.pop()? == extra_dir
    assert env.Path.XSH_STDLIB_PATH? == root
    assert e"DESTDIR"? == "/tmp/xsh-stdlib-env"
    let output = run.text xsh-env-helper
    assert "yes|/tmp/xsh-stdlib-env|" in output
  }

  env XSH_STDLIB_CUSTOM_PATH=f":{tool_dir}::" {
    let entries = env.path_entries("XSH_STDLIB_CUSTOM_PATH")?
    assert entries.len() == 4
    assert entries[0].empty
    assert entries[1].path == tool_dir
    assert entries[2].empty
    assert entries[3].empty
  }
}

test test_env_overlays_blocks_lookup_and_path_mutation_affect_children { |ctx|
  let root = test.temp_dir(ctx, name: "env-scope")?
  let tool = fp"{root}/env-scope-tool"

  tool.write(
    """#!/bin/sh
printf '%s|%s|%s|%s' "$CC" "$CFLAGS" "$DESTDIR" "$XSH_ENV_SCOPE"
""",
    mode: 0o755,
  )
  env.PATH.append(root)
  assert root in env.PATH

  env XSH_ENV_SCOPE=block DESTDIR=/tmp/xsh-env-scope HOME=$root {
    let dest = e"DESTDIR"?
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
    assert dest == "/tmp/xsh-env-scope"
    assert dest_path == "/tmp/xsh-env-scope"
    assert empty == ""
    assert default_bool == false
    assert default_count == 0
    assert home == root
    assert root in path_list
    assert entries |> any .name == "DESTDIR" and .value == "/tmp/xsh-env-scope"
    assert fallback == "fallback"
    assert truthy == false
    assert count == 7
    assert fallback_path == root
    let line = run.text CC=cc CFLAGS="-O2 -pipe" env-scope-tool
    assert line == "cc|-O2 -pipe|/tmp/xsh-env-scope|block"
  }

  let removed_path = env.PATH.pop()?
  assert removed_path == root
  assert root not in env.PATH
}

test test_path_literals_method_sugar_and_expr_env_blocks { |ctx|
  let root = test.temp_dir(ctx, name: "sugar")?
  let child_name = "child"
  let child = fp"{root}/{child_name}"
  root.mkdir()

  env ({
    HOME: root,
    CHILD: child,
    DIGEST: b"abc".sha256().hex(),
    ENCODED: b"abc".base64(),
    COUNT: 3,
  }) {
    let home = env.Path.HOME?
    let encoded = e"ENCODED"?
    let decoded = encoded.base64_decode()?

    let lines = """ alpha
beta """.trim()
  .lines()
  .collect()

    assert home == root
    assert "child" in env.Path.CHILD?
    assert decoded == b"abc"
    assert lines[1] == "beta"
    assert b"abc".compare(b"abd").byte == 3
    let line = run.text sh -c "printf '%s|%s|%s' \"\$HOME\" \"\$DIGEST\" \"\$COUNT\";"
    assert line == f"{root}|ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad|3"
  }?
}

test test_env_path_membership_matches_exact_entries {
  env PATH="/opt/xsh-membership/bin:/opt/xsh-other" {
    assert /opt/xsh-membership/bin in env.PATH
    assert /opt/xsh-other in env.PATH
    assert /opt/xsh-membership not in env.PATH
    assert /opt/xsh-membership/bin/tool not in env.PATH
  }
}

# A string literal is a path here; `path_sinks_argv.xsh` covers that. Text
# that is not a literal never converts.
test test_env_path_rejects_text_that_is_not_a_literal { |ctx|
  for statement in [
    "let found = dir in env.PATH",
    "env.PATH.append(dir)",
    "env.PATH.prepend(dir)",
  ] {
    let rejected = test.run_script(
      ctx,
      """let dir = "/opt/xsh-literal"
env PATH=/opt/xsh-literal {
  """ + statement + """\n}
""",
    )?
    assert rejected.status == 2, statement
    assert "Path" in rejected.stderr, rejected.stderr
  }
}

test test_env_string_reads_exactly_like_env_str {
  env XSH_ESTR_TEXT="  keep  " XSH_ESTR_EMPTY="" {
    # Computed names keep `env.get` from being rewritten to the form under
    # test.
    let text_name = "XSH_ESTR_TEXT"
    let absent_name = "XSH_ESTR_ABSENT"
    assert e"XSH_ESTR_TEXT"? == "  keep  "
    assert e"XSH_ESTR_TEXT"? == env.get(text_name)?

    # An empty value is present, so `??` keeps it.
    assert (e"XSH_ESTR_EMPTY" ?? "fallback") == ""

    # A missing name fails with the lookup's own error, so `??`, `match`, and
    # `test.error_kind` see what `env.Str.NAME` produces.
    assert (e"XSH_ESTR_ABSENT" ?? "fallback") == "fallback"
    test.error_kind(e"XSH_ESTR_ABSENT", "env-missing")
    assert str_failure(e"XSH_ESTR_ABSENT") == str_failure(env.get(absent_name))
    let described = match e"XSH_ESTR_ABSENT" {
      Ok(value) => f"set to {value}",
      Err(error) => error.message,
    }
    assert described == "environment value is unset"

    # An e-string interpolates inside an f-string like any other expression.
    assert f"[{e"XSH_ESTR_TEXT"?.trim()}]" == "[keep]"
  }
}

test test_env_string_assignment_converts_like_an_overlay_value {
  env XSH_ESTR_SCOPE=outer {
    e"XSH_ESTR_WORD" = "hello world"
    e"XSH_ESTR_PORT" = 8080
    e"XSH_ESTR_ON" = true
    e"XSH_ESTR_GRACE" = 90s
    e"XSH_ESTR_DIR" = /tmp/xsh-estr
    let count = 3
    e"XSH_ESTR_COUNT" = count + 1
    assert e"XSH_ESTR_WORD"? == "hello world"
    assert e"XSH_ESTR_PORT"? == "8080"
    assert e"XSH_ESTR_ON"? == "true"
    assert e"XSH_ESTR_GRACE"? == "90s"
    assert env.Path.XSH_ESTR_DIR? == /tmp/xsh-estr
    assert e"XSH_ESTR_COUNT"? == "4"

    # Reassignment replaces the value, including one an overlay set.
    e"XSH_ESTR_SCOPE" = "replaced"
    assert e"XSH_ESTR_SCOPE"? == "replaced"
  }
}

proc export_estr_marker(value: Str) [env] {
  e"XSH_ESTR_MARKER" = value
}

test test_env_string_assignment_lasts_until_the_enclosing_env_scope_ends { |ctx|
  let dir = test.temp_dir(ctx, name: "estr-scope")?
  env XSH_ESTR_OUTER=1 {
    env XSH_ESTR_INNER=1 {
      e"XSH_ESTR_NESTED" = "inner"
      assert e"XSH_ESTR_NESTED"? == "inner"
    }
    # The inner scope restored the environment it started with.
    test.error_kind(e"XSH_ESTR_NESTED", "env-missing")

    # A `cd` scope restores only the directory, so the assignment outlives it.
    cd $dir {
      e"XSH_ESTR_FROM_CD" = "kept"
    }
    assert e"XSH_ESTR_FROM_CD"? == "kept"

    # The environment is evaluator state, not a binding: a proc's assignment
    # is visible to its caller.
    export_estr_marker("from proc")
    assert e"XSH_ESTR_MARKER"? == "from proc"
  }
  test.error_kind(e"XSH_ESTR_FROM_CD", "env-missing")
  test.error_kind(e"XSH_ESTR_MARKER", "env-missing")
}

test test_env_string_assignment_reaches_child_processes {
  env XSH_ESTR_CHILD_SCOPE=1 {
    e"XSH_ESTR_CHILD" = "seen by child"
    let said = run.text printenv XSH_ESTR_CHILD
    assert said == "seen by child\n"

    # A per-command `NAME=value` word still overrides it for that child only.
    let overridden = run.text XSH_ESTR_CHILD=override printenv XSH_ESTR_CHILD
    assert overridden == "override\n"
    assert e"XSH_ESTR_CHILD"? == "seen by child"
  }
}

test test_env_string_keeps_non_utf8_bytes_for_paths_and_children {
  let raw = b"/tmp/xsh-estr-\xff" as Path
  env XSH_ESTR_RAW_SCOPE=1 {
    e"XSH_ESTR_RAW" = raw
    # The text read fails rather than decoding lossily; the path read and the
    # child keep the bytes.
    test.error_kind(e"XSH_ESTR_RAW", "invalid-utf8")
    assert (e"XSH_ESTR_RAW" ?? "fallback") == "fallback"
    assert env.Path.XSH_ESTR_RAW? == raw
    let child = run.bytes printenv XSH_ESTR_RAW
    assert child == b"/tmp/xsh-estr-\xff\n"
  }
}

test test_env_string_rejections { |ctx|
  for case in [
    {source: "let x = e\"{prefix}_HOME\"\n", code: "parse.env-string-name"},
    {source: "let x = e\"NOT-IDENT\"\n", code: "parse.env-string-name"},
    {source: "let x = e\"\"\n", code: "parse.env-string-name"},
    {source: "e\"X\" += \"y\"\n", code: "check.assign-target"},
    {source: "e\"X\" = null\n", code: "check.env-value"},
    {source: "e\"X\" = [\"a\"]\n", code: "check.env-value"},
    {source: "pure read() -> Str { e\"HOME\" ?? \"\" }\nlet home = read()\n", code: "check.pure-effect"},
    {source: "pure write() { e\"X\" = \"y\" }\nwrite()\n", code: "check.pure-effect"},
    {source: "proc write() [fs] { e\"X\" = \"y\" }\nwrite()\n", code: "check.effect-violation"},
  ] {
    let rejected = test.run_script(ctx, case.source)?
    assert rejected.status == 2, case.source
    assert case.code in rejected.stderr, rejected.stderr
  }
}

test test_env_string_value_with_nul_fails_at_runtime { |ctx|
  let _ = test.expect(ctx, "e\"XSH_ESTR_NUL\" = \"a\\0b\"\n", status: 3, stderr: ["NUL"])?
}

# A script that reads a variable as text stops with exit status 3 when the
# value its parent handed it is not UTF-8, through either spelling of the read.
test test_env_text_reads_of_non_utf8_values_stop_a_script { |ctx|
  let raw = b"\xff" as Path
  for source in ["use env\nlet _ = env.get(\"XSH_BAD_UTF8\")?\n", "let _ = env(\"XSH_BAD_UTF8\") ?\n"] {
    let script = test.temp_file(ctx, name: "env-invalid-utf8.xsh", contents: bytes.from_text(source))?
    let output = run.capture --text XSH_BAD_UTF8=$raw "xsh" $script
    assert output.status.exited_with(3), output.stderr
    assert "invalid-utf8" in output.stderr, output.stderr
  }
}
