proc test_process_module() [fs, process, error] {
  let current_pid = process.current_pid()?
  test.ok(current_pid > 0)?
  test.ok(process.list()? |> any .pid == current_pid, "process list should contain current pid")?
  test.ok(
    (process.list()?
      |> where .pid > 0 and .parent_pid >= 0 and .argv0 != "" and .uid >= 0 and .start_time.count_chars() == 20 and .start_time_ms > 0 and .runtime_seconds >= 0
      |> count()) > 0,
    "process list should contain typed fields",
  )?
  test.ok(process.threads(current_pid)? |> any .owner_pid == current_pid, "process threads should accept a pid")?
  test.ok(
    (process.threads()?
      |> where .pid > 0 and .owner_pid > 0 and .thread_id > 0 and .parent_pid >= 0 and .argv0 != "" and .uid >= 0 and .start_time.count_chars() == 20 and .start_time_ms > 0 and .runtime_seconds >= 0
      |> count()) > 0,
    "process threads should contain typed fields",
  )?
  let stats = process.stats(current_pid)?
  test.ok(stats.rss_kb >= 0)?
  test.ok(stats.vsz_kb >= 0)?
  test.ok(process.list()? |> any .pid > 0, "process list should contain entries")?
  test.ok(process.which("sh")?.display() != "")?
  test.ok((process.port(9)? |> count()) >= 0)?
  test.ok((process.ports()? |> count()) >= 0)?
  test.ok((process.ports(current_pid)? |> count()) >= 0)?
  test.eq(process.signal("TERM")?.name, "TERM")?
  test.eq(process.argv_words("cmd 'two words'")?, ["cmd", "two words"])?
  let command = process.command_argv("sh", ["sh", "-c", "exit 7"])
  let status = process.run(command)?
  test.ok(status.exited_with(7))?

  let ok_command = process.command {
    run true
  }

  test.ok(process.run(ok_command)?.exited_with(0))?
  let sleeper = process.command_argv("sh", ["sh", "-c", "sleep 5"])
  let child = process.spawn(sleeper)?
  test.ok(child.pid > 0)?
  process.kill(child.pid, signal: "TERM")?
  let any_handle = spawn run true ?
  let any = process.wait_any([any_handle])?
  test.eq(any.index, 0)?
  test.ok(any.pid > 0)?
  test.ok(any.status.exited_with(0))?
  let ready_one = spawn run true ?
  let ready_two = spawn run true ?
  let ready = process.wait_ready([ready_one, ready_two])?
  test.ok(ready.len() >= 1)?
  test.ok(ready[0].status.exited_with(0))?
  let handle = spawn run sh -c "sleep 5" ?
  handle.cancel(signal: "TERM", kill_after: 10ms)?
}

# The message of a rejected argument string, or the empty string when the
# string parsed. `test.error_kind` compares kinds only, so message parity is
# asserted through this.
pure argv_words_message(result: Result[List[Str]]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

# Every case the native `argv_words` unit test covered, plus whitespace runs,
# empty quoted arguments, and quote concatenation.
proc test_process_argv_words_parses_quotes_and_escapes() [process, error] {
  test.eq(
    process.argv_words("cmd 'two words' \"double quoted\" escaped\\ space 'literal *'")?,
    ["cmd", "two words", "double quoted", "escaped space", "literal *"],
  )?

  # Whitespace, including runs and leading or trailing whitespace, separates
  # words.
  test.eq(process.argv_words("one")?, ["one"])?
  test.eq(process.argv_words("plain words")?, ["plain", "words"])?
  test.eq(
    process.argv_words("""  spaced 	 out 
 lines  """)?,
    ["spaced", "out", "lines"],
  )?

  # Input with no words at all.
  let no_words: List[Str] = []
  test.eq(process.argv_words("")?, no_words)?
  test.eq(
    process.argv_words(""" 	\r
 """)?,
    no_words,
  )?

  # Explicit empty quotes keep an empty word.
  test.eq(process.argv_words("''")?, [""])?
  test.eq(process.argv_words("\"\"")?, [""])?
  test.eq(process.argv_words("''''")?, [""])?
  test.eq(process.argv_words("a '' b")?, ["a", "", "b"])?

  # Quote forms concatenate into one word.
  test.eq(process.argv_words("'a'b\"c\"")?, ["abc"])?
  test.eq(process.argv_words("'a b'c")?, ["a bc"])?
  test.eq(process.argv_words("\"a\"'b'")?, ["ab"])?

  # `\` escapes the next character outside quotes and inside double quotes.
  test.eq(process.argv_words("escaped\\ space")?, ["escaped space"])?
  test.eq(process.argv_words("quote\\'inside")?, ["quote'inside"])?
  test.eq(process.argv_words("\"a\\\"b\"")?, ["a\"b"])?
  test.eq(process.argv_words("\"a\\\\b\"")?, ["a\\b"])?
  test.eq(process.argv_words("\"\\$HOME\"")?, ["$HOME"])?
  test.eq(process.argv_words("'$HOME'")?, ["$HOME"])?

  # A quoted shell syntax character keeps its literal meaning.
  test.eq(process.argv_words("'*'")?, ["*"])?
  test.eq(process.argv_words("\"\\*\"")?, ["*"])?
  test.eq(process.argv_words("'|'")?, ["|"])?
  test.eq(process.argv_words("'`date`'")?, ["`date`"])?

  # Every ASCII byte outside the rejected set and the quote forms is an
  # ordinary word byte.
  test.eq(
    process.argv_words("!#%+,-./0123456789:=@ABCDEFGHIJKLMNOPQRSTUVWXYZ^_abcdefghijklmnopqrstuvwxyz~")?,
    ["!#%+,-./0123456789:=@ABCDEFGHIJKLMNOPQRSTUVWXYZ^_abcdefghijklmnopqrstuvwxyz~"],
  )?
}

# Every case the native `argv_words` unit test covered, plus each shell syntax
# character, the rejection messages, and unterminated input.
proc test_process_argv_words_rejects_shell_syntax() [process, error] {
  for text in [
    "echo hi | wc",
    "echo $HOME",
    "echo *",
    "echo $(date)",
    "echo `date`",
    "echo > file",
    "unterminated 'quote",
  ] {
    test.error_kind(process.argv_words(text), "argv-words")?
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
    test.error_kind(process.argv_words(f"echo ${character}"), "argv-words")?
    test.error_kind(process.argv_words(f"before${character}after"), "argv-words")?
  }

  # Escaping a shell syntax character outside quotes is rejected like an
  # unquoted one; inside double quotes `\` escapes it.
  test.error_kind(process.argv_words("echo \\*"), "argv-words")?
  test.error_kind(process.argv_words("a\\|b"), "argv-words")?
  test.error_kind(process.argv_words("echo \\$HOME"), "argv-words")?

  # `$` and `` ` `` are rejected inside double quotes too.
  test.error_kind(process.argv_words("\"$HOME\""), "argv-words")?
  test.error_kind(process.argv_words("\"`date`\""), "argv-words")?

  # Unterminated quotes and a trailing escape.
  test.error_kind(process.argv_words("unterminated 'quote"), "argv-words")?
  test.error_kind(process.argv_words("unterminated \"quote"), "argv-words")?
  test.error_kind(process.argv_words("'unterminated \""), "argv-words")?
  test.error_kind(process.argv_words("trailing\\"), "argv-words")?
  test.error_kind(process.argv_words("\"trailing\\"), "argv-words")?

  # The rejection messages name the offending character, and a multi-byte word
  # before it is sliced cleanly.
  test.eq(
    argv_words_message(process.argv_words("echo hi | wc")),
    "shell syntax character `|` is not accepted",
  )?
  test.eq(
    argv_words_message(process.argv_words("echo $HOME")),
    "shell syntax character `$` is not accepted",
  )?
  test.eq(
    argv_words_message(process.argv_words("echo `date`")),
    "shell syntax character ``` is not accepted",
  )?
  test.eq(
    argv_words_message(process.argv_words("caf\u{e9}|th\u{e9}")),
    "shell syntax character `|` is not accepted",
  )?
  test.eq(
    argv_words_message(process.argv_words("unterminated 'quote")),
    "unterminated single quote",
  )?
  test.eq(
    argv_words_message(process.argv_words("unterminated \"quote")),
    "unterminated double quote",
  )?
  test.eq(
    argv_words_message(process.argv_words("trailing\\")),
    "trailing escape",
  )?
}

# Unicode text: multi-byte characters stay inside a word, and every character
# the baseline treats as whitespace separates words.
proc test_process_argv_words_reads_unicode_text() [process, error] {
  test.eq(process.argv_words("h\u{e9}llo w\u{f6}rld")?, ["h\u{e9}llo", "w\u{f6}rld"])?
  test.eq(process.argv_words("'h\u{e9}llo w\u{f6}rld'")?, ["h\u{e9}llo w\u{f6}rld"])?
  test.eq(process.argv_words("\u{65e5}\u{672c} \u{8a9e}")?, ["\u{65e5}\u{672c}", "\u{8a9e}"])?
  test.eq(process.argv_words("a\u{e9}\u{2014}b")?, ["a\u{e9}\u{2014}b"])?

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
    test.eq(process.argv_words(f"a${space}b")?, ["a", "b"])?
  }

  let only_space = process.argv_words("\u{2003}\u{205f}")?
  test.eq(only_space.len(), 0)?

  # A multi-byte word and a multi-byte whitespace run together.
  test.eq(process.argv_words("\u{3b1}\u{3000}\u{3b2} \u{3b3}")?, ["\u{3b1}", "\u{3b2}", "\u{3b3}"])?
}

proc test_process_command_redirections(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "process-redirections")?
  let input = fp"${root}/input.txt"
  let log = fp"${root}/combined.log"
  input.write("from-stdin")?

  let command = process.command_argv(
    "sh",
    ["sh", "-c", "cat; printf stderr-line >&2"],
    stdin: input,
    stdout: log,
    stderr: log,
  )
  test.ok(process.run(command)?.exited_with(0))?
  test.eq(log.read_text()?, "from-stdinstderr-line")?

  let builder_log = fp"${root}/builder.log"
  let builder = process.command {
    stdout = builder_log
    stderr = builder_log
    run sh -c "printf builder-out; printf builder-err >&2"
  }
  test.ok(process.run(builder)?.exited_with(0))?
  test.eq(builder_log.read_text()?, "builder-outbuilder-err")?
}

proc test_process_timeout_errors() [process, error] {
  let command = process.command_argv("sh", ["sh", "-c", "sleep 1"], timeout: 10ms)
  match process.run(command) {
    Err(ProcessError.Timeout {message: message}) => test.ok("timed out" in message)?
    Err(is Timeout) => test.fail("timeout facet without nominal variant")?
    Err(error) => test.fail(f"unexpected process error: ${error.message}")?
    Ok(_) => test.fail("timed-out process succeeded")?
  }
}

proc test_process_wait_and_handle_contracts() [process, error] {
  let ok = spawn run true ?
  let ok_status = wait ok?
  let bad = spawn run false ?
  let bad_status = wait bad?
  test.ok(ok_status.ok)?
  test.ok(! bad_status.ok)?
  test.ok(bad_status.exited())?
  test.eq(bad_status.exit_code()?, 1)?

  let handle = spawn run true ?
  let status = wait handle?
  test.ok(handle.pid > 0)?
  test.eq(handle.command, "true")?
  test.eq(handle.argv[0], "true")?
  test.eq(handle.detached, false)?
  test.ok(status.ok)?

  let first = spawn run false ?
  let second = spawn run true ?
  let statuses = wait [first, second]?
  test.eq(statuses[0].exit_code()?, 1)?
  test.eq(statuses[1].exit_code()?, 0)?

  let duplicate = spawn run true ?
  match wait [duplicate, duplicate] {
    Err(ProcessError.Unknown {message: message}) => test.ok("already requested" in message)?
    Err(error) => test.fail(f"unexpected duplicate wait error: ${error.message}")?
    Ok(_) => test.fail("duplicate wait succeeded")?
  }

  let alias = spawn run true ?
  let alias_copy = alias
  let _ = wait alias?
  match wait alias_copy {
    Err(ProcessError.Unknown {message: message}) => test.ok("no longer live" in message)?
    Err(error) => test.fail(f"unexpected alias wait error: ${error.message}")?
    Ok(_) => test.fail("alias wait succeeded")?
  }
}

proc test_process_spawn_setup_errors() [process, env, error] {
  env PATH="/bin:/usr/bin" {
    match spawn run xsh-definitely-missing-command {
      Err(ProcessError.NotFound {message: message}) => test.ok("not found" in message)?
      Err(error) => test.fail(f"unexpected spawn error: ${error.message}")?
      Ok(_) => test.fail("missing command spawned")?
    }
  }

  match spawn run true > /definitely/missing/xsh-spawn-output {
    Err(ProcessError.Redirection {message: message}) => test.ok(message != "")?
    Err(error) => test.fail(f"unexpected redirection error: ${error.message}")?
    Ok(_) => test.fail("invalid redirection succeeded")?
  }
}

proc process_handle_from_proc() [process, error] -> Result[ProcessHandle] {
  return spawn run true ?
}

proc process_handle_from_record() [process, error] -> Result[Record] {
  let nested = spawn run true ?
  return {nested}
}

proc process_handle_from_ok() [process, error] -> Result[ProcessHandle] {
  let nested = spawn run true ?
  nested
}

proc process_handle_from_list() [process, error] -> Result[List[ProcessHandle]] {
  let nested = spawn run true ?
  return [nested]
}

proc test_process_spawn_timeout_and_return_transfer() [process, time, error] {
  let command = process.command_argv("sh", ["sh", "-c", "sleep 1"], timeout: 10ms)
  let handle = spawn command?
  time.sleep(50ms)?
  match wait handle {
    Err(ProcessError.Timeout {message: message}) => test.ok("timed out" in message)?
    Err(error) => test.fail(f"unexpected spawn timeout error: ${error.message}")?
    Ok(_) => test.fail("spawn timeout did not expire")?
  }

  let first = process_handle_from_proc()?
  let first_status = wait first?
  let bundle = process_handle_from_record()?
  let bundle_status = wait bundle.nested?
  let ok = process_handle_from_ok()?
  let ok_status = wait ok?
  let list = process_handle_from_list()?
  let list_status = wait list?
  test.ok(first_status.ok)?
  test.ok(bundle_status.ok)?
  test.ok(ok_status.ok)?
  test.ok(list_status[0].ok)?
}

proc test_process_spawn_traces(ctx: TestContext) [process, error] {
  let source = """
let h = spawn run sh -c "exit 7" ?
let status = wait h?
let c = spawn run sleep 1 ?
c.cancel(signal: "TERM", kill_after: 0ms)?
print \${status.exit_code()?}
"""
  let text_trace = test.run_xsht_trace(ctx, source, ["--trace", "--raw"])?
  test.ok(text_trace.success, text_trace.stderr)?
  test.eq(
    text_trace.stdout,
    """7
""",
  )?
  for kind in [
    "kind=spawn.start",
    "kind=spawn.ready",
    "kind=wait.start",
    "kind=wait.end",
    "kind=spawn.cancel",
  ] {
    test.contains(text_trace.stderr, kind)?
  }

  test.contains(text_trace.stderr, "b\"exit 7\"")?
  test.contains(text_trace.stderr, "status={kind:exit success:false code:7}")?
  test.contains(text_trace.stderr, "signal=b\"TERM\"")?
  test.contains(text_trace.stderr, "handle_id=1")?
  test.contains(text_trace.stderr, "handle_id=2")?

  let json_trace = test.run_xsht_trace(
    ctx,
    source,
    ["--trace", "--raw", "--trace-format", "jsonl"],
  )?
  test.ok(json_trace.success, json_trace.stderr)?
  test.eq(
    json_trace.stdout,
    """7
""",
  )?
  test.contains(json_trace.stderr, "\"kind\":\"spawn.start\"")?
  test.contains(json_trace.stderr, "\"kind\":\"wait.end\"")?
  test.contains(json_trace.stderr, "\"kind\":\"spawn.cancel\"")?
  test.contains(json_trace.stderr, "\"handle_id\":1")?
  test.contains(json_trace.stderr, "\"handle_id\":2")?
  test.contains(json_trace.stderr, "\"code\":7")?
}
