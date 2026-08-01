##! Shared host-side orchestration for xsh gym task runs: environment
##! configuration, workspace preparation, docker container runs, session
##! copy-out, run manifests, review checks, and cleanup.

## The runtime configuration for one task run.
export type Config = {
  docker: Str,
  platform: Str,
  gym_dir: Path,
  work_dir: Path,
  output_dir: Path,
  session_volume: Str,
  session_name: Str,
  task_file: Str,
  image: Str,
  pi_command: Str,
  pi_provider: Str,
  pi_model: Str,
  pi_thinking: Str,
  pi_telemetry: Str,
  pi_offline: Str,
  pi_agent_dir: Path,
  auth_file: Path,
  pi_binary: Str,
}

## Reads the task configuration from the environment.
export proc parse_config(
  docker: Str,
  platform: Str,
  gym_dir: Path,
  work_dir: Path,
  output_dir: Path,
  session_volume: Str,
  session_name: Str,
  task_file: Str,
  image: Str,
) [env, error] -> Result[Config] {
  return {
    docker: docker,
    platform: platform,
    gym_dir: gym_dir,
    work_dir: work_dir,
    output_dir: output_dir,
    session_volume: session_volume,
    session_name: session_name,
    task_file: task_file,
    image: image,
    pi_command: env.get_or("PI_COMMAND", "pi")?,
    pi_provider: env.get_or("PI_PROVIDER", "openrouter")?,
    pi_model: env.get_or("PI_MODEL", "deepseek/deepseek-v4-flash-0731")?,
    pi_thinking: env.get_or("PI_THINKING", "high")?,
    pi_telemetry: env.get_or("PI_TELEMETRY", "0")?,
    pi_offline: env.get_or("PI_OFFLINE", "1")?,
    pi_agent_dir: env.path("PI_AGENT_DIR", p"/run/pi-agent")?,
    auth_file: env.path("PI_AUTH_FILE")?,
    pi_binary: env.get_or("PI_BINARY", "")?,
  }
}

## Creates the work and output directories and stages the task files.
export proc prepare_workdir(cfg: Config, outputs: List[Str]) [fs, error] -> Result[Unit] {
  fs.mkdir(cfg.work_dir)?
  fs.mkdir(cfg.output_dir)?
  for name in ["agents.md", "handbook.md", "review.md", cfg.task_file] {
    fs.copy(fp"${cfg.gym_dir}/${name}", fp"${cfg.work_dir}/${name}", overwrite: true)?
  }
  for name in outputs {
    fs.remove(fp"${cfg.work_dir}/${name}", missing_ok: true)?
    fs.remove(fp"${cfg.output_dir}/${name}", missing_ok: true)?
  }
}

## Checks that the pi auth file exists, aborting with status 2 otherwise.
export proc ensure_auth(cfg: Config) [fs, io, error] -> Result[Unit] {
  if ! fs.exists(cfg.auth_file)? {
    eprint f"Pi auth file does not exist: ${cfg.auth_file.display()}"
    abort(2)
  }
}

## Recreates the session volume so a fresh run starts empty.
## Runs a command for its side effect without asserting success.
export proc best_effort(argv: List[Str]) [process, error] -> Result[Unit] {
  let _ = process.run(process.command_argv(argv[0], argv))?
  return
}

## Recreates the session volume so a fresh run starts empty.
export proc reset_session_volume(cfg: Config) [process, error] -> Result[Unit] {
  best_effort([cfg.docker, "volume", "rm", cfg.session_volume])?
  run $cfg.docker volume create $cfg.session_volume ?
}

## Resolves the image id for the run manifest.
export proc image_id(cfg: Config) [process, error] -> Result[Str] {
  let id = run.text $cfg.docker image inspect "--format" "{{.Id}}" $cfg.image ?
  return id.trim()
}

## Common docker run flags shared by the agent and eval containers.
export pure agent_flags(cfg: Config) -> List[Str] {
  return [
    "--rm",
    "--platform", cfg.platform,
    "--read-only",
    "--tmpfs", "/tmp:rw,noexec,nosuid,nodev",
    "--tmpfs", f"${cfg.pi_agent_dir.display()}:rw,noexec,nosuid,nodev",
    "--cap-drop=ALL",
    "--security-opt=no-new-privileges",
    "--workdir", "/work",
  ]
}

## Environment variables passed to the agent container.
export pure agent_envs(cfg: Config) -> List[Str] {
  return [
    "--env", f"PI_COMMAND=${cfg.pi_command}",
    "--env", f"PI_PROVIDER=${cfg.pi_provider}",
    "--env", f"PI_MODEL=${cfg.pi_model}",
    "--env", f"PI_THINKING=${cfg.pi_thinking}",
    "--env", f"PI_TELEMETRY=${cfg.pi_telemetry}",
    "--env", f"PI_OFFLINE=${cfg.pi_offline}",
    "--env", f"PI_CODING_AGENT_DIR=${cfg.pi_agent_dir.display()}",
  ]
}

## Runs a docker container with the given flags, mounts, envs, image, and
## command; returns the container exit code. Stdio is inherited, so agent
## output streams to the terminal.
export proc run_container(
  cfg: Config,
  flags: List[Str],
  mounts: List[Str],
  envs: List[Str],
  image: Str,
  command: List[Str],
) [process, error] -> Result[Int] {
  let argv = ["run"].extend(flags).extend(mounts).extend(envs).extend([image]).extend(command)
  let handle = spawn run $cfg.docker @argv ?
  let status = wait handle?
  if status.ok {
    return 0
  }
  return status.exit_code() ?? 1
}

## Copies the session jsonl and html out of the session volume.
export proc copy_session_out(cfg: Config) [process, error] -> Result[Unit] {
  let cid = run.text $cfg.docker create "-v" f"${cfg.session_volume}:/session" $cfg.image true ?
  let container = cid.trim()
  best_effort([cfg.docker, "cp", f"${container}:/session/${cfg.session_name}.jsonl", fp"${cfg.output_dir}/session.jsonl".display()])?
  best_effort([cfg.docker, "cp", f"${container}:/session/${cfg.session_name}.html", fp"${cfg.output_dir}/session.html".display()])?
  best_effort([cfg.docker, "rm", container])?
}

## Checks that /work/review.md exists, is non-empty, and keeps the two
## template section headings.
export proc check_review(work_dir: Path) [fs, error] -> Result[Bool] {
  let review_file = fp"${work_dir}/review.md"
  if ! fs.exists(review_file)? {
    return false
  }
  let meta = fs.metadata(review_file)?
  if meta.size == 0 {
    return false
  }
  let text = fs.read_text(review_file)?
  return text.contains("## XSH language proposals") and text.contains("## xsht friction")
}

## Removes the session volume after results are copied out.
export proc remove_session_volume(cfg: Config) [process, error] -> Result[Unit] {
  best_effort([cfg.docker, "volume", "rm", cfg.session_volume])?
}
