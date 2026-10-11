##! Transcribed from the uutils coreutils integration tests for pinky.

use support.uu as uu

type Account = {name: Str, gecos: Str, directory: Str, shell: Str}

# Expected account fields come from the system database, independently of the applet.
proc accounts() [fs, error] -> Result[List[Account], Error] {
  let entries: List[Account] = collect {
    for line in p"/etc/passwd".read_text()?.lines() {
      let fields = line.split(":")
      if fields.len() == 7 {
        yield {name: fields[0], gecos: fields[4], directory: fields[5], shell: fields[6]}
      }
    }
  }
  Ok(entries)
}

pure capitalized(name: Str) -> Str {
  if name == "" { "" } else { name.byte_slice(0, length: 1).upper() + name.byte_slice(1) }
}

pure long_output(login: Str, entries: List[Account], omit_directory: Bool = false) -> Str {
  for account in entries {
    if account.name == login {
      let real_name = account.gecos.replace("&", with: capitalized(account.name))
      let heading = f"Login name: {login:<28}In real life:  {real_name}\n"
      return if omit_directory {
        heading + "\n"
      } else {
        heading + f"Directory: {account.directory:<29}Shell:  {account.shell}\n\n"
      }
    }
  }
  f"Login name: {login:<28}In real life:  ???\n"
}

pure stable_fields(fields: List[Str]) -> List[Str] {
  [field for field in fields if field != "Idle" and !rx"^[0-9]{2}:[0-9]{2}$".matches(field)]
}

# Short-format expectations include every live session and its independently read metadata.
# A missing utmp database means no sessions; other read errors remain failures.
proc short_fields(option: Str) [fs, process, time, error] -> Result[List[Str], Error] {
  let entries = accounts()?
  let records = match unix.read_utmp() {
    Ok(found) => found,
    Err(failure) => {
      assert failure.errno == 2
      let empty: List[UnixUtmp] = []
      empty
    },
  }
  var expected = if option == "-i" {
    ["Login", "TTY", "Idle", "When"]
  } else if option == "-q" {
    ["Login", "TTY", "When"]
  } else {
    ["Login", "Name", "TTY", "Idle", "When", "Where"]
  }
  for record in records {
    continue when record.kind != "user_process" or record.user == ""
    expected += [record.user]
    if option != "-i" and option != "-q" {
      var name = "???"
      for account in entries {
        if account.name == record.user {
          name = account.gecos.split(",")[0].replace("&", with: capitalized(account.name))
          if name.byte_len() > 19 { name = name.byte_slice(0, length: 19) }
        }
      }
      expected += name.words()
    }
    let terminal = if record.line.starts_with("/") { Path(record.line) } else { fp"/dev/{record.line}" }
    var marker = "?"
    var idle = "?????"
    match fs.stat(terminal) {
      Ok(info) => {
        marker = if info.mode.bit_and(0o020) == 0 { "*" } else { "" }
        let raw_elapsed = time.now() / 1000 - info.atime_ns / 1000000000
        let elapsed = if raw_elapsed < 0 { 0 } else { raw_elapsed }
        idle = if elapsed < 60 { "" } else if elapsed < 86400 { f"{elapsed / 3600:02}:{elapsed % 3600 / 60:02}" } else { f"{elapsed / 86400}d" }
      },
      Err(failure) => { assert failure.errno == 2 },
    }
    expected += [marker + record.line]
    if option != "-q" { expected += idle.words() }
    expected += time.format(record.time_sec * 1000000000, "%b %e %H:%M", utc: true)?.words()
    if option != "-i" and option != "-q" { expected += record.host.words() }
  }
  Ok(expected)
}

# origin: uutils test_pinky::test_invalid_arg
test test_uu_pinky_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pinky", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_pinky::test_long_format
test test_uu_pinky_long_format { |ctx|
  let s = uu.scene(ctx)?
  let entries = accounts()?
  assert [account for account in entries if account.name == "root"].len() == 1
  let r = uu.invoke(s, "pinky", ["-l", "root"])?
  uu.succeeds(r)
  uu.stdout_is(r, long_output("root", entries))
  let bare = uu.invoke(s, "pinky", ["-lb", "root"])?
  uu.succeeds(bare)
  uu.stdout_is(bare, long_output("root", entries, true))
}

# origin: uutils test_pinky::test_long_format_multiple_users
test test_uu_pinky_long_format_multiple_users { |ctx|
  let s = uu.scene(ctx)?
  let runner = env.get_or("USER", "")?
  let entries = accounts()?
  let names = ["root", "root", "root", runner, "no_such_user"]
  let r = uu.invoke(s, "pinky", ["-l"].extend(names))?
  uu.succeeds(r)
  uu.stdout_is(r, [long_output(name, entries) for name in names].join(""))
  uu.stderr_is(r, "")
}

# origin: uutils test_pinky::test_long_format_wo_user
test test_uu_pinky_long_format_wo_user { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pinky", ["-l"])?
  uu.fails(r)
}

# origin: uutils test_pinky::test_lookup
test test_uu_pinky_lookup { |ctx|
  let s = uu.scene(ctx)?
  let expected = stable_fields(short_fields("")?)
  let r = uu.invoke(s, "pinky", ["--lookup"])?
  uu.succeeds(r)
  assert stable_fields(r.stdout.utf8()?.words()) == expected
}

# origin: uutils test_pinky::test_no_flag
test test_uu_pinky_no_flag { |ctx|
  let s = uu.scene(ctx)?
  let expected = stable_fields(short_fields("")?)
  let r = uu.invoke(s, "pinky", [])?
  uu.succeeds(r)
  assert stable_fields(r.stdout.utf8()?.words()) == expected
}

# origin: uutils test_pinky::test_short_format_i
test test_uu_pinky_short_format_i { |ctx|
  let s = uu.scene(ctx)?
  let expected = short_fields("-i")?
  let r = uu.invoke(s, "pinky", ["-i"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.words() == expected
}

# origin: uutils test_pinky::test_short_format_q
test test_uu_pinky_short_format_q { |ctx|
  let s = uu.scene(ctx)?
  let expected = stable_fields(short_fields("-q")?)
  let r = uu.invoke(s, "pinky", ["-q"])?
  uu.succeeds(r)
  assert stable_fields(r.stdout.utf8()?.words()) == expected
}
