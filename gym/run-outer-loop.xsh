##! Runs the host-side outer judge for a fixed number of inner task trials.
##!
##! The controller keeps the trial boundary deterministic: the inner Pi is
##! sandboxed by run-task-tags.xsh, while the outer Pi runs on the host and may
##! inspect the repository and update the runtime handbook between trials.

use gym

proc main() [fs, process, env, time, error, io] {
  let gym_dir = env.path("GYM_DIR")?
  let docker = env.get_or("DOCKER", "docker")?
  let platform = env.get_or("PLATFORM", "linux/arm64")?
  let base_image = env.get("BASE_IMAGE")?
  let auth_file = env.path("PI_AUTH_FILE")?
  let pi_command = env.get_or("PI_COMMAND", "pi")?
  let pi_provider = env.get_or("PI_PROVIDER", "openrouter")?
  let pi_model = env.get_or("PI_MODEL", "deepseek/deepseek-v4-flash-0731")?
  let pi_thinking = env.get_or("PI_THINKING", "high")?
  let pi_telemetry = env.get_or("PI_TELEMETRY", "0")?
  let pi_offline = env.get_or("PI_OFFLINE", "1")?
  let pi_agent_dir = env.path("PI_AGENT_DIR", p"/run/pi-agent")?
  let outer_command = env.get_or("OUTER_PI_COMMAND", "pi")?
  let outer_provider = env.get_or("OUTER_PI_PROVIDER", pi_provider)?
  let outer_model = env.get_or("OUTER_PI_MODEL", pi_model)?
  let outer_thinking = env.get_or("OUTER_PI_THINKING", pi_thinking)?
  let outer_offline = env.get_or("OUTER_PI_OFFLINE", "0")?
  let iteration_count = env.get_or("OUTER_ITERATIONS", "2")?.parse_int()?
  if iteration_count < 1 {
    eprint "OUTER_ITERATIONS must be positive"
    abort(2)
  }

  let stamp = time.now()
  let configured_outer_dir = env.get_or("OUTER_DIR", "")?
  let outer_dir = if configured_outer_dir == "" {
    fp"${gym_dir}/.outer/task-tags-${stamp}"
  } else {
    Path(configured_outer_dir)
  }
  let iterations_dir = fp"${outer_dir}/iterations"
  fs.mkdir(outer_dir)?
  fs.mkdir(iterations_dir)?

  let xsh_path = process.which("xsh")?
  let python_path = process.which("python3")?
  let outer_pi_path = process.which(outer_command)?
  let handbook_path = fp"${gym_dir}/runtime/handbook.md"
  let prompt_path = fp"${gym_dir}/outer-agent.md"
  let gym_doc_path = fp"${gym_dir}/../GYM.md"
  var reports: List[Record] = []
  var previous_handbook_sha = ""
  var overall_status = 0
  var iteration = 1

  while iteration <= iteration_count {
    let iteration_dir = fp"${iterations_dir}/${iteration}"
    let work_dir = fp"${iteration_dir}/work"
    let output_dir = fp"${iteration_dir}/output"
    let session_volume = f"xsh-gym-outer-tags-${stamp}-${iteration}"
    let judge_session = fp"${iteration_dir}/judge-session.jsonl"
    let judge_report = fp"${iteration_dir}/judge.md"
    let metrics_path = fp"${output_dir}/metrics.json"
    let handbook_before = hash.sha256(handbook_path)?.hex()
    let change_required = iteration == 1

    let inner_command = process.command_argv(
      xsh_path,
      [xsh_path.display(), f"${gym_dir}/run-task-tags.xsh"],
      env: {
        PATH: env.get("PATH")?,
        HOME: env.get("HOME")?,
        DOCKER: docker,
        PLATFORM: platform,
        GYM_DIR: gym_dir.display(),
        BASE_IMAGE: base_image,
        WORK_DIR: work_dir.display(),
        OUTPUT_DIR: output_dir.display(),
        SESSION_VOLUME: session_volume,
        PI_COMMAND: pi_command,
        PI_PROVIDER: pi_provider,
        PI_MODEL: pi_model,
        PI_THINKING: pi_thinking,
        PI_TELEMETRY: pi_telemetry,
        PI_OFFLINE: pi_offline,
        PI_AUTH_FILE: auth_file.display(),
        PI_AGENT_DIR: pi_agent_dir.display(),
        HANDBOOK_FILE: handbook_path.display(),
      },
    )
    let agent_started = time.now()
    let inner_status = process.run(inner_command)
    let agent_wall_ms = time.now() - agent_started
    let inner_code = match inner_status {
      Ok(status) => if status.ok { 0 } else { status.exit_code() ?? 1 },
      Err(_) => 1,
    }
    if inner_code != 0 {
      overall_status = inner_code
    }

    let metrics_status = process.run(process.command_argv(
      python_path,
      [
        python_path.display(),
        f"${gym_dir}/session-metrics.py",
        "--session", fp"${output_dir}/session.jsonl".display(),
        "--manifest", fp"${output_dir}/run.json".display(),
        "--output", metrics_path.display(),
        "--agent-wall-ms", f"${agent_wall_ms}",
      ],
    ))
    let metrics_code = match metrics_status {
      Ok(status) => if status.ok { 0 } else { status.exit_code() ?? 1 },
      Err(_) => 1,
    }
    if metrics_code != 0 {
      overall_status = metrics_code
    }

    let handbook_used_sha = if fs.exists(fp"${work_dir}/handbook.md")? {
      hash.sha256(fp"${work_dir}/handbook.md")?.hex()
    } else {
      ""
    }

    var judge_args = [
      outer_pi_path.display(),
      "--provider", outer_provider,
      "--model", outer_model,
      "--thinking", outer_thinking,
      "--approve",
      "--no-extensions",
      "--no-skills",
      "--no-prompt-templates",
      "--no-themes",
      "--tools", "read,write,edit,bash,grep,find,ls",
      "--system-prompt", prompt_path.display(),
      "--session", judge_session.display(),
      "--print",
      f"Inspect outer iteration ${iteration} for task-tags. Read ${gym_doc_path.display()} before acting. The run directory is ${iteration_dir.display()}. The inner output directory is ${output_dir.display()}. The exact judge report path is ${judge_report.display()}. handbook_change_required=${change_required}. The handbook hash before this iteration is ${handbook_before}. Analyze the run and write the report. If a handbook change is required, edit ${handbook_path.display()} before writing the report. The next iteration will stage the current handbook.",
    ]
    if outer_offline == "1" {
      judge_args = judge_args.extend(["--offline"])
    }
    let judge_status = process.run(process.command_argv(outer_pi_path, judge_args))
    let judge_code = match judge_status {
      Ok(status) => if status.ok { 0 } else { status.exit_code() ?? 1 },
      Err(_) => 1,
    }
    if judge_code != 0 {
      overall_status = judge_code
    }
    if fs.exists(judge_session)? {
      let _ = process.run(process.command_argv(
        outer_pi_path,
        [outer_pi_path.display(), "--export", judge_session.display(), fp"${iteration_dir}/judge.html".display()],
      ))
    }

    let handbook_after = hash.sha256(handbook_path)?.hex()
    let judge_report_exists = fs.exists(judge_report)?
    if handbook_used_sha != handbook_before {
      eprint "inner iteration did not use the handbook staged for it"
      overall_status = 1
    }
    if ! judge_report_exists {
      eprint "outer judge did not write its required report"
      overall_status = 1
    }
    if change_required and handbook_after == handbook_before {
      eprint "outer judge did not modify the handbook during required iteration"
      overall_status = 1
    }
    if iteration > 1 and handbook_used_sha != previous_handbook_sha {
      eprint "inner iteration did not use the handbook produced by the previous iteration"
      overall_status = 1
    }

    let report = {
      iteration: iteration,
      inner_exit_code: inner_code,
      metrics_exit_code: metrics_code,
      judge_exit_code: judge_code,
      handbook_before_sha256: handbook_before,
      handbook_used_sha256: handbook_used_sha,
      handbook_after_sha256: handbook_after,
      handbook_change_required: change_required,
      judge_report_exists: judge_report_exists,
      output_dir: output_dir.display(),
      metrics: metrics_path.display(),
    }
    json.write(fp"${iteration_dir}/outer.json", report, pretty: true)?
    reports = reports.extend([report])
    previous_handbook_sha = handbook_after
    iteration = iteration + 1
  }

  json.write(fp"${outer_dir}/outer-summary.json", {
    schema_version: 1,
    task: "task-tags",
    iterations: reports,
    final_handbook_sha256: hash.sha256(handbook_path)?.hex(),
    result: if overall_status == 0 { "pass" } else { "fail" },
  }, pretty: true)?
  print f"outer loop results: ${outer_dir.display()}"
  abort(overall_status)
}
