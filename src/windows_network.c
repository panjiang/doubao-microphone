#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <iphlpapi.h>
#include <netioapi.h>
#include <stdint.h>

// Prefer active physical Ethernet, then active physical Wi-Fi, then virtual NICs.
// Adapter aliases may be localized or renamed, so do not classify by their name.
uint32_t ptt_network_rank(uint32_t index) {
    MIB_IF_ROW2 row = {0};
    row.InterfaceIndex = index;
    if (GetIfEntry2(&row) != NO_ERROR || row.OperStatus != IfOperStatusUp) return 3;
    if (row.InterfaceAndOperStatusFlags.HardwareInterface) {
        if (row.Type == IF_TYPE_ETHERNET_CSMACD) return 0;
        if (row.Type == IF_TYPE_IEEE80211) return 1;
    }
    return 2;
}
