#!/usr/bin/env bash
# Install the system libraries Tauri needs on Ubuntu runners.
#
# Ubuntu mirrors reachable from hosted runners are sometimes extremely slow,
# so each attempt is time-boxed and the next mirror is tried on failure.
set -uo pipefail

packages=(libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev patchelf "$@")
mirrors=("http://azure.archive.ubuntu.com/ubuntu" "http://archive.ubuntu.com/ubuntu" "https://mirrors.edge.kernel.org/ubuntu")

# The man-db trigger is very slow on Ubuntu 24.04 images and not needed here.
sudo rm -f /var/lib/man-db/auto-update
export DEBIAN_FRONTEND=noninteractive

sources=$(ls /etc/apt/sources.list /etc/apt/sources.list.d/*.sources /etc/apt/sources.list.d/*.list 2>/dev/null || true)
for mirror in "${mirrors[@]}"; do
  echo "::group::Installing from $mirror"
  for f in $sources; do
    sudo sed -i -E "s#https?://([a-z0-9.-]*\.)?(archive|security)\.ubuntu\.com/ubuntu/?#${mirror}/#g; s#https?://azure\.archive\.ubuntu\.com/ubuntu/?#${mirror}/#g; s#https?://archive\.ubuntu\.com/ubuntu/?#${mirror}/#g; s#https://mirrors\.edge\.kernel\.org/ubuntu/?#${mirror}/#g" "$f"
  done
  if sudo timeout 180 apt-get update -q -o Acquire::Retries=2 -o Acquire::http::Timeout=20 &&
    sudo -E timeout 600 apt-get install -y -q --no-install-recommends -o Acquire::Retries=2 -o Acquire::http::Timeout=20 "${packages[@]}"; then
    echo "::endgroup::"
    echo "Installed system libraries from $mirror"
    exit 0
  fi
  echo "::endgroup::"
  echo "::warning::Installing from $mirror failed or timed out; trying the next mirror"
done
echo "::error::Could not install the Linux system libraries from any mirror"
exit 1
