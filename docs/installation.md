# Installing Auris Studio

The [release page](https://github.com/uthree/auris-studio/releases) offers desktop installers
and portable archives. Desktop releases contain the application, the `auris` CLI, the
`auris-mcp` server, the update helper and the Japanese dictionary from one build. Linux releases
contain the CLI and dictionary. Built-in physical instruments do not require a separate sound
library.

In the desktop app, **Help → Check for Updates** compares the installed version with the
latest published release. If an update is available, choose **Open Release Page** to download
the appropriate installer. The check is manual; installation follows the platform steps below.

## Windows

Run `auris-studio-v<version>-windows-x86_64-setup.exe`. The installer uses the current user's
`AppData/Local/Programs/Auris Studio` directory, so installation does not require an
administrator account. It creates a Start Menu shortcut and offers a desktop shortcut. The
command-line tools are added to the **user** `PATH` by default; open a new terminal after
installation to use `auris` and `auris-mcp` by name. The installer places the dictionary beside
the executables, where both tools find it. The update helper is installed beside
`auris-studio.exe` and is used by the desktop application's updater.

Run a newer setup file to upgrade the same installation. Use Windows **Installed apps** to
uninstall it. Uninstall removes the files and the `PATH` entry added by the installer; projects
and user settings are not installation files and remain available. The portable ZIP still runs
without installation when its folder is kept intact.

The installer is currently unsigned. Windows SmartScreen may require **More info → Run anyway**
after the publisher and download source have been checked.

## macOS

Open `auris-studio-v<version>-macos-universal.dmg` and drag `Auris Studio.app` to
**Applications**. The app contains matching command-line executables and the dictionary:

```sh
"/Applications/Auris Studio.app/Contents/MacOS/auris" help
"/Applications/Auris Studio.app/Contents/MacOS/auris-mcp"
```

The DMG does not add commands to the shell `PATH`. For a terminal shortcut, create a link in a
directory already on the user's `PATH`; agents can also use the paths above directly. Replace
the application in Applications with a newer version to upgrade it. The portable tar.gz keeps
root-level `auris` and `auris-mcp` links to the same executables inside its app bundle.

The application bundle also contains the update helper beside `auris-studio`. The application is
currently unsigned and not notarized. macOS may block its first launch;
see the release notes for the current manual opening instructions. macOS 13.4 or newer is
required.

## Linux

Extract `auris-v<version>-linux-x86_64.tar.gz` and keep `auris` beside its `Dictionary`
directory. Linux releases do not include the desktop application or MCP server.

## Building the installers

Tagging a release builds the portable archives, Windows setup executable and macOS DMG in
`.github/workflows/release.yml`. `workflow_dispatch` can build them without publishing. The
Windows setup uses `tools/installer/auris-studio.iss` and Inno Setup 6's `ISCC.exe`; its
`StageDir`, `OutputDir`, `AppVersion` and `IconPath` definitions are supplied by the workflow.
The macOS runner assembles the app bundle and uses `hdiutil` to create the DMG. The release
workflow does not sign either installer yet.
