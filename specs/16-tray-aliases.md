# Tray: Alias Management

## Purpose
Enables browsing share folder trees and creating/removing subfolder aliases from the menu bar UI without needing to construct paths by hand (JTBD 14).

## Requirements
- Provide a way to browse folders inside a mounted share from the UI (companion window or nested menu)
- Allow selecting a folder and creating a named alias with one action
- Show existing aliases per share with their target subpaths
- Provide "Remove Alias" action per alias
- Alias creation from UI calls the same engine `add_alias` function as CLI `alias add`
- Alias removal from UI calls the same engine `remove_alias` function as CLI `alias remove`
- Folder browsing calls the same engine `list_folders` function as CLI `folders`

## Constraints
- Folder browsing requires the share to be currently mounted — show a clear message if unmounted
- A companion window may be needed for folder tree navigation since tray menus have limited depth
- Folder listing may be slow on large shares — show a loading indicator

## Acceptance Criteria
1. User can browse folders inside a mounted share from the UI
2. Selecting a folder and naming an alias creates the alias symlink
3. Existing aliases are visible in the UI with their target paths
4. Aliases can be removed from the UI
5. Browse/create/remove actions all call engine functions (no separate code paths)
6. Clear feedback is shown if the share is not mounted
