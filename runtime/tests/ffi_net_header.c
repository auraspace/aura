/* Compile-only check for the opt-in public TCP declarations.  The runtime
 * source currently owns the same legacy typedefs, so consumers embedding
 * aura_rt.c include this header after the runtime or use its declarations in
 * a separate translation unit. */
#define AURA_FFI_DECLARE_NET
#include "../aura_ffi.h"

static AuraTcpStatus (*bind_fn)(uint16_t, uint16_t *, AuraTcpListener **)
    = aura_tcp_listener_bind;

int main(void)
{
  return bind_fn == 0;
}
