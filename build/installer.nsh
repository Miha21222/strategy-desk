; Strategy Desk — the installer's own pages, on top of electron-builder's assisted
; NSIS template. electron-builder includes build/installer.nsh by itself (no
; `nsis.include` needed) into BOTH compile passes: once with BUILD_UNINSTALLER
; defined (the uninstaller), once without (the installer). makensis runs with
; -WX, so an unreferenced function fails the build — hence the !ifdef fences.
;
; What it adds:
;   installer   — one page after the directory page: is TradingView Desktop on
;                 this PC? If not, a checkbox installs it through winget
;                 (TradingView.TradingViewDesktop); without winget the checkbox
;                 opens the download page instead. The desk itself needs
;                 nothing else from outside: Claude Code, Python and tvmcp ship
;                 inside, the login happens in the app's own wizard.
;   uninstaller — asks whether to remove the Python runtime the app built in
;                 %APPDATA%\strategy-desk\py (~600 MB). Chats, settings and the
;                 vault (the trader's notes) are never touched.
;
; The functions live INSIDE the macros: this file is read before MUI2.nsh and
; nsDialogs.nsh, and only the macro expansion (in assistedInstaller.nsh, after
; the directory page) happens where ${NSD_*} and MUI_HEADER_TEXT exist.
; Saved as UTF-8 with BOM so makensis (Unicode) reads the Russian strings.

!define SD_TV_WINGET_ID "TradingView.TradingViewDesktop"
!define SD_TV_DOWNLOAD_URL "https://www.tradingview.com/desktop/"

!ifndef BUILD_UNINSTALLER

Var SdTvFound        ; "1" when the TradingView.Desktop MSIX package is installed
Var SdWingetFound    ; "1" when winget answers
Var SdTvAction       ; "winget" | "open" | "" — what the user chose on the page
Var SdTvCheckbox

!macro customPageAfterChangeDir

  ; --- facts: is TradingView Desktop installed, is winget here ---------------
  Function sdDetect
    StrCpy $SdTvFound "0"
    StrCpy $SdWingetFound "0"
    ; The installer is a 32-bit process: without this, powershell.exe resolves
    ; to the SysWOW64 one, which may lack the Appx module.
    ${If} ${RunningX64}
      ${DisableX64FSRedirection}
    ${EndIf}
    nsExec::ExecToStack 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "if (Get-AppxPackage -Name TradingView.Desktop) { exit 0 } else { exit 1 }"'
    Pop $0
    Pop $1
    ${If} $0 == "0"
      StrCpy $SdTvFound "1"
    ${EndIf}
    nsExec::ExecToStack '"$LOCALAPPDATA\Microsoft\WindowsApps\winget.exe" --version'
    Pop $0
    Pop $1
    ${If} $0 == "0"
      StrCpy $SdWingetFound "1"
    ${EndIf}
    ${If} ${RunningX64}
      ${EnableX64FSRedirection}
    ${EndIf}
  FunctionEnd

  ; --- the page ---------------------------------------------------------------
  Function sdComponentsPre
    ${If} ${isUpdated}
      Abort
    ${EndIf}
    Call sdDetect
    !insertmacro MUI_HEADER_TEXT "TradingView Desktop" "Агент читает график и делает снимки через TradingView Desktop"
    nsDialogs::Create 1018
    Pop $0
    ${If} $0 == error
      Abort
    ${EndIf}

    ${If} $SdTvFound == "1"
      ${NSD_CreateLabel} 0 0 100% 36u "TradingView Desktop уже установлен — ничего делать не нужно.$\r$\n$\r$\nClaude Code, Python и сервер рыночных данных идут в поставке Strategy Desk; вход в Claude и остальная настройка — в мастере при первом запуске."
      Pop $1
      StrCpy $SdTvAction ""
    ${ElseIf} $SdWingetFound == "1"
      ${NSD_CreateLabel} 0 0 100% 36u "TradingView Desktop на этом компьютере не найден. Без него обзор идёт без картинок графика, а уровни не рисуются.$\r$\n$\r$\nУстановить его можно прямо сейчас через winget (нужен интернет, около 100 МБ)."
      Pop $1
      ${NSD_CreateCheckbox} 0 44u 100% 12u "Установить TradingView Desktop через winget (${SD_TV_WINGET_ID})"
      Pop $SdTvCheckbox
      ${NSD_Check} $SdTvCheckbox
      StrCpy $SdTvAction "winget"
    ${Else}
      ${NSD_CreateLabel} 0 0 100% 36u "TradingView Desktop на этом компьютере не найден, а winget недоступен, поэтому установщик не может поставить его сам.$\r$\n$\r$\nСкачайте его с ${SD_TV_DOWNLOAD_URL} — страницу можно открыть после установки."
      Pop $1
      ${NSD_CreateCheckbox} 0 44u 100% 12u "Открыть страницу загрузки TradingView Desktop после установки"
      Pop $SdTvCheckbox
      ${NSD_Check} $SdTvCheckbox
      StrCpy $SdTvAction "open"
    ${EndIf}

    ${NSD_CreateLabel} 0 70u 100% 24u "Claude Code отдельно ставить не нужно: он внутри приложения. Нужна только подписка Claude — вход сделаете в мастере при первом запуске."
    Pop $1
    nsDialogs::Show
  FunctionEnd

  Function sdComponentsLeave
    ${If} $SdTvAction != ""
      ${NSD_GetState} $SdTvCheckbox $0
      ${If} $0 != ${BST_CHECKED}
        StrCpy $SdTvAction ""
      ${EndIf}
    ${EndIf}
  FunctionEnd

  Page custom sdComponentsPre sdComponentsLeave

!macroend

; Runs inside the install section, after the files are in place.
!macro customInstall
  ${If} $SdTvAction == "winget"
    DetailPrint "winget install ${SD_TV_WINGET_ID} ..."
    ${If} ${RunningX64}
      ${DisableX64FSRedirection}
    ${EndIf}
    nsExec::ExecToLog '"$LOCALAPPDATA\Microsoft\WindowsApps\winget.exe" install --id ${SD_TV_WINGET_ID} -e --source winget --accept-package-agreements --accept-source-agreements --disable-interactivity'
    Pop $0
    ${If} ${RunningX64}
      ${EnableX64FSRedirection}
    ${EndIf}
    ${If} $0 != "0"
      DetailPrint "winget exited with $0"
      MessageBox MB_OK|MB_ICONEXCLAMATION "Не удалось установить TradingView Desktop через winget (код $0).$\r$\nStrategy Desk установлен; TradingView Desktop можно скачать позже с ${SD_TV_DOWNLOAD_URL} — мастер настройки напомнит." /SD IDOK
    ${EndIf}
  ${ElseIf} $SdTvAction == "open"
    ExecShell "open" "${SD_TV_DOWNLOAD_URL}"
  ${EndIf}
!macroend

!endif ; !BUILD_UNINSTALLER

!ifdef BUILD_UNINSTALLER

; Runs inside the uninstall section, before the installation directory goes.
; `deleteAppDataOnUninstall` stays false (chats, settings, journals survive);
; the one thing worth asking about is the 600 MB runtime the app built itself.
!macro customUnInstall
  ${IfNot} ${isUpdated}
    MessageBox MB_YESNO|MB_ICONQUESTION "Удалить также рабочее окружение Python, которое приложение собрало для себя (около 600 МБ в AppData)?$\r$\n$\r$\nПапка данных с обзорами и заметками, чаты и настройки не удаляются никогда. Окружение можно собрать заново при следующей установке." /SD IDNO IDNO sdKeepRuntime
      ; The runtime is per-user even when the app was installed for all users.
      ${If} $installMode == "all"
        SetShellVarContext current
      ${EndIf}
      RMDir /r "$APPDATA\${APP_PACKAGE_NAME}\py"
      ${If} $installMode == "all"
        SetShellVarContext all
      ${EndIf}
    sdKeepRuntime:
  ${EndIf}
!macroend

!endif ; BUILD_UNINSTALLER
