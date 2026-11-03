#define UNICODE
#define _UNICODE
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <setupapi.h>
#include <hidsdi.h>
#include <hidpi.h>
#include <stdio.h>
#include <stdint.h>
#include <wchar.h>
#include "../shared/ptt_protocol.h"

static HANDLE OpenControl(void) {
    GUID guid;
    SP_DEVICE_INTERFACE_DATA interfaceData = {0};
    HDEVINFO devices;
    HANDLE result = INVALID_HANDLE_VALUE;
    HidD_GetHidGuid(&guid);
    devices = SetupDiGetClassDevsW(&guid, NULL, NULL, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (devices == INVALID_HANDLE_VALUE) return result;
    interfaceData.cbSize = sizeof(interfaceData);
    for (DWORD i = 0; SetupDiEnumDeviceInterfaces(devices, NULL, &guid, i, &interfaceData); i++) {
        DWORD required = 0;
        PSP_DEVICE_INTERFACE_DETAIL_DATA_W detail;
        HANDLE handle;
        WCHAR product[128] = {0};
        PHIDP_PREPARSED_DATA preparsed;
        HIDP_CAPS caps;
        SetupDiGetDeviceInterfaceDetailW(devices, &interfaceData, NULL, 0, &required, NULL);
        if (!required) continue;
        detail = HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, required);
        if (!detail) break;
        detail->cbSize = sizeof(*detail);
        if (!SetupDiGetDeviceInterfaceDetailW(devices, &interfaceData, detail, required, NULL, NULL)) {
            HeapFree(GetProcessHeap(), 0, detail); continue;
        }
        handle = CreateFileW(detail->DevicePath, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL);
        if (handle != INVALID_HANDLE_VALUE) {
            if (HidD_GetProductString(handle, product, sizeof(product)) &&
                wcscmp(product, L"DoubaoPTT UMDF P0 Keyboard") == 0 && HidD_GetPreparsedData(handle, &preparsed)) {
                if (HidP_GetCaps(preparsed, &caps) == HIDP_STATUS_SUCCESS && caps.UsagePage == 0xff00 && caps.Usage == 1) {
                    result = CreateFileW(detail->DevicePath, GENERIC_READ | GENERIC_WRITE,
                        FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL);
                }
                HidD_FreePreparsedData(preparsed);
            }
            CloseHandle(handle);
        }
        HeapFree(GetProcessHeap(), 0, detail);
        if (result != INVALID_HANDLE_VALUE) break;
    }
    SetupDiDestroyDeviceInfoList(devices);
    return result;
}
static BOOL Command(HANDLE device, uint8_t operation, uint32_t key) {
    uint8_t bytes[32] = {1};
    bytes[1] = operation;
    memcpy(bytes + 2, &key, sizeof(key));
    if (!HidD_SetFeature(device, bytes, sizeof(bytes))) {
        fprintf(stderr, "set_feature_error=%lu operation=%u\n", GetLastError(), operation); return FALSE;
    }
    return TRUE;
}
static int Status(HANDLE device) {
    uint8_t bytes[32] = {1};
    PTT_STATUS status;
    if (!HidD_GetFeature(device, bytes, sizeof(bytes))) {
        fprintf(stderr, "get_feature_error=%lu\n", GetLastError()); return 1;
    }
    memcpy(&status, bytes + 1, sizeof(status));
    printf("version=%u online=%u held=%u expired=%u release_errors=%u\n", status.version,
        status.online, status.held_key, status.expiry_count, status.release_failures);
    return status.version == PTT_PROTOCOL_VERSION ? 0 : 1;
}
int wmain(int argc, wchar_t **argv) {
    HANDLE device;
    int result;
    if (!(argc == 2 && wcscmp(argv[1], L"status") == 0) &&
        !(argc == 3 && (wcscmp(argv[1], L"hold") == 0 || wcscmp(argv[1], L"expire") == 0) &&
          (wcscmp(argv[2], L"right-alt") == 0 || wcscmp(argv[2], L"right-ctrl") == 0))) {
        puts("umdf-probe status | hold <right-alt|right-ctrl> | expire <right-alt|right-ctrl>"); return 2;
    }
    device = OpenControl();
    if (device == INVALID_HANDLE_VALUE) { fprintf(stderr, "research_device_not_found_or_access_denied\n"); return 1; }
    if (argc == 2) { result = Status(device); CloseHandle(device); return result; }
    for (int key = 0xa0; key <= 0xa5; key++) {
        if (GetAsyncKeyState(key) < 0) { puts("release_physical_modifier_first"); CloseHandle(device); return 1; }
    }
    puts("starts_in_ms=3000 duration_ms=2000"); fflush(stdout);
    Sleep(3000);
    result = 1;
    if (Command(device, PTT_OP_BEGIN, wcscmp(argv[2], L"right-alt") == 0 ? PTT_KEY_RIGHT_ALT : PTT_KEY_RIGHT_CTRL)) {
        result = 0;
        for (int i = 0; i < 8; i++) {
            Sleep(250);
            if (wcscmp(argv[1], L"hold") == 0 && !Command(device, PTT_OP_RENEW, 0)) { result = 1; break; }
        }
        if (!Command(device, PTT_OP_RELEASE, 0)) result = 1;
        Sleep(100);
        if (Status(device)) result = 1;
    }
    CloseHandle(device);
    return result;
}
