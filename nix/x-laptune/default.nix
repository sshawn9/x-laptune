{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.x-laptune;
in
{
  options.programs.x-laptune = {
    enable = lib.mkEnableOption "the x-laptune command-line tool";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./package.nix { }";
      description = "The package shared by the x-laptune CLI and its services.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
