#ifndef AURA_PLATFORM_H
#define AURA_PLATFORM_H

#include <stdint.h>

/* Monotonic time is the only clock accepted by scheduler deadlines. */
int64_t aura_platform_monotonic_millis(void);

#endif
