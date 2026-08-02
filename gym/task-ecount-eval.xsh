##! Evaluates the task-ecount run inside the eval container: runs the
##! candidate, compares it with the fd/awk oracle, checks review.md, and
##! writes the run manifest. Result files are copied to /export on exit.

use gym

proc copy_results() [fs, error] -> Result[Unit] {
  for name in ["session.jsonl", "session.html", "run.json", "candidate.stdout", "oracle.stdout"] {
    let src = fp"/session/${name}"
    if fs.exists(src)? {
      fs.copy(src, fp"/export/${name}", overwrite: true)?
    }
  }
  for name in ["ecount.xsh", "review.md"] {
    let src = fp"/work/${name}"
    if fs.exists(src)? {
      fs.copy(src, fp"/export/${name}", overwrite: true)?
    }
  }
}

proc main() [fs, process, env, time, error, io] {
  defer copy_results()?
  var eval_status = 0
  var candidate_wall_ns = 0
  var candidate_user_ns = 0
  var candidate_system_ns = 0
  var oracle_wall_ns = 0
  var oracle_user_ns = 0
  var oracle_system_ns = 0

  if fs.exists(p"/work/ecount.xsh")? {
    let candidate = time.measure(process.command_argv(
      "xsh",
      ["xsh", "/work/ecount.xsh", "/usr/share"],
      stdout: p"/session/candidate.stdout",
    ))?
    candidate_wall_ns = candidate.wall_ns
    candidate_user_ns = candidate.user_ns
    candidate_system_ns = candidate.system_ns
    let candidate_status = candidate.status
    if ! candidate_status.ok {
      eval_status = candidate_status.exit_code() ?? 1
    }
    # The oracle is the byte-exact sh pipeline; keep it as one command so its
    # semantics and output do not drift from the original harness.
    let oracle = time.measure(process.command_argv(
      "sh",
      ["sh", "-c", "fd --color=never -tf . /usr/share | awk -F. 'NF > 1 {print tolower(\$NF)}' | sort | uniq -c | sort -n"],
      stdout: p"/session/oracle.stdout",
    ))?
    oracle_wall_ns = oracle.wall_ns
    oracle_user_ns = oracle.user_ns
    oracle_system_ns = oracle.system_ns
    if ! oracle.status.ok {
      eval_status = 1
    }
    if eval_status == 0 {
      let candidate = fs.read_text(p"/session/candidate.stdout")?
      let oracle = fs.read_text(p"/session/oracle.stdout")?
      if candidate == oracle {
        print "task-ecount evaluation passed"
      } else {
        eprint "task-ecount evaluation failed"
        eval_status = 1
      }
    }
  } else {
    eprint "pi completed without creating /work/ecount.xsh"
    eval_status = 1
  }

  if check_review(p"/work")? {
    print "task-ecount evaluation passed (review.md)"
  } else {
    eprint "task-ecount evaluation failed: review.md missing or incomplete"
    eval_status = 1
  }

  let agents_sha = hash.sha256(p"/work/agents.md")?.hex()
  let handbook_sha = hash.sha256(p"/work/handbook.md")?.hex()
  let task_sha = hash.sha256(p"/work/task-ecount.md")?.hex()
  let candidate_sha = if fs.exists(p"/session/candidate.stdout")? {
    hash.sha256(p"/session/candidate.stdout")?.hex()
  } else {
    ""
  }
  let oracle_sha = if fs.exists(p"/session/oracle.stdout")? {
    hash.sha256(p"/session/oracle.stdout")?.hex()
  } else {
    ""
  }
  let result = if eval_status == 0 { "pass" } else { "fail" }
  json.write(p"/session/run.json", {
    image_id: env.Str.GYM_IMAGE_ID?,
    platform: env.Str.GYM_PLATFORM?,
    provider: env.Str.PI_PROVIDER?,
    model: env.Str.PI_MODEL?,
    thinking: env.Str.PI_THINKING?,
    telemetry: env.Str.PI_TELEMETRY?,
    offline: env.Str.PI_OFFLINE?,
    result: result,
    session: "/session/task-ecount-session.jsonl",
    inputs: {agents_sha256: agents_sha, handbook_sha256: handbook_sha, task_sha256: task_sha},
    outputs: {candidate_sha256: candidate_sha, oracle_sha256: oracle_sha},
    timings: {
      candidate_wall_ns: candidate_wall_ns,
      candidate_user_ns: candidate_user_ns,
      candidate_system_ns: candidate_system_ns,
      oracle_wall_ns: oracle_wall_ns,
      oracle_user_ns: oracle_user_ns,
      oracle_system_ns: oracle_system_ns,
    },
  }, pretty: true)?
  abort(eval_status)
}
