/* Aura C1 runtime stub — linked into every binary produced by aura build. */
#include <stdio.h>
#include <stdlib.h>

void aura_println(const char *s) {
  if (s == NULL) {
    puts("null");
  } else {
    puts(s);
  }
}

/* Provided by generated code */
int aura_main(void);

int main(int argc, char **argv) {
  (void)argc;
  (void)argv;
  return aura_main();
}
