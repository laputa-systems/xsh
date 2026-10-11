# Keep only the upstream harness environment and caller overrides. Inherited
# environment values retain their raw bytes when the applet replaces the launcher.
type Launch = {caller_keys: List[Str], umask: Int?}

proc main(...words: List[Bytes]) [env, process, fs, error] {
  let launch = json.decode(words[0].utf8()?)?.require(Launch)?
  let kept = ["PATH", "LD_PRELOAD", "LLVM_PROFILE_FILE"].extend(launch.caller_keys)
  var environment: Map[Str, Bytes] = {LC_ALL: b"C", TZ: b"UTC"}
  for entry in env.entries()? {
    if let Ok(name) = entry.name.utf8() {
      if name in kept { environment[name] = entry.value }
    }
  }
  let argv = [Path.parse_bytes(word)? for word in words[1..]]
  if let mask = launch.umask {
    assert mask >= 0 and mask <= 0o777, "umask must contain only permission bits"
    # A shell changes only this child mask. Re-enter the launcher to clear shell
    # environment additions and retain the applet's explicit standalone argv0.
    let xsh = applet.current_exe()?
    let launcher = process.script_path()?
    let metadata = json.encode({caller_keys: launch.caller_keys, umask: null})?
    let setup = [p"/bin/sh", p"-c", Path(r"""umask "$1" || exit; shift; exec "$@"; """), p"uu-mask",
      Path(f"{mask / 64}{mask / 8 % 8}{mask % 8}"), xsh, launcher, Path(metadata)].extend(argv)
    unix.exec_env(process.command_argv(p"/bin/sh", setup), environment)?
  }
  # A standalone command is invoked by its name even when resolved to an absolute executable path.
  unix.exec_env(process.command_argv(argv[0], argv), environment, argv0: argv[0].basename())?
}
