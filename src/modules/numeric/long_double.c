#define _GNU_SOURCE
#include <errno.h>
#include <float.h>
#include <locale.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

_Static_assert(LDBL_MANT_DIG <= 128, "long double key capacity exceeded");

/* Keep libc's long double ABI inside C: Rust never receives a rounded double. */
int xsh_long_double(const char *input, char conversion, int precision, int alternate,
                    char *output, size_t capacity, size_t *consumed,
                    int *range_error, char *key) {
    locale_t locale = newlocale(LC_ALL_MASK, "C", (locale_t)0);
    if (!locale) return -1;
    locale_t previous = uselocale(locale);
    if (!previous) {
        freelocale(locale);
        return -1;
    }
    errno = 0;
    char *end;
    long double value = strtold(input, &end);
    *consumed = (size_t)(end - input);
    *range_error = errno == ERANGE;
    if (key) {
        if (end == input) { key[0] = '0'; key[1] = 0; }
        else if (isnan(value)) { key[0] = '1'; key[1] = 0; }
        else if (isinf(value)) { key[0] = signbit(value) ? '2' : '6'; key[1] = 0; }
        else if (value == 0) { key[0] = '4'; key[1] = 0; }
        else {
            int exponent;
            int negative = signbit(value);
            long double fraction = frexpl(fabsl(value), &exponent);
            uint32_t ordered_exponent = (uint32_t)exponent ^ UINT32_C(0x80000000);
            size_t at = 0;
            key[at++] = negative ? '3' : '5';
            for (int bit = 31; bit >= 0; --bit)
                key[at++] = '0' + (((ordered_exponent >> bit) & 1) ^ negative);
            for (int bit = 0; bit < LDBL_MANT_DIG; ++bit) {
                fraction *= 2;
                int digit = fraction >= 1;
                fraction -= digit;
                key[at++] = '0' + (digit ^ negative);
            }
            key[at] = 0;
        }
    }
    int result = 0;
    if (conversion) {
        char format[8];
        size_t at = 0;
        format[at++] = '%';
        if (alternate) format[at++] = '#';
        if (precision >= 0) { format[at++] = '.'; format[at++] = '*'; }
        format[at++] = 'L';
        format[at++] = conversion;
        format[at] = 0;
        result = precision >= 0 ? snprintf(output, capacity, format, precision, value)
                                : snprintf(output, capacity, format, value);
    }
    uselocale(previous);
    freelocale(locale);
    return result;
}

int xsh_long_double_precision(void) { return LDBL_MANT_DIG; }
