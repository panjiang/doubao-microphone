#include "AudioRing.h"
#include <stdatomic.h>
#include <stdlib.h>
struct PTTRing { _Atomic uint64_t read, write; _Atomic int failed; uint32_t capacity; float samples[]; };
PTTRing *ptt_ring_create(uint32_t capacity) {
    PTTRing *r = calloc(1, sizeof(PTTRing) + capacity * sizeof(float));
    if (r) { r->capacity=capacity; atomic_init(&r->read,0); atomic_init(&r->write,0); atomic_init(&r->failed,0); }
    return r;
}
void ptt_ring_destroy(PTTRing *r) { free(r); }
void ptt_ring_write(PTTRing *r, const float *const *channels, uint32_t count, uint32_t frames) {
    uint64_t w=atomic_load_explicit(&r->write,memory_order_relaxed);
    uint64_t rd=atomic_load_explicit(&r->read,memory_order_acquire);
    if (!count || w-rd+frames>r->capacity) { atomic_store(&r->failed,1); return; }
    for (uint32_t i=0;i<frames;i++) {
        float sum=0; for(uint32_t c=0;c<count;c++) sum+=channels[c][i];
        r->samples[(w+i)%r->capacity]=sum/count;
    }
    atomic_store_explicit(&r->write,w+frames,memory_order_release);
}
uint32_t ptt_ring_read(PTTRing *r,float *out,uint32_t max_frames) {
    uint64_t rd=atomic_load_explicit(&r->read,memory_order_relaxed);
    uint64_t w=atomic_load_explicit(&r->write,memory_order_acquire);
    uint32_t n=(uint32_t)(w-rd); if(n>max_frames)n=max_frames;
    for(uint32_t i=0;i<n;i++)out[i]=r->samples[(rd+i)%r->capacity];
    atomic_store_explicit(&r->read,rd+n,memory_order_release); return n;
}
uint32_t ptt_ring_count(PTTRing *r) { return (uint32_t)(atomic_load(&r->write)-atomic_load(&r->read)); }
int ptt_ring_failed(PTTRing *r) { return atomic_load(&r->failed); }
void ptt_ring_clear(PTTRing *r) { atomic_store_explicit(&r->read,atomic_load_explicit(&r->write,memory_order_acquire),memory_order_release); }
