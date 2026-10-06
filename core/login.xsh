#!/bin/xsh
use lib.gnu
use lib.accounts

type Options = {preauthenticated: Bool, preserve: Bool, remote: Str, help: Bool, version: Bool, users: List[Str]}

pure equal_groups(requested: List[Int], actual: List[Int]) -> Bool {
  for value in requested { if value not in actual { return false } }
  for value in actual { if value not in requested { return false } }
  true
}

proc main(...argv: List[Str]) [fs, env, process, error, io, time] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    preauthenticated: {form: "-f", default: false},
    preserve: {form: "-p", default: false},
    remote: {form: "-h HOST", default: ""},
    timeout: {form: "-t --timeout SECONDS", unsupported: true},
    hostname: {form: "-H", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    users: {form: "...USER"},
  })?
  if opts.help { gnu.help("Usage: login [-f] [-p] [-h HOST] [USER]\nAuthenticate an account and replace this process with its login shell.\n-f skips password authentication only for real and effective UID 0.\n-p preserves environment variables except identity, executable lookup, and shell/loader controls.\nPrivileged logins use canonical account files. Account fixture overrides can select only the unprivileged caller's identity."); return }
  if opts.version { gnu.version("login"); return }
  if opts.users.len() > 1 { gnu.extra_operand(opts.users[1]) }
  let identity = unix.id()?
  let root_caller = identity.uid == 0 and applet.current_euid() == 0
  if opts.preauthenticated and !root_caller { gnu.error("-f requires real and effective UID 0"); exit 1 }
  if opts.remote != "" and !root_caller { gnu.error("-h requires real and effective UID 0"); exit 1 }
  var name = opts.users.get(0) ?? ""
  if name == "" {
    gnu.write_text("login: ")
    name = io.stdin_line()?.trim()
  }
  if name == "" or name.byte_len() > 256 or !rx"^[A-Za-z0-9_.-]+$".matches(name) { gnu.error("Login incorrect"); exit 1 }
  let lookup = accounts.login_user(name, identity)
  if let Err(failure) = lookup { gnu.error(failure.message); exit 1 }
  let entry = lookup?
  let notice = accounts.nologin_file()
  if entry.uid != 0 and notice.exists()? { gnu.write_text(notice.read_text()?); exit 1 }
  if !opts.preauthenticated {
    let authenticated = accounts.authenticate(entry)
    if authenticated is Err(_) { gnu.error("Login incorrect"); exit 1 }
    if !authenticated? { gnu.error("Login incorrect"); exit 1 }
  }
  if let Err(failure) = accounts.check_expiration(entry) { gnu.error(failure.message); exit 1 }
  let requested = accounts.login_groups(entry)
  if let Err(failure) = requested { gnu.error(failure.message); exit 1 }
  let groups = requested?
  let actual: List[Int] = identity.groups |> map .gid
  let environment = accounts.session_environment(entry, opts.preserve, opts.remote)?
  let shell = fp"{entry.shell}"
  let home = fs.stat(entry.home, follow_symlinks: true)
  if let Err(failure) = home { gnu.name_error(entry.home.display(), failure); exit 1 }
  if home?.kind != "dir" { gnu.error("account home is not a directory"); exit 1 }
  let command = process.command_argv(shell, [entry.shell], cwd: entry.home)
  # Credential changes cannot roll back. Resolve groups and build the complete
  # session request first; a failed transition or exec ends this login process.
  if entry.uid != identity.uid or entry.uid != applet.current_euid() or entry.gid != identity.gid or entry.gid != identity.egid or !equal_groups(groups, actual) {
    if let Err(failure) = unix.set_credentials(uid: entry.uid, gid: entry.gid, groups: groups) { gnu.error(failure.message); exit 1 }
  }
  if let Err(failure) = unix.exec_env(command, environment, argv0: f"-{shell.basename()}") { gnu.error(failure.message); exit 1 }
}
