##! Evaluates task-tags in the eval container: runs the candidate against
##! three argument cases, compares each result with an external printf oracle,
##! checks the review and subprocess boundary, and writes the run manifest.

use gym

proc copy_results() [fs, error] -> Result[Unit] {
  for name in [
    "session.jsonl", "session.html", "run.json",
    "candidate.1.stdout", "candidate.2.stdout", "candidate.3.stdout",
    "oracle.1.stdout", "oracle.2.stdout", "oracle.3.stdout",
  ] {
    let src = fp"/session/${name}"
    if fs.exists(src)? {
      fs.copy(src, fp"/export/${name}", overwrite: true)?
    }
  }
  for name in ["tag.xsh", "review.md"] {
    let src = fp"/work/${name}"
    if fs.exists(src)? {
      fs.copy(src, fp"/export/${name}", overwrite: true)?
    }
  }
}

proc main() [fs, process, env, time, error, io] {
  defer copy_results()?
  var eval_status = 0
  var public_exact = false
  var hidden_exact = false
  var empty_exact = false
  var forbidden_operations = false

  if fs.exists(p"/work/tag.xsh")? {
    let public_candidate = time.measure(process.command_argv(
      "xsh",
      ["xsh", "/work/tag.xsh", "Alpha", "Two Words", "BETA"],
      stdout: p"/session/candidate.1.stdout",
      stderr: p"/session/candidate.1.stderr",
    ))?
    let public_oracle = time.measure(process.command_argv(
      "printf",
      ["printf", "tags: alpha, two words, beta\\n"],
      stdout: p"/session/oracle.1.stdout",
      stderr: p"/session/oracle.1.stderr",
    ))?

    let hidden_candidate = time.measure(process.command_argv(
      "xsh",
      ["xsh", "/work/tag.xsh", "MiXeD", "", "Three Words"],
      stdout: p"/session/candidate.2.stdout",
      stderr: p"/session/candidate.2.stderr",
    ))?
    let hidden_oracle = time.measure(process.command_argv(
      "printf",
      ["printf", "tags: mixed, , three words\\n"],
      stdout: p"/session/oracle.2.stdout",
      stderr: p"/session/oracle.2.stderr",
    ))?

    let empty_candidate = time.measure(process.command_argv(
      "xsh",
      ["xsh", "/work/tag.xsh"],
      stdout: p"/session/candidate.3.stdout",
      stderr: p"/session/candidate.3.stderr",
    ))?
    let empty_oracle = time.measure(process.command_argv(
      "printf",
      ["printf", "tags:\\n"],
      stdout: p"/session/oracle.3.stdout",
      stderr: p"/session/oracle.3.stderr",
    ))?

    public_exact = public_candidate.status.ok and public_oracle.status.ok and
      fs.read_text(p"/session/candidate.1.stdout")? == fs.read_text(p"/session/oracle.1.stdout")?
    hidden_exact = hidden_candidate.status.ok and hidden_oracle.status.ok and
      fs.read_text(p"/session/candidate.2.stdout")? == fs.read_text(p"/session/oracle.2.stdout")?
    empty_exact = empty_candidate.status.ok and empty_oracle.status.ok and
      fs.read_text(p"/session/candidate.3.stdout")? == fs.read_text(p"/session/oracle.3.stdout")?

    let source = fs.read_text(p"/work/tag.xsh")?
    forbidden_operations = ! source.contains("process.") and
      ! source.contains("spawn ") and ! source.contains("run ")

    if ! public_exact or ! hidden_exact or ! empty_exact or ! forbidden_operations {
      eval_status = 1
    }

    let handbook_sha = hash.sha256(p"/work/handbook.md")?.hex()
    let agents_sha = hash.sha256(p"/work/agents.md")?.hex()
    let task_sha = hash.sha256(p"/work/task-tags.md")?.hex()
    let candidate_sha = hash.sha256(p"/session/candidate.1.stdout")?.hex()
    let oracle_sha = hash.sha256(p"/session/oracle.1.stdout")?.hex()
    json.write(p"/session/run.json", {
      image_id: env.Str.GYM_IMAGE_ID?,
      platform: env.Str.GYM_PLATFORM?,
      provider: env.Str.PI_PROVIDER?,
      model: env.Str.PI_MODEL?,
      thinking: env.Str.PI_THINKING?,
      telemetry: env.Str.PI_TELEMETRY?,
      offline: env.Str.PI_OFFLINE?,
      result: if eval_status == 0 { "pass" } else { "fail" },
      session: "/session/task-tags-session.jsonl",
      inputs: {
        agents_sha256: agents_sha,
        handbook_sha256: handbook_sha,
        task_sha256: task_sha,
      },
      outputs: {
        candidate_sha256: candidate_sha,
        oracle_sha256: oracle_sha,
      },
      correctness: {
        public_exact: public_exact,
        hidden_exact: hidden_exact,
        empty_exact: empty_exact,
        forbidden_operations: forbidden_operations,
      },
      timings: {
        public_candidate_wall_ns: public_candidate.wall_ns,
        public_candidate_user_ns: public_candidate.user_ns,
        public_candidate_system_ns: public_candidate.system_ns,
        public_oracle_wall_ns: public_oracle.wall_ns,
        public_oracle_user_ns: public_oracle.user_ns,
        public_oracle_system_ns: public_oracle.system_ns,
        hidden_candidate_wall_ns: hidden_candidate.wall_ns,
        hidden_oracle_wall_ns: hidden_oracle.wall_ns,
        empty_candidate_wall_ns: empty_candidate.wall_ns,
        empty_oracle_wall_ns: empty_oracle.wall_ns,
      },
    }, pretty: true)?
  } else {
    eprint "pi completed without creating /work/tag.xsh"
    eval_status = 1
  }

  if ! check_review(p"/work")? {
    eprint "task-tags evaluation failed: review.md missing or incomplete"
    eval_status = 1
  }

  abort(eval_status)
}
