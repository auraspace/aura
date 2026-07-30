#include <assert.h>
#include <signal.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

int main(void)
{
#if defined(__unix__) || defined(__APPLE__)
  assert(aura_signal_install_shutdown() == 1);
  assert(!aura_signal_shutdown_requested());
  raise(SIGTERM);
  assert(aura_signal_shutdown_requested());
  aura_signal_clear_shutdown();
  assert(!aura_signal_shutdown_requested());
  raise(SIGINT);
  assert(aura_signal_shutdown_requested());
  aura_signal_clear_shutdown();
  assert(!aura_signal_shutdown_requested());
#else
  assert(aura_signal_install_shutdown() == 0);
  assert(!aura_signal_shutdown_requested());
#endif
  return 0;
}
