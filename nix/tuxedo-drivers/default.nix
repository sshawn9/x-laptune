{
  config,
  lib,
  ...
}:
let
  cfg = config.programs.x-laptune.tuxedo;
  tuxedoDrivers = config.boot.kernelPackages.callPackage ./package.nix { };
in
{
  imports = [ ../x-laptune ];

  options.programs.x-laptune.tuxedo = {
    enable = lib.mkEnableOption "the project's TUXEDO driver and udev rules";
  };

  config = lib.mkIf (config.programs.x-laptune.enable && cfg.enable) {
    hardware.tuxedo-drivers.enable = lib.mkForce false;
    boot.extraModulePackages = [ tuxedoDrivers ];
    boot.kernelModules = [
      "tuxedo_keyboard"
      "uniwill_wmi"
      "tuxedo_io"
    ];
    services.udev.packages = [ tuxedoDrivers ];
  };
}
