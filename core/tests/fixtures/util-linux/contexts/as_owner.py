import os
import sys


os.execv("/bin/setpriv", [
    "/bin/setpriv", "--reuid=10002", "--regid=10002", "--clear-groups",
    "--bounding-set=-all", "--inh-caps=-all", "--ambient-caps=-all",
    "--no-new-privs", "/usr/bin/python3", *sys.argv[1:],
])
