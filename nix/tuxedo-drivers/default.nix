{
  config,
  lib,
  ...
}:
let
  tuxedoDrivers = config.boot.kernelPackages.callPackage ./package.nix { };
in
{
  hardware.tuxedo-drivers.enable = lib.mkForce false;
  boot.extraModulePackages = [
    tuxedoDrivers
    config.boot.kernelPackages.acpi_call
  ];
  boot.kernelModules = [
    "tuxedo_keyboard"
    "uniwill_wmi"
    "tuxedo_io"
    "acpi_call"
  ];
  services.udev.packages = [ tuxedoDrivers ];
}
