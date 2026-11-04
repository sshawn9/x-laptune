{
  config,
  lib,
  ...
}:
let
  cfg = config.services.x-laptune.batteryControl;
  xLaptune = lib.getExe' config.programs.x-laptune.package "x-laptune";
in
{
  imports = [
    ../x-laptune
    ../tuxedo-drivers
  ];

  options.services.x-laptune.batteryControl = {
    enable = lib.mkEnableOption "the x-laptune battery charging profile service at boot (created only when programs.x-laptune.enable and programs.x-laptune.tuxedo.enable are both enabled)";

    profile = lib.mkOption {
      type = lib.types.enum [
        "high_capacity"
        "balanced"
        "stationary"
      ];
      default = "stationary";
      description = "The battery charging profile to apply at boot.";
    };
  };

  config =
    lib.mkIf (config.programs.x-laptune.enable && cfg.enable && config.programs.x-laptune.tuxedo.enable)
      {
        systemd.services.x-laptune-battery-profile-init = {
          description = "Set x-laptune battery charging profile at boot";
          wantedBy = [ "multi-user.target" ];
          after = [ "systemd-modules-load.service" ];
          serviceConfig = {
            Type = "oneshot";
            User = "root";
            ExecStart = "${xLaptune} battery ${cfg.profile}";
          };
        };
      };
}
