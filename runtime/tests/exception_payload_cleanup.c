#include <assert.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  uint64_t tag;
} FailurePayload;

typedef struct
{
  char *message;
  uint64_t tag;
} OwnedFailurePayload;

static int owned_payload_drops;
static int uncaught_cleanup_fd = -1;

static void destroy_owned_failure(void *data)
{
  OwnedFailurePayload *payload = (OwnedFailurePayload *)data;
  assert(payload != NULL);
  if (uncaught_cleanup_fd >= 0)
  {
    const char marker = 'd';
    (void)write(uncaught_cleanup_fd, &marker, 1);
  }
  free(payload->message);
  free(payload);
  owned_payload_drops++;
}

static void throw_payload(uint64_t tag)
{
  FailurePayload *payload = (FailurePayload *)malloc(sizeof(*payload));
  assert(payload != NULL);
  payload->tag = tag;
  aura_throw_obj("FailurePayload", payload);
  abort();
}

static void throw_owned_payload(uint64_t tag)
{
  OwnedFailurePayload *payload =
      (OwnedFailurePayload *)malloc(sizeof(*payload));
  assert(payload != NULL);
  payload->message = (char *)malloc(16);
  assert(payload->message != NULL);
  (void)snprintf(payload->message, 16, "payload-%llu",
                 (unsigned long long)tag);
  payload->tag = tag;
  aura_throw_obj_with_destructor("OwnedFailurePayload", payload,
                                 destroy_owned_failure);
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

static void test_source_span_survives_rethrow(void)
{
  jmp_buf outer_jb;
  jmp_buf inner_jb;
  if (setjmp(outer_jb) == 0)
  {
    aura_try_enter(&outer_jb);
    if (setjmp(inner_jb) == 0)
    {
      aura_try_enter(&inner_jb);
      aura_ex_set_source_span(41, 47);
      aura_throw_int(9);
    }
    else
    {
      assert(aura_ex_source_span_start() == 41);
      assert(aura_ex_source_span_end() == 47);
      aura_ex_rethrow();
    }
    abort();
  }
  else
  {
    assert(aura_ex_matches("Int"));
    assert(aura_ex_source_span_start() == 41);
    assert(aura_ex_source_span_end() == 47);
    aura_ex_clear();
    aura_try_leave();
  }
}

static void test_destructor_clears_nested_owned_payload(void)
{
  jmp_buf jb;
  if (setjmp(jb) == 0)
  {
    aura_try_enter(&jb);
    throw_owned_payload(UINT64_C(0x404));
  }
  else
  {
    OwnedFailurePayload *payload =
        (OwnedFailurePayload *)aura_ex_as_obj();
    assert(aura_ex_matches("OwnedFailurePayload"));
    assert(payload->tag == UINT64_C(0x404));
    assert(strcmp(payload->message, "payload-1028") == 0);
    aura_ex_clear();
    aura_try_leave();
  }
  assert(owned_payload_drops == 1);
}

static void test_destructor_transfers_on_rethrow(void)
{
  jmp_buf outer_jb;
  jmp_buf inner_jb;
  if (setjmp(outer_jb) == 0)
  {
    aura_try_enter(&outer_jb);
    if (setjmp(inner_jb) == 0)
    {
      aura_try_enter(&inner_jb);
      throw_owned_payload(UINT64_C(0x505));
    }
    else
    {
      assert(aura_ex_matches("OwnedFailurePayload"));
      aura_ex_rethrow();
    }
    abort();
  }
  else
  {
    OwnedFailurePayload *payload =
        (OwnedFailurePayload *)aura_ex_as_obj();
    assert(payload->tag == UINT64_C(0x505));
    assert(owned_payload_drops == 1);
    aura_ex_clear();
    aura_try_leave();
  }
  assert(owned_payload_drops == 2);
}

static void test_replacing_payload_disposes_old_value(void)
{
  jmp_buf jb;
  if (setjmp(jb) == 0)
  {
    aura_try_enter(&jb);
    throw_owned_payload(UINT64_C(0x606));
  }
  else if (owned_payload_drops == 2)
  {
    /* Throwing again from the same catch frame must not orphan the first
     * payload.  The second longjmp returns through this same setjmp. */
    throw_owned_payload(UINT64_C(0x607));
  }
  else
  {
    OwnedFailurePayload *payload =
        (OwnedFailurePayload *)aura_ex_as_obj();
    assert(aura_ex_matches("OwnedFailurePayload"));
    assert(payload->tag == UINT64_C(0x607));
    aura_ex_clear();
    aura_try_leave();
  }
  assert(owned_payload_drops == 4);
}

static void test_uncaught_payload_is_destroyed_before_abort(void)
{
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  pid_t child = fork();
  assert(child >= 0);
  if (child == 0)
  {
    close(pipe_fds[0]);
    uncaught_cleanup_fd = pipe_fds[1];
    throw_owned_payload(UINT64_C(0x707));
    abort();
  }

  close(pipe_fds[1]);
  char marker = 0;
  assert(read(pipe_fds[0], &marker, 1) == 1);
  close(pipe_fds[0]);
  int status = 0;
  assert(waitpid(child, &status, 0) == child);
  assert(marker == 'd');
  assert(WIFSIGNALED(status));
  assert(WTERMSIG(status) == SIGABRT);
}

int main(void)
{
  test_clear_then_leave();
  test_leave_is_final_cleanup_boundary();
  test_rethrow_transfers_ownership_once();
  test_leave_resets_scalar_pending_state();
  test_source_span_survives_rethrow();
  test_destructor_clears_nested_owned_payload();
  test_destructor_transfers_on_rethrow();
  test_replacing_payload_disposes_old_value();
  test_uncaught_payload_is_destroyed_before_abort();
  puts("exception payload cleanup: passed");
  return 0;
}
