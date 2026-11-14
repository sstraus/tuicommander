; NSIS installer hooks (bundle.windows.nsis.installerHooks in tauri.windows.conf.json).
;
; A dictation build with the vulkan feature imports vulkan-1.dll, and Windows
; does not start a process whose import is missing (0xC0000135). GPU drivers
; install the Khronos loader in System32; a machine without one (no GPU driver,
; some VMs) has none. Install the loader next to the app only on such a machine:
; with the loader and no Vulkan device whisper.cpp runs on the CPU. When System32
; has its own loader, drop a copy left by an earlier install so the system one
; (kept current by the driver) is used.
;
; scripts/fetch-vulkan-loader.py downloads the pinned loader to windows/vulkan/.
; A release sets TUIC_REQUIRE_VULKAN_LOADER=1 so a missing download fails the build.

!define TUIC_VULKAN_LOADER "${__FILEDIR__}\vulkan\vulkan-1.dll"
!define TUIC_VULKAN_LICENSE "${__FILEDIR__}\vulkan\VulkanRT-License.txt"

Var TuicSystemVulkanLoader

!macro NSIS_HOOK_POSTINSTALL
  !if /FileExists "${TUIC_VULKAN_LOADER}"
    ; The installer is 32-bit: without this, System32 reads SysWOW64.
    ${DisableX64FSRedirection}
    ${If} ${FileExists} "$WINDIR\System32\vulkan-1.dll"
      StrCpy $TuicSystemVulkanLoader 1
    ${Else}
      StrCpy $TuicSystemVulkanLoader 0
    ${EndIf}
    ${EnableX64FSRedirection}
    ${If} $TuicSystemVulkanLoader == 1
      Delete "$INSTDIR\vulkan-1.dll"
      Delete "$INSTDIR\VulkanRT-License.txt"
    ${Else}
      SetOutPath "$INSTDIR"
      File "/oname=vulkan-1.dll" "${TUIC_VULKAN_LOADER}"
      ; Apache-2.0 and MIT: the notices travel with the binary.
      File "/oname=VulkanRT-License.txt" "${TUIC_VULKAN_LICENSE}"
    ${EndIf}
  !else if "$%TUIC_REQUIRE_VULKAN_LOADER%" == "1"
    !error "Missing ${TUIC_VULKAN_LOADER}: run python scripts/fetch-vulkan-loader.py"
  !endif
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\vulkan-1.dll"
  Delete "$INSTDIR\VulkanRT-License.txt"
!macroend
