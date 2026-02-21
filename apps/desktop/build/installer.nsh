; CraftNet Windows Installer Script
; Additional NSIS script for electron-builder

!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"

; ============================================
; Custom Pages and Functions
; ============================================

; Custom installation directory page
!define MUI_DIRECTORYPAGE_VERIFYONLEAVE

; License page
!insertmacro MUI_PAGE_LICENSE "${BUILD_RESOURCES_DIR}\LICENSE.txt"

; ============================================
; Post-Install Actions
; ============================================

!macro customInstall
  ; Install and start the Windows service
  DetailPrint "Installing CraftNet daemon service..."
  
  ; Copy the daemon executable
  SetOutPath "$INSTDIR\daemon"
  
  ; Register as a Windows service using sc.exe
  ; The daemon will be configured to auto-start
  nsExec::ExecToLog 'sc create CraftNetDaemon binPath= "$INSTDIR\daemon\craftnet-daemon.exe" start= auto DisplayName= "CraftNet VPN Daemon"'
  
  ; Set service description
  nsExec::ExecToLog 'sc description CraftNetDaemon "CraftNet decentralized VPN daemon - provides P2P VPN connectivity"'
  
  ; Configure service recovery (restart on failure)
  nsExec::ExecToLog 'sc failure CraftNetDaemon reset= 86400 actions= restart/5000/restart/10000/restart/30000'
  
  ; Start the service
  nsExec::ExecToLog 'sc start CraftNetDaemon'
  
  ; Add firewall rule for the daemon
  DetailPrint "Configuring Windows Firewall..."
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="CraftNet VPN" dir=in action=allow program="$INSTDIR\daemon\craftnet-daemon.exe" enable=yes profile=any'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="CraftNet VPN" dir=out action=allow program="$INSTDIR\daemon\craftnet-daemon.exe" enable=yes profile=any'
  
  ; Register for auto-start with Windows (backup to service)
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "CraftNet" "$INSTDIR\CraftNet.exe --minimized"
  
  DetailPrint "CraftNet installation complete!"
!macroend

; ============================================
; Pre-Uninstall Actions
; ============================================

!macro customUnInstall
  ; Stop and remove the Windows service
  DetailPrint "Stopping CraftNet daemon service..."
  nsExec::ExecToLog 'sc stop CraftNetDaemon'
  
  ; Wait for service to stop
  Sleep 2000
  
  DetailPrint "Removing CraftNet daemon service..."
  nsExec::ExecToLog 'sc delete CraftNetDaemon'
  
  ; Remove firewall rules
  DetailPrint "Removing Windows Firewall rules..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="CraftNet VPN"'
  
  ; Remove auto-start registry entry
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "CraftNet"
  
  ; Clean up IPC named pipe (if exists)
  ; Note: Named pipes are automatically cleaned up when all handles are closed
  
  ; Clean up any remaining config files
  RMDir /r "$APPDATA\CraftNet"
  RMDir /r "$LOCALAPPDATA\CraftNet"
  
  DetailPrint "CraftNet uninstallation complete!"
!macroend

; ============================================
; Custom Functions
; ============================================

Function .onInstSuccess
  ; Show completion message
  MessageBox MB_ICONINFORMATION|MB_OK "CraftNet has been installed successfully!$\n$\nThe VPN daemon is now running in the background.$\nClick the CraftNet icon in your system tray to connect."
FunctionEnd

Function un.onUninstSuccess
  ; Show uninstall completion message
  MessageBox MB_ICONINFORMATION|MB_OK "CraftNet has been uninstalled.$\n$\nThank you for using CraftNet!"
FunctionEnd

; Check for admin rights
Function .onInit
  UserInfo::GetAccountType
  Pop $0
  ${If} $0 != "admin"
    MessageBox MB_ICONSTOP "Administrator privileges are required to install CraftNet.$\nPlease right-click the installer and select 'Run as administrator'."
    Abort
  ${EndIf}
FunctionEnd
