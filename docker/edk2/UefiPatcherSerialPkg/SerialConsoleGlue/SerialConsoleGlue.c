#include <Uefi.h>
#include <Guid/GlobalVariable.h>
#include <Guid/PcAnsi.h>
#include <Guid/EventGroup.h>
#include <Library/UefiDriverEntryPoint.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include <Library/DevicePathLib.h>
#include <Library/MemoryAllocationLib.h>
#include <Library/BaseMemoryLib.h>
#include <Library/DebugLib.h>
#include <Protocol/SerialIo.h>
#include <Protocol/SimpleTextOut.h>
#include <Protocol/DevicePath.h>

#define GLUE_MARKER_BOOT   L"SC-S1 glue: serial console attached (ConOut updated)\r\n"
#define GLUE_MARKER_READY  L"SC-S1 glue: ReadyToBoot\r\n"

typedef struct {
  VENDOR_DEVICE_PATH       Vendor;
  EFI_DEVICE_PATH_PROTOCOL End;
} TERMINAL_DEVICE_PATH;

STATIC EFI_GUID  mSetupVarGuid = { 0xEC87D643, 0xEBA4, 0x4BB5, { 0xA1, 0xE5, 0x3F, 0x3E, 0x36, 0xB2, 0x0D, 0xA9 } };
STATIC EFI_GUID  mSioVarGuid  = { 0x560BF58A, 0x1E0D, 0x4D7E, { 0x95, 0x3F, 0x29, 0x80, 0xA2, 0x61, 0xE0, 0x31 } };

STATIC CONST UINT64 kBaudMap[5] = { 115200, 57600, 38400, 19200, 9600 };

STATIC CONST EFI_GUID *CONST kTerminalGuidMap[4] = {
  &gEfiVTUTF8Guid,
  &gEfiVT100PlusGuid,
  &gEfiVT100Guid,
  &gEfiPcAnsiGuid
};

STATIC TERMINAL_DEVICE_PATH mTerminalPath = {
  {
    { MESSAGING_DEVICE_PATH, MSG_VENDOR_DP, { sizeof (VENDOR_DEVICE_PATH), 0 } },
    { 0, 0, 0, { 0, 0, 0, 0, 0, 0, 0, 0 } }
  },
  { END_DEVICE_PATH_TYPE, END_ENTIRE_DEVICE_PATH_SUBTYPE, { sizeof (EFI_DEVICE_PATH_PROTOCOL), 0 } }
};

STATIC CONST EFI_GUID *mTerminalGuid    = NULL;
STATIC EFI_EVENT       mReadyToBootEvent = NULL;
STATIC EFI_EVENT       mSerialIoNotifyEvent = NULL;
STATIC VOID            *mSerialIoRegistration = NULL;
STATIC BOOLEAN         mAttached = FALSE;
STATIC UINT8           mBaudIndex = 0;

STATIC
VOID
ReadConfigBytes (
  OUT UINT8  *BaudIndex,
  OUT UINT8  *TerminalIndex,
  OUT UINT8  *Enable
  )
{
  EFI_STATUS  Status;
  UINT8       *Setup;
  UINT8       Nv[16];
  UINTN       Size;

  *BaudIndex     = 0;
  *TerminalIndex = 0;
  *Enable        = 1;

  Size = 0;
  Status = gRT->GetVariable (L"Setup", &mSetupVarGuid, NULL, &Size, NULL);
  if (Status == EFI_BUFFER_TOO_SMALL) {
    Setup = AllocatePool (Size);
    if (Setup != NULL) {
      Status = gRT->GetVariable (L"Setup", &mSetupVarGuid, NULL, &Size, Setup);
      if (!EFI_ERROR (Status) && (Size > 0x5E)) {
        *BaudIndex     = Setup[0x5D];
        *TerminalIndex = Setup[0x5E];
      }
      FreePool (Setup);
    }
  }

  Size = sizeof (Nv);
  Status = gRT->GetVariable (L"PNP0501_0_NV", &mSioVarGuid, NULL, &Size, Nv);
  if (!EFI_ERROR (Status) && (Size >= 1)) {
    *Enable = Nv[0];
  }
}

STATIC
BOOLEAN
HasTerminalNode (
  IN CONST EFI_DEVICE_PATH_PROTOCOL  *DevicePath,
  IN CONST EFI_GUID                  *TerminalGuid
  )
{
  CONST EFI_DEVICE_PATH_PROTOCOL *Node;

  if (DevicePath == NULL) {
    return FALSE;
  }
  for (Node = DevicePath; !IsDevicePathEnd (Node); Node = NextDevicePathNode (Node)) {
    if ((DevicePathType (Node) == MESSAGING_DEVICE_PATH) &&
        (DevicePathSubType (Node) == MSG_VENDOR_DP) &&
        CompareGuid (&((VENDOR_DEVICE_PATH *)Node)->Guid, TerminalGuid)) {
      return TRUE;
    }
  }
  return FALSE;
}

STATIC
BOOLEAN
HasAnyTerminalNode (
  IN CONST EFI_DEVICE_PATH_PROTOCOL  *DevicePath
  )
{
  UINTN  Index;

  if (DevicePath == NULL) {
    return FALSE;
  }
  if (HasTerminalNode (DevicePath, mTerminalGuid)) {
    return TRUE;
  }
  for (Index = 0; Index < ARRAY_SIZE (kTerminalGuidMap); Index++) {
    if (HasTerminalNode (DevicePath, kTerminalGuidMap[Index])) {
      return TRUE;
    }
  }
  return FALSE;
}

STATIC
EFI_HANDLE
FindTerminalChild (
  VOID
  )
{
  EFI_STATUS                Status;
  EFI_HANDLE                *Handles;
  EFI_HANDLE                Child;
  EFI_DEVICE_PATH_PROTOCOL  *DevicePath;
  UINTN                     Count;
  UINTN                     Index;

  Handles = NULL;
  Status = gBS->LocateHandleBuffer (
                  ByProtocol,
                  &gEfiSimpleTextOutProtocolGuid,
                  NULL,
                  &Count,
                  &Handles
                  );
  if (EFI_ERROR (Status) || Handles == NULL) {
    return NULL;
  }
  Child = NULL;
  for (Index = 0; Index < Count; Index++) {
    DevicePath = NULL;
    Status = gBS->HandleProtocol (
                    Handles[Index],
                    &gEfiDevicePathProtocolGuid,
                    (VOID **)&DevicePath
                    );
    if (!EFI_ERROR (Status) && HasAnyTerminalNode (DevicePath)) {
      Child = Handles[Index];
      break;
    }
  }
  FreePool (Handles);
  return Child;
}

STATIC
EFI_STATUS
AppendInstanceToVariable (
  IN CONST CHAR16                   *Name,
  IN CONST EFI_DEVICE_PATH_PROTOCOL *DevicePath
  )
{
  EFI_STATUS                Status;
  EFI_DEVICE_PATH_PROTOCOL  *Buffer;
  EFI_DEVICE_PATH_PROTOCOL  *Walk;
  EFI_DEVICE_PATH_PROTOCOL  *OrigWalk;
  EFI_DEVICE_PATH_PROTOCOL  *Instance;
  EFI_DEVICE_PATH_PROTOCOL  *Merged;
  EFI_DEVICE_PATH_PROTOCOL  *Tail;
  UINTN                     Size;
  UINTN                     InstanceSize;
  UINTN                     PathSize;
  BOOLEAN                   Found;

  PathSize = GetDevicePathSize (DevicePath);
  Size = 0;
  Status = gRT->GetVariable ((CHAR16 *)Name, &gEfiGlobalVariableGuid, NULL, &Size, NULL);
  if (Status == EFI_NOT_FOUND) {
    return gRT->SetVariable (
             (CHAR16 *)Name,
             &gEfiGlobalVariableGuid,
             EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS | EFI_VARIABLE_NON_VOLATILE,
             PathSize,
             (VOID *)DevicePath
             );
  }
  if (Status != EFI_BUFFER_TOO_SMALL) {
    return Status;
  }

  Buffer = AllocateZeroPool (Size);
  if (Buffer == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  Status = gRT->GetVariable ((CHAR16 *)Name, &gEfiGlobalVariableGuid, NULL, &Size, Buffer);
  if (EFI_ERROR (Status)) {
    FreePool (Buffer);
    return Status;
  }

  Found = FALSE;
  Walk = DuplicateDevicePath (Buffer);
  if (Walk != NULL) {
    OrigWalk = Walk;
    while (TRUE) {
      Instance = GetNextDevicePathInstance (&Walk, &InstanceSize);
      if (Instance == NULL) {
        break;
      }
      if ((InstanceSize == PathSize) && (CompareMem (Instance, DevicePath, PathSize) == 0)) {
        Found = TRUE;
      }
      FreePool (Instance);
      if (Found) {
        break;
      }
    }
    FreePool (OrigWalk);
  }
  if (Found) {
    FreePool (Buffer);
    return EFI_SUCCESS;
  }

  Merged = AllocateZeroPool (Size + PathSize);
  if (Merged == NULL) {
    FreePool (Buffer);
    return EFI_OUT_OF_RESOURCES;
  }
  CopyMem (Merged, Buffer, Size);
  Tail = (EFI_DEVICE_PATH_PROTOCOL *)((UINT8 *)Merged + Size - sizeof (EFI_DEVICE_PATH_PROTOCOL));
  Tail->Type = END_DEVICE_PATH_TYPE;
  Tail->SubType = END_INSTANCE_DEVICE_PATH_SUBTYPE;
  SetDevicePathNodeLength (Tail, sizeof (EFI_DEVICE_PATH_PROTOCOL));
  CopyMem ((UINT8 *)Merged + Size, DevicePath, PathSize);
  Status = gRT->SetVariable (
           (CHAR16 *)Name,
           &gEfiGlobalVariableGuid,
           EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS | EFI_VARIABLE_NON_VOLATILE,
           Size + PathSize,
           Merged
           );
  FreePool (Merged);
  FreePool (Buffer);
  return Status;
}

STATIC
VOID
EFIAPI
OnReadyToBoot (
  IN EFI_EVENT  Event,
  IN VOID       *Context
  )
{
  EFI_HANDLE                    Child;
  EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL  *TextOut;

  Child = FindTerminalChild ();
  if (Child == NULL) {
    return;
  }
  TextOut = NULL;
  if (!EFI_ERROR (gBS->HandleProtocol (Child, &gEfiSimpleTextOutProtocolGuid, (VOID **)&TextOut)) &&
      (TextOut != NULL)) {
    (VOID)TextOut->OutputString (TextOut, GLUE_MARKER_READY);
  }
}

STATIC
VOID
AttachSerialConsole (
  VOID
  )
{
  EFI_STATUS                        Status;
  EFI_STATUS                        ConOutStatus;
  EFI_HANDLE                        *Handles;
  EFI_HANDLE                        SerialHandle;
  EFI_HANDLE                        Child;
  EFI_DEVICE_PATH_PROTOCOL          *Path;
  EFI_DEVICE_PATH_PROTOCOL          *ConsolePath;
  EFI_SERIAL_IO_PROTOCOL            *SerialIo;
  EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL   *TextOut;
  UINTN                             Count;
  UINTN                             Index;

  if (mAttached) {
    return;
  }

  Handles = NULL;
  Status = gBS->LocateHandleBuffer (
                  ByProtocol,
                  &gEfiSerialIoProtocolGuid,
                  NULL,
                  &Count,
                  &Handles
                  );
  if (EFI_ERROR (Status) || (Handles == NULL) || (Count == 0)) {
    return;
  }

  SerialHandle = Handles[0];
  SerialIo = NULL;
  if (!EFI_ERROR (gBS->HandleProtocol (SerialHandle, &gEfiSerialIoProtocolGuid, (VOID **)&SerialIo)) &&
      (SerialIo != NULL))
  {
    (VOID)SerialIo->SetAttributes (
                      SerialIo,
                      kBaudMap[(mBaudIndex > 4) ? 4 : mBaudIndex],
                      0,
                      0,
                      NoParity,
                      8,
                      OneStopBit
                      );
  }

  Child = NULL;
  for (Index = 0; (Index < Count) && (Child == NULL); Index++) {
    (VOID)gBS->ConnectController (Handles[Index], NULL, NULL, FALSE);
    Child = FindTerminalChild ();
  }
  FreePool (Handles);

  Path = NULL;
  if (Child != NULL) {
    Status = gBS->HandleProtocol (Child, &gEfiDevicePathProtocolGuid, (VOID **)&Path);
  } else {
    Status = gBS->HandleProtocol (SerialHandle, &gEfiDevicePathProtocolGuid, (VOID **)&Path);
  }
  if (EFI_ERROR (Status) || (Path == NULL)) {
    return;
  }
  ConsolePath = DuplicateDevicePath (Path);
  if (ConsolePath == NULL) {
    return;
  }
  if (Child == NULL) {
    Path = AppendDevicePathNode (ConsolePath, (EFI_DEVICE_PATH_PROTOCOL *)&mTerminalPath);
    FreePool (ConsolePath);
    if (Path == NULL) {
      return;
    }
    ConsolePath = Path;
  }

  ConOutStatus = AppendInstanceToVariable (L"ConOut", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ConIn", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ErrOut", ConsolePath);
  FreePool (ConsolePath);

  Child = FindTerminalChild ();
  if ((Child != NULL) && (ConOutStatus == EFI_SUCCESS)) {
    mAttached = TRUE;
    TextOut = NULL;
    if (!EFI_ERROR (gBS->HandleProtocol (Child, &gEfiSimpleTextOutProtocolGuid, (VOID **)&TextOut)) &&
        (TextOut != NULL))
    {
      (VOID)TextOut->OutputString (TextOut, GLUE_MARKER_BOOT);
    }
  }
}

STATIC
VOID
EFIAPI
OnSerialIoInstalled (
  IN EFI_EVENT  Event,
  IN VOID       *Context
  )
{
  AttachSerialConsole ();
}

EFI_STATUS
EFIAPI
SerialConsoleGlueEntry (
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  EFI_STATUS  Status;
  UINT8       BaudIndex;
  UINT8       TerminalIndex;
  UINT8       Enable;

  ReadConfigBytes (&BaudIndex, &TerminalIndex, &Enable);
  if (Enable == 0) {
    return EFI_SUCCESS;
  }
  mBaudIndex = BaudIndex;
  mTerminalGuid = kTerminalGuidMap[(TerminalIndex > 3) ? 3 : TerminalIndex];
  CopyGuid (&mTerminalPath.Vendor.Guid, mTerminalGuid);

  (VOID)gBS->CreateEventEx (
             EVT_NOTIFY_SIGNAL,
             TPL_CALLBACK,
             OnReadyToBoot,
             NULL,
             &gEfiEventReadyToBootGuid,
             &mReadyToBootEvent
             );

  AttachSerialConsole ();

  if (!mAttached) {
    Status = gBS->CreateEvent (
                    EVT_NOTIFY_SIGNAL,
                    TPL_CALLBACK,
                    OnSerialIoInstalled,
                    NULL,
                    &mSerialIoNotifyEvent
                    );
    if (!EFI_ERROR (Status)) {
      (VOID)gBS->RegisterProtocolNotify (
                  &gEfiSerialIoProtocolGuid,
                  mSerialIoNotifyEvent,
                  &mSerialIoRegistration
                  );
    }
  }
  return EFI_SUCCESS;
}
