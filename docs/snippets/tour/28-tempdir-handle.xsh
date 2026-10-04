let scratch = fs.tempdir()?
defer scratch.close()?

scratch.mkdir(p"logs")?
scratch.write(p"logs/app.log", "ok\n")?
print f"rooted read: {scratch.read_text(p"logs/app.log")?.trim()}"
print f"under the temp dir: {scratch.host_path()?.exists()?}"
