!define PRODUCT_VERSION		"2.0"

!define PRODUCT_NAME		"Simple TTS Reader"
!define PRODUCT_NAME_SETUP	"SimpleTTSReader"
!define PRODUCT_GUID		"{85CBCC28-E397-4fcd-802E-100BE5F064A2}"
!define PRODUCT_PUBLISHER	"OpenSource"
!define PRODUCT_WEB_SITE	"https://simplettsreader.sourceforge.io/"
!define PRODUCT_MAIN_EXE	"simplettsreader.exe"

!define RK_SOFT_MS_WIN_CV	"SOFTWARE\Microsoft\Windows\CurrentVersion"
!define RK_APP_PATHS		"${RK_SOFT_MS_WIN_CV}\App Paths\${PRODUCT_MAIN_EXE}"
!define RK_UNINSTALL		"${RK_SOFT_MS_WIN_CV}\Uninstall\${PRODUCT_GUID}"
!define RK_RUN				"${RK_SOFT_MS_WIN_CV}\Run"

!define SOURCE_DIR			".."

!include "MUI.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON	"${NSISDIR}\Contrib\Graphics\Icons\orange-install.ico"
!define MUI_UNICON	"${NSISDIR}\Contrib\Graphics\Icons\orange-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Name					"${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile					"${PRODUCT_NAME_SETUP}-${PRODUCT_VERSION}-setup.exe"
InstallDir				"$PROGRAMFILES64\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}"
InstallDirRegKey		HKLM "${RK_APP_PATHS}" ""
ShowInstDetails			show
ShowUnInstDetails		show
RequestExecutionLevel	admin
BrandingText			"$(^Name)"

Section "MainSection" SEC01
	SetShellVarContext	all
	SetOutPath			"$INSTDIR"
	SetOverwrite		ifnewer
	SetCompress			auto

	File "simplettsreader.exe"
	File "${SOURCE_DIR}\License.txt"

	CreateDirectory	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}"
	CreateShortCut	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"	"$INSTDIR\${PRODUCT_MAIN_EXE}"
	CreateShortCut	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\License.lnk"			"$INSTDIR\License.txt"
	CreateShortCut	"$DESKTOP\${PRODUCT_NAME}.lnk"											"$INSTDIR\${PRODUCT_MAIN_EXE}"
SectionEnd

Section -AdditionalIcons
	CreateShortCut "$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section -Post
	WriteUninstaller "$INSTDIR\Uninstall.exe"
	WriteRegStr HKLM "${RK_APP_PATHS}" ""					"$INSTDIR\${PRODUCT_MAIN_EXE}"
	WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayName"		"$(^Name)"
	WriteRegStr HKLM "${RK_UNINSTALL}" "UninstallString"	"$INSTDIR\Uninstall.exe"
	WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayIcon"		"$INSTDIR\${PRODUCT_MAIN_EXE}"
	WriteRegStr HKLM "${RK_UNINSTALL}" "DisplayVersion"		"${PRODUCT_VERSION}"
	WriteRegStr HKLM "${RK_UNINSTALL}" "URLInfoAbout"		"${PRODUCT_WEB_SITE}"
	WriteRegStr HKLM "${RK_UNINSTALL}" "Publisher"			"${PRODUCT_PUBLISHER}"
SectionEnd

Section Uninstall
	SetShellVarContext all

	Delete "$INSTDIR\simplettsreader.exe"
	Delete "$INSTDIR\License.txt"

	Delete "$INSTDIR\uninst.exe"
	Delete "$INSTDIR\Uninstall.exe"

	Delete	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
	Delete	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\License.lnk"
	Delete	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}\Uninstall.lnk"
	RMDir	"$SMPROGRAMS\${PRODUCT_PUBLISHER}\${PRODUCT_NAME}"
	Delete	"$DESKTOP\${PRODUCT_NAME}.lnk"

	RMDir "$INSTDIR"

	DeleteRegKey HKLM "${RK_UNINSTALL}"
	DeleteRegKey HKLM "${RK_APP_PATHS}"
	SetAutoClose true
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
