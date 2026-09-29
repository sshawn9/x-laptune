{ lib, rustPlatform }:
let
  manifest = builtins.fromTOML (builtins.readFile ../../Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = manifest.package.name;
  version = manifest.package.version;
  src = lib.fileset.toSource {
    root = ../../.;
    fileset = lib.fileset.unions [ ../../Cargo.toml ../../Cargo.lock ../../src ];
  };
  cargoLock.lockFile = ../../Cargo.lock;
  doCheck = false;
  postInstall = ''
    install -Dm644 src/tuxedo/fan/policies/baseline.json \
      "$out/share/x-laptune/fan-policy.json"
  '';
  meta = {
    description = manifest.package.description;
    mainProgram = "x-laptune";
    platforms = [ "x86_64-linux" ];
  };
}
