#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#include "../aura_ffi.h"

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static unsigned environment_destroys;
static AuraFfiCallback *active_callback;
static AuraFfiStatus nested_status;

static void destroy_environment(void *environment)
{
  assert(environment != NULL);
  environment_destroys++;
  free(environment);
}

static int32_t callback_reenters(void *environment, const void *payload,
                                 uint64_t payload_len)
{
  (void)environment;
  (void)payload;
  (void)payload_len;
  AuraFfiOutcome nested_outcome = AURA_FFI_OUTCOME_OK;
  nested_status = aura_ffi_callback_invoke(active_callback, 7,
                                           AURA_FFI_BOUNDARY_SYNC, NULL, 0,
                                           &nested_outcome);
  assert(nested_status == AURA_FFI_BUSY);
  return 0;
}

static int32_t callback_timeout(void *environment, const void *payload,
                                uint64_t payload_len)
{
  (void)environment;
  (void)payload;
  assert(payload_len == 3);
  return 6;
}

static void test_error_mapping(void)
{
  assert(aura_ffi_map_error(0) == AURA_FFI_OUTCOME_OK);
  assert(aura_ffi_map_error(1) == AURA_FFI_OUTCOME_CANCELLED);
  assert(aura_ffi_map_error(2) == AURA_FFI_OUTCOME_INVALID);
  assert(aura_ffi_map_error(3) == AURA_FFI_OUTCOME_NOT_FOUND);
  assert(aura_ffi_map_error(4) == AURA_FFI_OUTCOME_PERMISSION);
  assert(aura_ffi_map_error(5) == AURA_FFI_OUTCOME_UNAVAILABLE);
  assert(aura_ffi_map_error(6) == AURA_FFI_OUTCOME_TIMEOUT);
  assert(aura_ffi_map_error(-99) == AURA_FFI_OUTCOME_FOREIGN_ERROR);
}

static void test_lifetime_affinity_and_reentry(void)
{
  AuraFfiCallbackFrame *frame = NULL;
  AuraFfiCallback *callback = NULL;
  int *environment = (int *)malloc(sizeof(*environment));
  assert(environment != NULL);
  *environment = 42;
  assert(aura_ffi_callback_frame_new(7, &frame) == AURA_FFI_OK);
  assert(aura_ffi_callback_register(frame, callback_reenters, environment,
                                    destroy_environment, &callback) ==
         AURA_FFI_OK);
  active_callback = callback;

  AuraFfiOutcome outcome = AURA_FFI_OUTCOME_FOREIGN_ERROR;
  const char payload[] = "x";
  assert(aura_ffi_callback_invoke(callback, 8, AURA_FFI_BOUNDARY_SYNC,
                                  payload, 1, &outcome) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_callback_invoke(callback, 7, AURA_FFI_BOUNDARY_AWAIT,
                                  payload, 1, &outcome) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_callback_invoke(callback, 7, AURA_FFI_BOUNDARY_SYNC,
                                  payload, 1, &outcome) == AURA_FFI_OK);
  assert(outcome == AURA_FFI_OUTCOME_OK);
  assert(nested_status == AURA_FFI_BUSY);

  /* A live registration retains the frame; destroying it cannot create a
   * dangling callback target.  Invalidation makes future delivery fail. */
  assert(aura_ffi_callback_frame_destroy(&frame) == AURA_FFI_BUSY);
  assert(aura_ffi_callback_frame_invalidate(frame) == AURA_FFI_OK);
  assert(aura_ffi_callback_invoke(callback, 7, AURA_FFI_BOUNDARY_SYNC,
                                  payload, 1, &outcome) == AURA_FFI_INVALID);
  assert(aura_ffi_callback_deregister(callback) == AURA_FFI_OK);
  assert(environment_destroys == 1);
  assert(aura_ffi_callback_frame_destroy(&frame) == AURA_FFI_OK);
  assert(aura_ffi_callback_destroy(&callback) == AURA_FFI_OK);
  assert(callback == NULL && frame == NULL);
  active_callback = NULL;
}

static void test_shutdown_and_failure_outcome(void)
{
  AuraFfiCallbackFrame *frame = NULL;
  AuraFfiCallback *callback = NULL;
  int *environment = (int *)malloc(sizeof(*environment));
  assert(environment != NULL);
  assert(aura_ffi_callback_frame_new(9, &frame) == AURA_FFI_OK);
  assert(aura_ffi_callback_register(frame, callback_timeout, environment,
                                    destroy_environment, &callback) ==
         AURA_FFI_OK);
  AuraFfiOutcome outcome = AURA_FFI_OUTCOME_OK;
  assert(aura_ffi_callback_invoke(callback, 9, AURA_FFI_BOUNDARY_SYNC,
                                  "abc", 3, &outcome) == AURA_FFI_OK);
  assert(outcome == AURA_FFI_OUTCOME_TIMEOUT);
  assert(aura_ffi_callback_shutdown(callback) == AURA_FFI_OK);
  assert(environment_destroys == 2);
  assert(aura_ffi_callback_invoke(callback, 9, AURA_FFI_BOUNDARY_SYNC,
                                  NULL, 0, &outcome) == AURA_FFI_INVALID);
  assert(aura_ffi_callback_shutdown(callback) == AURA_FFI_INVALID);
  assert(aura_ffi_callback_destroy(&callback) == AURA_FFI_OK);
  assert(aura_ffi_callback_frame_destroy(&frame) == AURA_FFI_OK);
}

int main(void)
{
  test_error_mapping();
  test_lifetime_affinity_and_reentry();
  test_shutdown_and_failure_outcome();
  assert(environment_destroys == 2);
  return 0;
}
