#include <mimalloc.h>

__attribute__((constructor(102))) static void configure_xsh_arena_commit(void) {
    mi_option_set_default(mi_option_arena_eager_commit, 0);
}
