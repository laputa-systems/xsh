error FrontendError = Failed(message: Str)

type Job = {name: Str, attempts: Int, enabled: Bool}

proc classify(job: Job) [error] -> Result[Str] {
  if ! job.enabled {
    return "skip"
  }

  if job.attempts > 3 {
    return Err(FrontendError.Failed(f"too many attempts for ${job.name}"))
  }

  match job.name {
    "build" => return "compile"
    "test" => return "verify"
    _ => return "run"
  }
}

let jobs = [
  {name: "build", attempts: 1, enabled: true},
  {name: "test", attempts: 2, enabled: true},
  {name: "deploy", attempts: 1, enabled: false},
]

var labels = [classify(job)? for job in jobs]
print labels.join(",")
