test test_process_module {
  let current_pid = process.current_pid()?
  assert current_pid > 0
  assert process.list()? |> any .pid == current_pid, "process list should contain current pid"
  assert (process.list()?
    |> where .pid > 0 and .parent_pid >= 0 and .argv0 != "" and .uid >= 0 and .start_time.count_chars() == 20 and .start_time_ms > 0 and .runtime_seconds >= 0
    |> count()) > 0, "process list should contain typed fields"
  assert process.threads(current_pid)? |> any .owner_pid == current_pid, "process threads should accept a pid"
  assert (process.threads()?
    |> where .pid > 0 and .owner_pid > 0 and .thread_id > 0 and .parent_pid >= 0 and .argv0 != "" and .uid >= 0 and .start_time.count_chars() == 20 and .start_time_ms > 0 and .runtime_seconds >= 0
    |> count()) > 0, "process threads should contain typed fields"
  let stats = process.stats(current_pid)?
  assert stats.rss_kb >= 0
  assert stats.vsz_kb >= 0
  assert process.list()? |> any .pid > 0, "process list should contain entries"
  assert process.which("sh")?.display() != ""
  assert (process.port(9)? |> count()) >= 0
  assert (process.ports()? |> count()) >= 0
  assert (process.ports(current_pid)? |> count()) >= 0
  assert process.signal("TERM")?.name == "TERM"
  assert process.argv_words("cmd 'two words'")? == ["cmd", "two words"]
  let command = process.command_argv("sh", ["sh", "-c", "exit 7"])
  let status = process.run(command)?
  assert status.exited_with(7)

  let ok_command = process.command {
    run true
  }

  assert process.run(ok_command)?.exited_with(0)
  let sleeper = process.command_argv("sh", ["sh", "-c", "sleep 5"])
  let child = process.spawn(sleeper)?
  assert child.pid > 0
  process.kill(child.pid, signal: "TERM")
  let any_handle = spawn run true ?
  let any = process.wait_any([any_handle])?
  assert any.index == 0
  assert any.pid > 0
  assert any.status.exited_with(0)
  let ready_one = spawn run true ?
  let ready_two = spawn run true ?
  let ready = process.wait_ready([ready_one, ready_two])?
  assert ready.len() >= 1
  assert ready[0].status.exited_with(0)
  let handle = spawn run sh -c "sleep 5" ?
  handle.cancel(signal: "TERM", kill_after: 10ms)
}

test test_process_command_argv_requires_argv0 { |ctx|
  test.expect(
    ctx,
    """let command = process.command_argv("echo", [])
""",
    status: 2,
    stderr: ["check.process-argv-empty"],
  )?
}

test test_process_command_builder_rejects_invalid_run_entries_before_execution { |ctx|
  for case in [
    {entries: "", code: "check.builder-check"},
    {entries: "run true\nrun true", code: "check.builder-check"},
    {entries: "run true\ntimeout = 1s\nrun true", code: "check.builder-check"},
    {entries: "run true | run true", code: "check.builder-entry"},
    {entries: "run true > p\"out\"", code: "check.builder-entry"},
    {entries: "run.text true", code: "check.builder-entry"},
    {entries: "run.capture --text true", code: "check.builder-entry"},
    {entries: "run true ?", code: "check.builder-entry"},
  ] {
    for source in [
      "let command = process.command {\n" + case.entries + "\n}\n",
      "proc unused() [process, error] {\nlet command = process.command {\n" + case.entries + "\n}\n}\n",
    ] {
      let output = test.expect(ctx, "print \"started\"\n" + source, status: 2, stderr: [case.code])?
      assert "compact.indexed-build" not in output.stderr, output.stderr
      assert output.stdout == "", output.stdout
    }
  }
}

test test_process_command_builder_accepts_one_run_or_status_with_fields { |ctx|
  let output = test.expect(
    ctx,
    r"""let plain = process.command {
  timeout = 1s
  run true
}
let status = process.command {
  run.status true
  timeout = 1s
}
assert process.run(plain)?.exited_with(0)
assert process.run(status)?.exited_with(0)
print accepted
""",
    status: 0,
  )?
  assert output.stdout == "accepted\n", output.stdout
}

# The message of a rejected argument string, or the empty string when the
# string parsed. `test.error_kind` compares kinds only, so message parity is
# asserted through this.
pure argv_words_message(result: Result[List[Str]]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

# Every case the native `argv_words` unit test covered, plus whitespace runs,
# empty quoted arguments, and quote concatenation.
test test_process_argv_words_parses_quotes_and_escapes {
  assert process.argv_words("cmd 'two words' \"double quoted\" escaped\\ space 'literal *'")? == [
    "cmd",
    "two words",
    "double quoted",
    "escaped space",
    "literal *",
  ]

  # Whitespace, including runs and leading or trailing whitespace, separates
  # words.
  assert process.argv_words("one")? == ["one"]
  assert process.argv_words("plain words")? == ["plain", "words"]
  assert process.argv_words("""  spaced 	 out 
 lines  """)? == ["spaced", "out", "lines"]

  # Input with no words at all.
  let no_words = []
  assert process.argv_words("")? == no_words
  assert process.argv_words(""" 	\r
 """)? == no_words

  # Explicit empty quotes keep an empty word.
  assert process.argv_words("''")? == [""]
  assert process.argv_words("\"\"")? == [""]
  assert process.argv_words("''''")? == [""]
  assert process.argv_words("a '' b")? == ["a", "", "b"]

  # Quote forms concatenate into one word.
  assert process.argv_words("'a'b\"c\"")? == ["abc"]
  assert process.argv_words("'a b'c")? == ["a bc"]
  assert process.argv_words("\"a\"'b'")? == ["ab"]

  # `\` escapes the next character outside quotes and inside double quotes.
  assert process.argv_words("escaped\\ space")? == ["escaped space"]
  assert process.argv_words("quote\\'inside")? == ["quote'inside"]
  assert process.argv_words("\"a\\\"b\"")? == ["a\"b"]
  assert process.argv_words("\"a\\\\b\"")? == ["a\\b"]
  assert process.argv_words("\"\\$HOME\"")? == ["$HOME"]
  assert process.argv_words("'$HOME'")? == ["$HOME"]

  # A quoted shell syntax character keeps its literal meaning.
  assert process.argv_words("'*'")? == ["*"]
  assert process.argv_words("\"\\*\"")? == ["*"]
  assert process.argv_words("'|'")? == ["|"]
  assert process.argv_words("'`date`'")? == ["`date`"]

  # Every ASCII byte outside the rejected set and the quote forms is an
  # ordinary word byte.
  assert process.argv_words("!#%+,-./0123456789:=@ABCDEFGHIJKLMNOPQRSTUVWXYZ^_abcdefghijklmnopqrstuvwxyz~")? == [
    "!#%+,-./0123456789:=@ABCDEFGHIJKLMNOPQRSTUVWXYZ^_abcdefghijklmnopqrstuvwxyz~",
  ]
}

# Every case the native `argv_words` unit test covered, plus each shell syntax
# character, the rejection messages, and unterminated input.
test test_process_argv_words_rejects_shell_syntax {
  for text in [
    "echo hi | wc",
    "echo $HOME",
    "echo *",
    "echo $(date)",
    "echo `date`",
    "echo > file",
    "unterminated 'quote",
  ] {
    test.error_kind(process.argv_words(text), "argv-words")
  }

  # Every member of the rejected set, as its own word and inside a word.
  for character in [
    "|",
    "<",
    ">",
    ";",
    "&",
    "$",
    "`",
    "*",
    "?",
    "[",
    "]",
    "(",
    ")",
    "{",
    "}",
  ] {
    test.error_kind(process.argv_words(f"echo {character}"), "argv-words")
    test.error_kind(process.argv_words(f"before{character}after"), "argv-words")
  }

  # Escaping a shell syntax character outside quotes is rejected like an
  # unquoted one; inside double quotes `\` escapes it.
  test.error_kind(process.argv_words("echo \\*"), "argv-words")
  test.error_kind(process.argv_words("a\\|b"), "argv-words")
  test.error_kind(process.argv_words("echo \\$HOME"), "argv-words")

  # `$` and `` ` `` are rejected inside double quotes too.
  test.error_kind(process.argv_words("\"$HOME\""), "argv-words")
  test.error_kind(process.argv_words("\"`date`\""), "argv-words")

  # Unterminated quotes and a trailing escape.
  test.error_kind(process.argv_words("unterminated 'quote"), "argv-words")
  test.error_kind(process.argv_words("unterminated \"quote"), "argv-words")
  test.error_kind(process.argv_words("'unterminated \""), "argv-words")
  test.error_kind(process.argv_words("trailing\\"), "argv-words")
  test.error_kind(process.argv_words("\"trailing\\"), "argv-words")

  # The rejection messages name the offending character, and a multi-byte word
  # before it is sliced cleanly.
  assert argv_words_message(process.argv_words("echo hi | wc")) == "shell syntax character `|` is not accepted"
  assert argv_words_message(process.argv_words("echo $HOME")) == "shell syntax character `$` is not accepted"
  assert argv_words_message(process.argv_words("echo `date`")) == "shell syntax character ``` is not accepted"
  assert argv_words_message(process.argv_words("café|thé")) == "shell syntax character `|` is not accepted"
  assert argv_words_message(process.argv_words("unterminated 'quote")) == "unterminated single quote"
  assert argv_words_message(process.argv_words("unterminated \"quote")) == "unterminated double quote"
  assert argv_words_message(process.argv_words("trailing\\")) == "trailing escape"
}

# Unicode text: multi-byte characters stay inside a word, and every character
# the baseline treats as whitespace separates words.
test test_process_argv_words_reads_unicode_text {
  assert process.argv_words("héllo wörld")? == ["héllo", "wörld"]
  assert process.argv_words("'héllo wörld'")? == ["héllo wörld"]
  assert process.argv_words("日本 語")? == ["日本", "語"]
  assert process.argv_words("aé—b")? == ["aé—b"]

  for space in [
    "\u{85}",
    "\u{a0}",
    "\u{1680}",
    "\u{2000}",
    "\u{2003}",
    "\u{200a}",
    "\u{2028}",
    "\u{2029}",
    "\u{202f}",
    "\u{205f}",
    "\u{3000}",
  ] {
    assert process.argv_words(f"a{space}b")? == ["a", "b"]
  }

  let only_space = process.argv_words("\u{2003}\u{205f}")?
  assert only_space.is_empty()

  # A multi-byte word and a multi-byte whitespace run together.
  assert process.argv_words("α\u{3000}β γ")? == ["α", "β", "γ"]
}

test test_process_command_redirections { |ctx|
  let root = test.temp_dir(ctx, name: "process-redirections")?
  let input = fp"{root}/input.txt"
  let log = fp"{root}/combined.log"
  input.write("from-stdin")

  let command = process.command_argv(
    "sh",
    ["sh", "-c", "cat; printf stderr-line >&2"],
    stdin: input,
    stdout: log,
    stderr: log,
  )
  assert process.run(command)?.exited_with(0)
  assert log.read_text()? == "from-stdinstderr-line"

  let builder_log = fp"{root}/builder.log"
  let builder = process.command {
    stdout = builder_log
    stderr = builder_log
    run sh -c "printf builder-out; printf builder-err >&2"
  }
  assert process.run(builder)?.exited_with(0)
  assert builder_log.read_text()? == "builder-outbuilder-err"
}

test test_process_timeout_errors {
  let command = process.command_argv("sh", ["sh", "-c", "sleep 1"], timeout: 10ms)
  match process.run(command) {
    Err(ProcessError.Timeout {message: message}) => assert "timed out" in message
    Err(is Timeout) => test.fail("timeout facet without nominal variant")
    Err(error) => test.fail(f"unexpected process error: {error.message}")
    Ok(_) => test.fail("timed-out process succeeded")
  }
}

test test_process_wait_and_handle_contracts {
  let ok = spawn run true ?
  let ok_status = wait ok?
  let bad = spawn run false ?
  let bad_status = wait bad?
  assert ok_status.ok
  assert ! bad_status.ok
  assert bad_status.exited()
  assert bad_status.exit_code()? == 1

  let handle = spawn run true ?
  let status = wait handle?
  assert handle.pid > 0
  assert handle.command == "true"
  assert handle.argv[0] == "true"
  assert handle.detached == false
  assert status.ok

  let first = spawn run false ?
  let second = spawn run true ?
  let statuses = wait [first, second]?
  assert statuses[0].exit_code()? == 1
  assert statuses[1].exit_code()? == 0

  let duplicate = spawn run true ?
  match wait [duplicate, duplicate] {
    Err(ProcessError.Unknown {message: message}) => assert "already requested" in message
    Err(error) => test.fail(f"unexpected duplicate wait error: {error.message}")
    Ok(_) => test.fail("duplicate wait succeeded")
  }

  let alias = spawn run true ?
  let alias_copy = alias
  let _ = wait alias?
  match wait alias_copy {
    Err(ProcessError.Unknown {message: message}) => assert "no longer live" in message
    Err(error) => test.fail(f"unexpected alias wait error: {error.message}")
    Ok(_) => test.fail("alias wait succeeded")
  }
}

test test_process_spawn_setup_errors {
  env PATH="/bin:/usr/bin" {
    match spawn run xsh-definitely-missing-command {
      Err(ProcessError.NotFound {message: message}) => assert "not found" in message
      Err(error) => test.fail(f"unexpected spawn error: {error.message}")
      Ok(_) => test.fail("missing command spawned")
    }
  }

  match spawn run true > /definitely/missing/xsh-spawn-output {
    Err(ProcessError.Redirection {message: message}) => assert message != ""
    Err(error) => test.fail(f"unexpected redirection error: {error.message}")
    Ok(_) => test.fail("invalid redirection succeeded")
  }
}

proc process_handle_from_proc() [process, error] -> Result[ProcessHandle] {
  spawn run true ?
}

proc process_handle_from_record() [process, error] -> Result[Record] {
  let nested = spawn run true ?
  {nested}
}

proc process_handle_from_ok() [process, error] -> Result[ProcessHandle] {
  let nested = spawn run true ?
  nested
}

proc process_handle_from_list() [process, error] -> Result[List[ProcessHandle]] {
  let nested = spawn run true ?
  [nested]
}

test test_process_spawn_timeout_and_return_transfer {
  let command = process.command_argv("sh", ["sh", "-c", "sleep 1"], timeout: 10ms)
  let handle = spawn command?
  time.sleep(50ms)
  match wait handle {
    Err(ProcessError.Timeout {message: message}) => assert "timed out" in message
    Err(error) => test.fail(f"unexpected spawn timeout error: {error.message}")
    Ok(_) => test.fail("spawn timeout did not expire")
  }

  let first = process_handle_from_proc()?
  let first_status = wait first?
  let bundle = process_handle_from_record()?
  let bundle_status = wait bundle.nested?
  let ok = process_handle_from_ok()?
  let ok_status = wait ok?
  let list = process_handle_from_list()?
  let list_status = wait list?
  assert first_status.ok
  assert bundle_status.ok
  assert ok_status.ok
  assert list_status[0].ok
}

test test_process_spawn_traces { |ctx|
  let source = """\nlet h = spawn run sh -c "exit 7" ?
let status = wait h?
let c = spawn run sleep 1 ?
c.cancel(signal: "TERM", kill_after: 0ms)?
print \${status.exit_code()?}
"""
  let text_trace = test.run_xsht_trace(ctx, source, ["--trace", "--raw"])?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = text_trace
    assert assertion_condition, assertion_message
  }
  assert text_trace.stdout == """7
"""
  for kind in [
    "kind=spawn.start",
    "kind=spawn.ready",
    "kind=wait.start",
    "kind=wait.end",
    "kind=spawn.cancel",
  ] {
    assert kind in text_trace.stderr
  }

  assert "b\"exit 7\"" in text_trace.stderr
  assert "status={kind:exit success:false code:7}" in text_trace.stderr
  assert "signal=b\"TERM\"" in text_trace.stderr
  assert "handle_id=1" in text_trace.stderr
  assert "handle_id=2" in text_trace.stderr

  let json_trace = test.run_xsht_trace(
    ctx,
    source,
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = json_trace
    assert assertion_condition, assertion_message
  }
  assert json_trace.stdout == """7
"""
  assert "\"kind\":\"spawn.start\"" in json_trace.stderr
  assert "\"kind\":\"wait.end\"" in json_trace.stderr
  assert "\"kind\":\"spawn.cancel\"" in json_trace.stderr
  assert "\"handle_id\":1" in json_trace.stderr
  assert "\"handle_id\":2" in json_trace.stderr
  assert "\"code\":7" in json_trace.stderr
}

test test_bytes_stdin_redirection_is_exact_and_explicit {
  let payload = b"a\0\xff\n"
  let echoed = run.bytes cat < $payload
  assert echoed == payload
  assert (run.bytes cat < b"")? == b""
  let text = "text without a newline"
  assert (run.text cat < bytes.from_text(text))? == text
}

test test_bytes_stdin_rejects_invalid_targets_and_sources { |ctx|
  for source in [
    """run cat > b"output"
""",
    """run cat < b"first" < b"second"
""",
    """run cat | run cat < b"second"
""",
    """let payload: Result[Bytes] = Ok(b"input")
run cat < (payload)
""",
    """let command = process.command {stdout = b"output"; run cat}
""",
  ] {
    let checked = test.run_xsh(ctx, source)?
    {
      let assertion_actual = checked.status
      let assertion_expected = 2
      let assertion_message = checked.stderr
      assert assertion_actual == assertion_expected, assertion_message
    }
  }
}

test test_bytes_stdin_path_strings_and_once_only_expression { |ctx|
  let root = test.temp_dir(ctx, name: "bytes-stdin-path")?
  let input = fp"{root}/input"
  input.write("file content")
  let file_name = input.display()
  assert run.text cat < $file_name? == "file content"
  let result = test.run_script(
    ctx,
    r"""proc payload() [io] -> Bytes {print preparing; return b"content"}
let copied = run.bytes cat < (payload()) ?
print ${copied.utf8()?}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """preparing
content
"""
}

test test_bytes_stdin_trace_does_not_include_payload { |ctx|
  let result = test.run_xsht_trace(
    ctx,
    r"""let copied = run.bytes cat < b"private-input-payload" ?
print ${copied.len()}
""",
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "private-input-payload" not in result.stderr
    let assertion_message = result.stderr
    assert assertion_condition, assertion_message
  }
}

test test_bytes_stdin_empty_payload_closes_input_without_reading_inherited_stdin { |ctx|
  let output = test.expect(
    ctx,
    r"""let empty = run.bytes cat < b""
print ${empty.len()}
io.write_stdout_bytes(io.stdin_bytes()?)?
""",
    status: 0,
    stdin: b"inherited",
  )?
  assert output.stdout == "0\ninherited"
}

test test_bytes_stdin_reaches_command_plans_spawned_commands_and_streams { |ctx|
  let source = r"""let payload = b"cmd\0\xff"
let command = process.command { stdin = payload; run cat }
let _ = process.run(command)?
let explicit = process.command_argv("cat", ["cat"], stdin: payload)
let handle = spawn explicit?
let status = wait handle?
if status.ok == false { error.fail("command failed")? }
let streamed = run.stream --bytes cat < b"stream\n"
for chunk in streamed { io.write_stdout_bytes(chunk)? }
"""
  let script = test.temp_file(ctx, name: "bytes-stdin-routes.xsh", contents: bytes.from_text(source))?
  let output = run.capture --bytes ${ctx.xsh_bin} $script
  assert output.status.ok, output.stderr as Str
  assert output.stdout == b"cmd\0\xffcmd\0\xffstream\n"
}

# Starts a child that never reads the megabyte offered on its standard input
# and lets the handle go out of scope.
proc abandon_unread_input() [process, error] -> Int {
  let payload = bytes.zero(1048576)?
  let handle = spawn run sleep 30 < $payload ?
  handle.pid
}

test test_bytes_stdin_scope_cleanup_reaps_a_child_that_never_read_its_input {
  let pid = abandon_unread_input()
  test.error_kind(process.kill(pid, signal: "0"), "process-missing")
}

test test_argv_words_become_a_command_that_runs_with_those_arguments { |ctx|
  let output = test.expect(
    ctx,
    r"""let words = process.argv_words("sh -c 'printf \"<%s>\\n\" \"$@\"' ignored 'two words' escaped\\ space")?
let status = process.run(process.command_argv("sh", words))?
let env_command = process.command_argv("printenv", ["printenv", "XSH_PLAN"], Path("."), {XSH_PLAN: "ready"})
let env_status = process.run(env_command)?
let false_status = process.run(process.command_argv("false", ["false"]))?
print ${status.ok} ${env_status.ok} ${false_status.exited_with(1)}
""",
    status: 0,
  )?
  assert output.stdout == "<two words>\n<escaped space>\nready\ntrue true true\n"
}

# Polls until `marker` exists, for at most three seconds.
proc appears(marker: Path) [fs, time, error] -> Bool {
  var tries = 0
  while ! marker.exists() and tries < 300 {
    time.sleep(10ms)
    tries += 1
  }

  marker.exists()?
}

# A child that would create `marker` after 300 ms unless it is stopped first.
proc slow_toucher(marker: Path) [] -> Command {
  process.command_argv("sh", ["sh", "-c", r"sleep 0.3; touch $1", "sh", marker])
}

test test_process_handle_cancel_stops_the_child { |ctx|
  let root = test.temp_dir(ctx, name: "spawn-cancel")?
  let marker = fp"{root}/marker"
  let handle = spawn slow_toucher(marker)?
  handle.cancel(kill_after: 0ms)
  time.sleep(600ms)
  assert ! marker.exists()?
}

proc spawn_detached_and_drop(marker: Path) [process, error] {
  let command = process.command_argv(
    "sh",
    ["sh", "-c", r"sleep 0.1; touch $1", "sh", marker],
    detach: true,
  )
  let _ = spawn command?
}

test test_dropped_detached_process_keeps_running { |ctx|
  let root = test.temp_dir(ctx, name: "spawn-detached")?
  let marker = fp"{root}/marker"
  spawn_detached_and_drop(marker)
  assert appears(marker)
}

test test_process_spawn_options_and_kill_are_observable { |ctx|
  let root = test.temp_dir(ctx, name: "spawn-options")?
  let marker = fp"{root}/ready"
  let command = process.command {
    detach = true
    new_session = true
    ignore_hup = true
    run sh -c "printf ready > $1; exec sleep 10" sh $marker
  }

  let spawned = process.spawn(command)?
  assert appears(marker)
  process.kill(spawned.pid, signal: "TERM")
  assert spawned.detach
  assert spawned.new_session
  assert spawned.ignore_hup
  test.error_kind(process.kill(2147483647, signal: "0"), "process-missing")
}

# A scope cancels and reaps the non-detached children it still owns before its
# deferred actions run: each child here would write its marker after 300 ms,
# and the action looks 600 ms later, past the moment a surviving child would
# have written it.
test test_dropped_process_handle_is_cancelled_before_the_scope_defers_run { |ctx|
  let root = test.temp_dir(ctx, name: "dropped-handle")?
  let source = r"""proc observe(marker: Path) [fs, time, error] -> Result[Unit] {
  time.sleep(600ms)?
  print ${marker.exists()? == false}
  return Ok()
}

proc scoped(marker: Path) [process, fs, time, error] -> Result[Unit] {
  let command = process.command_argv("sh", ["sh", "-c", f"sleep 0.3; touch {marker}"])
  let h = spawn command?
  defer observe(marker)
  return Ok()
}

proc in_block(marker: Path) [process, fs, time, error] -> Result[Unit] {
  if true {
    let command = process.command_argv("sh", ["sh", "-c", f"sleep 0.3; touch {marker}"])
    let h = spawn command?
    defer observe(marker)
  }

  return Ok()
}

scoped(Path("ROOT/call"))?
in_block(Path("ROOT/block"))?
"""
  test.expect(ctx, source.replace("ROOT", with: root.display()), status: 0, stdout: ["true\ntrue\n"])?
}
