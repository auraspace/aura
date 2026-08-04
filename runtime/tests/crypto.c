#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

static void digest_hex(const char *input, const char *expected)
{
  const char *actual = aura_crypto_sha256(input);
  assert(actual != NULL && strcmp(actual, expected) == 0);
  free((void *)actual);
}

int main(void)
{
  unsigned char block[64];
  AuraSha256 portable;
  AuraSha256 accelerated;
  AuraSha256BlockFn backend = aura_sha256_get_block_fn();
#if defined(__x86_64__) && (defined(__clang__) || defined(__GNUC__))
  if (__builtin_cpu_supports("sha")) backend = aura_sha256_block_shani;
#endif

  digest_hex("", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  digest_hex("abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  digest_hex("The quick brown fox jumps over the lazy dog", "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");

  memset(block, 0, sizeof(block));
  for (size_t i = 0; i < sizeof(block); i++) block[i] = (unsigned char)(i * 37u + 11u);
  aura_sha256_init(&portable);
  aura_sha256_init(&accelerated);
  aura_sha256_block_portable(&portable, block);
  backend(&accelerated, block);
  assert(memcmp(portable.state, accelerated.state, sizeof(portable.state)) == 0);

  aura_sha256_init(&portable);
  aura_sha256_update(&portable, block, 3);
  aura_sha256_update(&portable, block + 3, 17);
  aura_sha256_update(&portable, block + 20, 44);
  unsigned char fragmented[32];
  aura_sha256_final(&portable, fragmented);
  const char *fragmented_hex = aura_digest_hex(fragmented);
  assert(fragmented_hex != NULL && strcmp(fragmented_hex, "94eb5de4943613fd048dc93393ab06877405faa39c11f53e9386083339833e7e") == 0);
  free((void *)fragmented_hex);

  unsigned char *million = (unsigned char *)malloc(1000000u);
  assert(million != NULL);
  memset(million, 'a', 1000000u);
  aura_sha256_init(&portable);
  aura_sha256_update(&portable, million, 1000000u);
  aura_sha256_final(&portable, fragmented);
  const char *million_hex = aura_digest_hex(fragmented);
  assert(million_hex != NULL && strcmp(million_hex, "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0") == 0);
  free((void *)million_hex);
  free(million);
  return 0;
}
