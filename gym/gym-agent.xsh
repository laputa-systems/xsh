##! Runs the pi agent inside the task container and streams the session file
##! to the container's stdout as pi persists it.
##!
##! argv: [session path, task file path]
##! The task file path is passed as pi's @file argument and its basename is
##! used in the completion prompt. Session entries are appended and flushed
##! synchronously by pi, so a concurrent tail -f shows each entry as written.

proc main(...argv: List[Str]) [fs, process, env, time, error, io] {
  # Path() from the arg strings: p-string interpolation does not expand
  # indexed spread parameters.
  let session = Path(argv[0])
  let task_path = Path(argv[1])
  let agent_dir = env.path("PI_CODING_AGENT_DIR")?
  fs.mkdir(agent_dir)?
  fs.copy(p"/run/pi-auth.json", fp"${agent_dir}/auth.json")?
  fs.chmod(fp"${agent_dir}/auth.json", 0o600)?

  let pi_command = env.get("PI_COMMAND")?
  match process.which(pi_command) {
    Ok(_) => {}
    Err(_) => {
      eprint "pi is not in the task image; set PI_BINARY to a Linux arm64 release"
      abort(127)
    }
  }

  # Pre-create an empty session file so the tail can start immediately; pi
  # rewrites an empty file with the session header at startup.
  fs.write(session, "")?

  let prompt = f"Complete ${task_path.name()} in /work. Run the required checks and leave the requested artifact there."
  let pi_argv = [
    pi_command,
    "--provider", env.get("PI_PROVIDER")?,
    "--model", env.get("PI_MODEL")?,
    "--thinking", env.get("PI_THINKING")?,
    "--approve",
    "--system-prompt", "/work/agents.md",
    "--no-extensions",
    "--no-skills",
    "--no-prompt-templates",
    "--no-themes",
    "--no-context-files",
    "--tools", "read,write,edit,bash",
    "--session", session.display(),
    "--print",
    f"@${task_path.display()}",
    prompt,
  ]
  let pi = spawn process.command_argv(pi_command, pi_argv)?
  let tail = spawn run tail -f ${session.display()} ?
  let status = wait pi?

  # Give the tail a moment to flush its last buffered lines before stopping it.
  time.sleep(200ms)?
  tail.cancel(signal: "TERM", kill_after: 100ms)?

  if fs.exists(session)? {
    let _ = process.run(process.command_argv(pi_command, [pi_command, "--export", session.display(), session.with_ext("html").display()]))?
  }

  let code = if status.ok { 0 } else { status.exit_code() ?? 1 }
  abort(code)
}
