; Plycast shipped as "3D Printing" up to v0.2.0. The NSIS product name sets the install folder,
; Start menu shortcut, autostart value and uninstall entry, so without this hook the renamed
; installer would leave the old copy installed next to the new one. Remove it quietly first.
; Settings (bundle identifier) and the LAN access code (credential service) keep their names,
; so they carry over untouched.
!macro NSIS_HOOK_PREINSTALL
  Push $R8
  Push $R9
  ReadRegStr $R8 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\3D Printing" "UninstallString"
  ReadRegStr $R9 HKCU "Software\badblli\3D Printing" ""
  ${If} $R9 == ""
    StrCpy $R9 "$LOCALAPPDATA\3D Printing"
  ${EndIf}
  ${If} $R8 != ""
  ${AndIf} ${FileExists} "$R9\uninstall.exe"
    ; Without a stored language the old uninstaller stops on a language picker, even with /P.
    WriteRegStr HKCU "Software\badblli\3D Printing" "Installer Language" "$LANGUAGE"
    ; /P runs the old uninstaller without UI and keeps app data; _?= keeps it in place so we can wait.
    ExecWait '"$R9\uninstall.exe" /P _?=$R9'
    Delete "$R9\uninstall.exe"
    RMDir "$R9"
  ${EndIf}
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "3D Printing"
  ; Only held the old install folder and installer language.
  DeleteRegKey HKCU "Software\badblli\3D Printing"
  Pop $R9
  Pop $R8
!macroend
