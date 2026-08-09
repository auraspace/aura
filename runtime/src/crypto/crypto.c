#if defined(__aarch64__) && (defined(__clang__) || defined(__GNUC__))
#include <arm_neon.h>
#if defined(__APPLE__)
int sysctlbyname(const char *, void *, size_t *, const void *, size_t);
#elif defined(__linux__)
#include <sys/auxv.h>
#include <asm/hwcap.h>
#endif
#define AURA_SHA256_TARGET_CRYPTO __attribute__((target("+crypto")))
#elif defined(__x86_64__) && (defined(__clang__) || defined(__GNUC__))
#include <immintrin.h>
#define AURA_SHA256_TARGET_CRYPTO __attribute__((target("sha,ssse3,sse4.1")))
#endif
#include <stdatomic.h>

typedef struct { uint32_t state[8]; uint64_t bits; size_t used; unsigned char block[64]; } AuraSha256;
typedef void (*AuraSha256BlockFn)(AuraSha256 *, const unsigned char *);

static uint32_t aura_sha_rotr(uint32_t x, unsigned n) { return (x >> n) | (x << (32u - n)); }

static void aura_sha256_block_portable(AuraSha256 *ctx, const unsigned char *block)
{
  static const uint32_t k[64] = {
      0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
      0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
      0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
      0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
      0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
      0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
      0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
      0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u};
  uint32_t w[64];
  for (size_t i=0;i<16;i++) w[i]=((uint32_t)block[i*4]<<24)|((uint32_t)block[i*4+1]<<16)|((uint32_t)block[i*4+2]<<8)|block[i*4+3];
  for (size_t i=16;i<64;i++) { uint32_t s0=aura_sha_rotr(w[i-15],7)^aura_sha_rotr(w[i-15],18)^(w[i-15]>>3); uint32_t s1=aura_sha_rotr(w[i-2],17)^aura_sha_rotr(w[i-2],19)^(w[i-2]>>10); w[i]=w[i-16]+s0+w[i-7]+s1; }
  uint32_t a=ctx->state[0],b=ctx->state[1],c=ctx->state[2],d=ctx->state[3],e=ctx->state[4],f=ctx->state[5],g=ctx->state[6],h=ctx->state[7];
  for (size_t i=0;i<64;i++) { uint32_t s1=aura_sha_rotr(e,6)^aura_sha_rotr(e,11)^aura_sha_rotr(e,25); uint32_t t1=h+s1+((e&f)^(~e&g))+k[i]+w[i]; uint32_t s0=aura_sha_rotr(a,2)^aura_sha_rotr(a,13)^aura_sha_rotr(a,22); uint32_t t2=s0+((a&b)^(a&c)^(b&c)); h=g;g=f;f=e;e=d+t1;d=c;c=b;b=a;a=t1+t2; }
  ctx->state[0]+=a;ctx->state[1]+=b;ctx->state[2]+=c;ctx->state[3]+=d;ctx->state[4]+=e;ctx->state[5]+=f;ctx->state[6]+=g;ctx->state[7]+=h;
}

#if defined(__aarch64__) && (defined(__clang__) || defined(__GNUC__))
static uint32x4_t aura_sha256_load_words(const unsigned char *data)
{
  return vreinterpretq_u32_u8(vrev32q_u8(vld1q_u8(data)));
}

static void AURA_SHA256_TARGET_CRYPTO aura_sha256_block_arm64(AuraSha256 *ctx, const unsigned char *block)
{
  static const uint32_t k[64] = {
      0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
      0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
      0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
      0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
      0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
      0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
      0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
      0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u};
  uint32x4_t abcd = vld1q_u32(ctx->state);
  uint32x4_t efgh = vld1q_u32(ctx->state + 4);
  const uint32x4_t original_abcd = abcd;
  const uint32x4_t original_efgh = efgh;
  uint32x4_t msg[4] = {
      aura_sha256_load_words(block), aura_sha256_load_words(block + 16),
      aura_sha256_load_words(block + 32), aura_sha256_load_words(block + 48)};
  for (size_t i = 0; i < 16; i++)
  {
    size_t slot = i & 3u;
    uint32x4_t schedule = vaddq_u32(msg[slot], vld1q_u32(k + i * 4u));
    size_t next = (slot + 1u) & 3u;
    msg[slot] = vsha256su0q_u32(msg[slot], msg[next]);
    uint32x4_t next_abcd = vsha256hq_u32(abcd, efgh, schedule);
    efgh = vsha256h2q_u32(efgh, abcd, schedule);
    abcd = next_abcd;
    if (i != 15)
      msg[slot] = vsha256su1q_u32(msg[slot], msg[(slot + 2u) & 3u], msg[(slot + 3u) & 3u]);
  }

  abcd = vaddq_u32(abcd, original_abcd);
  efgh = vaddq_u32(efgh, original_efgh);
  vst1q_u32(ctx->state, abcd);
  vst1q_u32(ctx->state + 4, efgh);
}

static _Bool aura_sha256_arm64_available(void)
{
#if defined(__APPLE__)
  int available = 0;
  size_t size = sizeof(available);
  return sysctlbyname("hw.optional.arm.FEAT_SHA256", &available, &size, NULL, 0) == 0 && available != 0;
#elif defined(__linux__) && defined(HWCAP_SHA2)
  return (getauxval(AT_HWCAP) & HWCAP_SHA2) != 0;
#else
  return 0;
#endif
}
#endif

#if defined(__x86_64__) && (defined(__clang__) || defined(__GNUC__))
static void AURA_SHA256_TARGET_CRYPTO aura_sha256_block_shani(AuraSha256 *ctx, const unsigned char *block)
{
  static const uint32_t k[64] = {
      0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
      0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
      0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
      0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
      0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
      0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
      0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
      0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u};
  uint32_t w[64];
  for (size_t i = 0; i < 16; i++)
    w[i] = ((uint32_t)block[i * 4] << 24) | ((uint32_t)block[i * 4 + 1] << 16) |
           ((uint32_t)block[i * 4 + 2] << 8) | block[i * 4 + 3];
  for (size_t i = 16; i < 64; i++)
  {
    uint32_t s0 = aura_sha_rotr(w[i - 15], 7) ^ aura_sha_rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
    uint32_t s1 = aura_sha_rotr(w[i - 2], 17) ^ aura_sha_rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
    w[i] = w[i - 16] + s0 + w[i - 7] + s1;
  }
  __m128i state0 = _mm_loadu_si128((const __m128i *)ctx->state);
  __m128i state1 = _mm_loadu_si128((const __m128i *)(ctx->state + 4));
  __m128i tmp = _mm_shuffle_epi32(state0, 0xB1);
  state1 = _mm_shuffle_epi32(state1, 0x1B);
  state0 = _mm_alignr_epi8(tmp, state1, 8);
  state1 = _mm_blend_epi16(state1, tmp, 0xF0);
  __m128i saved0 = state0;
  __m128i saved1 = state1;
  for (size_t i = 0; i < 64; i += 4)
  {
    __m128i message = _mm_set_epi32((int)(w[i + 3] + k[i + 3]), (int)(w[i + 2] + k[i + 2]),
                                    (int)(w[i + 1] + k[i + 1]), (int)(w[i] + k[i]));
    state1 = _mm_sha256rnds2_epu32(state1, state0, message);
    state0 = _mm_sha256rnds2_epu32(state0, state1, _mm_shuffle_epi32(message, 0x0E));
  }
  state0 = _mm_add_epi32(state0, saved0);
  state1 = _mm_add_epi32(state1, saved1);
  tmp = _mm_shuffle_epi32(state0, 0x1B);
  state1 = _mm_shuffle_epi32(state1, 0xB1);
  state0 = _mm_blend_epi16(tmp, state1, 0xF0);
  state1 = _mm_alignr_epi8(state1, tmp, 8);
  _mm_storeu_si128((__m128i *)ctx->state, state0);
  _mm_storeu_si128((__m128i *)(ctx->state + 4), state1);
}
#endif

static AuraSha256BlockFn aura_sha256_block_backend(void)
{
#if defined(__aarch64__) && (defined(__clang__) || defined(__GNUC__))
  if (aura_sha256_arm64_available()) return aura_sha256_block_arm64;
#elif defined(__x86_64__) && (defined(__clang__) || defined(__GNUC__))
  if (__builtin_cpu_supports("sha")) return aura_sha256_block_shani;
#endif
  return aura_sha256_block_portable;
}

static AuraSha256BlockFn aura_sha256_get_block_fn(void)
{
  static _Atomic(AuraSha256BlockFn) selected = (AuraSha256BlockFn)0;
  AuraSha256BlockFn fn = atomic_load_explicit(&selected, memory_order_acquire);
  if (fn == NULL)
  {
    fn = aura_sha256_block_backend();
    AuraSha256BlockFn expected = NULL;
    atomic_compare_exchange_strong_explicit(&selected, &expected, fn, memory_order_release, memory_order_relaxed);
    fn = atomic_load_explicit(&selected, memory_order_acquire);
  }
  return fn;
}

static void aura_sha256_init(AuraSha256 *ctx)
{ static const uint32_t s[8]={0x6a09e667u,0xbb67ae85u,0x3c6ef372u,0xa54ff53au,0x510e527fu,0x9b05688cu,0x1f83d9abu,0x5be0cd19u}; memcpy(ctx->state,s,sizeof(s));ctx->bits=0;ctx->used=0; }
static void aura_sha256_update(AuraSha256 *ctx,const unsigned char *data,size_t length)
{ AuraSha256BlockFn block_fn=aura_sha256_get_block_fn();ctx->bits+=(uint64_t)length*8u; if(ctx->used!=0){size_t take=64u-ctx->used;if(take>length)take=length;memcpy(ctx->block+ctx->used,data,take);ctx->used+=take;data+=take;length-=take;if(ctx->used==64u){block_fn(ctx,ctx->block);ctx->used=0;}} while(length>=64u){block_fn(ctx,data);data+=64u;length-=64u;} if(length!=0){memcpy(ctx->block,data,length);ctx->used=length;} }
static void aura_sha256_final(AuraSha256 *ctx,unsigned char digest[32])
{ AuraSha256BlockFn block_fn=aura_sha256_get_block_fn();size_t used=ctx->used;ctx->block[used++]=0x80u;if(used>56u){memset(ctx->block+used,0,64u-used);block_fn(ctx,ctx->block);used=0;}memset(ctx->block+used,0,56u-used);for(unsigned i=0;i<8;i++)ctx->block[56u+i]=(unsigned char)(ctx->bits>>(56u-i*8u));block_fn(ctx,ctx->block);for(unsigned i=0;i<8;i++)for(unsigned j=0;j<4;j++)digest[i*4+j]=(unsigned char)(ctx->state[i]>>(24u-j*8u)); }
static void aura_sha256_bytes(const unsigned char *data,size_t length,unsigned char digest[32])
{ AuraSha256 ctx;aura_sha256_init(&ctx);aura_sha256_update(&ctx,data,length);aura_sha256_final(&ctx,digest); }
static char *aura_digest_hex(const unsigned char digest[32])
{ static const char h[]="0123456789abcdef";char *out=(char *)malloc(65u);if(!out)return NULL;for(size_t i=0;i<32;i++){out[i*2]=h[digest[i]>>4];out[i*2+1]=h[digest[i]&15u];}out[64]='\0';return out; }

int aura_crypto_random_bytes_raw(void *output, size_t length)
{
  return aura_platform_random_bytes(output, length);
}

int aura_crypto_sha256_bytes(const void *input, size_t length, uint8_t output[32])
{
  if (output == NULL || (input == NULL && length != 0)) return 0;
  aura_sha256_bytes((const unsigned char *)input, length, output);
  return 1;
}

static uint32_t aura_md5_rotate(uint32_t value, uint32_t shift)
{
  return (value << shift) | (value >> (32u - shift));
}

static void aura_md5_block(uint32_t state[4], const uint8_t block[64])
{
  static const uint32_t shifts[64] = {
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21};
  static const uint32_t constants[64] = {
    0xd76aa478u, 0xe8c7b756u, 0x242070dbu, 0xc1bdceeeu, 0xf57c0fafu,
    0x4787c62au, 0xa8304613u, 0xfd469501u, 0x698098d8u, 0x8b44f7afu,
    0xffff5bb1u, 0x895cd7beu, 0x6b901122u, 0xfd987193u, 0xa679438eu,
    0x49b40821u, 0xf61e2562u, 0xc040b340u, 0x265e5a51u, 0xe9b6c7aau,
    0xd62f105du, 0x02441453u, 0xd8a1e681u, 0xe7d3fbc8u, 0x21e1cde6u,
    0xc33707d6u, 0xf4d50d87u, 0x455a14edu, 0xa9e3e905u, 0xfcefa3f8u,
    0x676f02d9u, 0x8d2a4c8au, 0xfffa3942u, 0x8771f681u, 0x6d9d6122u,
    0xfde5380cu, 0xa4beea44u, 0x4bdecfa9u, 0xf6bb4b60u, 0xbebfbc70u,
    0x289b7ec6u, 0xeaa127fau, 0xd4ef3085u, 0x04881d05u, 0xd9d4d039u,
    0xe6db99e5u, 0x1fa27cf8u, 0xc4ac5665u, 0xf4292244u, 0x432aff97u,
    0xab9423a7u, 0xfc93a039u, 0x655b59c3u, 0x8f0ccc92u, 0xffeff47du,
    0x85845dd1u, 0x6fa87e4fu, 0xfe2ce6e0u, 0xa3014314u, 0x4e0811a1u,
    0xf7537e82u, 0xbd3af235u, 0x2ad7d2bbu, 0xeb86d391u};
  uint32_t words[16];
  for (size_t i = 0; i < 16; i++) words[i] = (uint32_t)block[i * 4u] |
      ((uint32_t)block[i * 4u + 1u] << 8) | ((uint32_t)block[i * 4u + 2u] << 16) |
      ((uint32_t)block[i * 4u + 3u] << 24);
  uint32_t a = state[0], b = state[1], c = state[2], d = state[3];
  for (uint32_t i = 0; i < 64; i++) {
    uint32_t function, index;
    if (i < 16) { function = (b & c) | (~b & d); index = i; }
    else if (i < 32) { function = (d & b) | (~d & c); index = (5u * i + 1u) & 15u; }
    else if (i < 48) { function = b ^ c ^ d; index = (3u * i + 5u) & 15u; }
    else { function = c ^ (b | ~d); index = (7u * i) & 15u; }
    uint32_t next = d;
    d = c; c = b;
    b += aura_md5_rotate(a + function + constants[i] + words[index], shifts[i]);
    a = next;
  }
  state[0] += a; state[1] += b; state[2] += c; state[3] += d;
}

int aura_crypto_md5_bytes(const void *input, size_t length, uint8_t output[16])
{
  if (output == NULL || (input == NULL && length != 0) || length > (UINT64_MAX - 1u) / 8u)
    return 0;
  size_t original_length = length;
  uint32_t state[4] = {0x67452301u, 0xefcdab89u, 0x98badcfeu, 0x10325476u};
  const uint8_t *bytes = (const uint8_t *)input;
  while (length >= 64u) { aura_md5_block(state, bytes); bytes += 64u; length -= 64u; }
  uint8_t tail[128] = {0};
  if (length != 0) memcpy(tail, bytes, length);
  tail[length] = 0x80u;
  size_t tail_length = length < 56u ? 64u : 128u;
  uint64_t bits = (uint64_t)original_length * 8u;
  for (size_t i = 0; i < 8; i++) tail[tail_length - 8u + i] = (uint8_t)(bits >> (8u * i));
  aura_md5_block(state, tail);
  if (tail_length == 128u) aura_md5_block(state, tail + 64u);
  for (size_t i = 0; i < 4; i++) for (size_t j = 0; j < 4; j++)
    output[i * 4u + j] = (uint8_t)(state[i] >> (8u * j));
  return 1;
}

int aura_crypto_hmac_sha256_bytes(const void *key, size_t key_length,
                                  const void *input, size_t input_length,
                                  uint8_t output[32])
{
  if (output == NULL || (key == NULL && key_length != 0) ||
      (input == NULL && input_length != 0)) return 0;
  unsigned char normalized[64] = {0};
  if (key_length > sizeof(normalized)) aura_sha256_bytes(key, key_length, normalized);
  else if (key_length != 0) memcpy(normalized, key, key_length);
  unsigned char inner_pad[64], outer_pad[64], inner_digest[32];
  for (size_t i = 0; i < sizeof(normalized); i++) {
    inner_pad[i] = normalized[i] ^ 0x36u;
    outer_pad[i] = normalized[i] ^ 0x5cu;
  }
  AuraSha256 context;
  aura_sha256_init(&context);
  aura_sha256_update(&context, inner_pad, sizeof(inner_pad));
  aura_sha256_update(&context, input, input_length);
  aura_sha256_final(&context, inner_digest);
  aura_sha256_init(&context);
  aura_sha256_update(&context, outer_pad, sizeof(outer_pad));
  aura_sha256_update(&context, inner_digest, sizeof(inner_digest));
  aura_sha256_final(&context, output);
  return 1;
}

int aura_crypto_pbkdf2_sha256(const void *password, size_t password_length,
                              const void *salt, size_t salt_length,
                              uint32_t iterations, void *output,
                              size_t output_length)
{
  if (iterations == 0 || (password == NULL && password_length != 0) ||
      (salt == NULL && salt_length != 0) || (output == NULL && output_length != 0) ||
      salt_length > SIZE_MAX - 4u) return 0;
  unsigned char *result = (unsigned char *)output;
  unsigned char *message = (unsigned char *)malloc(salt_length + 4u);
  if (message == NULL && salt_length + 4u != 0) return 0;
  if (salt_length != 0) memcpy(message, salt, salt_length);
  size_t blocks = (output_length + 31u) / 32u;
  if (blocks > UINT32_MAX) { free(message); return 0; }
  for (size_t block = 1; block <= blocks; block++)
  {
    aura_write_int32_be(message + salt_length, (uint32_t)block);
    unsigned char u[32], accumulator[32];
    if (!aura_crypto_hmac_sha256_bytes(password, password_length, message,
                                       salt_length + 4u, u)) { free(message); return 0; }
    memcpy(accumulator, u, sizeof(accumulator));
    for (uint32_t round = 1; round < iterations; round++) {
      if (!aura_crypto_hmac_sha256_bytes(password, password_length, u, sizeof(u), u)) {
        free(message); return 0;
      }
      for (size_t i = 0; i < sizeof(accumulator); i++) accumulator[i] ^= u[i];
    }
    size_t offset = (block - 1u) * 32u;
    size_t copy = output_length - offset;
    if (copy > 32u) copy = 32u;
    memcpy(result + offset, accumulator, copy);
  }
  free(message);
  return 1;
}

const char *aura_crypto_sha256(const char *value)
{ const char *source=value==NULL?"":value;unsigned char digest[32];aura_sha256_bytes((const unsigned char *)source,strlen(source),digest);return aura_digest_hex(digest); }
const char *aura_crypto_hmac_sha256(const char *key,const char *value)
{ unsigned char kb[64]={0},inner[64],outer[64],digest[32];const unsigned char *k=(const unsigned char *)(key==NULL?"":key);size_t kl=strlen((const char *)k);if(kl>64u)aura_sha256_bytes(k,kl,kb);else memcpy(kb,k,kl);for(size_t i=0;i<64;i++){inner[i]=kb[i]^0x36u;outer[i]=kb[i]^0x5cu;}AuraSha256 ctx;aura_sha256_init(&ctx);aura_sha256_update(&ctx,inner,64u);const char *source=value==NULL?"":value;aura_sha256_update(&ctx,(const unsigned char *)source,strlen(source));aura_sha256_final(&ctx,digest);aura_sha256_init(&ctx);aura_sha256_update(&ctx,outer,64u);aura_sha256_update(&ctx,digest,32u);aura_sha256_final(&ctx,digest);return aura_digest_hex(digest); }
_Bool aura_crypto_constant_time_equals(const char *left,const char *right)
{ const unsigned char *a=(const unsigned char *)(left==NULL?"":left),*b=(const unsigned char *)(right==NULL?"":right);size_t al=strlen((const char *)a),bl=strlen((const char *)b),n=al>bl?al:bl;unsigned diff=(unsigned)(al^bl);for(size_t i=0;i<n;i++)diff|=(unsigned)(i<al?a[i]:0)^(unsigned)(i<bl?b[i]:0);return diff==0; }
const char *aura_crypto_random_bytes(int64_t length)
{ if(length<0||(uint64_t)length>SIZE_MAX-1u)return NULL;size_t n=(size_t)length;unsigned char *out=(unsigned char *)malloc(n+1u);if(!out)return NULL;
  if (!aura_platform_random_bytes(out, n)) { free(out); return NULL; }
  // Aura String is NUL-terminated; reject NUL bytes so the returned byte
  // string retains its requested length without truncating at the first byte.
  for (size_t i = 0; i < n; i++) if (out[i] == 0) out[i] = 1;
  out[n]='\0';return (const char *)out; }

static char *aura_binary_hex(const unsigned char *data, size_t length)
{
  static const char digits[] = "0123456789abcdef";
  if (length > (SIZE_MAX - 1u) / 2u) return NULL;
  char *out = (char *)malloc(length * 2u + 1u);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i++) { out[i * 2u] = digits[data[i] >> 4]; out[i * 2u + 1u] = digits[data[i] & 15u]; }
  out[length * 2u] = '\0';
  return out;
}

static int aura_hex_digit(unsigned char value)
{
  if (value >= '0' && value <= '9') return (int)(value - '0');
  if (value >= 'a' && value <= 'f') return (int)(value - 'a' + 10);
  if (value >= 'A' && value <= 'F') return (int)(value - 'A' + 10);
  return -1;
}

const char *aura_compress_text(const char *value, int64_t codec, int64_t level)
{
  const unsigned char *source = (const unsigned char *)(value == NULL ? "" : value);
  size_t source_len = strlen((const char *)source);
  uLong bound = compressBound((uLong)source_len);
  unsigned char *compressed = (unsigned char *)malloc((size_t)bound + 32u);
  if (compressed == NULL) return NULL;
  z_stream stream;
  memset(&stream, 0, sizeof(stream));
  int window_bits = codec == 0 ? 15 + 16 : 15;
  int normalized_level = level < 0 ? Z_DEFAULT_COMPRESSION : (level > 9 ? 9 : (int)level);
  if (deflateInit2(&stream, normalized_level, Z_DEFLATED, window_bits, 8, Z_DEFAULT_STRATEGY) != Z_OK) { free(compressed); return NULL; }
  stream.next_in = (Bytef *)source; stream.avail_in = (uInt)source_len;
  stream.next_out = compressed; stream.avail_out = (uInt)(bound + 32u);
  int result = deflate(&stream, Z_FINISH);
  size_t written = (size_t)stream.total_out;
  deflateEnd(&stream);
  if (result != Z_STREAM_END) { free(compressed); return NULL; }
  char *encoded = aura_binary_hex(compressed, written);
  free(compressed);
  return encoded;
}

const char *aura_decompress_text(const char *value, int64_t codec)
{
  const char *encoded = value == NULL ? "" : value;
  size_t encoded_len = strlen(encoded);
  if ((encoded_len & 1u) != 0 || encoded_len > 128u * 1024u * 1024u) return NULL;
  size_t compressed_len = encoded_len / 2u;
  unsigned char *compressed = (unsigned char *)malloc(compressed_len == 0 ? 1u : compressed_len);
  if (compressed == NULL) return NULL;
  for (size_t i = 0; i < compressed_len; i++) { int hi = aura_hex_digit((unsigned char)encoded[i * 2u]); int lo = aura_hex_digit((unsigned char)encoded[i * 2u + 1u]); if (hi < 0 || lo < 0) { free(compressed); return NULL; } compressed[i] = (unsigned char)((hi << 4) | lo); }
  size_t capacity = 4096u;
  unsigned char *output = (unsigned char *)malloc(capacity + 1u);
  if (output == NULL) { free(compressed); return NULL; }
  z_stream stream; memset(&stream, 0, sizeof(stream));
  int window_bits = codec == 0 ? 15 + 16 : 15;
  if (inflateInit2(&stream, window_bits) != Z_OK) { free(compressed); free(output); return NULL; }
  stream.next_in = compressed; stream.avail_in = (uInt)compressed_len;
  int result = Z_OK;
  while (result == Z_OK) {
    if (stream.total_out == capacity) { if (capacity >= 64u * 1024u * 1024u) { result = Z_MEM_ERROR; break; } capacity *= 2u; unsigned char *grown = (unsigned char *)realloc(output, capacity + 1u); if (grown == NULL) { result = Z_MEM_ERROR; break; } output = grown; }
    stream.next_out = output + stream.total_out; stream.avail_out = (uInt)(capacity - stream.total_out);
    result = inflate(&stream, Z_FINISH);
  }
  size_t written = (size_t)stream.total_out;
  inflateEnd(&stream); free(compressed);
  if (result != Z_STREAM_END || written > 64u * 1024u * 1024u || memchr(output, 0, written) != NULL) { free(output); return NULL; }
  output[written] = '\0';
  return (const char *)output;
}
