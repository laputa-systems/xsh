#!/usr/bin/env -S xsh --
##! Runs one leaf of a gate case inside the container: Cargo harness preparation or the documentation check. A leaf holds only
##! the kernel-released cargo flock and never a quarantine lease, so its hooks exit 130 at once instead of waiting.
use context
use gates

proc main(...arguments: List[Str]) [fs, process, env, error, io] {
  guard arguments.len() >= 2 else {
    return Err(gates.PlanError.Invalid(detail: "gate case requires prepare|execute, case ID and optional changed paths"))
  }
  let action = match arguments[0] {
    "prepare" => gates.Prepare
    "execute" => gates.ExecuteLeaf
    else => { return Err(gates.PlanError.Invalid(detail: "unsupported gate case action")) }
  }
  gates.run_case(context.create()?, action, arguments[1], arguments |> drop(2))
}

on INT [process] { exit 130 }
on TERM [process] { exit 130 }
