type Ran = {status: Int, stdout: Str, stderr: Str}

const INODE = rx"[0-9]{7,}"

proc run_unshare(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "lsns")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/unshare.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

# Runs lsns inside a new user namespace, where the kernel shows it nothing
# outside that namespace, so the listing holds exactly its own process.
proc lsns_in_userns(ctx: TestContext, namespaces: List[Str], args: List[Str]) [fs, process, error] -> Result[Ran] {
  let lsns_script = fp"{ctx.core_dir}/lsns.xsh"
  run_unshare(ctx, ["-r", @namespaces, ctx.xsh_bin.display(), lsns_script.display(), "--", @args])
}

proc lsns_here(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "lsns-here")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/lsns.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err, timeout: 20s)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc require_user_namespaces(ctx: TestContext) [fs, process, error] -> Result[Bool] {
  let probe = run_unshare(ctx, ["-r", "true"])?
  if probe.status != 0 {
    test.skip(f"the kernel refuses unprivileged user namespaces: {probe.stderr.trim()}")
    return false
  }
  true
}

proc root_name() [fs, error] -> Str {
  if let Ok(account) = user.by_uid(0) { account.name } else { "0" }
}

test test_lsns_lists_the_namespaces_of_the_calling_process { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let ran = lsns_in_userns(ctx, [], ["-o", "NS,TYPE,NPROCS,UID,USER"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == "        NS TYPE   NPROCS UID USER"
  let types = [fields.fields()[1] for fields in lines[1..]]
  let expected = ["mnt", "uts", "ipc", "pid", "cgroup", "user", "net", "time"]
  for name in types {
    assert name in expected, f"unexpected type {name}"
  }
  assert "user" in types and "mnt" in types and "net" in types
  for row in lines[1..] {
    let fields = row.fields()
    assert fields[2] == "1", f"only the lsns process lives in the namespace: {row}"
    assert fields[3] == "0"
    assert fields[4] == root_name()
  }
  # Sorted by inode, like the reference listing.
  var previous = 0
  for row in lines[1..] {
    let inode = row.fields()[0] as Int
    assert inode > previous
    previous = inode
  }
}

test test_lsns_filters_by_type_and_hides_headings { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let ran = lsns_in_userns(ctx, [], ["-t", "user", "-n", "-o", "NS,TYPE"])?
  assert ran.status == 0, ran.stderr
  assert rx"^[0-9]{10} user\n$".matches(ran.stdout), ran.stdout
}

test test_lsns_raw_format_escapes_blanks_and_keeps_headings { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let lsns_script = fp"{ctx.core_dir}/lsns.xsh"
  let ran = lsns_in_userns(ctx, [], ["-r", "-t", "user", "-o", "TYPE,PATH,COMMAND"])?
  assert ran.status == 0, ran.stderr
  let command = f"{ctx.xsh_bin} {lsns_script} -- -r -t user -o TYPE,PATH,COMMAND".replace(" ", with: "\\x20")
  let shaped = rx"/proc/[0-9]+/ns/user".replace(ran.stdout, with: "/proc/PID/ns/user")
  assert shaped == f"TYPE PATH COMMAND\nuser /proc/PID/ns/user {command}\n", shaped
}

test test_lsns_json_format_types_numbers_and_strings { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let ran = lsns_in_userns(ctx, [], ["-J", "-t", "user", "-o", "NS,TYPE,NPROCS,UID,NETNSID,NSFS"])?
  assert ran.status == 0, ran.stderr
  let shaped = INODE.replace(ran.stdout, with: "INODE")
  let expected = """{
   "namespaces": [
      {
         "ns": INODE,
         "type": "user",
         "nprocs": 1,
         "uid": 0,
         "netnsid": null,
         "nsfs": null
      }
   ]
}
"""
  assert shaped == expected, shaped
}

# Runs lsns as the child of a wrapper program started with `odd` as an
# argument, so the wrapper is the lowest process of every namespace and its
# command line is the one lsns reports.
proc lsns_under_wrapper(ctx: TestContext, odd: Str, lsns_args: List[Str]) [fs, process, error] -> Result[Ran] {
  let lsns_script = fp"{ctx.core_dir}/lsns.xsh"
  let words = [f"\"{word}\"" for word in lsns_args].join(", ")
  let wrapper = test.temp_file(ctx, name: "wrapper.xsh", contents: bytes.from_text(f"""let plan = process.command_argv(fp"{ctx.xsh_bin}", ["xsh", "{lsns_script}", {words}])
let status = process.run(plan)?
exit status.shell_code()?
"""))?
  run_unshare(ctx, ["-r", ctx.xsh_bin.display(), wrapper.display(), odd])
}

test test_lsns_escapes_control_characters_in_the_command { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let odd = "a\"b\\c\td"
  let as_json = lsns_under_wrapper(ctx, odd, ["-J", "-t", "user", "-o", "COMMAND"])?
  assert as_json.status == 0, as_json.stderr
  assert as_json.stdout.find("\"command\": \"") != null
  assert as_json.stdout.find("a\\\"b\\\\c\\td\"") != null, as_json.stdout
  let table = lsns_under_wrapper(ctx, odd, ["-t", "user", "-o", "COMMAND"])?
  assert table.status == 0, table.stderr
  assert table.stdout.find("a\"b\\c\\x09d\n") != null, table.stdout
  let raw = lsns_under_wrapper(ctx, odd, ["-r", "-t", "user", "-o", "COMMAND"])?
  assert raw.status == 0, raw.stderr
  assert raw.stdout.find("a\"b\\x5cc\\x09d\n") != null, raw.stdout
}

test test_lsns_task_limits_the_listing_to_one_process { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let missing = lsns_here(ctx, ["-p", "2147483646"])?
  assert missing.status == 0, missing.stderr
  assert missing.stdout == ""
  let as_json = lsns_here(ctx, ["-J", "-p", "2147483646"])?
  assert as_json.status == 0
  assert as_json.stdout == "{\n   \"namespaces\": [\n\n   ]\n}\n", as_json.stdout
}

test test_lsns_reports_owner_namespaces { |ctx|
  guard require_user_namespaces(ctx) else { return }
  # Namespaces this call creates are owned by the new user namespace; the ones
  # it inherits belong to a namespace the caller cannot see and report 0, as
  # does the new user namespace's own owner, which lies outside it.
  let ran = lsns_in_userns(ctx, ["-n", "-u"], ["-n", "-r", "-o", "TYPE,NS,PNS,ONS"])?
  assert ran.status == 0, ran.stderr
  let rows = [line.fields() for line in ran.stdout.lines()]
  var owner = ""
  for fields in rows {
    if fields[0] == "user" { owner = fields[1] }
  }
  assert owner != ""
  for fields in rows {
    assert fields[2] == "0", f"{fields[0]} has no parent namespace: {fields.join(" ")}"
    if fields[0] == "net" or fields[0] == "uts" {
      assert fields[3] == owner, f"{fields[0]} is owned by the new user namespace"
    } else if fields[0] != "user" {
      assert fields[3] == "0", f"{fields[0]} is inherited: {fields.join(" ")}"
    }
  }
}

test test_lsns_network_listing_adds_the_id_and_mount_columns { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let ran = lsns_in_userns(ctx, ["-n"], ["-t", "net"])?
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert rx"^ +NS TYPE NPROCS +PID USER +NETNSID NSFS COMMAND$".matches(lines[0]), lines[0]
  assert rx"^[0-9]{10} net +1 +[0-9]+ [a-z0-9]+ unassigned +".matches(lines[1]), lines[1]
}

test test_lsns_lists_where_a_namespace_file_is_bound { |ctx|
  guard require_user_namespaces(ctx) else { return }
  let root = test.temp_dir(ctx, name: "nsfs")?
  let first = fp"{root}/first"
  let second = fp"{root}/second"
  first.write("")
  second.write("")
  let lsns_script = fp"{ctx.core_dir}/lsns.xsh"
  let script = test.temp_file(ctx, name: "bind.xsh", contents: bytes.from_text(f"""linux.mount("/proc/self/ns/net", fp"{first}", options: ["bind"])?
linux.mount("/proc/self/ns/net", fp"{second}", options: ["bind"])?
let plan = process.command_argv(fp"{ctx.xsh_bin}", ["xsh", "{lsns_script}", "-r", "-t", "net", "-o", "NS,NSFS"])
let status = process.run(plan)?
exit status.shell_code()?
"""))?
  let ran = run_unshare(ctx, ["-r", "-m", "-n", ctx.xsh_bin.display(), script.display()])?
  if ran.status != 0 and ran.stderr.find("Operation not permitted") != null {
    test.skip(f"bind mounts are refused here: {ran.stderr.trim()}")
    return
  }
  assert ran.status == 0, ran.stderr
  let lines = ran.stdout.lines()
  assert lines[0] == "NS NSFS"
  assert rx"^[0-9]{10} ".matches(lines[1]), lines[1]
  assert lines[1].byte_slice(11) == f"{first},{second}", lines[1]
}

test test_lsns_rejects_invalid_arguments { |ctx|
  let kind = lsns_here(ctx, ["-t", "bogus"])?
  assert kind.status == 1
  assert kind.stderr == "lsns: unknown namespace type: bogus\n"
  let column = lsns_here(ctx, ["-o", "NS,BOGUS"])?
  assert column.status == 1
  assert column.stderr == "lsns: unknown column: BOGUS\n"
  let word = lsns_here(ctx, ["-p", "1x"])?
  assert word.status == 1
  assert word.stderr == "lsns: invalid PID argument: '1x'\n"
  let zero = lsns_here(ctx, ["-p", "0"])?
  assert zero.status == 1
  assert zero.stderr == "lsns: invalid PID argument: '0': Result not representable\n"
  let unknown = lsns_here(ctx, ["-x"])?
  assert unknown.status == 1
  assert unknown.stderr == "lsns: invalid option -- 'x'\nTry 'lsns --help' for more information.\n"
}
