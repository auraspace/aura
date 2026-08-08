#ifndef AURA_LLVM_EXCEPTIONS_H
#define AURA_LLVM_EXCEPTIONS_H

void aura_throw_string(const char *s);

/* LLVM's textual backend shares these small helpers with the exception C
 * translation unit so string/array builtins keep one native ABI. */
typedef struct {
  bool has;
  int64_t value;
} AuraLlvmOptInt;

void *aura_llvm_str_alloc(int64_t len);
void *aura_llvm_str_new(const char *source);
void aura_llvm_str_retain(void *value);
void aura_llvm_str_release(void *value);
bool aura_llvm_str_is_empty(void *value);
bool aura_llvm_str_starts_with(void *value, void *prefix);
bool aura_llvm_str_contains(void *value, void *needle);
bool aura_llvm_str_ends_with(void *value, void *suffix);
int64_t aura_llvm_str_char_at(void *value, int64_t index);
int64_t aura_llvm_str_index_of(void *value, void *needle);
void *aura_llvm_str_substring(void *value, int64_t start, int64_t end);
void *aura_llvm_str_trim(void *value, int64_t mode);
void *aura_llvm_str_case(void *value, bool upper);
AuraLlvmOptInt aura_llvm_str_to_int(void *value);
void *aura_llvm_str_split(void *value, void *separator);

void *aura_llvm_array_alloc(int64_t len, int64_t kind);
void aura_llvm_array_set(void *value, int64_t index, int64_t raw);
void *aura_llvm_array_clone(void *value);
void aura_llvm_array_clear(void *value);
void aura_llvm_array_reserve(void *value, int64_t capacity);
bool aura_llvm_array_is_empty(void *value);

#endif
