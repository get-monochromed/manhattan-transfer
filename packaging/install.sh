#!/bin/sh
set -eu
bundle_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
prefix=${1:-"$HOME/.local"}
install -Dm755 "$bundle_dir/bin/nearby-transfer" "$prefix/bin/nearby-transfer"
install -Dm755 "$bundle_dir/bin/nearby-transfer-service" "$prefix/bin/nearby-transfer-service"
install -Dm644 "$bundle_dir/io.github.NearbyTransfer.desktop" "$prefix/share/applications/io.github.NearbyTransfer.desktop"
install -Dm644 "$bundle_dir/nearby-transfer-send.desktop" "$prefix/share/kio/servicemenus/nearby-transfer-send.desktop"
if [ "$prefix" = "$HOME/.local" ]; then
    install -Dm644 "$bundle_dir/nearby-transfer.service" "$prefix/share/systemd/user/nearby-transfer.service"
fi
printf 'Installed Manhattan Transfer in %s\n' "$prefix"
