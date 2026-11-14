# Playing NVForever with Mod Organizer 2

Mod Organizer 2 (MO2) can start NVForever with the mods of its profile, the same way it
starts Fallout: New Vegas. Nothing needs installing or copying: add NVForever to MO2 once,
then pick it beside **Run**.

## Add NVForever to MO2 (once)

1. In MO2, open the executables list (the drop-down beside **Run**) and choose
   **<Edit…>**.
2. Press **+**, then **Add from file…**, and choose `nvforever.exe`.
3. Leave **Start in** as its folder and the arguments empty, then **OK**.

Started from MO2, NVForever runs on the game folder MO2 manages (Settings, Paths,
Managed Game), even when that is a copy of the game, as modlists such as Viva New Vegas
use; the folder in `NVForever.ini` is for starts outside MO2 and is not changed.
NVForever can be installed anywhere outside the game folder.

## What it uses from MO2

- **Mods** (left pane): the active mods' files appear in the game's `Data` folder for
  NVForever, overriding each other by MO2's priority, as they do for the game.
- **Plugins** (right pane): the plugins ticked there load, in that order. NVForever's own
  plugin list (`Documents\My Games\NVForever\plugins.txt`) is not used or changed by an
  MO2 start; started any other way, NVForever uses its own list as before.
- Started from a console, NVForever prints which game folder and plugin list it used:
  `setup: Mod Organizer 2 start: game folder from its instance (...)` and `... plugins
  from its profile (...)`.
- MO2 started with `-i <instance>` is not followed: NVForever finds the instance as MO2
  does by default (a portable install's own, else MO2's current global instance).

## What it does not use

- MO2's profile INIs (`Fallout.ini`, `FalloutPrefs.ini`): NVForever keeps its own settings
  in `NVForever.ini`.
- MO2's profile saves: NVForever's saves stay in `Documents\My Games\NVForever\saves`.
- NVSE, its plugins (`.dll`) and other programs hooking the original game do not run in
  NVForever. Mods that need NVSE script functions NVForever has not built yet may not
  work.

## How it works

MO2 starts programs with its file system hook (usvfs). NVForever sees the hook in its own
process and reads the plugin list MO2 shows it at the original game's list path
(`%LOCALAPPDATA%\FalloutNV\plugins.txt`), recognised by MO2's header line. Details and
sources: `crates/fnv-install/src/mo2.rs`, docs/design/install-layout.md.

## Checking a mod list's scripts without starting the game

`fnvtool` reads an MO2 profile directly, with MO2 closed: `--mo2-profile <profile
folder>` anywhere in a command that takes a load order stands for the profile's active
plugins, in order. Each plugin comes from the highest-priority active mod that has it,
or else from the game's Data folder (the instance's `gamePath`). For example, the
extender scripts the game will run from source, the ones that won't compile and the
functions NVForever lacks, per plugin:

```
fnvtool ext report "<game>\Data" --mo2-profile "<instance>\profiles\<profile>" --schema schemas/fnv
```

Only plugins are read this way. A mod's loose files and archives are not merged, so
commands that need meshes or textures still need MO2 or a merged Data folder.
