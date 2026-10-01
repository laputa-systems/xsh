type ProducerCheck = {status: Status, stdout: Str, stderr: Str}

test record_constructor_fields_keep_stored_producer_permissions [fs, process, error] { |ctx|
  for arguments in ["rows: rows", "...{rows: rows}"] {
    for permissions in ["time", ""] {
      let checked = check_producer_source(ctx, f"""type Envelope = {rows: Stream[Int]}
stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }
pure wrap(rows: Stream[Int]) -> Envelope { Envelope(${arguments}) }
let retained = wrap(delayed())
proc consumed() [${permissions}] -> List[Int] { retained.rows.collect() }
""")?
      if permissions == "time" {
        assert checked.status.exited_with(0), checked.stderr
      } else {
        assert_producer_effect_rejection(checked, "time")
      }
    }
  }
  true
}

proc check_producer_source(ctx: TestContext, source: Str) [fs, process, error] -> Result[ProducerCheck] {
  let file = test.temp_file(ctx, name: "producer-contract.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file ?
  let {status, stdout, stderr, ..} = checked
  {status, stdout, stderr}
}

proc assert_producer_effect_rejection(checked: ProducerCheck, effect: Str) [error] -> Unit {
  assert checked.status.exited_with(2), checked.stderr
  assert "check.effect-violation" in checked.stderr, checked.stderr
  assert effect in checked.stderr, checked.stderr
  assert "unknown" not in checked.stderr, checked.stderr
  assert "unrestricted" not in checked.stderr, checked.stderr
  assert "parse." not in checked.stderr, checked.stderr
}

pure producer_flow_source(body: Str, effects: Str) -> Str {
  f"""stream quiet() [] -> Stream[Int] { yield 1 }
stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 2 }
let quiet_rows = quiet()
let clock_rows = clocked()
pure identity(value) { value }
pure choose(flag: Bool, left, right) { if flag { (left) } else { (right) } }
pure independent() -> Int { identity(choose(true, 7, 8)) }
proc consume(flag: Bool) [${effects}] -> Unit {
  ${body}
  for item in selected { let _ = item }
}
"""
}

test producer_creation_keeps_body_and_cleanup_latent [fs, process, error] { |ctx|
  for clause in ["", "[time, env]"] {
    let checked = check_producer_source(ctx, f"""stream delayed() ${clause} -> Stream[Int] {
  defer { let _ = env.get("UNREAD_SETTING") }
  let _ = time.now()
  yield 1
}
proc create() [] -> Unit { let _ = delayed() }
""")?
    assert checked.status.exited_with(0), checked.stderr
    checked.stdout == ""
  }
}

test producer_omitted_default_stays_latent_at_creation [fs, process, error] { |ctx|
  let checked = check_producer_source(ctx, r"""proc argument() [time] -> Int { let _ = time.now(); 1 }
stream delayed(item: Int = argument()) [time] -> Stream[Int] { yield item }
proc create() [] -> Unit { let _ = delayed() }
""")?
  assert checked.status.exited_with(0), checked.stderr
  checked.stdout == ""
}

test producer_supplied_argument_retains_eager_creation_effects [fs, process, error] { |ctx|
  let prefix = r"""proc argument() [time] -> Int { let _ = time.now(); 1 }
stream inert(item: Int) [] -> Stream[Int] { yield item }
"""
  let accepted = check_producer_source(ctx, prefix + "proc create() [time] -> Unit { let _ = inert(argument()) }\n")?
  let rejected = check_producer_source(ctx, prefix + "proc create() [] -> Unit { let _ = inert(argument()) }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test captured_producer_keeps_its_pull_effect [fs, process, error] { |ctx|
  let body = "let selected = clock_rows"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, ""))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test producer_early_consumer_retains_cleanup_effects [fs, process, error] { |ctx|
  let prefix = r"""stream delayed() [time, env] -> Stream[Int] {
  defer { let _ = env.get("UNREAD_SETTING") }
  let _ = time.now()
  yield 1
}
let rows = delayed()
"""
  let accepted = check_producer_source(ctx, prefix + "proc consume() [time, env] -> Unit { for item in rows { let _ = item; break } }\n")?
  let rejected = check_producer_source(ctx, prefix + "proc consume() [time] -> Unit { for item in rows { let _ = item; break } }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "env")
}

test generic_producer_identity_preserves_pull_through_aliases [fs, process, error] { |ctx|
  let body = "let original = identity(clock_rows)\n  let selected = original"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, ""))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test generic_producer_choice_keeps_distinct_record_and_list_profiles [fs, process, error] { |ctx|
  let body = "let boxed = {quiet: identity(quiet_rows), clocked: identity(clock_rows)}\n  let sources = [boxed.quiet, boxed.clocked]\n  let selected = choose(flag, sources[0], sources[1])"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, ""))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test generic_producer_identity_keeps_optional_payload_profile [fs, process, error] { |ctx|
  let body = "let wrapped: Stream[Int]? = identity(clock_rows)\n  let selected = wrapped ?? quiet_rows"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, ""))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test generic_producer_identity_keeps_result_payload_profile [fs, process, error] { |ctx|
  let body = "let wrapped: Result[Stream[Int]] = Ok(identity(clock_rows))\n  let selected = wrapped?"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time, error"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, "error"))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test generic_producer_callback_retains_profile_without_pulling_payload [fs, process, error] { |ctx|
  let body = "let sources = [clock_rows, quiet_rows]\n  let wrapped = sources |> map { |value| identity(value) } |> first()\n  let selected = wrapped ?? quiet_rows"
  let accepted = check_producer_source(ctx, producer_flow_source(body, "time"))?
  let rejected = check_producer_source(ctx, producer_flow_source(body, ""))?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "time")
}

test path_line_stream_creation_requires_fs [fs, process, error] { |ctx|
  let accepted = check_producer_source(ctx, "proc open(file: Path) [fs] -> Unit { let _ = file.lines() }\n")?
  let rejected = check_producer_source(ctx, "proc open(file: Path) [] -> Unit { let _ = file.lines() }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "fs")
}

test path_byte_line_stream_creation_requires_fs [fs, process, error] { |ctx|
  let accepted = check_producer_source(ctx, "proc open(file: Path) [fs] -> Unit { let _ = file.bytes_lines() }\n")?
  let rejected = check_producer_source(ctx, "proc open(file: Path) [] -> Unit { let _ = file.bytes_lines() }\n")?
  assert accepted.status.exited_with(0), accepted.stderr
  assert_producer_effect_rejection(rejected, "fs")
}
