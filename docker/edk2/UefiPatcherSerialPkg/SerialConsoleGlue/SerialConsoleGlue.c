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
} VT_UTF8_DEVICE_PATH;

STATIC VT_UTF8_DEVICE_PATH mVtUtf8Path = {
  {
    { MESSAGING_DEVICE_PATH, MSG_VENDOR_DP, { sizeof (VENDOR_DEVICE_PATH), 0 } },
    EFI_VT_UTF8_GUID
  },
  { END_DEVICE_PATH_TYPE, END_ENTIRE_DEVICE_PATH_SUBTYPE, { sizeof (EFI_DEVICE_PATH_PROTOCOL), 0 } }
};

STATIC EFI_EVENT mReadyToBootEvent = NULL;

STATIC
BOOLEAN
HasVtUtf8Node (
  IN CONST EFI_DEVICE_PATH_PROTOCOL  *DevicePath
  )
{
  CONST EFI_DEVICE_PATH_PROTOCOL *Node;

  if (DevicePath == NULL) {
    return FALSE;
  }
  for (Node = DevicePath; !IsDevicePathEnd (Node); Node = NextDevicePathNode (Node)) {
    if ((DevicePathType (Node) == MESSAGING_DEVICE_PATH) &&
        (DevicePathSubType (Node) == MSG_VENDOR_DP) &&
        CompareGuid (&((VENDOR_DEVICE_PATH *)Node)->Guid, &gEfiVTUTF8Guid)) {
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
    if (!EFI_ERROR (Status) && HasVtUtf8Node (DevicePath)) {
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

EFI_STATUS
EFIAPI
SerialConsoleGlueEntry (
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  EFI_STATUS                    Status;
  EFI_STATUS                    ConOutStatus;
  EFI_HANDLE                    *Handles;
  EFI_HANDLE                    SerialHandle;
  EFI_HANDLE                    Child;
  EFI_DEVICE_PATH_PROTOCOL      *Path;
  EFI_DEVICE_PATH_PROTOCOL      *ConsolePath;
  EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL  *TextOut;
  UINTN                         Count;
  UINTN                         Index;

  Handles = NULL;
  Status = gBS->LocateHandleBuffer (
                  ByProtocol,
                  &gEfiSerialIoProtocolGuid,
                  NULL,
                  &Count,
                  &Handles
                  );
  if (EFI_ERROR (Status) || (Handles == NULL) || (Count == 0)) {
    return EFI_NOT_FOUND;
  }

  SerialHandle = Handles[0];
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
    return EFI_NOT_FOUND;
  }
  ConsolePath = DuplicateDevicePath (Path);
  if (ConsolePath == NULL) {
    return EFI_OUT_OF_RESOURCES;
  }
  if (Child == NULL) {
    Path = AppendDevicePathNode (ConsolePath, (EFI_DEVICE_PATH_PROTOCOL *)&mVtUtf8Path);
    FreePool (ConsolePath);
    if (Path == NULL) {
      return EFI_OUT_OF_RESOURCES;
    }
    ConsolePath = Path;
  }

  ConOutStatus = AppendInstanceToVariable (L"ConOut", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ConIn", ConsolePath);
  (VOID)AppendInstanceToVariable (L"ErrOut", ConsolePath);
  FreePool (ConsolePath);

  Child = FindTerminalChild ();
  if ((Child != NULL) && (ConOutStatus == EFI_SUCCESS)) {
    TextOut = NULL;
    if (!EFI_ERROR (gBS->HandleProtocol (Child, &gEfiSimpleTextOutProtocolGuid, (VOID **)&TextOut)) &&
        (TextOut != NULL)) {
      (VOID)TextOut->OutputString (TextOut, GLUE_MARKER_BOOT);
    }
  }

  (VOID)gBS->CreateEventEx (
                  EVT_NOTIFY_SIGNAL,
                  TPL_CALLBACK,
                  OnReadyToBoot,
                  NULL,
                  &gEfiEventReadyToBootGuid,
                  &mReadyToBootEvent
                  );
  return EFI_SUCCESS;
}
