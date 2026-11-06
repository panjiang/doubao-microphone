#ifndef PTT_AUDIO_RING_H
#define PTT_AUDIO_RING_H
#include <stdint.h>
#include <stddef.h>
typedef struct PTTRing PTTRing;
PTTRing *ptt_ring_create(uint32_t capacity);
void ptt_ring_destroy(PTTRing *ring);
void ptt_ring_write(PTTRing *ring, const float *const *channels, uint32_t channel_count, uint32_t frames);
uint32_t ptt_ring_read(PTTRing *ring, float *out, uint32_t max_frames);
uint32_t ptt_ring_count(PTTRing *ring);
int ptt_ring_failed(PTTRing *ring);
void ptt_ring_clear(PTTRing *ring);
#endif
