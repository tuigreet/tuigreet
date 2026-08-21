# tuigreet Changelog

## 0.12.0

### Added

- Suspend and hibernate actions are now available from the power menu. They use
  `loginctl suspend` and `loginctl hibernate` by default.
- `[power] suspend` and `[power] hibernate` configuration keys, plus the
  `--power-suspend` and `--power-hibernate` command-line options, allow
  overriding those actions.
- When user-menu filtering leaves exactly one eligible user, tuigreet now
  selects that user and begins session creation automatically.

### Changed

- `display.asterisks` is deprecated. Use `[secret] mode = "characters"` or
  `mode = "hidden"` instead. Existing configurations remain supported and emit a
  validation warning; `secret.mode` takes precedence when both are configured.
  The deprecated option **will be removed in a future release**.
- Configuration reload now uses the same configuration sources and command line
  overrides as initial startup.
- Higher-priority configuration files can now explicitly restore a setting to
  its default value, such as `show_time = false` or `width = 80`.

### Fixed

- Reloading configuration now replaces removed session commands, session
  directories, wrappers, and user-menu state instead of retaining stale values.
- Reloading session directories now refreshes the session-selection menu while
  retaining the selected session when it is still available.
- Explicit `secret.mode = "hidden"` and the legacy `display.asterisks = false`
  setting now correctly override lower-priority configuration layers.
- `[general] debug` and `log_file` now configure logging during startup.
- Constrained terminal sizes and narrow layouts no longer trigger unsigned
  arithmetic overflows while rendering.
- Corrected the documented names of several configuration environment variables.
- The top infobar now respects `layout.window_padding`.
- Background animations no longer bleed into the login form.
- Available sessions are discovered from `$XDG_DATA_DIRS` again. The default
  configuration values for `sessions_dirs`/`xsessions_dirs` were causing the
  discovery to be silently skipped.

## 0.11.1

### Fixed

- Available sessions are discovered from `$XDG_DATA_DIRS` again. The default
  configuration values for `sessions_dirs`/`xsessions_dirs` were causing the
  discovery to be silently skipped. [backport]
