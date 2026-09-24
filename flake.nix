{
  description = "loginsicompass, the accessible greetd login screen for sicompass";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Splits the build into a dependency derivation keyed on Cargo.lock alone
    # and the crate on top. Vendors git dependencies (sicompass-ui) by the rev
    # recorded in Cargo.lock, so there is no hash to keep up to date.
    crane.url = "github:ipetkov/crane";

    # For its dev shell only. The greeter needs exactly the renderer's
    # environment (SDL3, Vulkan, the Mesa vendor handling, AccessKit), and that
    # shell hook is long and subtle enough that a third copy would drift. The
    # crate itself comes in through Cargo.lock, not through this input.
    sicompass-ui = {
      url = "github:friendlyflow/sicompass-ui";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.crane.follows = "crane";
    };
  };

  outputs = { self, nixpkgs, crane, sicompass-ui }:
    let
      # Linux only: greetd, Wayland and /etc/passwd have no counterpart
      # elsewhere, and on other platforms the binary is an empty `main()`.
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (system: import nixpkgs { inherit system; });

      # Single source of truth for the version.
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
    in
    {
      devShells = forAllSystems (system:
        let pkgs = nixpkgsFor.${system}; in
        {
          default = sicompass-ui.devShells.${system}.default.overrideAttrs (old: {
            buildInputs = old.buildInputs ++ (with pkgs; [
              # The screen reader the greeter's accessibility toggle starts.
              orca
              # Test clients for a nested run inside desicompass.
              wayland-utils
              foot
            ]);
          });
        });

      packages = forAllSystems (system:
        let
          pkgs = nixpkgsFor.${system};
          craneLib = crane.mkLib pkgs;
          lib = pkgs.lib;
          commonArgs = {
            inherit version;
            pname = "loginsicompass";
            # crane's default filter keeps Cargo and .rs files only. The font
            # license texts are installed below and read by tests/packaging.rs.
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                (craneLib.fileset.commonCargoSources ./.)
                ./fonts
                ./THIRD-PARTY-LICENSES.html
              ];
            };
            strictDeps = true;
            cargoExtraArgs = "--locked";
            # tests/greeter_ui.rs drives the real renderer; ci.yml runs the
            # suite, where a display can be arranged.
            doCheck = false;

            # This used to say "it draws with tiny-skia into shared memory, so
            # it needs no GPU stack at all". That stopped being true when the
            # greeter grew a real login screen: it links `sicompass-ui`, the
            # app's own SDL3/Vulkan renderer, so that the login screen speaks
            # to a screen reader and renders text at all. The tiny-skia box is
            # still in there as `--render-backend shm`, reached only when the
            # Vulkan path cannot start.
            #
            # What it deliberately does NOT link is the `sicompass`
            # application crate, which would drag wasmtime, a bundled SQLite,
            # a headless-Chromium driver and an IMAP/SMTP stack into a login
            # screen. See sicompass-ui's Cargo.toml.
            nativeBuildInputs = with pkgs; [
              pkg-config
              # bindgen is in the graph (freetype-sys, sdl3-sys).
              rustPlatform.bindgenHook
            ];

            buildInputs = with pkgs; [
              # System SDL3: inside a Nix build there is no reason to compile a
              # vendored copy.
              sdl3
              freetype
              libwebp
              libxkbcommon
              wayland
              # accesskit_unix speaks AT-SPI2 over D-Bus. A greeter that
              # cannot reach it still renders; it is simply mute, which for
              # this application is the failure the whole project exists to
              # prevent.
              at-spi2-core
              dbus
              # The GL/GBM dispatch libraries, same pair as desicompass: the
              # loader goes on LD_LIBRARY_PATH below, the vendor comes from
              # /run/opengl-driver.
              libGL
              libgbm
              libdrm
            ];
          };
        in
        rec {
          loginsicompass = craneLib.buildPackage (commonArgs // {
            cargoArtifacts = craneLib.buildDepsOnly commonArgs;
            nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.makeWrapper ];

            # Same shape as desicompass's wrapper, and for the same reasons.
            #
            # vulkan-loader on LD_LIBRARY_PATH is what lets `ash::Entry::load()`
            # dlopen libvulkan.so.1. It is not in the binary's DT_NEEDED, so a
            # missing loader is a startup failure rather than a link error.
            # Deliberately no VK_ICD_FILENAMES: on NixOS the drivers live in
            # /run/opengl-driver and the loader finds them itself, and pinning a
            # path that does not exist makes it report zero ICDs.
            #
            # The three vendor variables are `--set-default` rather than
            # `--set`, so a non-NixOS host or a deliberate driver test still
            # wins. nixpkgs' own libgbm next to a system EGL of a different
            # version segfaulted inside libEGL_mesa on the GBM path, which is
            # why desicompass points at the system one. The greeter shares a
            # display with it, so it points at the same.
            postInstall = ''
              wrapProgram $out/bin/loginsicompass \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (with pkgs; [
                  vulkan-loader
                  sdl3
                  libGL
                  libgbm
                  libxkbcommon
                  wayland
                ])}" \
                --set-default __EGL_VENDOR_LIBRARY_DIRS /run/opengl-driver/share/glvnd/egl_vendor.d \
                --set-default LIBGL_DRIVERS_PATH        /run/opengl-driver/lib/dri \
                --set-default GBM_BACKENDS_PATH         /run/opengl-driver/lib/gbm

              # The fonts are inside the binary (through sicompass-ui), so their
              # licenses travel with it.
              install -Dm644 fonts/LICENSE-DejaVu.txt \
                $out/share/doc/loginsicompass/LICENSE-DejaVu.txt
              install -Dm644 fonts/LICENSE-NotoColorEmoji.txt \
                $out/share/doc/loginsicompass/LICENSE-NotoColorEmoji.txt
              install -Dm644 THIRD-PARTY-LICENSES.html \
                $out/share/doc/loginsicompass/THIRD-PARTY-LICENSES.html
            '';

            meta = with pkgs.lib; {
              description = "The accessible greetd login screen for sicompass";
              homepage = "https://github.com/friendlyflow/loginsicompass";
              license = licenses.gpl3Only;
              mainProgram = "loginsicompass";
              platforms = platforms.linux;
            };
          });
          default = loginsicompass;
        });
    };
}
