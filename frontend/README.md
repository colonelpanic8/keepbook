# keepbook frontend

Expo (React Native) app living in `frontend/`.

## Nix shells

- Default: `nix develop --impure`
- Android: `nix develop --impure .#android`
- iOS: `nix develop --impure .#ios`

If you use direnv: `direnv allow` in `frontend/`.

## Commands

- `yarn start` starts the dev server.
- `yarn android` builds and runs on Android (native dev client).
- `yarn web` runs in the browser.
- `just emulator` starts an Android emulator (expects an AVD named `keepbook_test` by default).
