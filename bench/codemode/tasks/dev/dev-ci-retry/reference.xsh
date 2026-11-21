use svc

let repos = [line.trim() for line in p"/work/repos.txt".lines()? if ! line.trim().is_empty()]

for repo in repos {
  let found = retry [100ms, 200ms, 400ms] on (svc.ServiceError.Timeout {..} | svc.ServiceError.Unavailable {..} | svc.ServiceError.RateLimited {..}) {
    svc.ci_latest(repo)?
  }

  let state = match found {
    Ok(ci) => ci.state
    Err(_) => "missing"
  }

  print (json.encode({repo: repo, state: state})?)
}
