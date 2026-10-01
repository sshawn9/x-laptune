{
  lib,
  stdenv,
  fetchFromGitLab,
  kernel,
  kernelModuleMakeFlags,
  kmod,
  pahole,
  bash,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "tuxedo-drivers-${kernel.version}";
  version = "4.24.0";

  src = fetchFromGitLab {
    group = "tuxedocomputers";
    owner = "development/packages";
    repo = "tuxedo-drivers";
    rev = "v${finalAttrs.version}";
    hash = "sha256-PCNbZ8GlzqdA1UPAwXmhsyCLyYb2drAD26IdBj596oQ=";
  };

  patches = [ ./patches/no-cp-usr.patch ];

  postInstall = ''
    install -Dm0644 -t "$out/etc/udev/rules.d" files/usr/lib/udev/rules.d/*.rules
    install -Dm0644 -t "$out/etc/udev/hwdb.d" files/usr/lib/udev/hwdb.d/*.hwdb

    for rule in "$out"/etc/udev/rules.d/*.rules; do
      substituteInPlace "$rule" \
        --replace-quiet "/bin/bash" "${lib.getExe bash}" \
        --replace-quiet "/bin/sh" "${lib.getExe bash}"
    done
  '';

  buildInputs = [ pahole ];
  nativeBuildInputs = [ kmod ] ++ kernel.moduleBuildDependencies;

  makeFlags = kernelModuleMakeFlags ++ [
    "KERNELRELEASE=${kernel.modDirVersion}"
    "KDIR=${kernel.dev}/lib/modules/${kernel.modDirVersion}/build"
    "INSTALL_MOD_PATH=${placeholder "out"}"
  ];

  doInstallCheck = true;

  meta = {
    description = "Keyboard and hardware I/O driver for TUXEDO Computers laptops";
    homepage = "https://gitlab.com/tuxedocomputers/development/packages/tuxedo-drivers";
    license = lib.licenses.gpl2Plus;
    platforms = lib.platforms.linux;
  };
})
