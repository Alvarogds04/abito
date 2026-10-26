; Abito Island — Windows installer (unit U4, decision B: NSIS-direct, no
; tauri-bundler downloads). Built inside the cross container, e.g.:
;   makensis -DVERSION=0.1.0 -DBINDIR=/abito/dist -DICON=/abito/src/icons/icon.ico \
;            -DOUTDIR=/abito/dist packaging/windows/abito.nsi
;
; Honest scope, stated in one line each:
; - Per-user install (RequestExecutionLevel user): no UAC, no admin, no
;   machine-wide writes. Lives under $LOCALAPPDATA like the Linux user-local
;   format-A install lives under the user's home.
; - UNSIGNED. No signing step is claimed anywhere; the VM checklist validates
;   the SmartScreen path of an unsigned binary explicitly.
; - Uninstall removes exactly what install created. The island's own config
;   directory is USER data and survives — the same policy as the Linux install.
Unicode true
!include MUI2.nsh

!ifndef VERSION
!error "VERSION must be passed with -DVERSION"
!endif
!ifndef BINDIR
!error "BINDIR must be passed with -DBINDIR"
!endif
!ifndef ICON
!error "ICON must be passed with -DICON"
!endif
!ifndef OUTDIR
!error "OUTDIR must be passed with -DOUTDIR"
!endif

Name "Abito Island"
OutFile "${OUTDIR}/abito-island-${VERSION}-win-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\abito-island"
InstallDirRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "InstallLocation"
RequestExecutionLevel user

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Abito Island"
  SetOutPath "$INSTDIR"
  File "${BINDIR}/abito-island.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Abito Island"
  CreateShortCut "$SMPROGRAMS\Abito Island\Abito Island.lnk" "$INSTDIR\abito-island.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "DisplayName" "Abito Island"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri" "Publisher" "Abito (unsigned build)"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\abito-island.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR" ; succeeds only when the folder is empty — never silent-recursion
  Delete "$SMPROGRAMS\Abito Island\Abito Island.lnk"
  RMDir "$SMPROGRAMS\Abito Island"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.abitog.AbitoTauri"
SectionEnd
