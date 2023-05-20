# Changelog

All notable changes to `self-replace` are documented here.

## 1.3.4

- Explicitly pass the process handle on Windows simplifying the implementation.
- Change the dummy command to use on windows away from `ping.exe` to `cmd.exe`.
- More correctly invoke `CreateProcessA` on Windows.

## 1.3.3

- Avoid the use of `atexit` and spawn immediately.  This has the advantage
  that even if the process crashes hard, we already have started the cleanup
  handler.
- Improve safety of the crate for Windows by avoiding unsupported life before main.

## 1.3.2

- Use an atomic rename on Windows in the self replacement case for the
  final step to avoid accidentally leaving partial executables behind.
- Resolved an issue where a potentially incorrect filename was computed
  on Windows in some cases.
- A temporary folder is now always preferred on windows for the temporary
  operations if possible.  This code path now also works correctly.

## 1.3.1

- Fixes a bug that caused the wrong path to be calculated internally
  creating an access error in some cases.

## 1.3.0

- Added support for `self_delete_outside_path` to support more complex
  uninstallation scenarios on Windows.

## 1.2.0

- Improve a race condition on Windows.

## 1.1.0

- Support older rustc releases.

## 1.0.0

- Initial release.
