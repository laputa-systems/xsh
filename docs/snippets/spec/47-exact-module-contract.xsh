const service_path = p"services/cache.xsh"

# begin example
type Service = exact module {
  export let name: Str
  export optional let description: Str
  export proc start() [process, error] -> Result[Unit, Error]
  export proc stop() [process, error] -> Result[Unit, Error]
}

# end example

proc restart_service() [fs, process, error, io] -> Result[Unit] {
  # begin example
  match module.load(service_path)?.require(Service) {
    Ok(service) => {
      service.stop()
      service.start()
    }
    Err(is UnexpectedExport) => print "the service exports more than its contract allows"
    Err(error) => return Err(error)
  }
  # end example
}
