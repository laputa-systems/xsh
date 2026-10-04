run --timeout=30s --cpumax=80 make check
let status = run.status --accept=[0, 1] grep -q pattern file
