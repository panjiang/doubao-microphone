#ifndef PTT_VHF_STATE_H
#define PTT_VHF_STATE_H
#include "ptt_protocol.h"

typedef int32_t (*PTT_SUBMIT)(void *context, const PTT_KEYBOARD_REPORT *report);
typedef enum PTT_RESULT {
    PttOk, PttInvalid, PttConflict, PttNotHeld, PttTransportError
} PTT_RESULT;
typedef struct PTT_STATE {
    uint32_t held_key;
    uint64_t deadline_ms;
    int32_t last_submit_status;
    uint32_t release_failures;
    uint32_t expiry_count;
} PTT_STATE;

void PttInit(PTT_STATE *state);
PTT_RESULT PttRelease(PTT_STATE *state, PTT_SUBMIT submit, void *context);
PTT_RESULT PttExpire(PTT_STATE *state, uint64_t now_ms,
                     PTT_SUBMIT submit, void *context);
PTT_RESULT PttApply(PTT_STATE *state, const PTT_COMMAND *command,
                    uint64_t now_ms, PTT_SUBMIT submit, void *context);
#endif
