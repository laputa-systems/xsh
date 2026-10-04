const prefix = "XDG"

proc configure() [process, env, error] {
  # begin example
  let home = e"HOME"? # e"HOME" is Result[Str], like env.Str.HOME
  let editor = e"VISUAL" ?? e"EDITOR" ?? "vi"
  let config = env.get(f"{prefix}_CONFIG_HOME") ?? f"{home}/.config"

  e"EDITOR" = editor # later child processes inherit it
  e"PORT" = 8080 # converted like an argv item
  env LC_ALL=C {
    e"TZ" = "UTC" # undone when this scope ends
    run date
  }
  # end example
  print $config
}

configure()
