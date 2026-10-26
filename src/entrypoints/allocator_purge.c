#include <mimalloc.h>

void xsh_set_allocator_purge_default(void) {
    mi_option_set_default(mi_option_purge_delay, 0);
}
