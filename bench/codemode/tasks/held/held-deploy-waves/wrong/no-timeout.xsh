use svc

type Plan = {release: Str, waves: List[List[Str]]}

type Deployed = {host: Str, revision: Int}

type Failed = {host: Str, kind: Str}

pure kind_of(failure: Error) -> Str {
  match failure {
    svc.ServiceError.Timeout => "Timeout"
    svc.ServiceError.Unavailable => "Unavailable"
    svc.ServiceError.RateLimited => "RateLimited"
    svc.ServiceError.NotFound => "NotFound"
    svc.ServiceError.PermissionDenied => "PermissionDenied"
    svc.ServiceError.Invalid => "Invalid"
    svc.ServiceError.Conflict => "Conflict"
    else => "Failed"
  }
}

let plan = json.read(p"/work/rollout.json")?.require(Plan)?

var seen: List[Str] = []
var deployed: List[Deployed] = []
var failed: List[Failed] = []
var skipped: List[Str] = []

for wave in plan.waves {
  var hosts: List[Str] = []
  for host in wave {
    if host not in seen {
      seen += [host]
      hosts += [host]
    }
  }

  if ! failed.is_empty() {
    skipped += hosts
    continue
  }

  let results = hosts |> par-map(jobs: 2) { |host|
    {host, result: svc.deploy_start(host, plan.release)}
  }

  for {host, result} in results {
    match result {
      Ok(started) => deployed += [Deployed(host:, revision: started.revision)]
      Err(failure) => failed += [Failed(host:, kind: kind_of(failure))]
    }
  }
}

print (json.encode({deployed, failed, skipped})?)

if ! failed.is_empty() {
  exit 4
}
