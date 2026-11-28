Synthetic total-conversion fixture for tests/test_uiprofile.py (invented names, no content from any real mod).

- `game/`: a tiny stand-in for the vanilla game folder (only the interface elements and localisation keys the
  interface profile reads).
- `user/`: a user folder whose playset (dlc_load.json) enables two mods in load order:
  - `renamed`: an overhaul that keeps the vanilla interface element names but renames the months (localisation
    keys `Jan`..`Dec`, `January`..`December`), the pause menu's buttons and the decisions panel title, partly
    through its own `.gui` keys and partly through a `replace/` localisation file;
  - `rebuilt`: replaces the main menu with buttons of its own names and ships a `hoi4_live_profile.json` for what
    cannot be derived (a title drawn as an image, a panel title, a dialog's confirming button).
