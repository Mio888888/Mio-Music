; Per-user NSIS installer. It follows the layout of the Tauri installer it replaces: the same
; install folder (%LOCALAPPDATA%\<product>), registry keys and uninstall entry, so a copy installed
; by the old app is updated in place. `/P /UPDATE` (what both updaters pass) runs it without pages
; and starts the app again at the end. `/S` is a fully silent install.
;
; Defines (makensis /D...): PRODUCT_NAME VERSION BUNDLE_ID PUBLISHER STAGE_DIR OUT_FILE ICON_FILE
Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!define EXE_NAME "mio-app.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define APP_KEY "Software\${PUBLISHER}\${PRODUCT_NAME}"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!insertmacro GetParameters
!insertmacro GetOptions

Name "${PRODUCT_NAME}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\${PRODUCT_NAME}"
InstallDirRegKey HKCU "${APP_KEY}" ""
BrandingText "${PRODUCT_NAME} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING

Var UpdateMode

!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipInUpdateMode
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipInUpdateMode
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE_NAME}"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"

Function .onInit
  StrCpy $UpdateMode 0
  ${GetParameters} $R0
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/P" $R1
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
  ClearErrors
FunctionEnd

Function SkipInUpdateMode
  ${If} $UpdateMode = 1
    Abort
  ${EndIf}
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"
  ; The updating app exits right before it starts this installer; give it a moment to let go.
  ${If} $UpdateMode = 1
    Sleep 1500
  ${EndIf}
  File "${STAGE_DIR}\${EXE_NAME}"
  File "${STAGE_DIR}\dav1d.dll"
  File "${STAGE_DIR}\TDesign-LICENSE.txt"
  File "${STAGE_DIR}\BackgroundFilterLicenses.txt"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE_NAME}"
  CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE_NAME}"

  WriteRegStr HKCU "${APP_KEY}" "" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${EXE_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1

  ; Updates restart the app themselves; a normal install offers it on the finish page.
  ${If} $UpdateMode = 1
    Exec '"$INSTDIR\${EXE_NAME}"'
  ${EndIf}
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${EXE_NAME}"
  Delete "$INSTDIR\dav1d.dll"
  Delete "$INSTDIR\TDesign-LICENSE.txt"
  Delete "$INSTDIR\BackgroundFilterLicenses.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "${APP_KEY}"
SectionEnd
