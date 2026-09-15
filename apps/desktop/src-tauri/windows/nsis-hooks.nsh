; CodexLink keeps a stable executable name across upgrades. Windows Explorer can
; retain an old icon for that path even after the embedded executable icon has
; changed. The executable itself carries the correct icon; invalidate Shell
; association/icon caches after install and uninstall so Explorer re-reads it.

!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'
!macroend
