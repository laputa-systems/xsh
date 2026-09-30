const build_defaults = {jobs: 4, timeout: 30s}

pure add(left: Int, right: Int) {
  left + right
}

pure build_jobs(jobs = build_defaults.jobs + 1) -> Int {
  jobs
}

pure initial_jobs() -> Int { 4 }

pure selected_jobs(jobs = initial_jobs()) -> Int {
  jobs
}
