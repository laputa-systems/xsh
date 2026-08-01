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

proc main() [fs, process, env, error, io] {
  defer copy_results()?
  var eval_status = 0

  if fs.exists(p"/work/ecount.xsh")? {
    let candidate_status = run.status xsh /work/ecount.xsh /usr/share > /session/candidate.stdout
    if ! candidate_status.ok {
      eval_status = candidate_status.exit_code() ?? 1
    }
    if eval_status == 0 {
      # The oracle is the byte-exact sh pipeline; keep it as one command so
      # its semantics and output do not drift from the original harness.
      let oracle_status = run.status sh -c "fd --color=never -tf . /usr/share | awk -F. 'NF > 1 {print tolower(\$NF)}' | sort | uniq -c | sort -n" > /session/oracle.stdout
      if ! oracle_status.ok {
        eval_status = 1
      }
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
  }, pretty: true)?
  abort(eval_status)
}
