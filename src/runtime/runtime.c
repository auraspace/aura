#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

#define AURA_TYPE_I32 1
#define AURA_TYPE_STRING 2
#define AURA_TYPE_BOOLEAN 3

typedef struct {
    int64_t tag;
    int64_t value;
} AuraAny;

void print_num(int64_t n) {
    printf("%lld\n", n);
    fflush(stdout);
}

int64_t aura_check_tag(int64_t val_tag, int64_t expected_tag) {
    return val_tag == expected_tag;
}

extern const char* aura_string_table[];

void print_str(const char* s) {
    printf("%s\n", s);
    fflush(stdout);
}

const char* aura_get_string(int64_t index) {
    return aura_string_table[index];
}

void* aura_alloc(size_t size) {
    return malloc(size);
}
