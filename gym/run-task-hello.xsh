##! Runs the task-hello gym task end to end: prepares the workspace, runs the
##! agent container, evaluates answer.txt and review.md on the host, and
##! records the run manifest.

use gym

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let cfg = parse_config(
    env.get_or("DOCKER", "docker")?,
    env.get_or("PLATFORM", "linux/arm64")?,
    env.path("GYM_DIR")?,
    env.path("WORK_DIR")?,
    env.path("OUTPUT_DIR")?,
    env.get("SESSION_VOLUME")?,
    "task-hello-session",
    "task-hello.md",
    env.get("BASE_IMAGE")?,
  )?

  prepare_workdir(cfg, ["answer.txt", "review.md", "session.jsonl", "session.html", "run.json"])?
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

  # Evaluate on the host: /work is a host bind mount (work_dir).
  var eval_status = 0
  if fs.exists(fp"${cfg.work_dir}/answer.txt")? {
    let content = fs.read_text(fp"${cfg.work_dir}/answer.txt")?.trim()
    if content == "hello" {
      print "task-hello evaluation passed (answer.txt)"
    } else {
      eprint "task-hello evaluation failed: unexpected content"
      eval_status = 1
    }
  } else {
    eprint "pi completed without creating /work/answer.txt"
    eval_status = 1
  }

  if check_review(cfg.work_dir)? {
    print "task-hello evaluation passed (review.md)"
  } else {
    eprint "task-hello evaluation failed: review.md missing or incomplete"
    eval_status = 1
  }

  copy_session_out(cfg)?
  let task_sha = hash.sha256(fp"${cfg.gym_dir}/task-hello.md")?.hex()
  let answer_sha = if fs.exists(fp"${cfg.work_dir}/answer.txt")? {
    hash.sha256(fp"${cfg.work_dir}/answer.txt")?.hex()
  } else {
    ""
  }
  let result = if eval_status == 0 { "pass" } else { "fail" }
  json.write(fp"${cfg.output_dir}/run.json", {
    image_id: image_ref,
    platform: cfg.platform,
    provider: cfg.pi_provider,
    model: cfg.pi_model,
    thinking: cfg.pi_thinking,
    telemetry: cfg.pi_telemetry,
    offline: cfg.pi_offline,
    result: result,
    session: f"/session/${cfg.session_name}.jsonl",
    inputs: {task_sha256: task_sha},
    outputs: {answer_sha256: answer_sha},
  }, pretty: true)?

  remove_session_volume(cfg)?
  if agent_status != 0 {
    abort(agent_status)
  }
  abort(eval_status)
}
