match net.validate_root_bundle(b"not a PEM certificate") {
  Ok(_) => print "valid bundle"
  Err(NetError.TrustStore {message}) => print $message
  Err(error) => print ${error.message}
}
