#define UNICODE
#define _UNICODE
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cfgmgr32.h>
#include <newdev.h>
#include <setupapi.h>
#include <stdio.h>
#include <wincrypt.h>
#include <wchar.h>
#include <initguid.h>
#include <devpkey.h>

static const WCHAR hardware_id[] = L"ROOT\\DOUBAOREMOTE_UMDF_TRIAL\0";

static BOOL trust(HCERTSTORE store, PCCERT_CONTEXT certificate, PCCERT_CONTEXT *added) {
    if (CertAddCertificateContextToStore(store, certificate, CERT_STORE_ADD_NEW, added)) {
        return TRUE;
    }
    if (GetLastError() != (DWORD)CRYPT_E_EXISTS) return FALSE;

    PCCERT_CONTEXT existing = CertFindCertificateInStore(
        store, X509_ASN_ENCODING, 0, CERT_FIND_EXISTING, certificate, NULL);
    BOOL same = existing && existing->cbCertEncoded == certificate->cbCertEncoded &&
        memcmp(existing->pbCertEncoded, certificate->pbCertEncoded, certificate->cbCertEncoded) == 0;
    CertFreeCertificateContext(existing);
    if (!same) SetLastError(ERROR_INVALID_DATA);
    return same;
}

// Match only this trial's hardware ID. The research device is independent.
static DWORD find_device(HDEVINFO devices, SP_DEVINFO_DATA *match, BOOL *found) {
    SP_DEVINFO_DATA device = { sizeof(device) };
    *found = FALSE;
    for (DWORD index = 0; SetupDiEnumDeviceInfo(devices, index, &device); ++index) {
        WCHAR ids[2048] = { 0 };
        DWORD type;
        if (!SetupDiGetDeviceRegistryPropertyW(devices, &device, SPDRP_HARDWAREID, &type,
                (BYTE *)ids, sizeof(ids), NULL) || type != REG_MULTI_SZ) continue;
        for (const WCHAR *id = ids; *id; id += wcslen(id) + 1) {
            if (_wcsicmp(id, hardware_id) != 0) continue;
            if (*found) return ERROR_DUP_NAME;
            *match = device;
            *found = TRUE;
        }
    }
    DWORD error = GetLastError();
    return error == ERROR_NO_MORE_ITEMS ? ERROR_SUCCESS : error;
}

static DWORD install(const WCHAR *inf, const WCHAR *public_certificate) {
    DWORD error = ERROR_SUCCESS;
    GUID class_guid;
    WCHAR class_name[MAX_CLASS_NAME_LEN];
    if (!SetupDiGetINFClassW(inf, &class_guid, class_name, ARRAYSIZE(class_name), NULL))
        return GetLastError();

    HDEVINFO devices = SetupDiGetClassDevsW(&class_guid, NULL, NULL, 0);
    if (devices == INVALID_HANDLE_VALUE) return GetLastError();

    SP_DEVINFO_DATA device = { sizeof(device) };
    BOOL found, registered = FALSE, staged = FALSE, reboot = FALSE;
    WCHAR published[MAX_PATH] = { 0 };
    PWSTR published_name = NULL;
    HCERTSTORE root = NULL, publishers = NULL;
    const void *certificate = NULL;
    PCCERT_CONTEXT added_root = NULL, added_publisher = NULL;

    error = find_device(devices, &device, &found);
    if (error != ERROR_SUCCESS) goto finish;
    if (!CryptQueryObject(CERT_QUERY_OBJECT_FILE, public_certificate, CERT_QUERY_CONTENT_FLAG_CERT,
            CERT_QUERY_FORMAT_FLAG_BINARY, 0, NULL, NULL, NULL, NULL, NULL, &certificate)) {
        error = GetLastError();
        goto finish;
    }
    root = CertOpenStore(CERT_STORE_PROV_SYSTEM_W, 0, 0,
        CERT_SYSTEM_STORE_LOCAL_MACHINE | CERT_STORE_OPEN_EXISTING_FLAG, L"Root");
    if (!root || !trust(root, certificate, &added_root)) {
        error = GetLastError();
        goto finish;
    }
    publishers = CertOpenStore(CERT_STORE_PROV_SYSTEM_W, 0, 0,
        CERT_SYSTEM_STORE_LOCAL_MACHINE | CERT_STORE_OPEN_EXISTING_FLAG, L"TrustedPublisher");
    if (!publishers || !trust(publishers, certificate, &added_publisher)) {
        error = GetLastError();
        goto finish;
    }

    staged = SetupCopyOEMInfW(inf, NULL, SPOST_PATH, SP_COPY_NOOVERWRITE,
        published, ARRAYSIZE(published), NULL, &published_name);
    if (!staged && GetLastError() != ERROR_FILE_EXISTS) {
        error = GetLastError();
        goto finish;
    }

    if (!found) {
        if (!SetupDiCreateDeviceInfoW(devices, L"DOUBAOREMOTE_UMDF_TRIAL", &class_guid, NULL,
                NULL, DICD_GENERATE_ID, &device) ||
            !SetupDiSetDeviceRegistryPropertyW(devices, &device, SPDRP_HARDWAREID,
                (const BYTE *)hardware_id, sizeof(hardware_id)) ||
            !SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devices, &device)) {
            error = GetLastError();
            goto finish;
        }
        registered = TRUE;
    }

    if (!UpdateDriverForPlugAndPlayDevicesW(NULL, hardware_id, inf, INSTALLFLAG_NONINTERACTIVE, &reboot)) {
        error = GetLastError();
        if (error != ERROR_NO_MORE_ITEMS) goto finish;
        WCHAR bound_inf[MAX_PATH];
        DEVPROPTYPE type;
        if (!published_name ||
            !SetupDiGetDevicePropertyW(devices, &device, &DEVPKEY_Device_DriverInfPath,
                &type, (BYTE *)bound_inf, sizeof(bound_inf), NULL, 0) ||
            type != DEVPROP_TYPE_STRING || _wcsicmp(bound_inf, published_name) != 0) goto finish;
        error = ERROR_SUCCESS;
    }
    if (!reboot) {
        ULONG status, problem;
        CONFIGRET result = CM_Get_DevNode_Status(&status, &problem, device.DevInst, 0);
        if (result != CR_SUCCESS) error = CM_MapCrToWin32Err(result, ERROR_GEN_FAILURE);
        else if ((status & DN_HAS_PROBLEM) && problem == CM_PROB_NEED_RESTART) reboot = TRUE;
        else if (!(status & DN_STARTED) || (status & DN_HAS_PROBLEM)) error = ERROR_DEVICE_NOT_AVAILABLE;
    }

finish:
    if (error != ERROR_SUCCESS) {
        // Keep trust if rollback leaves the newly staged driver in use.
        BOOL removed = !registered || SetupDiCallClassInstaller(DIF_REMOVE, devices, &device);
        BOOL unstaged = !staged || (published_name && SetupUninstallOEMInfW(published_name, 0, NULL));
        if (removed && unstaged) {
            if (added_publisher) CertDeleteCertificateFromStore(added_publisher);
            if (added_root) CertDeleteCertificateFromStore(added_root);
            added_publisher = added_root = NULL;
        }
    }
    CertFreeCertificateContext(added_publisher);
    CertFreeCertificateContext(added_root);
    CertFreeCertificateContext(certificate);
    if (publishers) CertCloseStore(publishers, 0);
    if (root) CertCloseStore(root, 0);
    SetupDiDestroyDeviceInfoList(devices);
    return error == ERROR_SUCCESS && reboot ? ERROR_SUCCESS_REBOOT_REQUIRED : error;
}

static BOOL install_cable(void) {
    WCHAR module[MAX_PATH], directory[MAX_PATH], executable[MAX_PATH];
    WCHAR command[MAX_PATH + 16];
    DWORD length = GetModuleFileNameW(NULL, module, ARRAYSIZE(module));
    if (!length || length >= ARRAYSIZE(module)) return FALSE;
    WCHAR *separator = wcsrchr(module, L'\\');
    if (!separator) return FALSE;
    *separator = L'\0';
    if (swprintf_s(directory, ARRAYSIZE(directory), L"%ls\\VB-CABLE", module) < 0 ||
        swprintf_s(executable, ARRAYSIZE(executable), L"%ls\\VBCABLE_Setup_x64.exe", directory) < 0 ||
        swprintf_s(command, ARRAYSIZE(command), L"\"%ls\" -i -h", executable) < 0) return FALSE;

    STARTUPINFOW startup = { sizeof(startup) };
    PROCESS_INFORMATION process = { 0 };
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = SW_HIDE;
    // The wizard verifies and holds the original vendor files while this runs.
    if (!CreateProcessW(executable, command, NULL, NULL, FALSE, 0, NULL, directory,
            &startup, &process)) return FALSE;
    CloseHandle(process.hThread);
    BOOL completed = WaitForSingleObject(process.hProcess, INFINITE) == WAIT_OBJECT_0;
    CloseHandle(process.hProcess);
    // The wizard checks the actual audio device after the vendor process exits.
    return completed;
}

int wmain(int argc, WCHAR **argv) {
    if (!SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)) return (int)GetLastError();
    BOOL with_cable = argc == 4 && wcscmp(argv[3], L"--install-cable") == 0;
    if (argc != 3 && !with_cable) return ERROR_INVALID_PARAMETER;
    HANDLE lock = CreateMutexW(NULL, FALSE, L"Global\\DoubaoRemote.UmdfTrial.Setup");
    if (!lock) return (int)GetLastError();
    DWORD wait = WaitForSingleObject(lock, 0);
    DWORD result = ERROR_INSTALL_ALREADY_RUNNING;
    if (wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED) {
        result = install(argv[1], argv[2]);
        if ((result == ERROR_SUCCESS || result == ERROR_SUCCESS_REBOOT_REQUIRED) &&
            with_cable && !install_cable()) result = 0x20000001; // Paired with the trial wizard.
        ReleaseMutex(lock);
    }
    CloseHandle(lock);
    return (int)result;
}
