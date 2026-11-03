#ifndef PTT_VHF_PROTOCOL_H
#define PTT_VHF_PROTOCOL_H

#include <stdint.h>

#define PTT_PROTOCOL_VERSION 1u
#define PTT_LEASE_MS 1000u
#define PTT_WATCHDOG_MS 100u
#define PTT_KEY_NONE 0u
#define PTT_KEY_RIGHT_ALT 1u
#define PTT_KEY_RIGHT_CTRL 2u
#define PTT_OP_BEGIN 1u
#define PTT_OP_RENEW 2u
#define PTT_OP_RELEASE 3u
#define PTT_DEVICE_TYPE 0x8000u
/* CTL_CODE: vendor device, function 0x800/0x801, METHOD_BUFFERED,
 * FILE_READ_DATA | FILE_WRITE_DATA. No arbitrary report/scan-code IOCTL. */
#define PTT_IOCTL_COMMAND ((PTT_DEVICE_TYPE << 16) | (3u << 14) | (0x800u << 2))
#define PTT_IOCTL_STATUS ((PTT_DEVICE_TYPE << 16) | (3u << 14) | (0x801u << 2))

typedef struct PTT_COMMAND {
    uint32_t version;
    uint32_t operation;
    uint32_t key;
    uint32_t reserved;
} PTT_COMMAND;

typedef struct PTT_STATUS {
    uint32_t version;
    uint32_t held_key;
    int32_t last_submit_status;
    uint32_t release_failures;
    uint32_t expiry_count;
    uint32_t online;
} PTT_STATUS;

/* Report ID 1, modifiers, reserved byte, six empty key slots. */
typedef struct PTT_KEYBOARD_REPORT {
    uint8_t report_id;
    uint8_t modifiers;
    uint8_t reserved;
    uint8_t keys[6];
} PTT_KEYBOARD_REPORT;

#endif
