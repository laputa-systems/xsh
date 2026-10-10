use core.lib.capability

type Ran = {status: Int, stdout: Str, stderr: Str}

# Prints the privilege state of the command in a form the tests can compare.
const STATE_SCRIPT = """let state = linux.privileges()?
print f"nnp={state.no_new_privs}"
print f"pdeathsig={state.parent_death_signal}"
print f"inheritable={json.encode(state.inheritable)?}"
print f"ambient={json.encode(state.ambient)?}"
print f"bounding={json.encode(state.bounding)?}"
print f"effective={json.encode(state.effective)?}"
print f"securebits={json.encode(state.securebits)?}"
"""

# The kernel's own rendering of the identity, including the saved IDs.
const IDS_SCRIPT = """for line in p"/proc/self/status".read_text()?.lines() {
  if line.starts_with("Uid:") or line.starts_with("Gid:") or line.starts_with("Groups:") { print $line }
}
"""

const ENV_SCRIPT = """for entry in env.list()? { print f"{entry.name}={entry.value}" }
"""

proc run_with_environment(ctx: TestContext, program: Path, words: List[Str], variables: Record) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let status = process.run(process.command_argv(program, words, root, variables, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

proc run_command(ctx: TestContext, program: Path, words: List[Str]) [fs, process, error] -> Result[Ran] {
  run_with_environment(ctx, program, words, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""})
}

# The applet is run as `xsh setpriv.xsh -- ARGS`, so every ARGS option reaches it.
proc setpriv(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/setpriv.xsh".display(), "--"].extend(args)
  run_command(ctx, ctx.xsh_bin, words)
}

# The words that make setpriv run another setpriv, for chains of two settings.
proc applet_words(ctx: TestContext) [fs, process, error] -> List[Str] {
  [ctx.xsh_bin.display(), fp"{ctx.core_dir}/setpriv.xsh".display(), "--"]
}

# A script the command can read after setpriv has dropped to another user.
proc shared_script(ctx: TestContext, name: Str, source: Str) [fs, error] -> Result[Str] {
  ctx.temp_root.chmod(0o755)
  let script = test.temp_file(ctx, name: name, contents: bytes.from_text(source))?
  script.chmod(0o644)
  script.display()
}

proc has_capability(number: Int) [process, error] -> Bool {
  match linux.privileges() {
    Ok(state) => number in state.effective
    Err(_) => false
  }
}

pure list_after(text: Str, label: Str) -> List[Int] {
  var found: List[Int] = []
  for line in text.lines() {
    if line.starts_with(label) {
      for word in line.byte_slice(label.byte_len()).trim().split("\t") {
        for piece in word.split(" ") {
          if piece != "" { found += [piece.parse_int() ?? -1] }
        }
      }
    }
  }
  found
}

test test_setpriv_help_and_version_exit_zero { |ctx|
  let help = setpriv(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.find("Usage:") != null and help.stdout.find(" setpriv [options] <program> [<argument>...]") != null
  for option in ["--dump", "--nnp", "--ambient-caps", "--inh-caps", "--bounding-set", "--reuid", "--regid", "--clear-groups", "--keep-groups", "--init-groups", "--list-caps", "--groups", "--securebits", "--pdeathsig", "--ptracer", "--reset-env"] {
    assert help.stdout.find(option) != null, option
  }
  let version = setpriv(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("setpriv")
  assert setpriv(ctx, ["-V"])?.stdout == version.stdout
  # Help ends option processing before later arguments are read.
  assert setpriv(ctx, ["--help", "--no-such-option"])?.status == 0
}

test test_setpriv_list_caps_names_every_known_capability { |ctx|
  let listed = setpriv(ctx, ["--list-caps"])?
  assert listed.status == 0
  let names = listed.stdout.lines()
  assert names.len() == capability.CAPABILITIES.len()
  assert names[0] == "chown" and names[-1] == "checkpoint_restore"
  for index in range(names.len()) {
    assert "cap_" + names[index] == capability.CAPABILITIES[index].name
  }
}

test test_setpriv_command_line_errors_use_getopt_wording { |ctx|
  let cases = [
    {args: [], text: "No program specified"},
    {args: ["--no-such-option"], text: "unrecognized option '--no-such-option'"},
    {args: ["-x"], text: "invalid option -- 'x'"},
    {args: ["--re", "id"], text: "option '--re' is ambiguous; possibilities: '--regid' '--reset-env' '--reuid'"},
    {args: ["--ruid"], text: "option '--ruid' requires an argument"},
    {args: ["--nnp=1", "id"], text: "option '--nnp' doesn't allow an argument"},
    {args: ["--dump", "id"], text: "--dump is incompatible with all other options"},
    {args: ["--dump", "--nnp"], text: "--dump is incompatible with all other options"},
    {args: ["--list-caps", "id"], text: "--list-caps must be specified alone"},
    {args: ["--list-caps", "--nnp"], text: "--list-caps must be specified alone"},
    {args: ["--selinux-label=x", "id"], text: "option '--selinux-label' is not supported"},
    {args: ["--seccomp-filter", "x", "id"], text: "option '--seccomp-filter' is not supported"},
  ]
  for case in cases {
    let result = setpriv(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stderr.find(case.text) != null, case.args.join(" ") + ": " + result.stderr
  }
  assert setpriv(ctx, ["--no-such-option"])?.stderr.find("Try 'setpriv --help' for more information.") != null
}

test test_setpriv_option_conflicts_and_duplicates_name_the_options { |ctx|
  let cases = [
    {args: ["--nnp", "--no-new-privs", "id"], text: "duplicate --no-new-privs option"},
    {args: ["--ruid=1", "--ruid=2", "id"], text: "duplicate ruid"},
    {args: ["--euid=1", "--euid=2", "id"], text: "duplicate euid"},
    {args: ["--ruid=1", "--reuid=2", "id"], text: "duplicate ruid or euid"},
    {args: ["--reuid=1", "--ruid=2", "id"], text: "duplicate ruid"},
    {args: ["--reuid=1", "--euid=2", "id"], text: "duplicate euid"},
    {args: ["--rgid=1", "--rgid=2", "--keep-groups", "id"], text: "duplicate rgid"},
    {args: ["--egid=1", "--regid=2", "--keep-groups", "id"], text: "duplicate rgid or egid"},
    {args: ["--keep-groups", "--keep-groups", "id"], text: "duplicate --keep-groups option"},
    {args: ["--groups=1", "--groups=2", "id"], text: "duplicate --groups option"},
    {args: ["--keep-groups", "--clear-groups", "id"], text: "options --keep-groups and --clear-groups cannot be combined"},
    {args: ["--groups=1", "--init-groups", "--reuid=1", "id"], text: "options --groups and --init-groups cannot be combined"},
    {args: ["--init-groups", "--clear-groups", "--reuid=1", "id"], text: "options --init-groups and --clear-groups cannot be combined"},
    {args: ["--regid=1", "id"], text: "--[re]gid requires --keep-groups, --clear-groups, --init-groups, or --groups"},
    {args: ["--init-groups", "id"], text: "--init-groups requires --ruid or --reuid"},
    {args: ["--init-groups", "--euid=1", "id"], text: "--init-groups requires --ruid or --reuid"},
    {args: ["--inh-caps=-all", "--inh-caps=-all", "id"], text: "duplicate --inh-caps option"},
    {args: ["--ambient-caps=-all", "--ambient-caps=-all", "id"], text: "duplicate --ambient-caps option"},
    {args: ["--bounding-set=-all", "--bounding-set=-all", "id"], text: "duplicate --bounding-set option"},
    {args: ["--securebits=-all", "--securebits=-all", "id"], text: "duplicate --securebits option"},
    {args: ["--pdeathsig=TERM", "--pdeathsig=keep", "id"], text: "duplicate --pdeathsig option"},
    {args: ["--ptracer=any", "--ptracer=none", "id"], text: "duplicate --ptracer option"},
  ]
  for case in cases {
    let result = setpriv(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stderr.find(case.text) != null, case.args.join(" ") + ": " + result.stderr
  }
}

test test_setpriv_values_are_validated_before_anything_changes { |ctx|
  let cases = [
    {args: ["--inh-caps=", "id"], text: "bad capability string"},
    {args: ["--inh-caps=chown", "id"], text: "bad capability string"},
    {args: ["--inh-caps=+chown,,+kill", "id"], text: "bad capability string"},
    {args: ["--inh-caps=+bogus", "id"], text: "unknown capability \"bogus\""},
    {args: ["--inh-caps=+cap_chown", "id"], text: "unknown capability \"cap_chown\""},
    {args: ["--inh-caps=+cap_99", "id"], text: "unknown capability \"cap_99\""},
    {args: ["--bounding-set=-ALL", "id"], text: "unknown capability \"ALL\""},
    {args: ["--ambient-caps=+", "id"], text: "unknown capability \"\""},
    {args: ["--securebits=noroot", "id"], text: "bad securebits string"},
    {args: ["--securebits=+bogus", "id"], text: "unrecognized securebit"},
    {args: ["--securebits=+all", "id"], text: "+all securebits is not allowed"},
    {args: ["--securebits=+keep_caps", "id"], text: "adjusting keep_caps does not make sense"},
    {args: ["--pdeathsig=bogus", "id"], text: "unknown signal: bogus"},
    {args: ["--pdeathsig=15", "id"], text: "unknown signal: 15"},
    {args: ["--ptracer=abc", "id"], text: "invalid PID argument: 'abc'"},
    {args: ["--ptracer=0", "id"], text: "invalid PID argument: '0'"},
    {args: ["--reuid=xsh-no-such-user", "id"], text: "failed to parse reuid: 'xsh-no-such-user'"},
    {args: ["--ruid=4294967295", "id"], text: "failed to parse ruid: '4294967295'"},
    {args: ["--regid=xsh-no-such-group", "--keep-groups", "id"], text: "failed to parse regid: 'xsh-no-such-group'"},
    {args: ["--groups=1,,2", "id"], text: "Invalid supplementary group id: ''"},
    {args: ["--groups=xsh-no-such-group", "id"], text: "Invalid supplementary group id: 'xsh-no-such-group'"},
    {args: ["--init-groups", "--reuid=4321", "id"], text: "uid 4321 not found, --init-groups requires an user that can be found on the system"},
  ]
  for case in cases {
    # `id` would print and exit 0 if the value had been accepted.
    let result = setpriv(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stderr.find(case.text) != null, case.args.join(" ") + ": " + result.stderr
    assert result.stdout == ""
  }
}

test test_setpriv_dump_layout_matches_util_linux { |ctx|
  # The layout util-linux prints, down to labels, "[none]" and comma lists.
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let me = unix.id()?
  let chain = applet_words(ctx).extend(["--inh-caps=-all", "--nnp", "--pdeathsig=TERM", ctx.xsh_bin.display()]).extend(applet_words(ctx)[1..])
  let short = run_command(ctx, ctx.xsh_bin, chain.extend(["-d"]))?
  assert short.status == 0, short.stderr
  let lines = short.stdout.lines()
  assert lines.len() == 11, short.stdout
  assert lines[0] == f"uid: {me.uid}"
  assert lines[1] == f"euid: {me.euid}"
  assert lines[2] == f"gid: {me.gid}"
  assert lines[3] == f"egid: {me.egid}"
  assert rx"^Supplementary groups: (\[none\]|[0-9]+(,[0-9]+)*)$".matches(lines[4])
  assert lines[5] == "no_new_privs: 1"
  assert lines[6] == "Inheritable capabilities: [none]"
  assert lines[7] == "Ambient capabilities: [none]"
  assert rx"^Capability bounding set: (\[none\]|[a-z_0-9]+(,[a-z_0-9]+)*)$".matches(lines[8])
  assert lines[9] == "Securebits: [none]"
  assert lines[10] == "Parent death signal: TERM"
  assert short.stdout.ends_with("\n")

  let long = run_command(ctx, ctx.xsh_bin, chain.extend(["-dd"]))?
  assert long.status == 0, long.stderr
  let all = long.stdout.lines()
  assert all.len() == 13
  assert all[5] == "no_new_privs: 1"
  assert rx"^Effective capabilities: (\[none\]|[a-z_0-9]+(,[a-z_0-9]+)*)$".matches(all[6])
  assert rx"^Permitted capabilities: (\[none\]|[a-z_0-9]+(,[a-z_0-9]+)*)$".matches(all[7])
  assert all[8] == "Inheritable capabilities: [none]"
  assert all[12] == "Parent death signal: TERM"
  # Only the layout is shared; -d and --dump are the same switch.
  assert setpriv(ctx, ["--dump"])?.stdout.lines().len() == 11
  assert child != ""
}

test test_setpriv_sets_no_new_privs_and_the_parent_death_signal { |ctx|
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let plain = setpriv(ctx, [ctx.xsh_bin.display(), child])?
  assert plain.status == 0, plain.stderr
  assert plain.stdout.find("nnp=false") != null and plain.stdout.find("pdeathsig=0") != null
  for choice in [["--nnp"], ["--no-new-privs"]] {
    let result = setpriv(ctx, choice.extend([ctx.xsh_bin.display(), child]))?
    assert result.status == 0, result.stderr
    assert result.stdout.find("nnp=true") != null
  }
  let signal = setpriv(ctx, ["--pdeathsig=TERM", ctx.xsh_bin.display(), child])?
  assert signal.stdout.find("pdeathsig=15") != null
  # A separate value, an SIG prefix and any case name the same signal.
  assert setpriv(ctx, ["--pdeathsig", "usr1", ctx.xsh_bin.display(), child])?.stdout.find("pdeathsig=10") != null
  assert setpriv(ctx, ["--pdeathsig=SIGHUP", ctx.xsh_bin.display(), child])?.stdout.find("pdeathsig=1") != null
  assert setpriv(ctx, ["--pdeathsig=RTMIN+1", ctx.xsh_bin.display(), child])?.stdout.find("pdeathsig=36") != null
  assert setpriv(ctx, ["--pdeathsig=RTMAX-1", ctx.xsh_bin.display(), child])?.stdout.find("pdeathsig=63") != null
}

test test_setpriv_pdeathsig_keep_and_clear_act_on_an_inherited_signal { |ctx|
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let tail = [ctx.xsh_bin.display()].extend(applet_words(ctx)[1..])
  let inherited = setpriv(ctx, ["--pdeathsig=TERM"].extend(tail).extend([ctx.xsh_bin.display(), child]))?
  assert inherited.stdout.find("pdeathsig=15") != null, inherited.stderr
  let kept = setpriv(ctx, ["--pdeathsig=TERM"].extend(tail).extend(["--pdeathsig=keep", ctx.xsh_bin.display(), child]))?
  assert kept.stdout.find("pdeathsig=15") != null, kept.stderr
  let cleared = setpriv(ctx, ["--pdeathsig=TERM"].extend(tail).extend(["--pdeathsig=clear", ctx.xsh_bin.display(), child]))?
  assert cleared.stdout.find("pdeathsig=0") != null, cleared.stderr
}

test test_setpriv_inheritable_and_ambient_edits_start_from_the_current_sets { |ctx|
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let emptied = setpriv(ctx, ["--inh-caps=-all", "--ambient-caps=-all", ctx.xsh_bin.display(), child])?
  assert emptied.status == 0, emptied.stderr
  assert emptied.stdout.find("inheritable=[]") != null and emptied.stdout.find("ambient=[]") != null
}

test test_setpriv_runs_the_command_and_reports_exec_failures { |ctx|
  let exit_seven = shared_script(ctx, "seven.xsh", "exit 7\n")?
  let result = setpriv(ctx, [ctx.xsh_bin.display(), exit_seven])?
  assert result.status == 7

  let missing = setpriv(ctx, ["/xsh-no-such-directory/program"])?
  assert missing.status == 127
  assert missing.stderr.find("failed to execute /xsh-no-such-directory/program: No such file or directory") != null
  let unresolved = setpriv(ctx, ["xsh-no-such-command"])?
  assert unresolved.status == 127
  assert unresolved.stderr.find("failed to execute xsh-no-such-command: No such file or directory") != null
  let empty = setpriv(ctx, [""])?
  assert empty.status == 127

  let plain = test.temp_file(ctx, name: "not-executable", contents: b"data")?
  plain.chmod(0o644)
  let refused = setpriv(ctx, [plain.display()])?
  assert refused.status == 126
  assert refused.stderr.find("Permission denied") != null
  let directory = setpriv(ctx, [ctx.temp_root.display()])?
  assert directory.status == 126

  # `--` ends option parsing, so an option-looking command is the command.
  let separated = setpriv(ctx, ["--", "--xsh-no-such-command"])?
  assert separated.status == 127
  assert separated.stderr.find("failed to execute --xsh-no-such-command") != null
}

test test_setpriv_reset_env_rebuilds_the_environment_from_the_account { |ctx|
  let account = user.current()
  if account is Err(_) { test.skip("the running user has no account entry") }
  let entry = account?
  let child = shared_script(ctx, "env.xsh", ENV_SCRIPT)?
  let words = applet_words(ctx).extend(["--reset-env", ctx.xsh_bin.display(), child])
  let result = run_with_environment(ctx, ctx.xsh_bin, words, {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", FOO: "bar", TERM: "xsh-term"})?
  assert result.status == 0, result.stderr
  let lines = result.stdout.lines()
  assert ! ("FOO=bar" in lines)
  assert "TERM=xsh-term" in lines
  assert f"HOME={entry.home.display()}" in lines
  assert f"USER={entry.name}" in lines
  assert f"LOGNAME={entry.name}" in lines
  assert f"SHELL={if entry.shell == "" { "/bin/sh" } else { entry.shell }}" in lines
  let expected_path = if entry.uid == 0 { "/usr/local/sbin:/usr/local/bin:/sbin:/bin:/usr/sbin:/usr/bin" } else { "/usr/local/bin:/bin:/usr/bin" }
  assert f"PATH={expected_path}" in lines
}

test test_setpriv_reports_the_kernel_refusal_in_its_own_words { |ctx|
  let me = unix.id()?
  if has_capability(8) or has_capability(7) or has_capability(6) { test.skip("the process holds CAP_SETPCAP, CAP_SETUID or CAP_SETGID") }
  let cases = [
    {args: ["--bounding-set=-all"], text: "setpriv: apply bounding set: Operation not permitted"},
    {args: ["--securebits=+noroot"], text: "setpriv: set process securebits failed: Operation not permitted"},
    {args: ["--clear-groups"], text: "setpriv: setgroups failed: Operation not permitted"},
    {args: ["--regid=4321", "--keep-groups"], text: "setpriv: setresgid failed: Operation not permitted"},
    {args: ["--reuid=4321"], text: "setpriv: setresuid failed: Operation not permitted"},
  ]
  for case in cases {
    let result = setpriv(ctx, case.args.extend(["/xsh-never-reached"]))?
    assert result.status == 127, case.args.join(" ")
    assert result.stderr.find(case.text) != null, case.args.join(" ") + ": " + result.stderr
  }
  # Changing to the identity the process already has is always permitted.
  let same = setpriv(ctx, [f"--reuid={me.uid}", "/xsh-no-such-command"])?
  assert same.status == 127
  assert same.stderr.find("failed to execute") != null
}

# The remaining tests change credentials and capabilities, which needs root
# and the matching capabilities: run them with `docker-xsht.sh --root`.

proc privileged_or_skip() [process, error, fs] {
  if user.current()?.uid != 0 { test.skip("changing credentials requires root") }
  if ! (has_capability(7) and has_capability(6) and has_capability(8)) { test.skip("needs CAP_SETUID, CAP_SETGID and CAP_SETPCAP") }
}

# Runs the identity script as ordinary users need to reach the xsh binary.
proc ids_after(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let script = shared_script(ctx, "ids.xsh", IDS_SCRIPT)?
  setpriv(ctx, args.extend([ctx.xsh_bin.display(), script]))
}

test test_setpriv_uid_options_set_real_effective_and_saved_ids { |ctx|
  privileged_or_skip()
  let cases = [
    {args: ["--reuid=65534"], uid: [65534, 65534, 65534, 65534]},
    {args: ["--ruid=1"], uid: [1, 0, 0, 0]},
    {args: ["--euid=2"], uid: [0, 2, 2, 2]},
    {args: ["--ruid=1", "--euid=2"], uid: [1, 2, 2, 2]},
    {args: ["--euid=0", "--ruid=1"], uid: [1, 0, 0, 0]},
  ]
  for case in cases {
    let result = ids_after(ctx, case.args.extend(["--keep-groups"]))?
    if result.status == 126 { test.skip("the unprivileged user cannot reach the xsh binary") }
    assert result.status == 0, case.args.join(" ") + ": " + result.stderr
    assert list_after(result.stdout, "Uid:") == case.uid, case.args.join(" ") + ": " + result.stdout
  }
}

test test_setpriv_gid_options_set_real_effective_and_saved_ids { |ctx|
  privileged_or_skip()
  let cases = [
    {args: ["--regid=3"], gid: [3, 3, 3, 3]},
    {args: ["--rgid=1"], gid: [1, 0, 0, 0]},
    {args: ["--egid=2"], gid: [0, 2, 2, 2]},
    {args: ["--rgid=1", "--egid=2"], gid: [1, 2, 2, 2]},
  ]
  for case in cases {
    let result = ids_after(ctx, case.args.extend(["--keep-groups"]))?
    assert result.status == 0, case.args.join(" ") + ": " + result.stderr
    assert list_after(result.stdout, "Gid:") == case.gid, case.args.join(" ") + ": " + result.stdout
  }
}

test test_setpriv_supplementary_groups_are_cleared_listed_or_initialized { |ctx|
  privileged_or_skip()
  let cleared = ids_after(ctx, ["--clear-groups"])?
  assert cleared.status == 0, cleared.stderr
  assert list_after(cleared.stdout, "Groups:").is_empty()
  let listed = ids_after(ctx, ["--groups=3,4,3"])?
  assert listed.status == 0, listed.stderr
  assert list_after(listed.stdout, "Groups:") == [3, 3, 4]
  let by_name = ids_after(ctx, ["--groups=root"])?
  assert by_name.status == 0, by_name.stderr
  assert list_after(by_name.stdout, "Groups:") == [0]
  # The root account always exists; its groups come from the group database.
  let initialized = ids_after(ctx, ["--reuid=0", "--regid=0", "--init-groups"])?
  assert initialized.status == 0, initialized.stderr
  assert 0 in list_after(initialized.stdout, "Groups:")
}

test test_setpriv_keeps_capabilities_through_a_uid_change_with_ambient_caps { |ctx|
  privileged_or_skip()
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let state = linux.privileges()?
  if ! (0 in state.permitted and 0 in state.bounding) { test.skip("CAP_CHOWN is not available") }
  let result = setpriv(ctx, ["--reuid=65534", "--regid=65534", "--clear-groups", "--inh-caps=+chown", "--ambient-caps=+chown", ctx.xsh_bin.display(), child])?
  if result.status == 126 { test.skip("the unprivileged user cannot reach the xsh binary") }
  assert result.status == 0, result.stderr
  assert result.stdout.find("inheritable=[0]") != null, result.stdout
  assert result.stdout.find("ambient=[0]") != null, result.stdout
  assert result.stdout.find("effective=[0]") != null, result.stdout
  # Without the ambient set the same change leaves a non-root process empty.
  let bare = setpriv(ctx, ["--reuid=65534", "--regid=65534", "--clear-groups", ctx.xsh_bin.display(), child])?
  assert bare.stdout.find("effective=[]") != null, bare.stdout
  assert bare.stdout.find("ambient=[]") != null
}

test test_setpriv_ambient_names_must_be_inheritable_while_all_takes_what_it_can { |ctx|
  privileged_or_skip()
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let state = linux.privileges()?
  if ! (0 in state.permitted and 0 in state.bounding) { test.skip("CAP_CHOWN is not available") }
  let refused = setpriv(ctx, ["--ambient-caps=+chown", ctx.xsh_bin.display(), child])?
  assert refused.status == 127
  assert refused.stderr.find("setpriv: apply ambient capabilities: Operation not permitted") != null
  let tolerated = setpriv(ctx, ["--inh-caps=+chown", "--ambient-caps=+all", ctx.xsh_bin.display(), child])?
  assert tolerated.status == 0, tolerated.stderr
  assert tolerated.stdout.find("ambient=[0]") != null, tolerated.stdout
}

test test_setpriv_bounding_set_edits_drop_and_cannot_grow { |ctx|
  privileged_or_skip()
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let state = linux.privileges()?
  if ! (0 in state.bounding) { test.skip("CAP_CHOWN is not in the bounding set") }
  let dropped = setpriv(ctx, ["--bounding-set=-chown", ctx.xsh_bin.display(), child])?
  assert dropped.status == 0, dropped.stderr
  assert ! (dropped.stdout.find("bounding=[0,") != null or dropped.stdout.find("bounding=[0]") != null)
  let only = setpriv(ctx, ["--bounding-set=-all,+chown", ctx.xsh_bin.display(), child])?
  assert only.stdout.find("bounding=[0]") != null, only.stdout
  # A capability that was dropped cannot be named back into the set.
  let chain = ["--bounding-set=-chown", ctx.xsh_bin.display()].extend(applet_words(ctx)[1..]).extend(["--bounding-set=+chown", ctx.xsh_bin.display(), child])
  let regrown = setpriv(ctx, chain)?
  assert regrown.status == 127
  assert regrown.stderr.find("setpriv: apply bounding set: Operation not permitted") != null
  # `+all` only names what is still there.
  let everything = setpriv(ctx, ["--bounding-set=-chown", ctx.xsh_bin.display()].extend(applet_words(ctx)[1..]).extend(["--bounding-set=+all", ctx.xsh_bin.display(), child]))?
  assert everything.status == 0, everything.stderr
}

test test_setpriv_securebits_are_edited_from_the_current_flags { |ctx|
  privileged_or_skip()
  let child = shared_script(ctx, "state.xsh", STATE_SCRIPT)?
  let flags = setpriv(ctx, ["--securebits=+noroot,+noroot_locked,+no_setuid_fixup", ctx.xsh_bin.display(), child])?
  assert flags.status == 0, flags.stderr
  assert flags.stdout.find("securebits=[\"noroot\",\"noroot_locked\",\"no_setuid_fixup\"]") != null, flags.stdout
  let removed = setpriv(ctx, ["--securebits=+noroot,+no_setuid_fixup,-noroot", ctx.xsh_bin.display(), child])?
  assert removed.stdout.find("securebits=[\"no_setuid_fixup\"]") != null, removed.stdout
  # A locked flag stays: the kernel refuses to clear it.
  let chain = ["--securebits=+noroot,+noroot_locked", ctx.xsh_bin.display()].extend(applet_words(ctx)[1..]).extend(["--securebits=-noroot", ctx.xsh_bin.display(), child])
  let locked = setpriv(ctx, chain)?
  assert locked.status == 127
  assert locked.stderr.find("setpriv: set process securebits failed: Operation not permitted") != null
}
