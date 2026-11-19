#include "shim.h"

int xsh_musl_mbtowc(wchar_t *out, const char *s, size_t n) {
    if (!s) return 0;
    if (!n) return -1;
    unsigned char c = (unsigned char)*s;
    if (out) *out = c < 128 ? c : 0xdf00 + c;
    return c != 0;
}
unsigned long xsh_musl_wctype(const char *name) {
    static const char *const names[] = {"alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space", "upper", "xdigit"};
    for (unsigned long i = 0; i < sizeof names / sizeof *names; i++) if (!strcmp(name, names[i])) return i + 1;
    return 0;
}
int xsh_musl_iswctype(wint_t c, unsigned long cls) {
    if (c > 127) return 0;
    int upper = c >= 'A' && c <= 'Z', lower = c >= 'a' && c <= 'z', digit = c >= '0' && c <= '9';
    switch (cls) {
    case 1: return upper || lower || digit;
    case 2: return upper || lower;
    case 3: return c == ' ' || c == '\t';
    case 4: return c < 32 || c == 127;
    case 5: return digit;
    case 6: return c >= 33 && c <= 126;
    case 7: return lower;
    case 8: return c >= 32 && c <= 126;
    case 9: return c >= 33 && c <= 126 && !upper && !lower && !digit;
    case 10: return c == ' ' || (c >= '\t' && c <= '\r');
    case 11: return upper;
    case 12: return digit || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
    default: return 0;
    }
}
wint_t xsh_musl_towlower(wint_t c) { return c >= 'A' && c <= 'Z' ? c + 32 : c; }
wint_t xsh_musl_towupper(wint_t c) { return c >= 'a' && c <= 'z' ? c - 32 : c; }
