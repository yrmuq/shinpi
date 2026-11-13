# lore

## Required Behavior

- Do not build or publish aarch64 AppImage or Flatpak packages until that support is explicitly requested.
- Keep the About attribution table limited to Component, Upstream and Integration Details, matching yoin's dev UI; the user explicitly excluded a License column.
- Appearance Reset preserves devices, saved readings, notification timestamps, history and notification rules; Dev Reset restores only window dimensions and logo size.

## External Compatibility Constraints

- `flatpak-builder --run` rejects `--user`; the sandbox launch check passes only `--arch`, because that mode has a separate option table from the build command.

## Compatibility Workarounds

- Recreate hidden window components before showing them again: Slint 1.18 resets native window attributes on Wayland hide, losing the class and initial title; remove this workaround only after verifying that the backend preserves both across hide/show.
