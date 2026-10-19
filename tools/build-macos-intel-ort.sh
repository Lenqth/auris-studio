#!/usr/bin/env bash
# ort 2.0.0-rc.13 has no Intel macOS prebuilt. Keep the universal release by
# building its matching ONNX Runtime and merging its static dependencies.
set -euo pipefail

ort_version=$(awk '/^name = "ort-sys"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
if [[ "$ort_version" != "2.0.0-rc.13" ]]; then
  echo "No Intel ONNX Runtime override needed for ort-sys $ort_version"
  exit 0
fi

work=$(mktemp -d "${RUNNER_TEMP:?}/auris-intel-ort.XXXXXX")
source_dir="$work/source"
build_dir="$work/build"
lib_dir="$work/lib"
git clone --depth 1 --branch v1.28.0 https://github.com/microsoft/onnxruntime.git "$source_dir"
test "$(git -C "$source_dir" rev-parse HEAD)" = da9b5e364c465de65c49d91e696cd6485270757f

"$source_dir/build.sh" --build_dir "$build_dir" --config Release \
  --update --build --skip_tests --parallel 3 --use_coreml \
  --osx_arch x86_64 --compile_no_warning_as_error \
  --cmake_extra_defines CMAKE_OSX_DEPLOYMENT_TARGET=13.3 \
  onnxruntime_BUILD_UNIT_TESTS=OFF

# ort-sys accepts a single libonnxruntime.a, including its third-party objects.
# Keep the output outside the build tree so it cannot include itself.
mkdir -p "$lib_dir"
find "$build_dir/Release" -type f -name '*.a' | sort > "$work/archives.txt"
test -s "$work/archives.txt"
libtool -static -o "$lib_dir/libonnxruntime.a" -filelist "$work/archives.txt"
lipo -verify_arch x86_64 "$lib_dir/libonnxruntime.a"
echo "ORT_INTEL_LIB_DIR=$lib_dir" >> "${GITHUB_ENV:?}"
