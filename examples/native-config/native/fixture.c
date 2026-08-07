#include "fixture.h"

int aura_native_fixture_value(void)
{
#if AURA_NATIVE_FIXTURE == 1
  return 42;
#else
  return 0;
#endif
}
