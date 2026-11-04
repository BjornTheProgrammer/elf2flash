{
  description = "elf2flash - convert and flash ELF files to microcontrollers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed (
          system: f nixpkgs.legacyPackages.${system}
        );

      # libusb1-sys's build script calls `sw_vers` on macOS
      nativeBuildInputs =
        pkgs:
        [ pkgs.pkg-config ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.darwin.DarwinTools ];
      buildInputs =
        pkgs: [ pkgs.libusb1 ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.udev ];
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "elf2flash";
          version = (fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeBuildInputs pkgs;
          buildInputs = buildInputs pkgs;

          meta = {
            description = "A tool to help flash ELF files to microcontrollers";
            license = with pkgs.lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "elf2flash";
          };
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });
    };
}
