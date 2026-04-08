#ifndef AURA_RT_H
#define AURA_RT_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* 
 * Aura Object Header 
 */
typedef struct {
    void* vtable;
    uintptr_t ref_count;
} AuraObject;

typedef struct {
    uint32_t storage[48];
} AuraJmpBuf;

typedef struct AuraHandlerFrame {
    struct AuraHandlerFrame* prev;
    void* catch_entry;
    void* cleanup_stack;
    AuraJmpBuf jump_buf;
} AuraHandlerFrame;

/* 
 * Aura String Layout 
 */
typedef struct {
    AuraObject header;
    uintptr_t len;
    const char* data;
} AuraString;

/* 
 * Memory Management 
 */
void* aura_alloc(size_t size, size_t align);

/*
 * Exception Runtime
 */
void aura_try_begin(AuraHandlerFrame* frame);
void aura_try_end(AuraHandlerFrame* frame);
AuraObject* aura_current_exception(void);
void aura_throw(AuraObject* exception) __attribute__((noreturn));

/* 
 * Reference Counting (ARC) 
 */
void aura_retain(AuraObject* obj);
void aura_release(AuraObject* obj);

/* 
 * String Operations 
 */
AuraString* aura_string_new_utf8(const char* ptr, size_t len);

/* 
 * Standard Library Surface 
 */
void aura_println(AuraString* str);

/* 
 * Error Handling 
 */
void aura_panic(const char* msg_ptr, size_t msg_len) __attribute__((noreturn));

#ifdef __cplusplus
}
#endif

#endif /* AURA_RT_H */
