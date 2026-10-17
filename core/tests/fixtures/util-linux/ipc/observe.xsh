type ApiObservation = {
  errno: Int?,
  value: Int?,
  waiting_for_increase: Int?,
  waiting_for_zero: Int?,
  pid: Int?,
}

type ApiReport = {before: Int, after: Int, observations: List[ApiObservation]}

proc observe_api(operation: Str, identity: Str, index: Int) [process, error] -> Result[ApiObservation, Error] {
  if operation == "posix_semaphore" {
    match linux.posix_semaphore_value(bytes.from_text(identity)) {
      Ok(value) => Ok({errno: null, value: value, waiting_for_increase: null, waiting_for_zero: null, pid: null})
      Err(failure) => Ok({errno: failure.errno, value: null, waiting_for_increase: null, waiting_for_zero: null, pid: null})
    }
  } else if operation == "posix_mqueue" {
    match linux.posix_message_queue_messages(bytes.from_text(identity)) {
      Ok(value) => Ok({errno: null, value: value, waiting_for_increase: null, waiting_for_zero: null, pid: null})
      Err(failure) => Ok({errno: failure.errno, value: null, waiting_for_increase: null, waiting_for_zero: null, pid: null})
    }
  } else if operation == "sysv_semaphore" {
    let id = identity.parse_int()?
    match linux.system_v_semaphore_element(id, index) {
      Ok(element) => Ok({
        errno: null,
        value: element.value,
        waiting_for_increase: element.waiting_for_increase,
        waiting_for_zero: element.waiting_for_zero,
        pid: element.pid,
      })
      Err(failure) => Ok({errno: failure.errno, value: null, waiting_for_increase: null, waiting_for_zero: null, pid: null})
    }
  } else {
    Err(error.failure(f"unknown IPC observation {operation}"))
  }
}

let pid = process.current_pid()?
let before = linux.open_files(pid)?.collect().len()
let index = if args.len() > 2 { args[2].parse_int()? } else { 0 }
var observations: List[ApiObservation] = []
for _ in range(8) {
  observations = observations.extend([observe_api(args[0], args[1], index)?])
}
let after = linux.open_files(pid)?.collect().len()
let report: ApiReport = {before: before, after: after, observations: observations}
print json.encode(report)?
