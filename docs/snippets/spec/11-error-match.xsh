proc wait_for(command: Command) [process, error, io] -> Result[Unit] {
  # begin example
  match process.run(command) {
    Ok(status) => print ${status.ok}
    Err(ProcessError.Timeout { message }) => print $message
    Err(is PermissionDenied) => print "permission denied"
    Err(error) => return Err(error)
  }
  # end example
}
