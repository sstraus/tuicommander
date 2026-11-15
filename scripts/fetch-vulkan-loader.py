#!/usr/bin/env python3
"""Download the pinned Khronos Vulkan loader for the Windows installer.

The NSIS hook (src-tauri/windows/installer-hooks.nsh) installs this
vulkan-1.dll next to the app only on machines without a system loader, so a
vulkan build still starts there and dictates on the CPU. The archive is
LunarG's runtime components for the same SDK version CI builds against; the
checksum pins its exact bytes.

Usage: python scripts/fetch-vulkan-loader.py
"""

import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

VERSION = "1.4.309.0"
URL = f"https://sdk.lunarg.com/sdk/download/{VERSION}/windows/VulkanRT-{VERSION}-Components.zip"
SHA256 = "7d969f4d7b44e387667d3148f61559497c22d50cbe3d50adc9e5409afbce2df1"
MEMBERS = {
    f"VulkanRT-{VERSION}-Components/x64/vulkan-1.dll": "vulkan-1.dll",
    f"VulkanRT-{VERSION}-Components/VulkanRT-License.txt": "VulkanRT-License.txt",
}
DEST = Path(__file__).resolve().parent.parent / "src-tauri" / "windows" / "vulkan"


def main() -> int:
    # The server answers 403 to urllib's default User-Agent.
    request = urllib.request.Request(URL, headers={"User-Agent": "tuicommander-release"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != SHA256:
        print(f"fetch-vulkan-loader: checksum mismatch for {URL}: {digest}", file=sys.stderr)
        return 1
    DEST.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for member, name in MEMBERS.items():
            (DEST / name).write_bytes(archive.read(member))
            print(f"fetch-vulkan-loader: {DEST / name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
