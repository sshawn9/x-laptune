# 编译与临时测试

本文说明如何编译本仓库的程序和 TUXEDO 驱动，在当前开机期间加载驱动、测试 CLI，最后卸载。

以下命令在 `x-laptune` 仓库根目录执行。构建使用普通用户；加载、卸载内核模块和操作硬件时使用 `sudo`。整个流程不执行 `nixos-rebuild switch`，也不启用系统服务。

## 1. 编译 Rust 程序

```bash
nix build --out-link result
```

默认构建产物包含：

- `result/bin/x-laptune`：统一 CLI，提供硬件查询和设置。
- `result/bin/memory-thermal-control`：持续内存温控程序。

**默认 `nix build` 不会构建 TUXEDO 内核驱动。** 驱动需要下面的独立构建步骤。

## 2. 确认用于编译驱动的内核

驱动必须针对当前运行的内核构建。本流程从宿主机 NixOS 配置取得 `boot.kernelPackages`，再用它编译本仓库的驱动包定义。

在同一个终端设置配置仓库路径和主机名。下面对应当前仓库所在的环境；更换配置仓库或主机后修改这两个值：

```bash
export XLAPTUNE_NIXOS_CONFIG="$HOME/ghq/github.com/sshawn9/nixos-config"
export XLAPTUNE_NIXOS_HOST="x"
```

查看正在运行和配置中选择的内核：

```bash
uname -r

nix eval --impure --json --expr '
let
  nixos = builtins.getFlake ("git+file://" + builtins.getEnv "XLAPTUNE_NIXOS_CONFIG");
  kernel = nixos.nixosConfigurations.${builtins.getEnv "XLAPTUNE_NIXOS_HOST"}.config.boot.kernelPackages.kernel;
in {
  version = kernel.modDirVersion;
  package = toString kernel;
}'

readlink -f /run/booted-system/kernel
```

`version` 应与 `uname -r` 一致；`package` 应对应最后一条命令显示的已启动内核，例如后者是该包下的 `bzImage`。这同时核对内核版本和具体构建产物。

如果配置已经选择了另一个内核，而系统仍在运行旧内核，先停止这套流程，使用与当前启动内核对应的配置构建驱动。不要直接使用本仓库 nixpkgs 的默认内核，也不要强制忽略模块的版本检查。

## 3. 编译 TUXEDO 驱动和 acpi_call

编译 TUXEDO 驱动：

```bash
nix build --impure --out-link result-tuxedo-drivers --expr '
let
  nixos = builtins.getFlake ("git+file://" + builtins.getEnv "XLAPTUNE_NIXOS_CONFIG");
  kernelPackages = nixos.nixosConfigurations.${builtins.getEnv "XLAPTUNE_NIXOS_HOST"}.config.boot.kernelPackages;
in kernelPackages.callPackage ./nix/tuxedo-drivers/package.nix { }
'
```

这里使用的是**本仓库的 `nix/tuxedo-drivers/package.nix`**，驱动版本、源码和补丁都由该文件决定。宿主机配置只负责提供匹配的内核及其构建环境。

编译风扇模式切换所需的独立内核模块 `acpi_call`：

```bash
nix build --impure --out-link result-acpi-call --expr '
let
  nixos = builtins.getFlake ("git+file://" + builtins.getEnv "XLAPTUNE_NIXOS_CONFIG");
in nixos.nixosConfigurations.${builtins.getEnv "XLAPTUNE_NIXOS_HOST"}.config.boot.kernelPackages.acpi_call
'
```

这时仓库根目录应有三个产物链接：

| 链接 | 用途 |
| --- | --- |
| `result` | Rust CLI 和内存温控程序 |
| `result-tuxedo-drivers` | 本仓库定义的 TUXEDO 驱动 |
| `result-acpi-call` | 匹配当前内核的 `acpi_call` 模块 |

这些链接已被 `.gitignore` 排除。后续命令通过链接定位产物，不依赖固定的 `/nix/store` 哈希。

## 4. 临时加载驱动

下面按相关模块尚未加载的情况执行。如果之前已经加载过，先按第 6 节卸载；重新编译不会替换内存中已经加载的驱动。

复制执行下面这一整段。`sudo bash -e` 启动独立 Bash，任一步失败只结束这段脚本，不会关闭当前终端。

```bash
sudo bash -e <<'SH'
kernel_release="$(uname -r)"
tuxedo_modules="$PWD/result-tuxedo-drivers/lib/modules/$kernel_release/updates/src"
acpi_module="$PWD/result-acpi-call/lib/modules/$kernel_release/misc/acpi_call.ko"

if [ ! -d "$tuxedo_modules" ] || [ ! -f "$acpi_module" ]; then
  echo "Module files not found for $kernel_release. Check the build outputs and kernel version." >&2
  exit 1
fi

modprobe -a battery sparse-keymap i8042 led-class-multicolor led-class wmi
insmod "$tuxedo_modules/tuxedo_compatibility_check/tuxedo_compatibility_check.ko.xz"
insmod "$tuxedo_modules/tuxedo_keyboard.ko.xz"
insmod "$tuxedo_modules/uniwill_wmi.ko.xz"
insmod "$tuxedo_modules/tuxedo_io/tuxedo_io.ko.xz"
insmod "$acpi_module"

test -c /dev/tuxedo_io
printf 'TUXEDO drivers loaded for %s\n' "$kernel_release"
SH
```

- `modprobe -a` 加载当前内核提供的依赖模块；同时指定多个模块时需要 `-a`。
- `insmod` 从本次构建产物的明确路径加载模块，避免误用其他版本。
- `tuxedo_keyboard` 依赖兼容性模块，`uniwill_wmi` 和 `tuxedo_io` 依赖 `tuxedo_keyboard`，因此按上面的顺序加载。
- `acpi_call` 供风扇模式切换调用固件、修改手动控制位。

这个加载流程不会安装驱动包中的 udev/hwdb 规则。本节针对以 root 权限进行的驱动和 CLI 测试；完整系统接入由 NixOS 模块负责。

## 5. 查询和测试

先查询全部状态：

```bash
sudo ./result/bin/x-laptune
```

程序依次执行电池、风扇、OEM 性能模式、CPU 和单次内存温度查询。某一项失败会报告错误并继续其余查询，最终返回非零退出状态。

也可以单独查询：

```bash
sudo ./result/bin/x-laptune battery
sudo ./result/bin/x-laptune fan
sudo ./result/bin/x-laptune oem-mode
sudo ./result/bin/x-laptune cpu
./result/bin/x-laptune memory-temp
```

未加载 TUXEDO 驱动时，电池设计容量、当前电量和百分比仍可查询。无法读取的充电模式信息显示为 `--`，JSON 中对应字段为 `null`；设置充电模式仍需要驱动提供接口。

输出 JSON：

```bash
sudo ./result/bin/x-laptune --json
```

批量查询时，每个成功的查询输出一行独立 JSON；错误写入标准错误。

需要实际测试风扇切换时，先请求全速，观察风扇响应，再恢复自动模式。下面两条命令分开执行：

```bash
sudo ./result/bin/x-laptune fan full
sudo ./result/bin/x-laptune fan auto
```

`fan auto` 会解除手动控制并恢复 EC 自动风扇策略。查询中的风扇编号目前没有对应到物理左右位置；`MIN RPM`、`MAX RPM` 为 `--` 表示该硬件范围未知。

查看其他操作参数：

```bash
./result/bin/x-laptune --help
./result/bin/x-laptune battery --help
./result/bin/x-laptune oem-mode --help
./result/bin/x-laptune cpu --help
```

## 6. 测试后卸载

如果测试过风扇全速模式，先恢复自动模式：

```bash
sudo ./result/bin/x-laptune fan auto
```

关闭正在使用这些驱动的程序后，按依赖的逆序卸载：

```bash
sudo rmmod tuxedo_io uniwill_wmi tuxedo_keyboard tuxedo_compatibility_check acpi_call
```

临时加载不会修改开机加载配置。重启后，内核模块按照系统原有配置加载。

卸载驱动不等于回滚所有硬件设置；如果另外修改过充电模式或 OEM 性能模式，应单独恢复所需设置。

## 7. 常见问题

| 现象 | 检查方向 |
| --- | --- |
| `result` 下没有 `lib/modules` | `result` 是 Rust 程序；驱动在 `result-tuxedo-drivers` |
| `Module files not found` | 确认在仓库根目录执行、三个产物链接存在，以及构建内核与运行内核一致 |
| `insmod: ... File exists` | 对应模块已加载；先卸载旧模块再加载，新编译产物不会自动替换已加载模块 |
| `Invalid module format` | 核对运行内核和构建内核的版本、具体包路径，并查看内核日志 |
| `/dev/tuxedo_io` 不存在 | 检查前面的模块加载是否成功，尤其是 `tuxedo_io`；不要跳过加载错误 |
| 提示先加载 `acpi_call` | 完成 `result-acpi-call` 的构建和加载步骤 |
| 卸载提示模块正在使用 | 先停止占用它的程序，按第 6 节的顺序卸载 |

查看当前启动的最近内核日志：

```bash
sudo journalctl -k -b -n 80 --no-pager
```
