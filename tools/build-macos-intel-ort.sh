#!/usr/bin/env bash
# ort 2.0.0-rc.13 has no Intel macOS prebuilt. Keep the universal release by
# building its matching ONNX Runtime and merging its static dependencies.
set -euo pipefail

ort_version=$(awk '/^name = "ort-sys"$/ { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
if [[ "$ort_version" != "2.0.0-rc.13" ]]; then
  echo "No Intel ONNX Runtime override needed for ort-sys $ort_version"
  exit 0
fi

lib_dir="${RUNNER_TEMP:?}/auris-intel-ort-lib"

work=$(mktemp -d "$RUNNER_TEMP/auris-intel-ort.XXXXXX")
if [[ ! -f "$lib_dir/libonnxruntime.a" ]]; then
source_dir="$work/source"
build_dir="$work/build"
git clone --depth 1 --branch v1.28.0 https://github.com/microsoft/onnxruntime.git "$source_dir"
test "$(git -C "$source_dir" rev-parse HEAD)" = da9b5e364c465de65c49d91e696cd6485270757f

# The final shared target makes CMake build all transitive dependencies, including
# EXCLUDE_FROM_ALL targets such as RE2, before their archives are collected below.
"$source_dir/build.sh" --build_dir "$build_dir" --config Release --build_shared_lib \
  --update --build --skip_tests --parallel 3 --use_coreml \
  --osx_arch x86_64 --compile_no_warning_as_error \
  --cmake_extra_defines CMAKE_OSX_DEPLOYMENT_TARGET=13.4 \
  CMAKE_POLICY_VERSION_MINIMUM=3.5 \
  onnxruntime_BUILD_UNIT_TESTS=OFF

# ort-sys accepts a single libonnxruntime.a, including its third-party objects.
# Keep the output outside the build tree so it cannot include itself.
mkdir -p "$lib_dir/include"
find "$build_dir/Release" -type f -name '*.a' | sort > "$work/archives.txt"
test -s "$work/archives.txt"
libtool -static -o "$lib_dir/libonnxruntime.a" -filelist "$work/archives.txt"
cp "$source_dir"/include/onnxruntime/core/session/*.h "$lib_dir/include/"
fi

# Cache completed build output even if validation fails. Restored libraries must
# pass the same validation before the application can use them.
echo "ORT_INTEL_LIB_DIR=$lib_dir" >> "${GITHUB_ENV:?}"
lipo "$lib_dir/libonnxruntime.a" -verify_arch x86_64

# Catch missing dependencies before rebuilding both Rust application architectures.
cat > "$work/check.cc" <<'CPP'
#include <onnxruntime_c_api.h>
int main() {
  return OrtGetApiBase()->GetApi(ORT_API_VERSION) == nullptr;
}
CPP
clang_runtime=$(xcrun --sdk macosx clang -print-libgcc-file-name)
c++ -std=c++20 -arch x86_64 -mmacosx-version-min=13.4 \
  -I "$lib_dir/include" "$work/check.cc" \
  "$lib_dir/libonnxruntime.a" "$clang_runtime" -framework Foundation -framework CoreML \
  -o "$work/check"
arch -x86_64 "$work/check"
