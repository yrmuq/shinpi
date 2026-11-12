# lore

## Required Behavior

- Do not build or publish aarch64 AppImage or Flatpak packages until that support is explicitly requested.

## External Compatibility Constraints

- `flatpak-builder --run` rejects `--user`; the sandbox launch check passes only `--arch`, because that mode has a separate option table from the build command.
