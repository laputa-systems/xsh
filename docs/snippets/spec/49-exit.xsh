proc require_root(uid: Int) {
  # begin example
  if uid != 0 {
    eprint "must run as root"
    exit 77
  }
  # end example
}

require_root(0)
