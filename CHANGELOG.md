# Changelog

## 0.3.0 — 2026-10-16

- Honor the original suite's LedMask for both F87 profiles. Hide Rainbow wheel
  (6), Trail (9), Sweep (14) and Storm (18), which were incorrectly offered.
  The selector now maps its 14 available effects to their original hardware IDs.
  CLI, profile application and RGB-only loading reject unavailable normal effects;
  reading and backing up existing device state still preserve unknown fields.

- Experimental AULA F87 standard support over USB, using its identity and shared
  protocol recovered from the original Windows suite. The detected model appears
  in the interface; existing F87 Pro profiles and storage paths remain compatible.
- Reject applying full profiles, RGB-only imports and legacy backups from another model.
- F87 standard hardware effects still need physical validation. The suite declares
  the same effect table as the Pro; this does not establish the cause of issue #1.
- F87 standard radio configuration remains unavailable.

## 0.2.0 — 2026-10-16

- Portuguese and English interfaces, with separate screenshots in each README.
- Settings page with device-reported version, debounce stages, idle sleep and experimental Tap sensitivity.
- Local language, start-at-login and quit/minimize preferences; Ctrl+1…5 navigates tabs.
- Restore the three keyboard behavior settings without changing RGB, keys or macros.
- Settings use the existing staged apply, backup, readback and undo flow.
- Firmware flashing and full factory reset remain unavailable; Tap semantics and physical timings need validation.

## 0.1.0 — 2026-10-15

First public Linux preview.

- Native GTK4 interface with an aligned 87-key layout and light/dark themes.
- Eighteen RGB effects with animated local previews, pause and reduced motion.
- USB key assignments, shortcuts, media actions and a validated macro editor.
- Experimental 2.4 GHz lighting and key assignments on layers 0/1.
- Named local profiles, RGB-only loading across connections, import and export.
- Backups before writes, stale-state checks, readback verification and undo.
- Linux x86_64 tar archive, Debian package, Arch package, desktop icon and udev rules.

Radio macro editing and layers 2/3 require USB. Bluetooth, firmware flashing and
arbitrary per-key RGB are not implemented. Radio reads can be intermittent;
physical macro timing and every playback mode remain unverified.
