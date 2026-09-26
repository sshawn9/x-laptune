{
  description = "x-laptune: Linux laptop hardware monitoring and control";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      package = pkgs.callPackage ./nix/x-laptune/package.nix { };
      mkApp = binary: description: {
        type = "app";
        meta.description = description;
        program = "${package}/bin/${binary}";
      };
    in {
      nixosModules.default = import ./nix;

      packages.${system}.default = package;
      apps.${system} = rec {
        default = x-laptune;
        x-laptune = mkApp "x-laptune" "Monitor and control laptop hardware";
        memory-thermal-control = mkApp "memory-thermal-control" "Control CPU throttling from memory temperature";
      };
      checks.${system}.package = self.packages.${system}.default;
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [ cargo rustc rustfmt clippy ];
      };
    };
}
