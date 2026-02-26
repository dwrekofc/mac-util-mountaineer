# Tray: Favorites Management

## Purpose
Enables adding and removing managed network shares directly from the menu bar app without needing a terminal (JTBD 13).

## Requirements
- Provide an "Add Favorite" action that presents a form/flow to enter: share name, Thunderbolt host, Fallback host, username
- Optional field: remote share name (defaults to share name if omitted)
- On add: the new favorite starts mounting immediately (same behavior as CLI `favorites add`)
- Provide a "Remove Favorite" action per share with option to clean up (unmount + remove symlink)
- Show confirmation before removing a favorite
- Report dependent aliases that will be affected by removal
- UI actions call the same engine `add_or_update_share` and `remove_share` functions as the CLI

## Constraints
- The add flow may use a companion panel/window since a tray menu has limited input capability
- All validation (duplicate names, required fields) is performed by the engine, not duplicated in UI code

## Acceptance Criteria
1. "Add Favorite" presents input fields for name, TB host, fallback host, username
2. New favorite appears in the tray menu immediately after adding
3. The share begins mounting automatically after add
4. "Remove Favorite" with cleanup unmounts and removes the symlink
5. Confirmation is shown before removal
6. Affected aliases are reported during removal
