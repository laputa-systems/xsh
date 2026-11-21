type Job = {argv: List[Str]}
let job = Job(argv: ["true"])
run @job.argv
