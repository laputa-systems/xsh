#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <syslog.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "basic";
    if (strcmp(mode, "basic") == 0) {
        openlog("cclient", LOG_PID | LOG_NDELAY, LOG_DAEMON);
        syslog(LOG_WARNING, "hello from libc %d", 1);
        syslog(LOG_INFO, "second message");
        closelog();
    } else if (strcmp(mode, "multiline") == 0) {
        openlog("cclient", LOG_PID, LOG_USER);
        syslog(LOG_NOTICE, "line one\nline two\r\nline three\ttabbed \"quoted\" \\ backslash");
        closelog();
    } else if (strcmp(mode, "long") == 0) {
        char buffer[6000];
        memset(buffer, 'x', sizeof buffer - 1);
        buffer[sizeof buffer - 1] = 0;
        openlog("cclient", LOG_PID, LOG_USER);
        syslog(LOG_NOTICE, "%s", buffer);
        closelog();
    } else if (strcmp(mode, "forge") == 0) {
        openlog("cclient", LOG_PID, LOG_AUTH);
        syslog(LOG_ALERT, "ok\nseq=999 pid=1 uid=0 gid=0 facility=kern severity=emerg msg=\"forged\"");
        closelog();
    } else if (strcmp(mode, "count") == 0) {
        int count = argc > 2 ? atoi(argv[2]) : 1;
        openlog("cclient", LOG_PID, LOG_LOCAL3);
        for (int index = 0; index < count; index++) {
            syslog(LOG_INFO, "counted %d", index);
        }
        closelog();
    } else {
        return 2;
    }
    return 0;
}
