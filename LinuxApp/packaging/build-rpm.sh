#!/usr/bin/env bash
# Körs INUTI en fedora:40-container (se ../../.github/workflows/
# linuxapp-packaging-rpm.yml) med /work monterat till repo-roten och
# VERSION satt i miljön. Bygger release-binären, härleder RPM-Requires
# från binärens EGNA DT_NEEDED-lista (readelf -d -> rpm -qf, samma teknik
# som linux-packaging-rpm.yml för bastion-cli, bara en annan
# biblioteksfamilj), och bygger .rpm-paketet.
set -euo pipefail

dnf install -y -q gtk4-devel libadwaita-devel vte291-gtk4-devel \
  pkgconf-pkg-config rpm-build binutils gcc curl

# Fedora 40:s dnf-paketerade `rust`/`cargo` (1.86) är för gammalt för
# gtk4-rs 0.11.4-stacken (cairo-rs/gdk4/gtk4 m.fl. kräver rustc 1.92+) —
# samma `dtolnay/rust-toolchain@stable`-princip som linuxapp-build.yml/
# linuxapp-packaging.yml (.deb) använder på Ubuntu, fast via rustup direkt
# eftersom vi kör i en ren Fedora-container utan den GitHub Actions-
# specifika actionen (CI-fynd: "rustc 1.86.0 is not supported").
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
source "$HOME/.cargo/env"

cd /work/LinuxApp
cargo build --release --verbose

BIN=target/release/bastion-linuxapp
NEEDED=$(LC_ALL=C readelf -d "$BIN" | sed -n 's/.*Shared library: \[\(.*\)\].*/\1/p')
echo "Direkta delade bibliotek (DT_NEEDED):"
echo "$NEEDED"

PACKAGES=""
for LIB in $NEEDED; do
  case "$LIB" in
    libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libutil.so*|libgcc_s.so*|ld-linux*|linux-vdso*)
      continue
      ;;
  esac
  LIBPATH=$(ldconfig -p | awk -v lib="$LIB" '$1 == lib && $0 ~ /x86-64/ { print $NF; exit }')
  if [ -z "$LIBPATH" ]; then
    echo "::error::hittar inte $LIB (x86-64) via ldconfig"
    exit 1
  fi
  LIBPATH=$(readlink -f "$LIBPATH")
  PKG=$(rpm -qf --qf '%{NAME}\n' "$LIBPATH" 2>/dev/null | head -1)
  if [ -z "$PKG" ]; then
    echo "::error::$LIBPATH (från $LIB) tillhör inget känt RPM-paket"
    exit 1
  fi
  PACKAGES="$PACKAGES $PKG"
done

EXTRA=$(printf '%s\n' $PACKAGES | sort -u | grep -v '^$' | grep -v '^glibc$' | tr '\n' ' ' | sed 's/ *$//')
REQUIRES="glibc${EXTRA:+ $EXTRA}"
echo "Härledd Requires-rad: $REQUIRES"

TOPDIR=/work/LinuxApp/rpmbuild
mkdir -p "$TOPDIR"/SPECS "$TOPDIR"/SOURCES "$TOPDIR"/BUILD "$TOPDIR"/BUILDROOT "$TOPDIR"/RPMS "$TOPDIR"/SRPMS
cat > "$TOPDIR/SPECS/bastion-linuxapp.spec" <<SPEC
%global debug_package %{nil}

Name: bastion-linuxapp
Version: ${VERSION}
Release: 1%{?dist}
Summary: Fri, öppen, fristående SSH-klient (GTK4-skrivbordsklient)
License: MIT
URL: https://github.com/blixten85/bastion
Requires: ${REQUIRES}

%description
bastion-linuxapp är den native Rust/GTK4/libadwaita-skrivbordsklienten:
värdlista, terminal (VTE4), SFTP, Docker, kommandobibliotek, synk.

%install
mkdir -p %{buildroot}/usr/bin %{buildroot}/usr/share/applications
cp /work/LinuxApp/target/release/bastion-linuxapp %{buildroot}/usr/bin/bastion-linuxapp
chmod 755 %{buildroot}/usr/bin/bastion-linuxapp
cp /work/LinuxApp/packaging/bastion-linuxapp.desktop %{buildroot}/usr/share/applications/bastion-linuxapp.desktop

%files
/usr/bin/bastion-linuxapp
/usr/share/applications/bastion-linuxapp.desktop
SPEC

rpmbuild --define "_topdir $TOPDIR" -bb "$TOPDIR/SPECS/bastion-linuxapp.spec"
mkdir -p /work/dist-out
find "$TOPDIR/RPMS" -name '*.rpm' -exec cp {} /work/dist-out/ \;
