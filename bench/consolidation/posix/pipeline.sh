set -eu
exec /usr/bin/awk '/^#define[ \t]+CAP_[A-Z0-9_]+[ \t]+[0-9]+[ \t]*$/ { printf "{\"%s\",%s},\n", tolower($2), $3 }' "$1/capability.h"
