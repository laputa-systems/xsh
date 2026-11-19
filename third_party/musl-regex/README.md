Musl 1.2.5 regex sources from https://musl.libc.org/releases/musl-1.2.5.tar.gz,
licensed under COPYRIGHT. Rust's x86_64 musl toolchain uses this version.

The only upstream edit replaces musl-internal type includes in regex.h with
stddef.h and ptrdiff_t. shim.h prefixes symbols and supplies C-locale functions;
locale_impl.h disables translation to match XSH's C locale.
