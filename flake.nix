{
  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.zst";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forEachSystem = nixpkgs.lib.genAttrs systems;
    pkgsForEach = system: nixpkgs.legacyPackages.${system}.extend rust-overlay.overlays.default;

    mkCraneLib = pkgs:
      (crane.mkLib pkgs).overrideToolchain (p:
        # Build tools
          (p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            extensions = ["rustfmt" "rust-analyzer" "clippy"];
            targets = [];
          });
  in {
    packages = forEachSystem (system: let
      pkgs = pkgsForEach system;
      craneLib = mkCraneLib pkgs;
    in {
      tuigreet = pkgs.callPackage ./nix/package.nix {inherit craneLib;};
      default = self.packages.${system}.tuigreet;
    });

    devShells = forEachSystem (system: let
      pkgs = pkgsForEach system;
      craneLib = mkCraneLib pkgs;
    in {
      default = pkgs.callPackage ./nix/shell.nix {inherit self craneLib;};
    });

    checks = forEachSystem (system: let
      pkgs = pkgsForEach system;
      tuigreet-pkg = self.packages.${system}.tuigreet;
    in {
      tuigreet-test = pkgs.callPackage ./nix/tests/default.nix {
        inherit tuigreet-pkg;
      };
    });

    formatter = forEachSystem (system: let
      pkgs = pkgsForEach system;
    in
      pkgs.writeShellApplication {
        name = "nix3-fmt-wrapper";

        runtimeInputs = [
          pkgs.alejandra
          pkgs.fd
          pkgs.prettier
          pkgs.deno
          pkgs.tombi
        ];

        text = ''
          # Format Nix with Alejandra
          fd "$@" -t f -e nix -x alejandra -q '{}'

          # Format TOML with Tombi
          fd "$@" -t f -e toml -x tombi format '{}'

          # Format CSS with Prettier
           fd "$@" -t f -e css -x prettier --write '{}'
        '';
      });

    hydraJobs = self.packages;
  };
}
