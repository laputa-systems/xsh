error HookFailed = Failed(message: Str)

let helper = fp"{args[0]}"

on USR1 [error] {
  Err(HookFailed.Failed(message: "boom"))?
}

let _ = process.run(process.command_argv(helper, ["os-probe", "signal-parent-then-sleep", "USR1", "50"]))?
