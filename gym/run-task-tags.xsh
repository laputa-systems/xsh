##! Runs task-tags end to end: prepares the workspace, runs the inner Pi
##! agent, evaluates the artifact in a second container, and copies out the
##! session and manifest.

use gym

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let cfg = parse_config(
    env.get_or("DOCKER", "docker")?,
    env.get_or("PLATFORM", "linux/arm64")?,
    env.path("GYM_DIR")?,
    env.path("WORK_DIR")?,
    env.path("OUTPUT_DIR")?,
    env.get("SESSION_VOLUME")?,
    "task-tags-session",
    "task-tags.md",
    env.get("BASE_IMAGE")?,
  )?

  prepare_workdir(cfg, [
    "tag.xsh", "review.md",
    "session.jsonl", "session.html", "run.json",
    "candidate.1.stdout", "candidate.2.stdout", "candidate.3.stdout",
    "oracle.1.stdout", "oracle.2.stdout", "oracle.3.stdout",
  ])?
  ensure_auth(cfg)?
  reset_session_volume(cfg)?
  let image_ref = image_id(cfg)?

  let flags = agent_flags(cfg)
  let mounts = [
    "--mount", f"type=bind,src=${cfg.auth_file.display()},dst=/run/pi-auth.json,readonly",
    "--mount", f"type=bind,src=${cfg.work_dir.display()},dst=/work",
    "--mount", f"type=volume,src=${cfg.session_volume},dst=/session",
    "--mount", f"type=bind,src=${cfg.gym_dir.display()}/gym-agent.xsh,dst=/run/gym-agent.xsh,readonly",
    "--mount", f"type=bind,src=${cfg.work_dir.display()}/agents.md,dst=/work/agents.md,readonly",
    "--mount", f"type=bind,src=${cfg.work_dir.display()}/handbook.md,dst=/work/handbook.md,readonly",
    "--mount", f"type=bind,src=${cfg.work_dir.display()}/${cfg.task_file},dst=/work/${cfg.task_file},readonly",
  ]
  let envs = agent_envs(cfg)
  let command = [
    "xsh", "/run/gym-agent.xsh", "--",
    f"/session/${cfg.session_name}.jsonl",
    f"/work/${cfg.task_file}",
  ]
  let agent_status = run_container(cfg, flags, mounts, envs, cfg.image, command)?

  let eval_flags = [
    "--rm",
    "--platform", cfg.platform,
    "--read-only",
    "--tmpfs", "/tmp:rw,noexec,nosuid,nodev",
    "--cap-drop=ALL",
    "--security-opt=no-new-privileges",
    "--workdir", "/work",
  ]
  let eval_mounts = [
    "--mount", f"type=bind,src=${cfg.work_dir.display()},dst=/work,readonly",
    "--mount", f"type=volume,src=${cfg.session_volume},dst=/session",
    "--mount", f"type=bind,src=${cfg.output_dir.display()},dst=/export",
    "--mount", f"type=bind,src=${cfg.gym_dir.display()}/gym.xsh,dst=/run/gym.xsh,readonly",
    "--mount", f"type=bind,src=${cfg.gym_dir.display()}/task-tags-eval.xsh,dst=/run/task-tags-eval.xsh,readonly",
  ]
  let eval_envs = [
    "--env", f"GYM_IMAGE_ID=${image_ref}",
    "--env", f"GYM_PLATFORM=${cfg.platform}",
    "--env", f"PI_PROVIDER=${cfg.pi_provider}",
    "--env", f"PI_MODEL=${cfg.pi_model}",
    "--env", f"PI_THINKING=${cfg.pi_thinking}",
    "--env", f"PI_TELEMETRY=${cfg.pi_telemetry}",
    "--env", f"PI_OFFLINE=${cfg.pi_offline}",
  ]
  let eval_status = run_container(
    cfg,
    eval_flags,
    eval_mounts,
    eval_envs,
    cfg.image,
    ["xsh", "/run/task-tags-eval.xsh"],
  )?

  copy_session_out(cfg)?
  remove_session_volume(cfg)?
  if agent_status != 0 {
    abort(agent_status)
  }
  abort(eval_status)
}
