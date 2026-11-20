#!/bin/sh
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
build_dir=${NEARBY_BUILD_DIR:-"$root/.build"}
output_dir=${1:-"$root/dist/Manhattan Transfer"}
jobs=${NEARBY_BUILD_JOBS:-4}
cargo build --locked --release --jobs "$jobs" --manifest-path "$root/Cargo.toml" --target-dir "$build_dir/rust"
cmake -S "$root/desktop" -B "$build_dir/qt" -DCMAKE_BUILD_TYPE=Release
cmake --build "$build_dir/qt" --parallel "$jobs"
"$root/packaging/stage.sh" "$output_dir" "$build_dir/qt/nearby-transfer" "$build_dir/rust/release/nearby-transfer-service"
