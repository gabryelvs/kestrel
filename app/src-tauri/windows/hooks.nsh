; Kestrel's additions to Tauri's NSIS installer (installerHooks in tauri.conf.json). Tauri inserts each macro into its own
; script, so $UpdateMode, $DeleteAppDataCheckboxState, MAINBINARYNAME, PRODUCTNAME and CheckIfAppIsRunning are Tauri's. Every
; LogicLib name used here is one Tauri's own script uses, except AndIfNot and Cmd, which come from the same header. Plain ASCII.

!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri's installer runs this uninstaller in place with _?= during an upgrade, so $EXEDIR is the install folder;
  ; a person's uninstall runs from the copy NSIS makes in a temp folder. Only a real uninstall asks.
  ${If} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
  ${AndIfNot} ${Silent}
    ; Tauri asks whether to close a running Kestrel right after this hook, and Cancel there stops the uninstall. Asked only
    ; there, it would come after a Yes below, and leave the changes undone and Kestrel installed. Asked first, Cancel stops
    ; the uninstall before anything changed, and no window in the middle of an Apply holds the journal's lock. Tauri's own
    ; check then finds nothing. Its labels can't clash with this one's: they carry the line of the insertion.
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
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
        ; The flag first and only once, because looking at it clears it, and a start that failed leaves the question's 0 in
        ; the variable. Exit 4 is no failure: all that is left is System Protection, which stays on on purpose. Exit 3 is
        ; anything else left: a value something else changed, or another account's changes.
        ${If} ${Errors}
          MessageBox MB_OK|MB_ICONEXCLAMATION "Kestrel couldn't undo everything. Check those settings in Windows."
        ${ElseIf} $0 = 4
          MessageBox MB_OK|MB_ICONINFORMATION "Kestrel undid its changes. System Protection stays on, because turning it off deletes this drive's restore points. You can turn it off in Windows."
        ${ElseIf} $0 <> 0
          MessageBox MB_OK|MB_ICONEXCLAMATION "Kestrel couldn't undo everything. Check those settings in Windows."
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Delete the application data" also removes the undo history, after the question above. Run in place (setup's
  ; maintenance page, or a newer setup run by hand), no question was asked, so the changes are still on the PC and the
  ; history stays with them. The journal's two files are deleted by name and the folder goes only if that leaves it empty
  ; (so one with anything else in it stays). RMDir /r would go into anything marked as a folder, a junction included, and a
  ; standard user may have made this path one before Kestrel ever ran. $APPDATA under "all" is ProgramData as the shell
  ; names it, which is where Kestrel's own code looks, whatever the environment says.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
    SetShellVarContext all
    Delete "$APPDATA\Kestrel\journal.jsonl"
    Delete "$APPDATA\Kestrel\journal.lock"
    RMDir "$APPDATA\Kestrel"
    ; Tauri's block just above, under the first two of these conditions, switched to the current user for its own folders,
    ; and nothing after this hook uses a shell folder, so that is what is put back.
    SetShellVarContext current
  ${EndIf}
!macroend
