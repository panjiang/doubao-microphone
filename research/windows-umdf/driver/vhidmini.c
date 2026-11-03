/*++

Copyright (C) Microsoft Corporation, All Rights Reserved.

Module Name:

    vhidmini.cpp

Abstract:

    This module contains the implementation of the driver

Environment:

    Windows Driver Framework (WDF)

--*/

#include "vhidmini.h"
EVT_WDF_OBJECT_CONTEXT_CLEANUP PttCleanup;
static int32_t PttSubmit(void *context, const PTT_KEYBOARD_REPORT *report);

//
// This is the default report descriptor for the virtual Hid device returned
// by the mini driver in response to IOCTL_HID_GET_REPORT_DESCRIPTOR.
//
HID_REPORT_DESCRIPTOR       G_DefaultReportDescriptor[] = {
    0x06,0x00, 0xFF,                // USAGE_PAGE (Vender Defined Usage Page)
    0x09,0x01,                      // USAGE (Vendor Usage 0x01)
    0xA1,0x01,                      // COLLECTION (Application)
    0x85,CONTROL_FEATURE_REPORT_ID,    // REPORT_ID (1)
    0x09,0x01,                         // USAGE (Vendor Usage 0x01)
    0x15,0x00,                         // LOGICAL_MINIMUM(0)
    0x26,0xff, 0x00,                   // LOGICAL_MAXIMUM(255)
    0x75,0x08,                         // REPORT_SIZE (0x08)
    0x96,(FEATURE_REPORT_SIZE_CB & 0xff), (FEATURE_REPORT_SIZE_CB >> 8), // REPORT_COUNT
    0xB1,0x00,                         // FEATURE (Data,Ary,Abs)
    0x09,0x01,                         // USAGE (Vendor Usage 0x01)
    0x75,0x08,                         // REPORT_SIZE (0x08)
    0x96,(INPUT_REPORT_SIZE_CB & 0xff), (INPUT_REPORT_SIZE_CB >> 8), // REPORT_COUNT
    0x81,0x00,                         // INPUT (Data,Ary,Abs)
    0x09,0x01,                         // USAGE (Vendor Usage 0x01)
    0x75,0x08,                         // REPORT_SIZE (0x08)
    0x96,(OUTPUT_REPORT_SIZE_CB & 0xff), (OUTPUT_REPORT_SIZE_CB >> 8), // REPORT_COUNT
    0x91,0x00,                         // OUTPUT (Data,Ary,Abs)
    0xC0,                           // END_COLLECTION
    // Separate standard keyboard collection, Report ID 2.
    0x05,0x01,0x09,0x06,0xA1,0x01,0x85,0x02,
    0x05,0x07,0x19,0xE0,0x29,0xE7,0x15,0x00,0x25,0x01,
    0x75,0x01,0x95,0x08,0x81,0x02,
    0x75,0x08,0x95,0x01,0x81,0x03,
    0x19,0x00,0x29,0x65,0x15,0x00,0x25,0x65,
    0x75,0x08,0x95,0x06,0x81,0x00,0xC0
};

//
// This is the default HID descriptor returned by the mini driver
// in response to IOCTL_HID_GET_DEVICE_DESCRIPTOR. The size
// of report descriptor is currently the size of G_DefaultReportDescriptor.
//

HID_DESCRIPTOR              G_DefaultHidDescriptor = {
    0x09,   // length of HID descriptor
    0x21,   // descriptor type == HID  0x21
    0x0100, // hid spec release
    0x00,   // country code == Not Specified
    0x01,   // number of HID class descriptors
    {                                       //DescriptorList[0]
        0x22,                               //report descriptor type 0x22
        sizeof(G_DefaultReportDescriptor)   //total length of report descriptor
    }
};

NTSTATUS
DriverEntry(
    _In_  PDRIVER_OBJECT    DriverObject,
    _In_  PUNICODE_STRING   RegistryPath
    )
/*++

Routine Description:
    DriverEntry initializes the driver and is the first routine called by the
    system after the driver is loaded. DriverEntry specifies the other entry
    points in the function driver, such as EvtDevice and DriverUnload.

Parameters Description:

    DriverObject - represents the instance of the function driver that is loaded
    into memory. DriverEntry must initialize members of DriverObject before it
    returns to the caller. DriverObject is allocated by the system before the
    driver is loaded, and it is released by the system after the system unloads
    the function driver from memory.

    RegistryPath - represents the driver specific path in the Registry.
    The function driver can use the path to store driver related data between
    reboots. The path does not store hardware instance specific data.

Return Value:

    STATUS_SUCCESS, or another status value for which NT_SUCCESS(status) equals
                    TRUE if successful,

    STATUS_UNSUCCESSFUL, or another status for which NT_SUCCESS(status) equals
                    FALSE otherwise.

--*/
{
    WDF_DRIVER_CONFIG       config;
    NTSTATUS                status;

    KdPrint(("DriverEntry for VHidMini\n"));

#ifdef _KERNEL_MODE
    //
    // Opt-in to using non-executable pool memory on Windows 8 and later.
    // https://msdn.microsoft.com/en-us/library/windows/hardware/hh920402(v=vs.85).aspx
    //
    ExInitializeDriverRuntime(DrvRtPoolNxOptIn);
#endif

    WDF_DRIVER_CONFIG_INIT(&config, EvtDeviceAdd);

    status = WdfDriverCreate(DriverObject,
                            RegistryPath,
                            WDF_NO_OBJECT_ATTRIBUTES,
                            &config,
                            WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        KdPrint(("Error: WdfDriverCreate failed 0x%x\n", status));
        return status;
    }

    return status;
}

NTSTATUS
EvtDeviceAdd(
    _In_  WDFDRIVER         Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
/*++
Routine Description:

    EvtDeviceAdd is called by the framework in response to AddDevice
    call from the PnP manager. We create and initialize a device object to
    represent a new instance of the device.

Arguments:

    Driver - Handle to a framework driver object created in DriverEntry

    DeviceInit - Pointer to a framework-allocated WDFDEVICE_INIT structure.

Return Value:

    NTSTATUS

--*/
{
    NTSTATUS                status;
    WDF_OBJECT_ATTRIBUTES   deviceAttributes;
    WDFDEVICE               device;
    PDEVICE_CONTEXT         deviceContext;
    PHID_DEVICE_ATTRIBUTES  hidAttributes;
    UNREFERENCED_PARAMETER  (Driver);

    KdPrint(("Enter EvtDeviceAdd\n"));

    //
    // Mark ourselves as a filter, which also relinquishes power policy ownership
    //
    WdfFdoInitSetFilter(DeviceInit);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(
                            &deviceAttributes,
                            DEVICE_CONTEXT);
    deviceAttributes.ExecutionLevel = WdfExecutionLevelPassive;
    deviceAttributes.EvtCleanupCallback = PttCleanup;

    status = WdfDeviceCreate(&DeviceInit,
                            &deviceAttributes,
                            &device);
    if (!NT_SUCCESS(status)) {
        KdPrint(("Error: WdfDeviceCreate failed 0x%x\n", status));
        return status;
    }

    deviceContext = GetDeviceContext(device);
    deviceContext->Device       = device;
    deviceContext->DeviceData   = 0;
    PttInit(&deviceContext->State);
    WDF_OBJECT_ATTRIBUTES lockAttributes;
    WDF_OBJECT_ATTRIBUTES_INIT(&lockAttributes);
    lockAttributes.ParentObject = device;
    status = WdfWaitLockCreate(&lockAttributes, &deviceContext->Lock);
    if (!NT_SUCCESS(status)) return status;

    hidAttributes = &deviceContext->HidDeviceAttributes;
    RtlZeroMemory(hidAttributes, sizeof(HID_DEVICE_ATTRIBUTES));
    hidAttributes->Size         = sizeof(HID_DEVICE_ATTRIBUTES);
    hidAttributes->VendorID     = HIDMINI_VID;
    hidAttributes->ProductID    = HIDMINI_PID;
    hidAttributes->VersionNumber = HIDMINI_VERSION;

    status = QueueCreate(device,
                         &deviceContext->DefaultQueue);
    if( !NT_SUCCESS(status) ) {
        return status;
    }

    status = ManualQueueCreate(device,
                               &deviceContext->ManualQueue);
    if( !NT_SUCCESS(status) ) {
        return status;
    }

    //
    // Use default "HID Descriptor" (hardcoded). We will set the
    // wReportLength memeber of HID descriptor when we read the
    // the report descriptor either from registry or the hard-coded
    // one.
    //
    deviceContext->HidDescriptor = G_DefaultHidDescriptor;

    deviceContext->ReportDescriptor = G_DefaultReportDescriptor;
    WDF_TIMER_CONFIG timerConfig;
    WDF_OBJECT_ATTRIBUTES timerAttributes;
    WDF_TIMER_CONFIG_INIT(&timerConfig, EvtTimerFunc);
    timerConfig.AutomaticSerialization = FALSE;
    WDF_OBJECT_ATTRIBUTES_INIT(&timerAttributes);
    timerAttributes.ParentObject = device;
    timerAttributes.ExecutionLevel = WdfExecutionLevelPassive;
    status = WdfTimerCreate(&timerConfig, &timerAttributes, &deviceContext->LeaseTimer);
    return status;
}

#ifdef _KERNEL_MODE
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL EvtIoDeviceControl;
#else
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL          EvtIoDeviceControl;
#endif

NTSTATUS
QueueCreate(
    _In_  WDFDEVICE         Device,
    _Out_ WDFQUEUE          *Queue
    )
/*++
Routine Description:

    This function creates a default, parallel I/O queue to proces IOCTLs
    from hidclass.sys.

Arguments:

    Device - Handle to a framework device object.

    Queue - Output pointer to a framework I/O queue handle, on success.

Return Value:

    NTSTATUS

--*/
{
    NTSTATUS                status;
    WDF_IO_QUEUE_CONFIG     queueConfig;
    WDF_OBJECT_ATTRIBUTES   queueAttributes;
    WDFQUEUE                queue;
    PQUEUE_CONTEXT          queueContext;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(
                            &queueConfig,
                            WdfIoQueueDispatchParallel);

#ifdef _KERNEL_MODE
    queueConfig.EvtIoInternalDeviceControl  = EvtIoDeviceControl;
#else
    //
    // HIDclass uses INTERNAL_IOCTL which is not supported by UMDF. Therefore
    // the hidumdf.sys changes the IOCTL type to DEVICE_CONTROL for next stack
    // and sends it down
    //
    queueConfig.EvtIoDeviceControl          = EvtIoDeviceControl;
#endif

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(
                            &queueAttributes,
                            QUEUE_CONTEXT);

    status = WdfIoQueueCreate(
                            Device,
                            &queueConfig,
                            &queueAttributes,
                            &queue);

    if( !NT_SUCCESS(status) ) {
        KdPrint(("WdfIoQueueCreate failed 0x%x\n",status));
        return status;
    }

    queueContext = GetQueueContext(queue);
    queueContext->Queue         = queue;
    queueContext->DeviceContext = GetDeviceContext(Device);
    queueContext->OutputReport  = 0;

    *Queue = queue;
    return status;
}

VOID
EvtIoDeviceControl(
    _In_  WDFQUEUE          Queue,
    _In_  WDFREQUEST        Request,
    _In_  size_t            OutputBufferLength,
    _In_  size_t            InputBufferLength,
    _In_  ULONG             IoControlCode
    )
/*++
Routine Description:

    This event callback function is called when the driver receives an

    (KMDF) IOCTL_HID_Xxx code when handlng IRP_MJ_INTERNAL_DEVICE_CONTROL
    (UMDF) IOCTL_HID_Xxx, IOCTL_UMDF_HID_Xxx when handling IRP_MJ_DEVICE_CONTROL

Arguments:

    Queue - A handle to the queue object that is associated with the I/O request

    Request - A handle to a framework request object.

    OutputBufferLength - The length, in bytes, of the request's output buffer,
            if an output buffer is available.

    InputBufferLength - The length, in bytes, of the request's input buffer, if
            an input buffer is available.

    IoControlCode - The driver or system defined IOCTL associated with the request

Return Value:

    NTSTATUS

--*/
{
    NTSTATUS                status;
    BOOLEAN                 completeRequest = TRUE;
    WDFDEVICE               device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT         deviceContext = NULL;
    PQUEUE_CONTEXT          queueContext = GetQueueContext(Queue);
    UNREFERENCED_PARAMETER  (OutputBufferLength);
    UNREFERENCED_PARAMETER  (InputBufferLength);

    deviceContext = GetDeviceContext(device);

    switch (IoControlCode)
    {
    case IOCTL_HID_GET_DEVICE_DESCRIPTOR:   // METHOD_NEITHER
        //
        // Retrieves the device's HID descriptor.
        //
        _Analysis_assume_(deviceContext->HidDescriptor.bLength != 0);
        status = RequestCopyFromBuffer(Request,
                            &deviceContext->HidDescriptor,
                            deviceContext->HidDescriptor.bLength);
        break;

    case IOCTL_HID_GET_DEVICE_ATTRIBUTES:   // METHOD_NEITHER
        //
        //Retrieves a device's attributes in a HID_DEVICE_ATTRIBUTES structure.
        //
        status = RequestCopyFromBuffer(Request,
                            &queueContext->DeviceContext->HidDeviceAttributes,
                            sizeof(HID_DEVICE_ATTRIBUTES));
        break;

    case IOCTL_HID_GET_REPORT_DESCRIPTOR:   // METHOD_NEITHER
        //
        //Obtains the report descriptor for the HID device.
        //
        status = RequestCopyFromBuffer(Request,
                            deviceContext->ReportDescriptor,
                            deviceContext->HidDescriptor.DescriptorList[0].wReportLength);
        break;

    case IOCTL_HID_READ_REPORT:             // METHOD_NEITHER
        //
        // Returns a report from the device into a class driver-supplied
        // buffer.
        //
        status = ReadReport(queueContext, Request, &completeRequest);
        break;

    case IOCTL_HID_WRITE_REPORT:            // METHOD_NEITHER
        //
        // Transmits a class driver-supplied report to the device.
        //
        status = WriteReport(queueContext, Request);
        break;

#ifdef _KERNEL_MODE

    case IOCTL_HID_GET_FEATURE:             // METHOD_OUT_DIRECT

        status = GetFeature(queueContext, Request);
        break;

    case IOCTL_HID_SET_FEATURE:             // METHOD_IN_DIRECT

        status = SetFeature(queueContext, Request);
        break;

    case IOCTL_HID_GET_INPUT_REPORT:        // METHOD_OUT_DIRECT

        status = GetInputReport(queueContext, Request);
        break;

    case IOCTL_HID_SET_OUTPUT_REPORT:       // METHOD_IN_DIRECT

        status = SetOutputReport(queueContext, Request);
        break;

#else // UMDF specific

    //
    // HID minidriver IOCTL uses HID_XFER_PACKET which contains an embedded pointer.
    //
    //   typedef struct _HID_XFER_PACKET {
    //     PUCHAR reportBuffer;
    //     ULONG  reportBufferLen;
    //     UCHAR  reportId;
    //   } HID_XFER_PACKET, *PHID_XFER_PACKET;
    //
    // UMDF cannot handle embedded pointers when marshalling buffers between processes.
    // Therefore a special driver mshidumdf.sys is introduced to convert such IRPs to
    // new IRPs (with new IOCTL name like IOCTL_UMDF_HID_Xxxx) where:
    //
    //   reportBuffer - passed as one buffer inside the IRP
    //   reportId     - passed as a second buffer inside the IRP
    //
    // The new IRP is then passed to UMDF host and driver for further processing.
    //

    case IOCTL_UMDF_HID_GET_FEATURE:        // METHOD_NEITHER

        status = GetFeature(queueContext, Request);
        break;

    case IOCTL_UMDF_HID_SET_FEATURE:        // METHOD_NEITHER

        status = SetFeature(queueContext, Request);
        break;

    case IOCTL_UMDF_HID_GET_INPUT_REPORT:  // METHOD_NEITHER

        status = GetInputReport(queueContext, Request);
        break;

    case IOCTL_UMDF_HID_SET_OUTPUT_REPORT: // METHOD_NEITHER

        status = SetOutputReport(queueContext, Request);
        break;

#endif // _KERNEL_MODE

    case IOCTL_HID_GET_STRING:                      // METHOD_NEITHER

        status = GetString(Request);
        break;

    case IOCTL_HID_GET_INDEXED_STRING:              // METHOD_OUT_DIRECT

        status = GetIndexedString(Request);
        break;

    case IOCTL_HID_SEND_IDLE_NOTIFICATION_REQUEST:  // METHOD_NEITHER
        //
        // This has the USBSS Idle notification callback. If the lower driver
        // can handle it (e.g. USB stack can handle it) then pass it down
        // otherwise complete it here as not inplemented. For a virtual
        // device, idling is not needed.
        //
        // Not implemented. fall through...
        //
    case IOCTL_HID_ACTIVATE_DEVICE:                 // METHOD_NEITHER
    case IOCTL_HID_DEACTIVATE_DEVICE:               // METHOD_NEITHER
    case IOCTL_GET_PHYSICAL_DESCRIPTOR:             // METHOD_OUT_DIRECT
        //
        // We don't do anything for these IOCTLs but some minidrivers might.
        //
        // Not implemented. fall through...
        //
    default:
        status = STATUS_NOT_IMPLEMENTED;
        break;
    }

    //
    // Complete the request. Information value has already been set by request
    // handlers.
    //
    if (completeRequest) {
        WdfRequestComplete(Request, status);
    }
}

NTSTATUS
RequestCopyFromBuffer(
    _In_  WDFREQUEST        Request,
    _In_  PVOID             SourceBuffer,
    _When_(NumBytesToCopyFrom == 0, __drv_reportError(NumBytesToCopyFrom cannot be zero))
    _In_  size_t            NumBytesToCopyFrom
    )
/*++

Routine Description:

    A helper function to copy specified bytes to the request's output memory

Arguments:

    Request - A handle to a framework request object.

    SourceBuffer - The buffer to copy data from.

    NumBytesToCopyFrom - The length, in bytes, of data to be copied.

Return Value:

    NTSTATUS

--*/
{
    NTSTATUS                status;
    WDFMEMORY               memory;
    size_t                  outputBufferLength;

    status = WdfRequestRetrieveOutputMemory(Request, &memory);
    if( !NT_SUCCESS(status) ) {
        KdPrint(("WdfRequestRetrieveOutputMemory failed 0x%x\n",status));
        return status;
    }

    WdfMemoryGetBuffer(memory, &outputBufferLength);
    if (outputBufferLength < NumBytesToCopyFrom) {
        status = STATUS_INVALID_BUFFER_SIZE;
        KdPrint(("RequestCopyFromBuffer: buffer too small. Size %d, expect %d\n",
                (int)outputBufferLength, (int)NumBytesToCopyFrom));
        return status;
    }

    status = WdfMemoryCopyFromBuffer(memory,
                                    0,
                                    SourceBuffer,
                                    NumBytesToCopyFrom);
    if( !NT_SUCCESS(status) ) {
        KdPrint(("WdfMemoryCopyFromBuffer failed 0x%x\n",status));
        return status;
    }

    WdfRequestSetInformation(Request, NumBytesToCopyFrom);
    return status;
}

NTSTATUS
ReadReport(
    _In_  PQUEUE_CONTEXT    QueueContext,
    _In_  WDFREQUEST        Request,
    _Always_(_Out_)
          BOOLEAN*          CompleteRequest
    )
/*++

Routine Description:

    Handles IOCTL_HID_READ_REPORT for the HID collection. Normally the request
    will be forwarded to a manual queue for further process. In that case, the
    caller should not try to complete the request at this time, as the request
    will later be retrieved back from the manually queue and completed there.
    However, if for some reason the forwarding fails, the caller still need
    to complete the request with proper error code immediately.

Arguments:

    QueueContext - The object context associated with the queue

    Request - Pointer to  Request Packet.

    CompleteRequest - A boolean output value, indicating whether the caller
            should complete the request or not

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;

    KdPrint(("ReadReport\n"));

    //
    // forward the request to manual queue
    //
    status = WdfRequestForwardToIoQueue(
                            Request,
                            QueueContext->DeviceContext->ManualQueue);
    if( !NT_SUCCESS(status) ) {
        KdPrint(("WdfRequestForwardToIoQueue failed with 0x%x\n", status));
        *CompleteRequest = TRUE;
    }
    else {
        *CompleteRequest = FALSE;
    }

    return status;
}

NTSTATUS
WriteReport(
    _In_  PQUEUE_CONTEXT    QueueContext,
    _In_  WDFREQUEST        Request
    )
/*++

Routine Description:

    Handles IOCTL_HID_WRITE_REPORT all the collection.

Arguments:

    QueueContext - The object context associated with the queue

    Request - Pointer to  Request Packet.

Return Value:

    NT status code.

--*/

{
    NTSTATUS                status;
    HID_XFER_PACKET         packet;
    ULONG                   reportSize;
    PHIDMINI_OUTPUT_REPORT  outputReport;

    KdPrint(("WriteReport\n"));

    status = RequestGetHidXferPacket_ToWriteToDevice(
                            Request,
                            &packet);
    if( !NT_SUCCESS(status) ) {
        return status;
    }

    if (packet.reportId != CONTROL_COLLECTION_REPORT_ID) {
        //
        // Return error for unknown collection
        //
        status = STATUS_INVALID_PARAMETER;
        KdPrint(("WriteReport: unkown report id %d\n", packet.reportId));
        return status;
    }

    //
    // before touching buffer make sure buffer is big enough.
    //
    reportSize = sizeof(HIDMINI_OUTPUT_REPORT);

    if (packet.reportBufferLen < reportSize) {
        status = STATUS_INVALID_BUFFER_SIZE;
        KdPrint(("WriteReport: invalid input buffer. size %d, expect %d\n",
                            packet.reportBufferLen, reportSize));
        return status;
    }

    outputReport = (PHIDMINI_OUTPUT_REPORT)packet.reportBuffer;

    //
    // Store the device data in device extension.
    //
    QueueContext->DeviceContext->DeviceData = outputReport->Data;

    //
    // set status and information
    //
    WdfRequestSetInformation(Request, reportSize);
    return status;
}


HRESULT
GetFeature(PQUEUE_CONTEXT QueueContext, WDFREQUEST Request)
{
    HID_XFER_PACKET packet;
    NTSTATUS status = RequestGetHidXferPacket_ToReadFromDevice(Request, &packet);
    if (!NT_SUCCESS(status)) return status;
    if (packet.reportId != 1 || packet.reportBufferLen < 32) return STATUS_INVALID_PARAMETER;
    UCHAR bytes[32] = {1};
    PTT_STATUS output = {0};
    PDEVICE_CONTEXT ctx = QueueContext->DeviceContext;
    WdfWaitLockAcquire(ctx->Lock, NULL);
    output.version = PTT_PROTOCOL_VERSION;
    output.held_key = ctx->State.held_key;
    output.last_submit_status = ctx->State.last_submit_status;
    output.release_failures = ctx->State.release_failures;
    output.expiry_count = ctx->State.expiry_count;
    output.online = !ctx->Stopping;
    memcpy(bytes + 1, &output, sizeof(output));
    WdfWaitLockRelease(ctx->Lock);
    memcpy(packet.reportBuffer, bytes, sizeof(bytes));
    WdfRequestSetInformation(Request, sizeof(bytes));
    return STATUS_SUCCESS;
}

NTSTATUS
SetFeature(PQUEUE_CONTEXT QueueContext, WDFREQUEST Request)
{
    HID_XFER_PACKET packet;
    NTSTATUS status = RequestGetHidXferPacket_ToWriteToDevice(Request, &packet);
    if (!NT_SUCCESS(status)) return status;
    if (packet.reportId != 1 || packet.reportBufferLen != 32) return STATUS_INVALID_PARAMETER;
    HIDMINI_CONTROL_INFO info;
    memcpy(&info, packet.reportBuffer, sizeof(info));
    if (info.ReportId != 1 || info.u.Dummy.Dummy2 != 0) return STATUS_INVALID_PARAMETER;
    PTT_COMMAND command = {PTT_PROTOCOL_VERSION, info.ControlCode, info.u.Dummy.Dummy1, 0};
    PDEVICE_CONTEXT ctx = QueueContext->DeviceContext;
    WdfWaitLockAcquire(ctx->Lock, NULL);
    PTT_RESULT result = PttApply(&ctx->State, &command, GetTickCount64(), PttSubmit, ctx);
    if (!ctx->Ticking && (ctx->State.held_key || ctx->ReportCount)) {
        ctx->Ticking = TRUE;
        WdfTimerStart(ctx->LeaseTimer, WDF_REL_TIMEOUT_IN_MS(10));
    }
    WdfWaitLockRelease(ctx->Lock);
    switch (result) {
    case PttOk: WdfRequestSetInformation(Request, packet.reportBufferLen); return STATUS_SUCCESS;
    case PttInvalid: return STATUS_INVALID_PARAMETER;
    case PttConflict: return STATUS_DEVICE_BUSY;
    case PttNotHeld: return STATUS_INVALID_DEVICE_STATE;
    default: return STATUS_DEVICE_NOT_READY;
    }
}

NTSTATUS
GetInputReport(
    _In_  PQUEUE_CONTEXT    QueueContext,
    _In_  WDFREQUEST        Request
    )
/*++

Routine Description:

    Handles IOCTL_HID_GET_INPUT_REPORT for all the collection.

Arguments:

    QueueContext - The object context associated with the queue

    Request - Pointer to Request Packet.

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;
    HID_XFER_PACKET         packet;
    ULONG                   reportSize;
    PHIDMINI_INPUT_REPORT   reportBuffer;

    KdPrint(("GetInputReport\n"));

    status = RequestGetHidXferPacket_ToReadFromDevice(
                            Request,
                            &packet);
    if( !NT_SUCCESS(status) ) {
        return status;
    }

    if (packet.reportId != CONTROL_COLLECTION_REPORT_ID) {
        //
        // If collection ID is not for control collection then handle
        // this request just as you would for a regular collection.
        //
        status = STATUS_INVALID_PARAMETER;
        KdPrint(("GetInputReport: invalid report id %d\n", packet.reportId));
        return status;
    }

    reportSize = sizeof(HIDMINI_INPUT_REPORT);
    if (packet.reportBufferLen < reportSize) {
        status = STATUS_INVALID_BUFFER_SIZE;
        KdPrint(("GetInputReport: output buffer too small. Size %d, expect %d\n",
                            packet.reportBufferLen, reportSize));
        return status;
    }

    reportBuffer = (PHIDMINI_INPUT_REPORT)(packet.reportBuffer);

    reportBuffer->ReportId = CONTROL_COLLECTION_REPORT_ID;
    reportBuffer->Data     = QueueContext->OutputReport;

    //
    // Report how many bytes were copied
    //
    WdfRequestSetInformation(Request, reportSize);
    return status;
}


NTSTATUS
SetOutputReport(
    _In_  PQUEUE_CONTEXT    QueueContext,
    _In_  WDFREQUEST        Request
    )
/*++

Routine Description:

    Handles IOCTL_HID_SET_OUTPUT_REPORT for all the collection.

Arguments:

    QueueContext - The object context associated with the queue

    Request - Pointer to Request Packet.

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;
    HID_XFER_PACKET         packet;
    ULONG                   reportSize;
    PHIDMINI_OUTPUT_REPORT  reportBuffer;

    KdPrint(("SetOutputReport\n"));

    status = RequestGetHidXferPacket_ToWriteToDevice(
                            Request,
                            &packet);
    if( !NT_SUCCESS(status) ) {
        return status;
    }

    if (packet.reportId != CONTROL_COLLECTION_REPORT_ID) {
        //
        // If collection ID is not for control collection then handle
        // this request just as you would for a regular collection.
        //
        status = STATUS_INVALID_PARAMETER;
        KdPrint(("SetOutputReport: unkown report id %d\n", packet.reportId));
        return status;
    }

    //
    // before touching buffer make sure buffer is big enough.
    //
    reportSize = sizeof(HIDMINI_OUTPUT_REPORT);

    if (packet.reportBufferLen < reportSize) {
        status = STATUS_INVALID_BUFFER_SIZE;
        KdPrint(("SetOutputReport: invalid input buffer. size %d, expect %d\n",
                            packet.reportBufferLen, reportSize));
        return status;
    }

    reportBuffer = (PHIDMINI_OUTPUT_REPORT)packet.reportBuffer;

    QueueContext->OutputReport = reportBuffer->Data;

    //
    // Report how many bytes were copied
    //
    WdfRequestSetInformation(Request, reportSize);
    return status;
}


NTSTATUS
GetStringId(
    _In_  WDFREQUEST        Request,
    _Out_ ULONG            *StringId,
    _Out_ ULONG            *LanguageId
    )
/*++

Routine Description:

    Helper routine to decode IOCTL_HID_GET_INDEXED_STRING and IOCTL_HID_GET_STRING.

Arguments:

    Request - Pointer to Request Packet.

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;
    ULONG                   inputValue;

#ifdef _KERNEL_MODE

    WDF_REQUEST_PARAMETERS  requestParameters;

    //
    // IOCTL_HID_GET_STRING:                      // METHOD_NEITHER
    // IOCTL_HID_GET_INDEXED_STRING:              // METHOD_OUT_DIRECT
    //
    // The string id (or string index) is passed in Parameters.DeviceIoControl.
    // Type3InputBuffer. However, Parameters.DeviceIoControl.InputBufferLength
    // was not initialized by hidclass.sys, therefore trying to access the
    // buffer with WdfRequestRetrieveInputMemory will fail
    //
    // Another problem with IOCTL_HID_GET_INDEXED_STRING is that METHOD_OUT_DIRECT
    // expects the input buffer to be Irp->AssociatedIrp.SystemBuffer instead of
    // Type3InputBuffer. That will also fail WdfRequestRetrieveInputMemory.
    //
    // The solution to the above two problems is to get Type3InputBuffer directly
    //
    // Also note that instead of the buffer's content, it is the buffer address
    // that was used to store the string id (or index)
    //

    WDF_REQUEST_PARAMETERS_INIT(&requestParameters);
    WdfRequestGetParameters(Request, &requestParameters);

    inputValue = PtrToUlong(
        requestParameters.Parameters.DeviceIoControl.Type3InputBuffer);

    status = STATUS_SUCCESS;

#else

    WDFMEMORY               inputMemory;
    size_t                  inputBufferLength;
    PVOID                   inputBuffer;

    //
    // mshidumdf.sys updates the IRP and passes the string id (or index) through
    // the input buffer correctly based on the IOCTL buffer type
    //

    status = WdfRequestRetrieveInputMemory(Request, &inputMemory);
    if( !NT_SUCCESS(status) ) {
        KdPrint(("WdfRequestRetrieveInputMemory failed 0x%x\n",status));
        return status;
    }
    inputBuffer = WdfMemoryGetBuffer(inputMemory, &inputBufferLength);

    //
    // make sure buffer is big enough.
    //
    if (inputBufferLength < sizeof(ULONG))
    {
        status = STATUS_INVALID_BUFFER_SIZE;
        KdPrint(("GetStringId: invalid input buffer. size %d, expect %d\n",
                            (int)inputBufferLength, (int)sizeof(ULONG)));
        return status;
    }

    inputValue = (*(PULONG)inputBuffer);

#endif

    //
    // The least significant two bytes of the INT value contain the string id.
    //
    *StringId = (inputValue & 0x0ffff);

    //
    // The most significant two bytes of the INT value contain the language
    // ID (for example, a value of 1033 indicates English).
    //
    *LanguageId = (inputValue >> 16);

    return status;
}


NTSTATUS
GetIndexedString(
    _In_  WDFREQUEST        Request
    )
/*++

Routine Description:

    Handles IOCTL_HID_GET_INDEXED_STRING

Arguments:

    Request - Pointer to Request Packet.

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;
    ULONG                   languageId, stringIndex;

    status = GetStringId(Request, &stringIndex, &languageId);

    // While we don't use the language id, some minidrivers might.
    //
    UNREFERENCED_PARAMETER(languageId);

    if (NT_SUCCESS(status)) {

        if (stringIndex != VHIDMINI_DEVICE_STRING_INDEX)
        {
            status = STATUS_INVALID_PARAMETER;
            KdPrint(("GetString: unkown string index %d\n", stringIndex));
            return status;
        }

        status = RequestCopyFromBuffer(Request, VHIDMINI_DEVICE_STRING, sizeof(VHIDMINI_DEVICE_STRING));
    }
    return status;
}


NTSTATUS
GetString(
    _In_  WDFREQUEST        Request
    )
/*++

Routine Description:

    Handles IOCTL_HID_GET_STRING.

Arguments:

    Request - Pointer to Request Packet.

Return Value:

    NT status code.

--*/
{
    NTSTATUS                status;
    ULONG                   languageId, stringId;
    size_t                  stringSizeCb;
    PWSTR                   string;

    status = GetStringId(Request, &stringId, &languageId);

    // While we don't use the language id, some minidrivers might.
    //
    UNREFERENCED_PARAMETER(languageId);

    if (!NT_SUCCESS(status)) {
        return status;
    }

    switch (stringId){
    case HID_STRING_ID_IMANUFACTURER:
        stringSizeCb = sizeof(VHIDMINI_MANUFACTURER_STRING);
        string = VHIDMINI_MANUFACTURER_STRING;
        break;
    case HID_STRING_ID_IPRODUCT:
        stringSizeCb = sizeof(VHIDMINI_PRODUCT_STRING);
        string = VHIDMINI_PRODUCT_STRING;
        break;
    case HID_STRING_ID_ISERIALNUMBER:
        stringSizeCb = sizeof(VHIDMINI_SERIAL_NUMBER_STRING);
        string = VHIDMINI_SERIAL_NUMBER_STRING;
        break;
    default:
        status = STATUS_INVALID_PARAMETER;
        KdPrint(("GetString: unkown string id %d\n", stringId));
        return status;
    }

    status = RequestCopyFromBuffer(Request, string, stringSizeCb);
    return status;
}


NTSTATUS
ManualQueueCreate(WDFDEVICE Device, WDFQUEUE *Queue)
{
    WDF_IO_QUEUE_CONFIG config;
    WDF_IO_QUEUE_CONFIG_INIT(&config, WdfIoQueueDispatchManual);
    config.PowerManaged = WdfFalse;
    return WdfIoQueueCreate(Device, &config, WDF_NO_OBJECT_ATTRIBUTES, Queue);
}

void
EvtTimerFunc(WDFTIMER Timer)
{
    PDEVICE_CONTEXT ctx = GetDeviceContext(WdfTimerGetParentObject(Timer));
    WdfWaitLockAcquire(ctx->Lock, NULL);
    if (!ctx->Stopping) {
        (void)PttExpire(&ctx->State, GetTickCount64(), PttSubmit, ctx);
        while (ctx->ReportCount) {
            WDFREQUEST request;
            NTSTATUS status = WdfIoQueueRetrieveNextRequest(ctx->ManualQueue, &request);
            if (!NT_SUCCESS(status)) break;
            PTT_KEYBOARD_REPORT *report = &ctx->Reports[ctx->ReportHead];
            status = RequestCopyFromBuffer(request, report, sizeof(*report));
            if (NT_SUCCESS(status)) {
                ctx->ReportHead = (ctx->ReportHead + 1) % 16;
                ctx->ReportCount--;
            }
            WdfRequestComplete(request, status);
            if (!NT_SUCCESS(status)) break;
        }
    }
    ctx->Ticking = !ctx->Stopping && (ctx->State.held_key || ctx->ReportCount);
    if (ctx->Ticking) WdfTimerStart(Timer, WDF_REL_TIMEOUT_IN_MS(10));
    WdfWaitLockRelease(ctx->Lock);
}

NTSTATUS
CheckRegistryForDescriptor(
        WDFDEVICE Device
        )
/*++

Routine Description:

    Read "ReadFromRegistry" key value from device parameters in the registry.

Arguments:

    device - pointer to a device object.

Return Value:

    NT status code.

--*/

{
    WDFKEY          hKey = NULL;
    NTSTATUS        status;
    UNICODE_STRING  valueName;
    ULONG           value;

    status = WdfDeviceOpenRegistryKey(Device,
                                  PLUGPLAY_REGKEY_DEVICE,
                                  KEY_READ,
                                  WDF_NO_OBJECT_ATTRIBUTES,
                                  &hKey);
    if (NT_SUCCESS(status)) {

        RtlInitUnicodeString(&valueName, L"ReadFromRegistry");

        status = WdfRegistryQueryULong (hKey,
                                  &valueName,
                                  &value);

        if (NT_SUCCESS (status)) {
            if (value == 0) {
                status = STATUS_UNSUCCESSFUL;
            }
        }

        WdfRegistryClose(hKey);
    }

    return status;
}

NTSTATUS
ReadDescriptorFromRegistry(
        WDFDEVICE Device
        )
/*++

Routine Description:

    Read HID report descriptor from registry

Arguments:

    device - pointer to a device object.

Return Value:

    NT status code.

--*/
{
    WDFKEY          hKey = NULL;
    NTSTATUS        status;
    UNICODE_STRING  valueName;
    WDFMEMORY       memory;
    size_t          bufferSize;
    PVOID           reportDescriptor;
    PDEVICE_CONTEXT deviceContext;
    WDF_OBJECT_ATTRIBUTES   attributes;

    deviceContext = GetDeviceContext(Device);

    status = WdfDeviceOpenRegistryKey(Device,
                                  PLUGPLAY_REGKEY_DEVICE,
                                  KEY_READ,
                                  WDF_NO_OBJECT_ATTRIBUTES,
                                  &hKey);

    if (NT_SUCCESS(status)) {

        RtlInitUnicodeString(&valueName, L"MyReportDescriptor");

        WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
        attributes.ParentObject = Device;

        status = WdfRegistryQueryMemory (hKey,
                                  &valueName,
                                  NonPagedPool,
                                  &attributes,
                                  &memory,
                                  NULL);

        if (NT_SUCCESS (status)) {

            reportDescriptor = WdfMemoryGetBuffer(memory, &bufferSize);

            KdPrint(("No. of report descriptor bytes copied: %d\n", (INT) bufferSize));

            //
            // Store the registry report descriptor in the device extension
            //
            deviceContext->ReadReportDescFromRegistry = TRUE;
            deviceContext->ReportDescriptor = reportDescriptor;
            deviceContext->HidDescriptor.DescriptorList[0].wReportLength = (USHORT)bufferSize;
        }

        WdfRegistryClose(hKey);
    }

    return status;
}


// Caller holds Lock. Release discards undelivered presses and always has room.
static int32_t PttSubmit(void *context, const PTT_KEYBOARD_REPORT *report)
{
    PDEVICE_CONTEXT ctx = context;
    if (ctx->Stopping) return (int32_t)STATUS_DEVICE_NOT_READY;
    if (report->modifiers == 0) ctx->ReportHead = ctx->ReportCount = 0;
    if (ctx->ReportCount == 16) return (int32_t)STATUS_BUFFER_OVERFLOW;
    unsigned index = (ctx->ReportHead + ctx->ReportCount) % 16;
    ctx->Reports[index] = *report;
    ctx->Reports[index].report_id = 2;
    ctx->ReportCount++;
    return 0;
}
void PttCleanup(WDFOBJECT Object)
{
    PDEVICE_CONTEXT ctx = GetDeviceContext(Object);
    if (ctx->Lock) {
        WdfWaitLockAcquire(ctx->Lock, NULL);
        ctx->Stopping = TRUE;
        WdfWaitLockRelease(ctx->Lock);
    }
    if (ctx->LeaseTimer) WdfTimerStop(ctx->LeaseTimer, TRUE);
}
