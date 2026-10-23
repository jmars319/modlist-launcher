# Steam Play button

Steam lets a game's launch options name a different program to run, with `%command%` standing for the game's own command. Modlist Launcher uses this so Steam's Play button starts a modlist through its own Mod Organizer:

```text
"C:\...\modlist-launcher.exe" --steam <installation id> -- %command%
```

## Set it up

1. In the launcher, open **••• > Steam Play Button…** on the modlist's card.
2. Check the Steam account and app ID. Both are detected from MO2's game folder.
3. Exit Steam completely (Steam menu > Exit). Steam rewrites its settings when it closes, so changes made while it runs would be lost.
4. Press **Apply**.

To set it by hand instead, press **Copy**. Then in Steam, right-click the game, choose **Properties > General > Launch Options**, and paste.

## What happens when you press Play

- **Normally:** the launcher starts MO2's Play executable for that modlist and waits in the background, without a window, until the game exits. Steam sees the session for its whole length, so playtime and Steam Cloud sync work as usual.
- **MO2's waiting dialog:** MO2, started this way, shows "Mod Organizer is waiting on an application to close before exiting" with the game's process listed. It isn't an error: MO2 is waiting so it can close after the game. The launcher minimizes it so it doesn't cover the loading game. If you open it from the taskbar, don't press **Exit Now** while playing, because that closes MO2 while the game is using its virtual mod folder.
- **Hold Shift while pressing Play:** a small window offers **With Mods**, **Without Mods** (Steam's original command, unchanged), or **Cancel**.
- **If the modded launch can't go ahead,** the same window explains why and still offers Without Mods. For example, a different MO2 is open, the installation is missing, or the modlist was removed from the launcher.

## Undo

Steam must be closed for any of these.

- **••• > Steam Play Button… > Restore Previous** puts back the launch options the game had before.
- **Removing a modlist** from the launcher restores its launch options first (**Restore and Remove**). If Steam is running, nothing is removed until you exit Steam and try again.
- **Uninstalling the launcher** restores every connected game's launch options. If Steam is running, the uninstaller asks you to exit it and choose Retry. If you skip, it tells you which games to fix by hand.

The launcher only restores a value it set itself. If you changed a game's launch options in Steam afterwards, they are left as they are.

## Steam Cloud

Steam Cloud syncs a game's saves when a Steam-tracked session starts and ends, provided the saves are in the location Steam expects. MO2 profiles with **local savegames** turned on keep saves inside the profile folder instead, where Steam Cloud cannot see them. Check that MO2 profile setting if a modlist's saves don't sync. The launcher does not change it.
