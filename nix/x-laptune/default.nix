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
    enable = lib.mkEnableOption "x-laptune and its hardware features and services";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./package.nix { }";
      description = "The package shared by the x-laptune CLI and its services.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
    boot.extraModulePackages = [ config.boot.kernelPackages.acpi_call ];
    boot.kernelModules = [
      "spd5118"
      "intel_powerclamp"
      "acpi_call"
    ];
    systemd.tmpfiles.rules = [
      "d /etc/x-laptune 0755 root root -"
      "C /etc/x-laptune/fan-policy.json 0644 root root - ${cfg.package}/share/x-laptune/fan-policy.json"
    ];
  };
}
