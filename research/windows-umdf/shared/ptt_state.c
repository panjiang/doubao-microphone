#include "ptt_state.h"

void PttInit(PTT_STATE *state) {
    PTT_STATE empty = {0};
    *state = empty;
}

static int32_t SubmitKey(uint32_t key, PTT_SUBMIT submit, void *context) {
    PTT_KEYBOARD_REPORT report = {0};
    report.report_id = 1;
    if (key == PTT_KEY_RIGHT_ALT) report.modifiers = 0x40;
    if (key == PTT_KEY_RIGHT_CTRL) report.modifiers = 0x10;
    return submit(context, &report);
}

PTT_RESULT PttRelease(PTT_STATE *state, PTT_SUBMIT submit, void *context) {
    if (!state->held_key) return PttOk;
    state->last_submit_status = SubmitKey(PTT_KEY_NONE, submit, context);
    if (state->last_submit_status < 0) {
        state->release_failures++;
        /* Retain ownership; the watchdog must retry, not report success. */
        state->deadline_ms = 0;
        return PttTransportError;
    }
    state->held_key = PTT_KEY_NONE;
    state->deadline_ms = 0;
    return PttOk;
}

PTT_RESULT PttExpire(PTT_STATE *state, uint64_t now_ms,
                     PTT_SUBMIT submit, void *context) {
    if (!state->held_key || now_ms < state->deadline_ms) return PttOk;
    if (state->deadline_ms != 0) state->expiry_count++;
    return PttRelease(state, submit, context);
}

PTT_RESULT PttApply(PTT_STATE *state, const PTT_COMMAND *command,
                    uint64_t now_ms, PTT_SUBMIT submit, void *context) {
    PTT_RESULT result;
    if (command->version != PTT_PROTOCOL_VERSION || command->reserved != 0)
        return PttInvalid;
    if (command->operation == PTT_OP_BEGIN) {
        if (command->key != PTT_KEY_RIGHT_ALT && command->key != PTT_KEY_RIGHT_CTRL)
            return PttInvalid;
    } else if (command->operation == PTT_OP_RENEW || command->operation == PTT_OP_RELEASE) {
        if (command->key != PTT_KEY_NONE) return PttInvalid;
    } else return PttInvalid;
    result = PttExpire(state, now_ms, submit, context);
    if (result != PttOk) return result;
    switch (command->operation) {
    case PTT_OP_BEGIN:
        if (state->held_key && state->held_key != command->key) return PttConflict;
        if (!state->held_key) {
            state->last_submit_status = SubmitKey(command->key, submit, context);
            if (state->last_submit_status < 0) return PttTransportError;
            state->held_key = command->key;
        }
        state->deadline_ms = now_ms + PTT_LEASE_MS;
        return PttOk;
    case PTT_OP_RENEW:
        if (!state->held_key) return PttNotHeld;
        state->deadline_ms = now_ms + PTT_LEASE_MS;
        return PttOk;
    case PTT_OP_RELEASE:
        return PttRelease(state, submit, context);
    default:
        return PttInvalid;
    }
}
