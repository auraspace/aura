#include <setjmp.h>
#include <stdint.h>

typedef struct {
    void *vtable;
    uintptr_t ref_count;
} AuraObject;

typedef struct {
    jmp_buf env;
} AuraJmpBuf;

typedef struct AuraHandlerFrame {
    struct AuraHandlerFrame *prev;
    void *catch_entry;
    void *cleanup_stack;
    AuraJmpBuf jump_buf;
} AuraHandlerFrame;

typedef int (*AuraTryBodyFn)(void *user_data);

static int aura_runtime_throw_body(void *user_data) {
    extern void aura_throw(AuraObject *exception);
    aura_throw((AuraObject *)user_data);
    return 0;
}

int aura_runtime_try_invoke(AuraHandlerFrame *frame, AuraTryBodyFn body, void *user_data) {
    extern void aura_try_begin(void *frame_ptr);
    extern void aura_try_end(void *frame_ptr);

    aura_try_begin(frame);
    int jump = setjmp(frame->jump_buf.env);
    if (jump == 0) {
        jump = body(user_data);
    }
    aura_try_end(frame);
    return jump;
}

int aura_runtime_try_throw(AuraHandlerFrame *frame, AuraObject *exception) {
    return aura_runtime_try_invoke(frame, aura_runtime_throw_body, exception);
}

void aura_runtime_longjmp(void *env, int value) {
    longjmp(((AuraJmpBuf *)env)->env, value);
}
