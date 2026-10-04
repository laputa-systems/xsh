proc backup(src: Path, dest: Path) {
  run tar -czf $dest $src
  print "backup written"
}

backup(/var/lib/app, /backups/app.tgz)?
