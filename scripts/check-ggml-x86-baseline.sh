#!/usr/bin/env bash
# Fail when the whisper.cpp static libraries in a cargo target dir contain
# AVX-512 code (zmm registers). Release builds must run on every x86-64 CPU
# with AVX2; an AVX-512 instruction crashes dictation on the others (#151).
#
# Usage: check-ggml-x86-baseline.sh <cargo-target-dir>
#
# The check reads the libraries that whisper-rs-sys builds with CMake, not the
# final executable: ggml is the only C code that GGML_NATIVE controls. It also
# requires ymm registers in ggml-cpu, so an empty or unreadable disassembly
# (no machine code, wrong tool) fails instead of passing.
set -euo pipefail

root=${1:?usage: $0 <cargo-target-dir>}

disasm=
for candidate in llvm-objdump objdump \
  "/c/Program Files/LLVM/bin/llvm-objdump.exe" "C:/Program Files/LLVM/bin/llvm-objdump.exe"; do
  if command -v "$candidate" >/dev/null 2>&1 || [[ -x $candidate ]]; then
    disasm=$candidate
    break
  fi
done
if [[ -z $disasm ]]; then
  echo "check-ggml-x86-baseline: no llvm-objdump or objdump found" >&2
  exit 2
fi

libs=()
while IFS= read -r lib; do
  libs+=("$lib")
done < <(find "$root/" -type f -path '*whisper-rs-sys-*' \
  \( -name 'libggml*.a' -o -name 'ggml*.lib' -o -name 'libwhisper.a' -o -name 'whisper.lib' \) | sort)

if (( ${#libs[@]} == 0 )); then
  echo "check-ggml-x86-baseline: no whisper-rs-sys static library under $root" >&2
  exit 2
fi

status=0
seen_cpu=0
for lib in "${libs[@]}"; do
  listing=$("$disasm" -d "$lib")
  zmm=$(grep -ciE 'zmm[0-9]' <<<"$listing" || true)
  ymm=$(grep -ciE 'ymm[0-9]' <<<"$listing" || true)
  echo "$lib: zmm=$zmm ymm=$ymm"
  if (( zmm > 0 )); then
    grep -iE -m 5 'zmm[0-9]' <<<"$listing" >&2
    status=1
  fi
  case $(basename "$lib") in
    libggml-cpu.a | ggml-cpu.lib)
      seen_cpu=1
      if (( ymm == 0 )); then
        echo "check-ggml-x86-baseline: $lib has no ymm code; the disassembly is empty or AVX2 is off" >&2
        status=1
      fi
      ;;
  esac
done

if (( ! seen_cpu )); then
  echo "check-ggml-x86-baseline: no ggml-cpu library under $root" >&2
  status=1
fi
if (( status != 0 )); then
  echo "check-ggml-x86-baseline: FAIL — ggml must be built with GGML_NATIVE=OFF and without AVX-512" >&2
fi
exit "$status"
