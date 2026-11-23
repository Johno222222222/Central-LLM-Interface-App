; Stop the running app and its bundled llama-server before files are replaced.
; llama-server keeps llama.dll open, and NSIS then loops on "Error opening file for writing".

!macro StopAppAndLlamaServer
  ; nsExec does not open a console window. A missing process is not a failure.
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM llama-server.exe'
  Pop $0
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM central-llm-interface.exe'
  Pop $0
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM "Central LLM Interface App.exe"'
  Pop $0
  Sleep 2000
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro StopAppAndLlamaServer
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro StopAppAndLlamaServer
!macroend
