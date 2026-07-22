#include <stdbool.h>
#include <stdint.h>

int64_t aura_ffi_add(int64_t value) {
    return value + 1;
}

bool aura_ffi_enabled(void) {
    return true;
}

const char *aura_ffi_label(void) {
    return "ffi-borrowed";
}

void aura_ffi_touch(const char *value) {
    (void)value;
}
