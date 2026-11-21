use svc

type Person = {person: Str, pages: Int, critical: Int}

type TeamReport = {team: Str, people: List[Person], unassigned: Int}

proc shifts_of(team: Str) -> Result[List[svc.Shift]] {
  retry [200ms, 200ms, 200ms] on (svc.ServiceError.Timeout | svc.ServiceError.Unavailable | svc.ServiceError.RateLimited) {
    svc.oncall_shifts(team)?
  }
}

proc pages_of(team: Str) -> Result[List[svc.PageEvent]] {
  retry [200ms, 200ms, 200ms] on (svc.ServiceError.Timeout | svc.ServiceError.Unavailable | svc.ServiceError.RateLimited) {
    svc.oncall_pages(team)?
  }
}

pure summarize(team: Str, shifts: List[svc.Shift], events: List[svc.PageEvent]) -> TeamReport {
  var seen: List[Str] = []
  var people: Map[Str, Person] = {}
  for s in shifts {
    people[s.person] = Person(person: s.person, pages: 0, critical: 0)
  }

  var unassigned = 0
  for event in events {
    if event.id in seen {
      continue
    }

    seen += [event.id]
    let owners = [s.person for s in shifts if s.start <= event.at and event.at < s.end]
    if owners.is_empty() {
      unassigned += 1
      continue
    }

    let current = people.get(owners[0]) ?? Person(person: owners[0], pages: 0, critical: 0)
    let critical = if event.severity == "critical" { 1 } else { 0 }
    people[owners[0]] = {...current, pages: current.pages + 1, critical: current.critical + critical}
  }

  let ordered = people.values() |> sort-by { |p| {first: 0 - p.pages, second: p.person} }
  TeamReport(team:, people: ordered, unassigned:)
}

var teams: List[Str] = []
for line in p"/work/teams.txt".read_text()?.split("\n") {
  let name = line.trim()
  if name != "" and ! name.starts_with("#") and name not in teams {
    teams += [name]
  }
}

var reports: List[TeamReport] = []
var unknown: List[Str] = []
var failed: List[Str] = []

for team in teams {
  let shifts = match shifts_of(team) {
    Ok(found) => found
    Err(svc.ServiceError.NotFound) => {
      unknown += [team]
      continue
    }
    Err(_) => {
      failed += [team]
      continue
    }
  }

  let events = match pages_of(team) {
    Ok(found) => found
    Err(svc.ServiceError.NotFound) => {
      unknown += [team]
      continue
    }
    Err(_) => {
      failed += [team]
      continue
    }
  }

  reports += [summarize(team, shifts, events)]
}

print (json.encode({teams: reports, unknown, failed})?)
if ! failed.is_empty() {
  exit 4
}
