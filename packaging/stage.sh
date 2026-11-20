#!/bin/sh
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
output=$1
install -Dm755 "$2" "$output/bin/nearby-transfer"
install -Dm755 "$3" "$output/bin/nearby-transfer-service"
for file in 'Launch Manhattan Transfer' install.sh; do
    install -Dm755 "$root/packaging/$file" "$output/$file"
done
for file in io.github.NearbyTransfer.desktop nearby-transfer-send.desktop nearby-transfer.service; do
    install -Dm644 "$root/packaging/$file" "$output/$file"
done
for file in README.md LICENSE THIRD_PARTY.md; do
    install -Dm644 "$root/$file" "$output/$file"
done
install -Dm644 "$root/third_party/localsend/LICENSE" "$output/licenses/LocalSend.txt"
install -Dm644 "$root/third_party/rqs_lib/LICENSE" "$output/licenses/rqs_lib.txt"
install -Dm644 "$root/third_party/rqs_lib/LICENSE-crQuickShare" "$output/licenses/crQuickShare.txt"
install -Dm644 "$root/third_party/KuickShare-NOTICES.md" "$output/licenses/KuickShare-NOTICES.md"
