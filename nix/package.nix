{
  lib,
  stdenv,
  craneLib,
  installShellFiles,
  versionCheckHook,
}: let
  commonArgs = {
    pname = "tuigreet";
    version = (lib.importTOML ../Cargo.toml).workspace.package.version;
    src = let
      s = ../.;
      fs = lib.fileset;
    in
      fs.toSource {
        root = s;
        fileset = fs.unions [
          (s + /contrib)
          (s + /crates)
          (s + /xtask)
          (s + /Cargo.lock)
          (s + /Cargo.toml)
        ];
      };
    strictDeps = true;
  };

  cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {name = "tuigreet-deps";});
in
  craneLib.buildPackage (commonArgs
    // {
      inherit cargoArtifacts;
      useNextest = true;

      nativeInstallCheckInputs = [versionCheckHook];
      doInstallCheck = true;

      nativeBuildInputs = [installShellFiles];

      postInstall =
        (lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
          $out/bin/xtask man --out-dir target/xtask/man
          $out/bin/xtask comp --out-dir target/xtask/completions

          installManPage target/xtask/man/tuigreet.1
          installShellCompletion --cmd tuigreet \
            --bash target/xtask/completions/tuigreet.bash \
            --fish target/xtask/completions/tuigreet.fish \
            --zsh target/xtask/completions/_tuigreet
        '')
        + ''
          rm "$out/bin/xtask"
        '';

      meta = {
        description = "Stylish graphical console greeter for greetd";
        license = lib.licenses.gpl3Only;
        maintainers = with lib.maintainers; [NotAShelf];
        mainProgram = "tuigreet";
      };
    })
