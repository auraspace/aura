#include "demo.h"

int64_t demo_abi_version(void)
{
  return 1;
}

int64_t demo_add(int64_t left, int64_t right)
{
  return left + right;
}
