type Job = {name: Str, argv: List[Str], cwd: Str, env: Map[Str]}

type Outcome = {name: Str, exit: Int?, stdout: Str, stderr: Str}

proc run_job(job: Job) -> Outcome {
  let argv = job.argv
  var outcome = Outcome(name: job.name, exit: null, stdout: "", stderr: "")
  cd (fp"/work/{job.cwd}") {
    env (job.env) {
      match try run.capture --text @argv {
        Ok(out) => {
          outcome = Outcome(name: job.name, exit: out.status.exit_code() ?? -1, stdout: out.stdout, stderr: out.stderr)
        }
        Err(_) => {}
      }
    }
  }

  outcome
}

let jobs = json.read(p"/work/jobs.json")?.require(List[Job])?
let results = [run_job(job) for job in jobs]
p"/work/results.json".write(json.encode(results)?)
