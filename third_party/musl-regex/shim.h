#include <stdlib.h>
#include <wchar.h>
#include <wctype.h>
#include <ctype.h>
#include <string.h>
#define regex_t xsh_musl_regex_t
#define regmatch_t xsh_musl_regmatch_t
#define regcomp xsh_musl_regcomp
#define regexec xsh_musl_regexec
#define regfree xsh_musl_regfree
#define regerror xsh_musl_regerror
#define __tre_mem_new_impl xsh_musl_tre_mem_new_impl
#define __tre_mem_alloc_impl xsh_musl_tre_mem_alloc_impl
#define __tre_mem_destroy xsh_musl_tre_mem_destroy
#define hidden __attribute__((visibility("hidden")))
#define mbtowc xsh_musl_mbtowc
#define wctype_t unsigned long
#define wctype xsh_musl_wctype
#define iswctype xsh_musl_iswctype
#define iswalnum(c) xsh_musl_iswctype(c, 1)
#define iswalpha(c) xsh_musl_iswctype(c, 2)
#define iswblank(c) xsh_musl_iswctype(c, 3)
#define iswcntrl(c) xsh_musl_iswctype(c, 4)
#undef iswdigit
#define iswdigit(c) xsh_musl_iswctype(c, 5)
#define iswgraph(c) xsh_musl_iswctype(c, 6)
#define iswlower(c) xsh_musl_iswctype(c, 7)
#define iswprint(c) xsh_musl_iswctype(c, 8)
#define iswpunct(c) xsh_musl_iswctype(c, 9)
#define iswspace(c) xsh_musl_iswctype(c, 10)
#define iswupper(c) xsh_musl_iswctype(c, 11)
#define iswxdigit(c) xsh_musl_iswctype(c, 12)
#define towlower xsh_musl_towlower
#define towupper xsh_musl_towupper
#undef isdigit
#define isdigit(c) ((unsigned)(c) - '0' < 10)
int xsh_musl_mbtowc(wchar_t *, const char *, size_t);
unsigned long xsh_musl_wctype(const char *);
int xsh_musl_iswctype(wint_t, unsigned long);
wint_t xsh_musl_towlower(wint_t);
wint_t xsh_musl_towupper(wint_t);

#undef CHARCLASS_NAME_MAX
#define CHARCLASS_NAME_MAX 14
#undef RE_DUP_MAX
#define RE_DUP_MAX 255
