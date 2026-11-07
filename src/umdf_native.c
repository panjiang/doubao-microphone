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

#ifdef DOUBAO_UMDF_TRIAL
#define CONTROL_PRODUCT L"DoubaoMicrophone UMDF Trial Keyboard"
#else
#define CONTROL_PRODUCT L"DoubaoPTT UMDF P0 Keyboard"
#endif

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
                wcscmp(product, CONTROL_PRODUCT) == 0 && HidD_GetPreparsedData(handle, &preparsed)) {
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

// The public app ABI only exposes a device handle and fixed-key commands.
intptr_t ptt_umdf_open(void) { return (intptr_t)OpenControl(); }
uint32_t ptt_umdf_command(intptr_t h, uint8_t op, uint32_t key) {
    if (Command((HANDLE)h, op, key)) return 0;
    DWORD error = GetLastError(); return error ? error : ERROR_GEN_FAILURE;
}
void ptt_umdf_close(intptr_t h) { if ((HANDLE)h != INVALID_HANDLE_VALUE) CloseHandle((HANDLE)h); }
