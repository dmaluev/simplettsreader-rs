!include MUI2.nsh
!include shortcut-properties.nsh

!define PRODUCT_VERSION    "2.0.3"

!define PRODUCT_NAME       "Simple TTS Reader"
!define PRODUCT_NAME_SETUP "SimpleTTSReader"
!define PRODUCT_MAIN_EXE   "simplettsreader.exe"
!define PRODUCT_MAIN_DIR   "${PRODUCT_NAME}"
!define PRODUCT_GUID       "{85CBCC28-E397-4fcd-802E-100BE5F064A2}"
!define PRODUCT_PUBLISHER  "Dmitry Maluev"
!define PRODUCT_URL        "https://simplettsreader.sourceforge.io/"

!define RK_SOFT_MS_WIN_CV  "SOFTWARE\Microsoft\Windows\CurrentVersion"
!define RK_APP_PATHS       "${RK_SOFT_MS_WIN_CV}\App Paths\${PRODUCT_MAIN_EXE}"
!define RK_UNINSTALL       "${RK_SOFT_MS_WIN_CV}\Uninstall\${PRODUCT_GUID}"

!define SOURCE_DIR         ".."

BrandingText          "$(^Name)"
InstallDir            "$PROGRAMFILES64\${PRODUCT_MAIN_DIR}"
InstallDirRegKey      HKLM "${RK_APP_PATHS}" ""
Name                  "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile               "${PRODUCT_NAME_SETUP}-${PRODUCT_VERSION}-setup.exe"
RequestExecutionLevel admin
ShowInstDetails       show
ShowUnInstDetails     show
Unicode               true

!define MUI_ABORTWARNING
!define MUI_ICON   "${NSISDIR}\Contrib\Graphics\Icons\orange-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\orange-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Section Main
    SetOutPath $INSTDIR
    SetOverwrite ifnewer

    File "simplettsreader.exe"
    File "${SOURCE_DIR}\Changelog.txt"
    File "${SOURCE_DIR}\License (Slint).html"
    File "${SOURCE_DIR}\License.html"
SectionEnd

Section Uninstall
    SetAutoClose true
    SetShellVarContext all

    Delete "$INSTDIR\simplettsreader.exe"
    Delete "$INSTDIR\Changelog.txt"
    Delete "$INSTDIR\License (Slint).html"
    Delete "$INSTDIR\License.html"

    Delete "$INSTDIR\uninst.exe"
    Delete "$INSTDIR\Uninstall.exe"

    RMDir $INSTDIR

    Delete "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\License (Slint).lnk"
    Delete "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\License.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\Uninstall.lnk"
    RMDir  "$SMPROGRAMS\${PRODUCT_MAIN_DIR}"

    Delete "$DESKTOP\${PRODUCT_NAME}.lnk"

    DeleteRegKey HKLM "${RK_APP_PATHS}"
    DeleteRegKey HKLM "${RK_UNINSTALL}"
    SetRegView 64
    DeleteRegKey HKLM "SOFTWARE\Classes\CLSID\${PRODUCT_GUID}"
    SetRegView 32
SectionEnd

Section -Shortcuts
    SetShellVarContext all

    CreateDirectory "$SMPROGRAMS\${PRODUCT_MAIN_DIR}"
    CreateShortCut  "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_MAIN_EXE}"
    CreateShortCut  "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\License (Slint).lnk" "$INSTDIR\License (Slint).html"
    CreateShortCut  "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\License.lnk"         "$INSTDIR\License.html"

    CreateShortCut  "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\Uninstall.lnk" "$INSTDIR\Uninstall.exe"

    CreateShortCut  "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_MAIN_EXE}"
SectionEnd

Section -RegUninstall
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${RK_APP_PATHS}" ""                "$INSTDIR\${PRODUCT_MAIN_EXE}"
    WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayIcon"     "$INSTDIR\${PRODUCT_MAIN_EXE}"
    WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayName"     "$(^Name)"
    WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayVersion"  "${PRODUCT_VERSION}"
    WriteRegStr HKLM "${RK_UNINSTALL}" "Publisher"       "${PRODUCT_PUBLISHER}"
    WriteRegStr HKLM "${RK_UNINSTALL}" "UninstallString" "$INSTDIR\Uninstall.exe"
    WriteRegStr HKLM "${RK_UNINSTALL}" "URLInfoAbout"    "${PRODUCT_URL}"
SectionEnd

Section -RegToast
    SetShellVarContext all

    !insertmacro ShortcutSetToastProperties "$SMPROGRAMS\${PRODUCT_MAIN_DIR}\${PRODUCT_NAME}.lnk" "${PRODUCT_GUID}" "DmitryMaluev.SimpleTTSReader"
    pop $0
    ${If} $0 <> 0
        MessageBox MB_ICONEXCLAMATION "Shortcut-Attributes to enable Toast Messages couldn't be set"
        SetErrors
        Abort
    ${EndIf}
    SetRegView 64
    WriteRegStr HKLM "SOFTWARE\Classes\CLSID\${PRODUCT_GUID}\LocalServer32" "" "$INSTDIR\${PRODUCT_MAIN_EXE}"
    SetRegView 32
SectionEnd

Section -VisualStudioRuntime
    File "VC_redist.x64.exe"
    ExecWait '"$INSTDIR\VC_redist.x64.exe" /quiet'
    Delete "$INSTDIR\VC_redist.x64.exe"
SectionEnd

Function .onInit
    ReadRegStr $R0 HKLM "${RK_UNINSTALL}" "UninstallString"
    StrCmp $R0 "" done
    MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "${PRODUCT_NAME} is already installed.$\n$\nClick OK to remove the \
    previous version or Cancel to cancel this upgrade." /SD IDOK IDOK uninst
    Abort
uninst:
    ClearErrors
    ExecWait '$R0 _?=$INSTDIR'
    IfErrors no_remove_uninstaller done
no_remove_uninstaller:
done:
FunctionEnd
