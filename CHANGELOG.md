# Changelog

All notable changes to `self-replace` are documented here.

## Unreleased

- Use an atomic rename on Windows in the self replacement case for the
  final step to avoid accidentally leaving partial executables behind.

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
