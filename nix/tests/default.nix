{
  lib,
  testers,
  tuigreet-pkg,
  sway,
  kbd,
  coreutils,
  writeText,
}: let
  swayConfig = writeText "tuigreet-test-sway.conf" ''
    output * bg #000000 solid_color
  '';
in
  testers.nixosTest {
    name = "tuigreet";
    meta.maintainers = with lib.maintainers; [NotAShelf];

    nodes.machine = {
      users.users.alice = {
        isNormalUser = true;
        description = "Test User";
        password = "test123";
      };

      programs.sway.enable = true;
      virtualisation.qemu.options = ["-vga none -device virtio-gpu-pci"];

      services.greetd = {
        enable = true;
        settings = {
          terminal.vt = 1;
          default_session = {
            command = "${tuigreet-pkg}/bin/tuigreet --config /etc/tuigreet/config.toml --numlock --time --remember --remember-user-session";
            user = "greeter";
          };
        };
      };

      environment = {
        systemPackages = [tuigreet-pkg sway kbd];
        variables = {
          WLR_RENDERER = "pixman";
          SWAYSOCK = "/tmp/tuigreet-sway.sock";
        };
        etc."tuigreet/config.toml".text = ''
          [general]
          debug = true
          log_file = "/tmp/tuigreet.log"

          [session]
          sessions_dirs = ["/etc/wayland-sessions"]
          xsessions_dirs = []

          [display]
          greeting = "Welcome to tuigreet!"
        '';
        etc."wayland-sessions/sway.desktop".text = ''
          [Desktop Entry]
          Name=Sway
          Exec=${coreutils}/bin/env WLR_RENDERER=pixman SWAYSOCK=/tmp/tuigreet-sway.sock ${sway}/bin/sway --config ${swayConfig}
          Type=Application
          DesktopNames=sway
        '';
      };

      systemd.tmpfiles.rules = [
        "d /var/cache/tuigreet 0755 greeter greeter -"
      ];
    };

    testScript = ''
      import datetime as dt
      import re

      def wait_for_prompt(text: str, tty: str = "1") -> None:
          try:
              machine.wait_until_tty_matches(tty, text, timeout=dt.timedelta(seconds=45))
          except Exception:
              print(machine.succeed("journalctl -u greetd --no-pager -n 40"))
              print(machine.succeed("cat /tmp/tuigreet.log"))
              raise

      machine.wait_for_unit("greetd.service")
      wait_for_prompt("Username:")
      machine.screenshot("greeter-startup")

      with subtest("installed package and Linux-console Num Lock"):
          machine.succeed("tuigreet --version | grep -F 'tuigreet ${tuigreet-pkg.version}'")
          for asset in [
              "share/man/man1/tuigreet.1.gz",
              "share/bash-completion/completions/tuigreet.bash",
              "share/fish/vendor_completions.d/tuigreet.fish",
              "share/zsh/site-functions/_tuigreet",
          ]:
              machine.succeed(f"test -s ${tuigreet-pkg}/{asset}")
          leds = machine.succeed("setleds -F < /dev/tty1")
          assert re.search(r"NumLock\s+on", leds), leds
          machine.succeed("tuigreet --list-outputs")

      with subtest("terminal sizing applies during startup"):
          machine.succeed(
              "printf '[terminal]\\ncols = 60\\nrows = 20\\n' > /tmp/test-terminal.toml"
          )
          machine.succeed(
              "systemd-run --unit=tuigreet-terminal-probe "
              "--property=TTYPath=/dev/tty8 --property=StandardInput=tty "
              "--property=StandardOutput=tty --property=StandardError=journal "
              "${tuigreet-pkg}/bin/tuigreet --mock --config /tmp/test-terminal.toml"
          )
          wait_for_prompt("Username:", "8")
          size = machine.succeed("stty -F /dev/tty8 size").strip()
          assert size == "20 60", f"Unexpected configured console size: {size}"
          machine.succeed("systemctl stop tuigreet-terminal-probe.service")

      with subtest("real PAM rejection and cancellation"):
          machine.send_chars("alice\n")
          wait_for_prompt("Password:")
          machine.send_chars("incorrect\n")
          wait_for_prompt("Authentication failed")
          wait_for_prompt("Password:")
          machine.send_key("esc")
          wait_for_prompt("Username:")

      with subtest("hot reload preserves a discovered desktop session"):
          machine.succeed(
              "sed 's/Welcome to tuigreet!/Reloaded greeting/' /etc/tuigreet/config.toml "
              "> /tmp/reloaded.toml; "
              "cp --remove-destination /tmp/reloaded.toml /etc/tuigreet/config.toml"
          )
          wait_for_prompt("Reloaded greeting")
          machine.send_chars("alice\n")
          wait_for_prompt("Password:")
          machine.send_chars("test123\n")
          machine.wait_for_file("/tmp/tuigreet-sway.sock")
          machine.succeed("pgrep -u alice -x sway")
          machine.succeed("test \"$(cat /var/cache/tuigreet/lastuser)\" = alice")
          machine.succeed(
              "test \"$(cat /var/cache/tuigreet/lastsession-path-alice)\" "
              "= /etc/wayland-sessions/sway.desktop"
          )
          machine.screenshot("authenticated-sway-session")

      with subtest("remembered user can authenticate again"):
          machine.succeed("su - alice -c 'SWAYSOCK=/tmp/tuigreet-sway.sock swaymsg exit'")
          wait_for_prompt("Password:")
          machine.send_chars("test123\n")
          machine.wait_until_succeeds("pgrep -u alice -x sway")
          machine.succeed("! grep -F test123 /tmp/tuigreet.log")
    '';
  }
