# tuigreet

Graphical console greeter for [greetd](https://git.sr.ht/~kennylevinsen/greetd).

![Screenshot of tuigreet](https://github.com/apognu/tuigreet/blob/master/contrib/screenshot.png)

```
Usage: tuigreet [OPTIONS]

Options:
    -h, --help          show this usage information
    -v, --version       print version information
    -d, --debug [FILE]  enable debug logging to the provided file, or to
                        /tmp/tuigreet.log
    -c, --cmd COMMAND   command to run
        --env KEY=VALUE environment variables to run the default session with
                        (can appear more than once)
    -s, --sessions DIRS colon-separated list of Wayland session paths
        --session-wrapper 'CMD [ARGS]...'
                        wrapper command to initialize the non-X11 session
    -x, --xsessions DIRS
                        colon-separated list of X11 session paths
        --xsession-wrapper 'CMD [ARGS]...'
                        wrapper command to initialize X server and launch X11
                        sessions (default: startx /usr/bin/env)
        --no-xsession-wrapper
                        do not wrap commands for X11 sessions
    -w, --width WIDTH   width of the main prompt (default: 80)
    -i, --issue         show the host's issue file
    -g, --greeting GREETING
                        show custom text above login prompt
    -t, --time          display the current date and time
        --time-format FORMAT
                        custom strftime format for displaying date and time
    -r, --remember      remember last logged-in username
        --remember-session
                        remember last selected session
        --remember-user-session
                        remember last selected session for each user
        --user-menu     allow graphical selection of users from a menu
        --user-menu-min-uid UID
                        minimum UID to display in the user selection menu
        --user-menu-max-uid UID
                        maximum UID to display in the user selection menu
        --theme THEME   define the application theme colors
        --asterisks     display asterisks when a secret is typed
        --asterisks-char CHARS
                        characters to be used to redact secrets (default: *)
        --window-padding PADDING
                        padding inside the terminal area (default: 0)
        --container-padding PADDING
                        padding inside the main prompt container (default: 1)
        --prompt-padding PADDING
                        padding between prompt rows (default: 1)
        --greet-align [left|center|right]
                        alignment of the greeting text in the main prompt
                        container (default: 'center')
        --power-shutdown 'CMD [ARGS]...'
                        command to run to shut down the system
        --power-reboot 'CMD [ARGS]...'
                        command to run to reboot the system
        --power-no-setsid
                        do not prefix power commands with setsid
        --kb-command [1-12]
                        F-key to use to open the command menu
        --kb-sessions [1-12]
                        F-key to use to open the sessions menu
        --kb-power [1-12]
                        F-key to use to open the power menu
```

## Usage

The default configuration tends to be as minimal as possible, visually speaking, only showing the authentication prompts and some minor information in the status bar. You may print your system's `/etc/issue` at the top of the prompt with `--issue` and the current date and time with `--time` (and possibly customize it with `--time-format`). You may include a custom one-line greeting message instead of `/etc/issue` with `--greeting`.

The initial prompt container will be 80 column wide. You may change this with `--width` in case you need more space (for example, to account for large PAM challenge messages). Please refer to usage information (`--help`) for more customization options. Various padding settings are available through the `*-padding` options.

You can instruct `tuigreet` to remember the last username that successfully opened a session with the `--remember` option (that way, the username field will be pre-filled). Similarly, the command and session configuration can be retained between runs with the `--remember-session` option (when using this, the `--cmd` value is overridden by manual selections). You can also remember the selected session per user with the `--remember-user-session` flag. In this case, the selected session will only be saved on successful authentication. Check the [cache instructions](#cache-instructions) if `/var/cache/tuigreet` doesn't exist after installing tuigreet.

You may change the command that will be executed after opening a session by hitting `F2` and amending the command. Alternatively, you can list the system-declared sessions (or custom ones) by hitting `F3`. Power options are available through `F12`.

## Install

### From source

Building from source requires an installation of Rust's `stable` toolchain, including `cargo`.

```
$ git clone https://github.com/apognu/tuigreet && cd tuigreet
$ cargo build --release
# mv target/release/tuigreet /usr/local/bin/tuigreet
```

<a id="cache-instructions"></a>
Cache directory must be created for `--remember*` features to work. The directory must be owned by the user running the greeter.

```
# mkdir /var/cache/tuigreet
# chown greeter:greeter /var/cache/tuigreet
# chmod 0755 /var/cache/tuigreet
```

### From Arch Linux

On ArchLinux, `tuigreet` is available from the [extra](https://archlinux.org/packages/extra/x86_64/greetd-tuigreet/) repo and is installable through pacman:

```
$ pacman -S greetd-tuigreet
```

Two more distributions are available from the [AUR](https://aur.archlinux.org/packages?O=0&K=tuigreet): `greetd-tuigreet-bin` is the precompiled release for the latest tagged release of `tuigreet` and `greetd-tuigreet-git` is a rolling release always following the `master` branch of this repository.
Those can be installed via your preferred AUR helper.

### From Gentoo

On Gentoo, `tuigreet` is available as a package `gui-apps/tuigreet`:

```
$ emerge --ask --verbose gui-apps/tuigreet
```

### From NixOS

On NixOS `greetd` and `tuigreet` both available via `<nixpkgs>` main repository.
Please refer to the snippet below for the minimal `tuigreet` configuration:

```nix
{ pkgs, ... }:
{
  services.greetd = {
    enable = true;
    settings = {
      default_session = {
        command = "${pkgs.greetd.tuigreet}/bin/tuigreet --time --cmd sway";
        user = "greeter";
      };
    };
  };
}
```

[More details](https://search.nixos.org/options?channel=unstable&show=services.greetd.settings&from=0&size=50&sort=relevance&query=greetd)

### Pre-built binaries

Pre-built binaries of `tuigreet` for several architectures can be found in the [releases](https://github.com/apognu/tuigreet/releases) section of this repository. The [tip prerelease](https://github.com/apognu/tuigreet/releases/tag/tip) is continuously built and kept in sync with the `master` branch.

## Running the tests

Tests from the default features should run without any special consideration by running `cargo test`.

If you intend to run the whole test suite, you will need to perform some setup. One of our features uses NSS to list and filter existing users on the system, and in order not to rely on actual users being created on the host, we use [libnss_wrapper](https://cwrap.org/nss_wrapper.html) to mock responses from NSS. Without this, the tests would use the real user list from your system and probably fail because it cannot find the one it looks for.

After installing `libnss_wrapper` on your system (or compiling it to get the `.so`), you can run those specific tests as such:

```
$ export NSS_WRAPPER_PASSWD=contrib/fixtures/passwd
$ export NSS_WRAPPER_GROUP=contrib/fixtures/group
$ LD_PRELOAD=/path/to/libnss_wrapper.so cargo test --features nsswrapper nsswrapper_ # To run those tests specifically
$ LD_PRELOAD=/path/to/libnss_wrapper.so cargo test --all-features # To run the whole test suite
```

## Configuration

Edit `/etc/greetd/config.toml` and set the `command` setting to use `tuigreet`:

```
[terminal]
vt = 1

[default_session]
command = "tuigreet --cmd sway"
user = "greeter"
```

Please refer to [greetd's wiki](https://man.sr.ht/~kennylevinsen/greetd/) for more information on setting up `greetd`.

### Sessions

The available sessions are fetched from `desktop` files in `/usr/share/xsessions` and `/usr/share/wayland-sessions`. If you want to provide custom directories, you can set the `--sessions` arguments with a colon-separated list of directories for `tuigreet` to fetch session definitions some other place.

#### Desktop environments

`greetd` only accepts environment-less commands to be used to start a session. Therefore, if your desktop environment requires either arguments or environment variables, you will need to create a wrapper script and refer to it in an appropriate desktop file.

For example, to run X11 Gnome, you may need to start it through `startx` and configure your `~/.xinitrc` (or an external `xinitrc` with a wrapper script):

```
exec gnome-session
```

To run Wayland Gnome, you would need to create a wrapper script akin to the following:

```
XDG_SESSION_TYPE=wayland dbus-run-session gnome-session
```

Then refer to your wrapper script in a custom desktop file (in a directory declared with the `-s/--sessions` option):

```
Name=Wayland Gnome
Exec=/path/to/my/wrapper.sh
```

#### Common wrappers

Two options allows you to automatically wrap run commands around sessions started from desktop files, depending on whether they come `/usr/share/wayland-sessions` or `/usr/share/xsessions`: `--sessions-wrapper` and `--xsessions-wrapper`. With this, you can prepend another command on front of the sessions you run to set up the required environment to run these kinds of sessions.

By default, unless you change it, all X11 sessions (those picked up from `/usr/share/xsessions`) are prepended with `startx /usr/bin/env`, so the X11 server is started properly.

### Power management

Two power actions are possible from `tuigreet`, shutting down (through `shutdown -h now`) and rebooting (with `shutdown -r now`) the machine. This requires that those commands be executable by regular users, which is not the case on some distros.

To alleviate this, there are two options that can be used to customize the commands that are run: `--power-shutdown` and `--power-reboot`. The provided commands must be non-interactive, meaning they will not be able to print anything or prompt for anything. If you need to use `sudo` or `doas`, they will need to be configured to run passwordless for those specific commands.

An example for `/etc/greetd/config.toml`:

```
[default_session]
command = "tuigreet --power-shutdown 'sudo systemctl poweroff'"
```

Note that, by default, all commands are prefixed with `setsid` to completely detach the command from our TTY. If you would prefer to run the commands as is, or if `setsid` does not exist on your system, you can use `--power-no-setsid`.

### User menu

Optionally, a user can be selected from a menu instead of typing out their name, with the `--user-menu` option, this will present all users returned by NSS at the time `tuigreet` was run, with a UID within the acceptable range. The values for the minimum and maximum UIDs are selected as follows, for each value:

 * A user-provided value, through `--user-menu-min-uid` or `--user-menu-max-uid`;
 * **Or**, the available values for `UID_MIN` or `UID_MAX` from `/etc/login.defs`;
 * **Or**, hardcoded `1000` for minimum UID and `60000` for maximum UID.

### Theming

A theme specification can be given through the `--theme` argument to control some of the colors used to draw the UI. This specification string must have the following format: `component1=color;component2=color[;...]` where the component is one of the value listed in the table below, and the color is a valid ANSI color name as listed [here](https://github.com/ratatui/ratatui/blob/c7c34980254e4ffe6416cb8e20ba2c49300948a3/ratatui-core/src/style/color.rs#L15).

Mind that the specification string include semicolons, which are command delimiters in most shells, hence, you should enclose it in single-quotes so it is considered a single argument instead.

Please note that we can only render colors as supported by the running terminal. In the case of the Linux virtual console, those colors might not look as good as one may think. Your mileage may vary.

| Component name | Description                                                                        |
| -------------- | ---------------------------------------------------------------------------------- |
| text           | Base text color other than those specified below                                   |
| time           | Color of the date and time. If unspecified, falls back to `text`                   |
| container      | Background color for the centered containers used throughout the app               |
| border         | Color of the borders of those containers                                           |
| title          | Color of the containers' titles. If unspecified, falls back to `border`            |
| greet          | Color of the issue of greeting message. If unspecified, falls back to `text`       |
| prompt         | Color of the prompt ("Username:", etc.)                                            |
| input          | Color of user input feedback                                                       |
| action         | Color of the actions displayed at the bottom of the screen                         |
| button         | Color of the keybindings for those actions. If unspecified, falls back to `action` |

Below is a screenshot of the greeter with the following theme applied: `border=magenta;text=cyan;prompt=green;time=red;action=blue;button=yellow;container=black;input=red`:

![Screenshot of tuigreet](https://github.com/apognu/tuigreet/blob/master/contrib/screenshot-themed.png)
