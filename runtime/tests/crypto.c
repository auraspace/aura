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

  unsigned char binary_digest[32];
  assert(aura_crypto_sha256_bytes("abc", 3, binary_digest));
  const char *binary_hex = aura_digest_hex(binary_digest);
  assert(binary_hex != NULL && strcmp(binary_hex,
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") == 0);
  free((void *)binary_hex);

  unsigned char md5[16];
  assert(aura_crypto_md5_bytes("abc", 3, md5));
  static const char md5_digits[] = "0123456789abcdef";
  char md5_hex[33];
  for (size_t i = 0; i < sizeof(md5); i++) {
    md5_hex[i * 2] = md5_digits[md5[i] >> 4];
    md5_hex[i * 2 + 1] = md5_digits[md5[i] & 15u];
  }
  md5_hex[32] = '\0';
  assert(strcmp(md5_hex, "900150983cd24fb0d6963f7d28e17f72") == 0);

  unsigned char derived[32];
  assert(aura_crypto_pbkdf2_sha256("password", 8, "salt", 4, 1, derived, sizeof(derived)));
  const char *derived_hex = aura_digest_hex(derived);
  assert(derived_hex != NULL && strcmp(derived_hex,
      "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b") == 0);
  free((void *)derived_hex);

  unsigned char nonce[32];
  assert(aura_crypto_random_bytes_raw(nonce, sizeof(nonce)));
  return 0;
}
