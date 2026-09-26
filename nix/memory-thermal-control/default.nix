{
  config,
  lib,
  ...
}:
let
  cfg = config.services.x-laptune.memoryThermalControl;
in
{
  imports = [ ../x-laptune ];

  options.services.x-laptune.memoryThermalControl = {
    enable = lib.mkEnableOption "x-laptune memory thermal control";

    allowedUsers = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "alice" ];
      description = "Users allowed to perform all systemd manage-units operations on the memory thermal control service without a password.";
    };

    intervalMs = lib.mkOption {
      type = lib.types.ints.positive;
      default = 2000;
      description = "The memory temperature sampling interval in milliseconds.";
    };
  };

  config = lib.mkIf cfg.enable {
    boot.kernelModules = [
      "spd5118"
      "intel_powerclamp"
    ];
    boot.extraModprobeConfig = ''
      options intel_powerclamp max_idle=75
    '';

    security.polkit = lib.mkIf (cfg.allowedUsers != [ ]) {
      enable = true;
      extraConfig = ''
        polkit.addRule(function(action, subject) {
          if (action.id === "org.freedesktop.systemd1.manage-units" &&
              action.lookup("unit") === "x-laptune-memory-thermal-control.service" &&
              ${builtins.toJSON cfg.allowedUsers}.indexOf(subject.user) !== -1) {
            return polkit.Result.YES;
          }
        });
      '';
    };

    systemd.services.x-laptune-memory-thermal-control = {
      description = "x-laptune memory temperature control";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-modules-load.service" ];

      serviceConfig = {
        Type = "exec";
        User = "root";
        ExecStart = "${lib.getExe' config.programs.x-laptune.package "memory-thermal-control"} --interval-ms ${toString cfg.intervalMs}";
        Restart = "on-failure";
        RestartSec = 5;
      };
    };
  };
}
