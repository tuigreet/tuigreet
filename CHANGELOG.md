# tuigreet Changelog

## 0.12.0

### Added

- Suspend and hibernate actions are now available from the power menu. They use
  `systemctl suspend` and `systemctl hibernate` on systemd systems, or the
  corresponding `loginctl` commands on elogind systems. The greeter remains
  available after resuming.
- `[power] suspend` and `[power] hibernate` configuration keys, plus the
  `--power-suspend` and `--power-hibernate` command-line options, allow
  overriding those actions.
- Runtime shell-completion requests are now supported. `cargo xtask comp` also
  generates shell completion scripts for packagers.
- `--default-session` accepts a regular expression matching the session path;
  the same setting is available as `[session] default_session` and
  `TUIGREET_DEFAULT_SESSION`.
- Num Lock can be enabled at startup with `--numlock`, `[general] numlock`, or
  `TUIGREET_NUMLOCK`.

### Changed

- `display.asterisks` is deprecated. Use `[secret] mode = "characters"` or
  `mode = "hidden"` instead. Existing configurations remain supported and emit a
  validation warning; `secret.mode` takes precedence when both are configured.
  The deprecated option **will be removed in a future release**.
- Configuration reload now uses the same configuration sources and command line
  overrides as initial startup.
- Higher-priority configuration files can now explicitly restore a setting to
  its default value, such as `show_time = false` or `width = 80`.
- Boolean command-line options now accept `--no-*` forms to disable values set
  by configuration, such as `--no-time`.
- Numeric command-line options now reject values outside their supported ranges,
  including layout and background-animation settings.
- Configuration validation now rejects malformed `[session] environments`
  entries and conflicting `remember.session` / `remember.user_session` settings.
- The default user-menu UID range now follows `UID_MIN` and `UID_MAX` from
  `/etc/login.defs` unless overridden in configuration.

### Fixed

- Reloading configuration now replaces removed session commands, session
  directories, wrappers, and user-menu state instead of retaining stale values.
- Reloading session directories now refreshes the session-selection menu while
  retaining the selected session when it is still available.
- Unrelated configuration reloads no longer discard the desktop session to
  launch.
- Password reveal mode now tracks the editing cursor even when masking is
  hidden.
- Modified `f` character bindings, such as `Ctrl+f`, are accepted correctly.
- Idle timeouts that cannot fit the system clock are rejected during validation.
- Terminal state is restored when initialization or terminal input fails.
- Packaged builds now load the same embedded localization catalogs as source
  builds.
- Explicit `secret.mode = "hidden"` and the legacy `display.asterisks = false`
  setting now correctly override lower-priority configuration layers.
- `[general] debug` and `log_file` now configure logging during startup.
- Failed automatic authentication responses now return to the username prompt
  instead of starting another authentication attempt.
- Retrying after a rejected password now reuses the greetd connection instead of
  racing a replacement connection against daemon-side session cleanup.
- When a remembered user is enabled alongside a one-user menu, tuigreet no
  longer submits duplicate session-creation requests.
- Unicode input editing remains correct for long buffers beyond the 16-bit
  cursor range.
- Empty secret-display character pools are rejected with an error instead of
  causing a rendering panic.
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
