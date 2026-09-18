// Спека docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md 5:
// EPA-лестница, one-shot BifEpaStage против ресет-лупа, post-reset отчёт.
#include <Uefi.h>
#include <Library/BaseLib.h>
#include <Library/IoLib.h>
#include <Library/PrintLib.h>
#include <Library/SerialPortLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/UefiDriverEntryPoint.h>
#include <Library/UefiRuntimeServicesTableLib.h>
#include "EpaConfig.h"

#define EPA_VAR_NAME  L"BifEpaStage"

#define EPA_STAGE_IDLE     0
#define EPA_STAGE_WRITTEN  1

#define EPA_ACCESS_MMIO32  0
#define EPA_ACCESS_PCI_CF8 1

STATIC EFI_GUID  mEpaVarGuid = { 0x6F3B2C9D, 0x4A1E, 0x4D8F, { 0xB5, 0xC7, 0x2E, 0x9A, 0x0D, 0x8F, 0x1C, 0x34 } };

STATIC
VOID
ProbPrint (
  IN CONST CHAR8  *Fmt,
  ...
  )
{
  CHAR8    Buf[192];
  VA_LIST  Args;
  UINTN    Len;

  VA_START (Args, Fmt);
  AsciiVSPrint (Buf, sizeof (Buf), Fmt, Args);
  VA_END (Args);
  Len = AsciiStrLen (Buf);
  if (Len > 0) {
    SerialPortWrite ((UINT8 *)Buf, Len);
  }
}

STATIC
UINT32
EpaRead32 (
  IN CONST MMR_ENTRY  *Entry
  )
{
  if (Entry->Kind == EPA_ACCESS_MMIO32) {
    return MmioRead32 ((UINTN)Entry->Addr);
  }
  IoWrite32 (0xCF8, 0x80000000u | (UINT32)Entry->Addr);
  return IoRead32 (0xCFC);
}

STATIC
VOID
EpaWrite32 (
  IN CONST MMR_ENTRY  *Entry,
  IN UINT32            Value
  )
{
  if (Entry->Kind == EPA_ACCESS_MMIO32) {
    MmioWrite32 ((UINTN)Entry->Addr, Value);
    return;
  }
  IoWrite32 (0xCF8, 0x80000000u | (UINT32)Entry->Addr);
  IoWrite32 (0xCFC, Value);
}

STATIC
UINT32
EpaGetStage (
  VOID
  )
{
  EFI_STATUS  Status;
  UINTN       Size;
  UINT32      Stage;

  Size = sizeof (Stage);
  Stage = EPA_STAGE_IDLE;
  Status = gRT->GetVariable (EPA_VAR_NAME, &mEpaVarGuid, NULL, &Size, &Stage);
  if (EFI_ERROR (Status)) {
    ProbPrint ("BIF-EPA: prev stage: none (%r)\n", Status);
    return EPA_STAGE_IDLE;
  }
  return Stage;
}

STATIC
EFI_STATUS
EpaSetStage (
  IN UINT32  Stage
  )
{
  return gRT->SetVariable (
                EPA_VAR_NAME,
                &mEpaVarGuid,
                EFI_VARIABLE_NON_VOLATILE | EFI_VARIABLE_BOOTSERVICE_ACCESS,
                sizeof (Stage),
                &Stage
                );
}

STATIC
VOID
EpaDumpAll (
  IN CONST CHAR8  *Prefix
  )
{
  UINTN  Index;

  for (Index = 0; Index < EPA_ENTRY_COUNT; Index++) {
    ProbPrint ("BIF-EPA: %a [%u] %a raw=0x%08x\n",
               Prefix, (UINT32)Index, mMmrEntries[Index].Tag, EpaRead32 (&mMmrEntries[Index]));
  }
}

EFI_STATUS
EFIAPI
BifEpaProbeEntry (
  IN EFI_HANDLE        ImageHandle,
  IN EFI_SYSTEM_TABLE  *SystemTable
  )
{
  UINT32  Stage;
  UINTN   Index;
  UINT32  Mismatch;
  UINT32  Raw;
  UINT32  New;
  UINT32  Readback;

  SerialPortInitialize ();
  ProbPrint ("BIF-EPA: probe mode=%a entries=%u reset=%a\n",
             EPA_MODE_WRITE ? "write" : "read",
             (UINT32)EPA_ENTRY_COUNT,
             EPA_RESET_COLD ? "cold" : "warm");
  Stage = EpaGetStage ();
  ProbPrint ("BIF-EPA: prev stage=%u\n", Stage);

  if ((EPA_MODE_WRITE == 0) || (EPA_ENTRY_COUNT == 0)) {
    EpaDumpAll ("read");
    EpaSetStage (EPA_STAGE_IDLE);
    return EFI_SUCCESS;
  }

  if (Stage == EPA_STAGE_WRITTEN) {
    ProbPrint ("BIF-EPA: post-reset report (PEI overwrite check)\n");
    for (Index = 0; Index < EPA_ENTRY_COUNT; Index++) {
      Raw = EpaRead32 (&mMmrEntries[Index]);
      if ((Raw & ~mMmrEntries[Index].AndMask) == (mMmrEntries[Index].OrValue & ~mMmrEntries[Index].AndMask)) {
        ProbPrint ("BIF-EPA: [%u] %a raw=0x%08x KEPT\n", (UINT32)Index, mMmrEntries[Index].Tag, Raw);
      } else {
        ProbPrint ("BIF-EPA: [%u] %a raw=0x%08x LOST (PEI reprogrammed?)\n", (UINT32)Index, mMmrEntries[Index].Tag, Raw);
      }
    }
    EpaSetStage (EPA_STAGE_IDLE);
    return EFI_SUCCESS;
  }

  Mismatch = 0;
  for (Index = 0; Index < EPA_ENTRY_COUNT; Index++) {
    Raw = EpaRead32 (&mMmrEntries[Index]);
    New = (Raw & mMmrEntries[Index].AndMask) | mMmrEntries[Index].OrValue;
    EpaWrite32 (&mMmrEntries[Index], New);
    Readback = EpaRead32 (&mMmrEntries[Index]);
    ProbPrint ("BIF-EPA: write [%u] %a raw=0x%08x new=0x%08x rb=0x%08x %a\n",
               (UINT32)Index, mMmrEntries[Index].Tag, Raw, New, Readback,
               Readback == New ? "OK" : "MISMATCH");
    if (Readback != New) {
      Mismatch++;
    }
  }

  if (Mismatch != 0) {
    ProbPrint ("BIF-EPA: %u mismatch - NO reset\n", Mismatch);
    EpaSetStage (EPA_STAGE_IDLE);
    return EFI_SUCCESS;
  }

  if (EFI_ERROR (EpaSetStage (EPA_STAGE_WRITTEN))) {
    ProbPrint ("BIF-EPA: stage marker failed - NO reset\n");
    return EFI_SUCCESS;
  }
  ProbPrint ("BIF-EPA: readback OK - %a reset in %u ms\n",
             EPA_RESET_COLD ? "cold" : "warm", (UINT32)EPA_PAUSE_MS);
  gBS->Stall (EPA_PAUSE_MS * 1000);
  gRT->ResetSystem (EPA_RESET_COLD ? EfiResetCold : EfiResetWarm, EFI_SUCCESS, 0, NULL);
  ProbPrint ("BIF-EPA: ResetSystem returned!\n");
  return EFI_SUCCESS;
}
