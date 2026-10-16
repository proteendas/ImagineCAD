; Inno Setup script for the ImagineCAD Windows installer.
; Build:  iscc /DAppVersion=0.1.0 /DSourceDir=..\..  packaging\windows\imaginecad.iss

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\.."
#endif

[Setup]
AppId={{6E0B6C51-3C5B-4C2A-9D6E-1C4A2B7F9E21}
AppName=ImagineCAD
AppVersion={#AppVersion}
AppPublisher=Proteen Das
AppCopyright=Copyright (C) 2026 Proteen Das
LicenseFile={#SourceDir}\LICENSE
DefaultDirName={autopf}\ImagineCAD
DefaultGroupName=ImagineCAD
UninstallDisplayIcon={app}\imaginecad.exe
OutputDir={#SourceDir}\dist
OutputBaseFilename=ImagineCAD-{#AppVersion}-windows-x64-setup
SetupIconFile={#SourceDir}\assets\icon.ico
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
WizardStyle=modern
PrivilegesRequiredOverridesAllowed=dialog

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assocdxf"; Description: "Offer ImagineCAD for .dxf files"; GroupDescription: "File associations"; Flags: unchecked
Name: "assocdwg"; Description: "Offer ImagineCAD for .dwg files"; GroupDescription: "File associations"; Flags: unchecked

[Files]
Source: "{#SourceDir}\target\release\imaginecad.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE"; DestDir: "{app}"; DestName: "NOTICE.txt"; Flags: ignoreversion
Source: "{#SourceDir}\dist\THIRD_PARTY_LICENSES.html"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SourceDir}\dist\libredwg\*"; DestDir: "{app}\libredwg"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\ImagineCAD"; Filename: "{app}\imaginecad.exe"
Name: "{group}\Uninstall ImagineCAD"; Filename: "{uninstallexe}"
Name: "{autodesktop}\ImagineCAD"; Filename: "{app}\imaginecad.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.icad"; ValueType: string; ValueName: ""; ValueData: "ImagineCAD.Drawing"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\ImagineCAD.Drawing"; ValueType: string; ValueName: ""; ValueData: "ImagineCAD Drawing"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ImagineCAD.Drawing\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\imaginecad.exe,0"
Root: HKA; Subkey: "Software\Classes\ImagineCAD.Drawing\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\imaginecad.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.dxf\OpenWithProgids"; ValueType: string; ValueName: "ImagineCAD.Drawing"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocdxf
Root: HKA; Subkey: "Software\Classes\.dwg\OpenWithProgids"; ValueType: string; ValueName: "ImagineCAD.Drawing"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocdwg
; "Open with" for every type ImagineCAD reads (drawings and sketch photos).
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "ImagineCAD"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\imaginecad.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".icad"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".dxf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".dwg"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".png"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".jpg"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\imaginecad.exe\SupportedTypes"; ValueType: string; ValueName: ".jpeg"; ValueData: ""

[Run]
Filename: "{app}\imaginecad.exe"; Description: "{cm:LaunchProgram,ImagineCAD}"; Flags: nowait postinstall skipifsilent
