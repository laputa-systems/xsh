const defaults = {jobs: 4, timeout: 30s}
pure jobs(value = defaults.jobs + 1) -> Int { value }
pure timeout(value = defaults.timeout) -> Duration { value }
pure default_path(value = Path("config")) -> Path { value }
print ${jobs()} ${jobs(9)} ${timeout() == 30s} ${default_path().display()}
