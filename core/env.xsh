#!/bin/xsh
error EnvError = Usage : Usage | Failed

pure split_words(text: Str) -> List[Str] {
  text.split(" ") |> where . != ""
}

pure is_assignment(arg: Str) -> Bool {
  let parts = arg.split("=", maxsplit: 1)
  parts.len() > 1 and parts[0] != ""
}

proc print_environment() [env] {
  for item in env.list() |> sort-by .name {
    print f"{item.name}={item.value}"
  }
}

proc handle_status(status: Status) [error] {
  return when status.ok

  if status.exited() {
    exit status.exit_code()?
  }

  return Err(EnvError.Failed("command was signaled"))
}

proc main(...raw: List[Str]) [process, env, error] {
  var argv = raw

  if argv.len() >= 1 and argv[0].starts_with("-S ") {
    argv = split_words(argv[0].split(" ") |> drop(1).join(" "))
    var rest_index = 1

    while rest_index < raw.len() {
      argv += [raw[rest_index]]
      rest_index += 1
    }
  } else if argv.len() >= 2 and argv[0] == "-S" {
    argv = split_words(argv[1])
    var rest_index = 2

    while rest_index < raw.len() {
      argv += [raw[rest_index]]
      rest_index += 1
    }
  }

  if argv.is_empty() {
    print_environment()
    return
  }

  var path_update = ""
  var xsh_module_path_update = ""
  var index = 0

  while index < argv.len() and is_assignment(argv[index]) {
    let parts = argv[index].split("=", maxsplit: 1)
    let value = parts.get(1) ?? ""

    match parts[0] {
      "PATH" => path_update = value
      "XSH_MODULE_PATH" => xsh_module_path_update = value
      else => return Err(EnvError.Usage(f"env: unsupported assignment {parts[0]}"))
    }

    index += 1
  }

  if index >= argv.len() {
    print_environment()
    return
  }

  var command_argv = []

  while index < argv.len() {
    command_argv += [argv[index]]
    index += 1
  }

  if path_update != "" and xsh_module_path_update != "" {
    handle_status(
      process.run(
        process.command_argv(
          command_argv[0],
          command_argv,
          env: {PATH: path_update, XSH_MODULE_PATH: xsh_module_path_update},
        ),
      )?,
    )
  } else if path_update != "" {
    handle_status(process.run(process.command_argv(command_argv[0], command_argv, env: {PATH: path_update}))?)
  } else if xsh_module_path_update != "" {
    handle_status(
      process.run(process.command_argv(command_argv[0], command_argv, env: {XSH_MODULE_PATH: xsh_module_path_update}))?,
    )
  } else {
    let status = run.status @command_argv ?
    handle_status(status)
  }
}
