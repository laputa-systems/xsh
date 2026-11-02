proc backup(src: Path, dest: Path) {
  run tar -czf $dest $src
  print "backup written"
}

backup(p"/var/lib/app", p"/backups/app.tgz")
