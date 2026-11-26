#!/usr/bin/env bash
# Build native Vulkan tools: Ubuntu 22.04 has no glslc and its headers are too old.
set -euo pipefail
work=${1:?usage: $0 <work-directory> <install-prefix>}
prefix=${2:?usage: $0 <work-directory> <install-prefix>}
test "$(uname -m)" = aarch64
mkdir -p "$work" "$prefix/bin" "$prefix/lib"
work=$(cd "$work" && pwd)
prefix=$(cd "$prefix" && pwd)

fetch() {
  local repository=$1 revision=$2 checksum=$3 destination=$4
  local archive="$work/${repository##*/}-$revision.tar.gz"
  curl --fail --location --retry 3 "https://codeload.github.com/$repository/tar.gz/$revision" -o "$archive"
  printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check
  mkdir -p "$destination"
  tar -xzf "$archive" -C "$destination" --strip-components=1
}

# shaderc v2023.8, resolved to an immutable commit. The other revisions are
# recorded in that commit's DEPS; tests and examples are not built.
source="$work/shaderc"
fetch google/shaderc e6edd6d48fa5bdd9d176794c6810fae7f8e938e1 f34caf0bccf80ebc08e71b1d1057a6f11410659e0f2d3b618edb9a28012a777b "$source"
fetch KhronosGroup/glslang a91631b260cba3f22858d6c6827511e636c2458a 8bba14a39f7a666587b15e66098e816e900974d9450b401db3d25d5a4ce5c145 "$source/third_party/glslang"
fetch KhronosGroup/SPIRV-Headers 1c6bb2743599e6eb6f37b2969acc0aef812e32e3 1a248f4199f4a30b2e7304ed4d62f731765bab12d9fa4c5abb189a5e2d57f1f5 "$source/third_party/spirv-headers"
fetch KhronosGroup/SPIRV-Tools f0cc85efdbbe3a46eae90e0f915dc1509836d0fc cb2f7a474feb5a530384594ce17904c1c192800bbd0e563548c784f7b72f8695 "$source/third_party/spirv-tools"
# Match the existing x64 SDK's headers while keeping the distro runtime loader.
fetch KhronosGroup/Vulkan-Headers 952f776f6573aafbb62ea717d871cd1d6816c387 4c744e80c908bcc65c2057482f44e57d7e3d748f6eae4cd759fd593018f4ce35 "$work/vulkan-headers"
cp -a "$work/vulkan-headers/include" "$prefix/"
ln -sf "$(pkg-config --variable=libdir vulkan)/libvulkan.so" "$prefix/lib/libvulkan.so"
# Catches the missing C++ declarations observed in the whisper Vulkan build.
cat > "$work/headers-smoke.cpp" <<'CPP'
#include <vulkan/vulkan.hpp>
#include <cstdio>
int main() {
  vk::LayerSettingsCreateInfoEXT settings;
  vk::LayerSettingEXT setting;
  auto driver = vk::DriverId::eMesaDozen;
  (void)settings; (void)setting; (void)driver;
  uint32_t runtime = 0;
  if (vkEnumerateInstanceVersion(&runtime) != VK_SUCCESS) return 1;
  std::printf("Vulkan headers=%u runtime=%u.%u.%u\n", VK_HEADER_VERSION,
              VK_VERSION_MAJOR(runtime), VK_VERSION_MINOR(runtime), VK_VERSION_PATCH(runtime));
}
CPP
c++ -std=c++17 -I "$prefix/include" "$work/headers-smoke.cpp" -L "$prefix/lib" -lvulkan -o "$work/headers-smoke"
"$work/headers-smoke"
cmake -S "$source" -B "$work/build" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DSHADERC_SKIP_TESTS=ON \
  -DSHADERC_SKIP_EXAMPLES=ON -DSPIRV_SKIP_TESTS=ON
cmake --build "$work/build" --target glslc_exe --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-2}"
install -m 755 "$work/build/glslc/glslc" "$prefix/bin/glslc"
"$prefix/bin/glslc" --version
# Exercise the compiler, not just its loader/version path.
printf '#version 450\nvoid main() {}\n' > "$work/smoke.comp"
"$prefix/bin/glslc" "$work/smoke.comp" -o "$work/smoke.spv"
test -s "$work/smoke.spv"
