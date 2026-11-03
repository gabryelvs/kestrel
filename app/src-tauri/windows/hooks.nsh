; Kestrel's additions to Tauri's NSIS installer (installerHooks in tauri.conf.json). Tauri inserts each macro into its own
; script, so $UpdateMode, $DeleteAppDataCheckboxState and MAINBINARYNAME are Tauri's. Every LogicLib name used here is one
; Tauri's own script uses, except AndIfNot and Cmd, which come from the same header. Plain ASCII.

!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri's installer runs this uninstaller in place with _?= during an upgrade, so $EXEDIR is the install folder;
  ; a person's uninstall runs from the copy NSIS makes in a temp folder. Only a real uninstall asks.
  ${If} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
  ${AndIfNot} ${Silent}
    ; An ExecWait that can't start its program sets the error flag and leaves $0 as it was, so the flag is the only sign
    ; that the program ran. un.onInit's GetOptions leaves the flag set when there is no /UPDATE, so it is cleared before
    ; each start. A has-changes that didn't run asks nothing.
    ClearErrors
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tweaks-has-changes' $0
    ${IfNot} ${Errors}
    ${AndIf} $0 = 0
      ; Cmd runs the MessageBox and takes its IDYES jump as the answer, so No falls out of the block with no label.
      ${If} ${Cmd} `MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 "Undo Kestrel's changes before removing it?" IDYES`
        ClearErrors
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --tweaks-revert-all' $0
        ; Exit 3 is not only a failure: System Protection and another account's changes stay open on purpose.
        ${If} ${Errors}
        ${OrIf} $0 <> 0
          MessageBox MB_OK|MB_ICONEXCLAMATION "Kestrel couldn't undo everything. Check those settings in Windows."
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Delete the application data" also removes the undo history, after the question above.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExpandEnvStrings $0 "%ProgramData%\Kestrel"
    RMDir /r "$0"
  ${EndIf}
!macroend
