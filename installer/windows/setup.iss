#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef PublishDir
  #define PublishDir "..\..\artifacts\windows"
#endif
[Setup]
AppId={{0F951EA0-DFAD-4F72-944D-0FC36FE01D15}
AppName=HL100 Drop
AppVersion={#AppVersion}
AppPublisher=HL100
AppPublisherURL=https://github.com/cveloy102/hl100-drop
DefaultDirName={localappdata}\Programs\HL100 Drop
DefaultGroupName=HL100 Drop
PrivilegesRequired=lowest
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\artifacts
OutputBaseFilename=HL100DropSetup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\HL100Drop.exe
SetupIconFile=..\..\apps\windows\assets\drop.ico
[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"
[Tasks]
Name: "desktopicon"; Description: "Создать ярлык на рабочем столе"; Flags: unchecked
[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\HL100 Drop"; Filename: "{app}\HL100Drop.exe"
Name: "{autodesktop}\HL100 Drop"; Filename: "{app}\HL100Drop.exe"; Tasks: desktopicon
[Run]
Filename: "{app}\HL100Drop.exe"; Description: "Запустить HL100 Drop"; Flags: nowait postinstall skipifsilent
