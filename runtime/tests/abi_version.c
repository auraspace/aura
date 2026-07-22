#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

int main(void)
{
  const char *available = aura_runtime_abi_identity();
  assert(strcmp(available, AURA_RT_ABI_ID) == 0);
  assert(aura_runtime_abi_version() == AURA_RT_ABI_VERSION);
  assert(aura_runtime_check_abi(AURA_RT_ABI_VERSION, AURA_RT_ABI_ID) == 1);
  assert(aura_runtime_check_abi(999u, AURA_RT_ABI_ID) == 0);
  assert(aura_runtime_check_abi(AURA_RT_ABI_VERSION, "aura-c-abi/999.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1") == 0);
  return 0;
}
