#!/bin/xsh
use lib.auth

type GettyOptions = {
  no_prompt: Bool,
  hardware_flow: Bool,
  local_line: Bool,
  no_issue: Bool,
  issue_file: Path,
  login: Str,
  host: Str,
  init_string: Str,
  baud: Int,
  tty: Str,
  term: Str,
}

pure parse_getty_args(argv: List[Str]) -> Result[GettyOptions] {
  let opts = cli.applet(
    argv,
    {
      no_prompt: {
        form: "-n --no-prompt",
        default: false,
      },
      no_issue: {
        form: "-i --no-issue",
        default: false,
      },
      issue_file: {
        form: "-f --issue-file FILE",
        kind: "Path",
        default: /etc/issue,
      },
      login: {
        form: "-l --login-program PROGRAM",
        default: "",
      },
      host: {
        form: "-H --host HOST",
        default: "",
      },
      init_string: {
        form: "-I --init-string STRING",
        default: "",
      },
      gnu: {unsupported: {"-t": "login-name timeout is not available", "--timeout": "login-name timeout is not available", "-m": "modem baud detection is not available", "-w": "modem carriage-return wait is not available"}},
      hardware_flow: {form: "-h", default: false},
      local_line: {form: "-L", default: false},
      operands: {
        form: "...ARG",
      },
    },
  )?
  let operands = opts.operands

  if operands.len() != 2 and operands.len() != 3 {
    return Err(auth.AuthError.Failed("missing operand"))
  }

  if ! rx"^[0-9]+$".matches(operands[0]) {
    return Err(auth.AuthError.Failed(f"invalid baud rate {operands[0]}; multiple baud rates are not supported"))
  }
  guard let baud = operands[0].parse_int() else {
    return Err(auth.AuthError.Failed(f"invalid baud rate {operands[0]}"))
  }

  {
    no_prompt: opts.no_prompt,
    hardware_flow: opts.hardware_flow,
    local_line: opts.local_line,
    no_issue: opts.no_issue,
    issue_file: opts.issue_file,
    login: opts.login,
    host: opts.host,
    init_string: opts.init_string,
    baud: baud,
    tty: operands[1],
    term: if operands.len() == 3 { operands[2] } else { "" },
  }
}

# The terminal named by getty can differ from its inherited standard streams.
# Open without waiting for carrier and preserve every unrelated termios field.
proc configure_serial(options: GettyOptions) [process, error] {
  let table = unix.tty_table()
  if options.baud not in table.speeds { return Err(auth.AuthError.Failed(f"unsupported baud rate {options.baud}")) }
  # A zero baud operand retains the line speed inherited from the service.
  return unless options.baud != 0 or options.hardware_flow or options.local_line
  let fd = if options.tty == "-" { 0 } else {
    let terminal_path = if options.tty.starts_with("/") { fp"{options.tty}" } else { fp"/dev/{options.tty}" }
    unix.open_fd(terminal_path, true, true)?
  }
  defer { if fd != 0 { unix.close_fd(fd) } }
  let attrs = unix.tty_attrs(fd)?
  var cflag = attrs.cflag
  for requested in [if options.hardware_flow { "crtscts" } else { "" }, if options.local_line { "clocal" } else { "" }] {
    continue when requested == ""
    var found = false
    for flag in table.flags {
      if flag.name == requested {
        cflag = cflag.clear_bits(flag.mask).bit_or(flag.value)
        found = true
      }
    }
    if ! found { return Err(auth.AuthError.Failed(f"terminal flag {requested} is unavailable on this platform")) }
  }
  let speed = if options.baud == 0 { attrs.ospeed } else { options.baud }
  let input_speed = if options.baud == 0 { attrs.ispeed } else { options.baud }
  unix.set_tty_attrs({...attrs, cflag: cflag, ispeed: input_speed, ospeed: speed}, fd)
}

proc run_external_login(options: GettyOptions, username: Str) [process, error] -> Result[Int] {
  let login = if options.login == "" { process.which("login")?.display() } else { options.login }
  var argv = [login]

  if options.host != "" {
    argv += ["-h"]
    argv += [options.host]
  }

  if username != "" {
    argv += [username]
  }

  let env_record = if options.term == "" { {} } else { {TERM: options.term} }
  let status = process.run(process.command_argv(login, argv, env: env_record))?

  return status.exit_code()? when status.exited()

  1
}

proc main(...argv: List[Str]) [fs, process, error, io] -> Result[Int] {
  let options = parse_getty_args(argv)?
  configure_serial(options)

  if options.init_string != "" {
    io.write_stdout(options.init_string)
  }

  if ! options.no_issue and options.issue_file.exists() {
    io.write_stdout(options.issue_file.read_text()?)
  }

  var username = ""

  if ! options.no_prompt {
    username = tui.read_secret("login: ")?
  }

  run_external_login(options, username)?
}

exit main(@args)?
