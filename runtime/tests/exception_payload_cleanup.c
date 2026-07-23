#include <assert.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  uint64_t tag;
} FailurePayload;

static void throw_payload(uint64_t tag)
{
  FailurePayload *payload = (FailurePayload *)malloc(sizeof(*payload));
  assert(payload != NULL);
  payload->tag = tag;
  aura_throw_obj("FailurePayload", payload);
  abort();
}

static void test_clear_then_leave(void)
{
  jmp_buf jb;
  if (setjmp(jb) == 0)
  {
    aura_try_enter(&jb);
    throw_payload(UINT64_C(0x101));
  }
  else
  {
    assert(aura_ex_matches("FailurePayload"));
    assert(((FailurePayload *)aura_ex_as_obj())->tag == UINT64_C(0x101));
    aura_ex_clear();
    aura_try_leave();
  }
}

static void test_leave_is_final_cleanup_boundary(void)
{
  jmp_buf jb;
  if (setjmp(jb) == 0)
  {
    aura_try_enter(&jb);
    throw_payload(UINT64_C(0x202));
  }
  else
  {
    assert(aura_ex_matches("FailurePayload"));
    /* Deliberately omit aura_ex_clear: leaving the frame must release it. */
    aura_try_leave();
    assert(!aura_ex_matches("FailurePayload"));
  }
}

static void test_rethrow_transfers_ownership_once(void)
{
  jmp_buf outer_jb;
  jmp_buf inner_jb;
  if (setjmp(outer_jb) == 0)
  {
    aura_try_enter(&outer_jb);
    if (setjmp(inner_jb) == 0)
    {
      aura_try_enter(&inner_jb);
      throw_payload(UINT64_C(0x303));
    }
    else
    {
      assert(aura_ex_matches("FailurePayload"));
      aura_ex_rethrow();
    }
    abort();
  }
  else
  {
    assert(aura_ex_matches("FailurePayload"));
    assert(((FailurePayload *)aura_ex_as_obj())->tag == UINT64_C(0x303));
    aura_ex_clear();
    aura_try_leave();
  }
}

static void test_leave_resets_scalar_pending_state(void)
{
  jmp_buf jb;
  if (setjmp(jb) == 0)
  {
    aura_try_enter(&jb);
    aura_throw_int(7);
  }
  else
  {
    assert(aura_ex_matches("Int"));
    aura_try_leave();
    assert(!aura_ex_matches("Int"));
  }
}

int main(void)
{
  test_clear_then_leave();
  test_leave_is_final_cleanup_boundary();
  test_rethrow_transfers_ownership_once();
  test_leave_resets_scalar_pending_state();
  puts("exception payload cleanup: passed");
  return 0;
}
