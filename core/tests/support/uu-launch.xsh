# Keep only the upstream harness environment and caller overrides. Inherited
# environment values retain their raw bytes when the applet replaces the launcher.
proc main(...words: List[Bytes]) [env, process, fs, error] {
  let caller_keys = json.decode(words[0].utf8()?)?.require(List[Str])?
  let kept = ["PATH", "LD_PRELOAD", "LLVM_PROFILE_FILE"].extend(caller_keys)
  var environment: Map[Str, Bytes] = {LC_ALL: b"C", TZ: b"UTC"}
  for entry in env.entries()? {
    if let Ok(name) = entry.name.utf8() {
      if name in kept { environment[name] = entry.value }
    }
  }
  let argv = [Path.parse_bytes(word)? for word in words[1..]]
  unix.exec_env(process.command_argv(argv[0], argv), environment)?
}
