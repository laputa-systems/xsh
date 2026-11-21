use svc

type Payout = {ref: Str, account: Str, cents: Int}

type Sent = {ref: Str, transfer: Str}

type Failed = {ref: Str, kind: Str}

type Settled = {sent: Str?, failed: Str?}

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

pure transient(failure: Error) -> Bool {
  failure is (svc.ServiceError.Timeout | svc.ServiceError.Unavailable | svc.ServiceError.RateLimited)
}

proc settle(payout: Payout) -> Result[Settled] {
  var sends = 0
  while true {
    sends += 1
    let failure = match svc.payouts_send(payout.ref, payout.account, payout.cents, timeout: 500ms) {
      Ok(receipt) => {
        return Ok(Settled(sent: receipt.transfer, failed: null))
      }
      Err(failure) => failure
    }

    if ! transient(failure) {
      return Ok(Settled(sent: null, failed: kind_of(failure)))
    }

    let state = match svc.payouts_status(payout.ref) {
      Ok(state) => state
      Err(_) => {
        return Ok(Settled(sent: null, failed: null))
      }
    }

    if state.transfer != null {
      return Ok(Settled(sent: state.transfer, failed: null))
    }

    if sends == 3 {
      return Ok(Settled(sent: null, failed: kind_of(failure)))
    }

    time.sleep(200ms)?
  }

  Ok(Settled(sent: null, failed: null))
}

let payouts = json.read(p"/work/payouts.json")?.require(List[Payout])?

var sent: List[Sent] = []
var failed: List[Failed] = []
var unknown: List[Str] = []

for payout in payouts {
  let settled = settle(payout)?
  if settled.sent != null {
    sent += [Sent(ref: payout.ref, transfer: settled.sent)]
  } else if settled.failed != null {
    failed += [Failed(ref: payout.ref, kind: settled.failed)]
  } else {
    unknown += [payout.ref]
  }
}

json.write(p"/work/payout-report.json", {sent, failed, unknown})?
print f"sent={sent.len()} failed={failed.len()} unknown={unknown.len()}"

if ! failed.is_empty() or ! unknown.is_empty() {
  exit 4
}
