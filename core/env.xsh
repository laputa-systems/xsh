#!/bin/xsh
error EnvError = Usage(message: Str) : Usage | Failed(message: Str)

pure split_words(text: Str) -> List[Str] {
  text.split(" ") |> where . != ""
}

pure is_assignment(arg: Str) -> Bool {
  let parts = arg.split("=", maxsplit: 1)
  parts.len() > 1 and parts[0] != ""
}

proc print_environment() [env] {
  for item in env.list() |> sort-by .name {
    print f"${item.name}=${item.value}"
  }
}

proc handle_status(status: Status) [error] {
  if status.ok {
    return
  }

  if status.exited() {
    abort(status.exit_code()?)
  }

  return Err(EnvError.Failed("command was signaled"))
}

proc main(...raw: List[Str]) [process, env, error] {
  var argv = raw

  if argv.len() >= 1 and argv[0].starts_with("-S ") {
    argv = split_words((argv[0].split(" ") |> drop(1)).join(" "))
    var rest_index = 1

    while rest_index < raw.len() {
      argv = argv.push(raw[rest_index])
      rest_index += 1
    }
  } else if argv.len() >= 2 and argv[0] == "-S" {
    argv = split_words(argv[1])
    var rest_index = 2

    while rest_index < raw.len() {
      argv = argv.push(raw[rest_index])
      rest_index += 1
    }
  }

  if argv.len() == 0 {
    print_environment()
    return
  }

  var path_update = ""
  var xsh_module_path_update = ""
  var index = 0

  while index < argv.len() and is_assignment(argv[index]) {
    let parts = argv[index].split("=", maxsplit: 1)
    let value = parts.get(1, "")

    match parts[0] {
      "PATH" => path_update = value
      "XSH_MODULE_PATH" => xsh_module_path_update = value
      _ => return Err(EnvError.Usage(f"env: unsupported assignment ${parts[0]}"))
    }

    index += 1
  }

  if index >= argv.len() {
    print_environment()
    return
  }

  var command_argv: List[Str] = []

  while index < argv.len() {
    command_argv = command_argv.push(argv[index])
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
    )?
  } else if path_update != "" {
    handle_status(process.run(process.command_argv(command_argv[0], command_argv, env: {PATH: path_update}))?)?
  } else if xsh_module_path_update != "" {
    handle_status(
      process.run(process.command_argv(command_argv[0], command_argv, env: {XSH_MODULE_PATH: xsh_module_path_update}))?,
    )?
  } else {
    handle_status(process.run(process.command_argv(command_argv[0], command_argv))?)?
  }
}
