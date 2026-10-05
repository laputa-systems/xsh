proc require_root(uid: Int) {
  # begin example
  guard uid == 0 else {
    eprint "must run as root"
    exit 77
  }
  # end example
}

require_root(0)
