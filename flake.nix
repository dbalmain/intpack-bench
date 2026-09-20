{
  description = "intpack-bench — benchmark harness for integer sequence codecs";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # fastpfor's build.rs looks in $OUT_DIR/lib, but GNUInstallDirs on
      # NixOS/Fedora installs the static lib to lib64. Inject libdir=lib on
      # cmake *configure* only (`--build` / `--install` must stay untouched).
      cmakeForCargo = pkgs: pkgs.writeShellScriptBin "cmake" ''
        set -euo pipefail
        case "''${1:-}" in
          --build|--install|-E|-P|--find-package|--list-presets|--help|-help|--version|-version)
            exec ${pkgs.cmake}/bin/cmake "$@"
            ;;
        esac
        exec ${pkgs.cmake}/bin/cmake -DCMAKE_INSTALL_LIBDIR=lib "$@"
      '';
    in {
      packages = forAll (pkgs:
        let
          mk = { features }: pkgs.rustPlatform.buildRustPackage {
            pname = "intpack-bench";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            # Codecs are compared on this machine's ISA; a portable build would
            # hobble the SIMD ones and measure the wrong thing.
            RUSTFLAGS = "-C target-cpu=native";
            doCheck = false;
            buildFeatures = features;
            # cmake is only required for `--features cpp` (the `fastpfor` crate's
            # build.rs). The wrapper is not the cmake setup hook, so it will not
            # try to cmake-configure this Cargo project.
            nativeBuildInputs = [ (cmakeForCargo pkgs) pkgs.stdenv.cc ];
            dontUseCmakeConfigure = true;
            # The gcc wrapper drops `-march=native` unless this is 0. FastPFor's
            # `cpp_native` feature relies on that flag for SSSE3/SSE4.2.
            NIX_ENFORCE_NO_NATIVE = "0";
          };
        in rec {
          intpack-bench = mk { features = []; };
          cpp = mk { features = [ "cpp" ]; };
          default = intpack-bench;
        });

      apps = forAll (pkgs:
        let
          bin = "${self.packages.${pkgs.system}.default}/bin/intpack-bench";
          # gen → bench → report, pinned to one core, into results/<hostname>/.
          run-all = pkgs.writeShellApplication {
            name = "intpack-bench-all";
            runtimeInputs = [ pkgs.util-linux ];
            text = ''
              set -euo pipefail
              scale="''${SCALE:-1}"
              budget="''${BUDGET:-2}"
              ${bin} gen --out data/synthetic --scale "$scale"
              pin=""
              if command -v taskset >/dev/null; then pin="taskset -c ''${CORE:-2}"; fi
              # shellcheck disable=SC2086
              $pin ${bin} bench --data data --budget "$budget" "$@"
            '';
          };
        in {
          default = { type = "app"; program = bin; };
          all = { type = "app"; program = "${run-all}/bin/intpack-bench-all"; };
          cpp = { type = "app"; program = "${self.packages.${pkgs.system}.cpp}/bin/intpack-bench"; };
        });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc clippy rustfmt rust-analyzer util-linux ];
          nativeBuildInputs = [ (cmakeForCargo pkgs) pkgs.stdenv.cc ];
          RUSTFLAGS = "-C target-cpu=native";
          # See packages: FastPFor's cmake `-march=native` must survive the wrapper.
          NIX_ENFORCE_NO_NATIVE = "0";
        };
      });
    };
}
