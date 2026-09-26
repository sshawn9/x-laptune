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
  imports = [ ../x-laptune ];

  options.services.x-laptune.batteryControl = {
    enable = lib.mkEnableOption "the x-laptune battery charging profile service at boot";

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

  config = lib.mkIf cfg.enable {
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
