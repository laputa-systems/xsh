let help = run.capture --text tar --help
print f"{"BusyBox" in help.stdout + help.stderr}"
