proc build(dir: Path) [env, error] {
  # begin example
  cd $dir { # error: parse.scope-command
    print here
  }
  env MODE=fast { # error: parse.scope-command
    print here
  }
  # end example
}
