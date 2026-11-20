# lore

## Required Behavior

- Do not build or publish aarch64 AppImage or Flatpak packages until that support is explicitly requested.
- Keep the About attribution table limited to Component, Upstream and Integration Details, matching yoin's dev UI; the user explicitly excluded a License column.
- Appearance Reset preserves devices, saved readings, notification timestamps, history, notification rules and the update-check preference; Dev Reset restores window dimensions, logo size, the resize lock and automatic update checks.

- Battery polling and low-charge notifications continue while all application windows are closed.
- Reclaim hidden UI components without losing the selected settings tab or popup pin state; periodic UI refreshes must only visit visible windows.
- Reuse decoded SVG images through a bounded cache so repeated refreshes do not retain an unbounded set of images.
- Treat the instance counter of a directly attached USB HID device as transient and keep selections separate by source, USB port and interface when a hardware address or serial is unavailable.

## External Compatibility Constraints

- `flatpak-builder --run` rejects `--user`; the sandbox launch check passes only `--arch`, because that mode has a separate option table from the build command.
- Preserve KWin's `Order` when present and remove legacy `rules`/`count` there: Plasma 6.8 migrates a non-empty legacy list by deleting every unlisted rule group, even those listed in `Order`.
- Preserve other KWin rule sections and their order; keep a non-overwritten backup before changing `kwinrulesrc`, and abort on read or backup errors.

## Compatibility Workarounds

- Recreate hidden window components before showing them again: Slint 1.18 resets native window attributes on Wayland hide, losing the class and initial title; remove this workaround only after verifying that the backend preserves both across hide/show.
- Give device and notice panels distinct native classes and recognize them before captions arrive; Wayland title propagation can occur after the first KWin placement.
- Wait for KWin rule reload acknowledgement and a subsequent round trip before exposing the tray item; reconfigure is asynchronous and an early click can precede the taskbar rules.
