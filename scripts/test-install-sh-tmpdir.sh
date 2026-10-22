#!/usr/bin/env bash
# website/public/install.sh downloads the Linux .deb/.rpm to a scratch file
# before `sudo dpkg -i` / `sudo rpm -i`. That file must live in a private dir
# under the caller's $TMPDIR — never a hard-coded /tmp — and be gone afterwards.
# Runs the real script on any host with stub uname/curl/sudo/dpkg/rpm on a
# PATH that holds nothing else, so a real package manager is never reached.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
installer="${here}/../website/public/install.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/install-sh.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# run_case <manager> <asset-suffix>
run_case() {
  local manager="$1" suffix="$2" fail="${3:-}"
  local dir="${work}/${manager}${fail:+-fail}"
  local bin="${dir}/bin" tmp="${dir}/tmp" record="${dir}/record"
  mkdir -p "${bin}" "${tmp}"
  for tool in grep head mktemp rm mkdir dirname chmod cat ls cut; do
    ln -s "$(command -v "${tool}")" "${bin}/${tool}"
  done
  printf '#!/bin/sh\necho Linux\n' >"${bin}/uname"
  printf '#!/bin/sh\nexec "$@"\n' >"${bin}/sudo"
  # `curl -s <api>` prints a release JSON line; `curl -fsSL <url> -o <file>` downloads.
  cat >"${bin}/curl" <<EOF
#!/bin/sh
for last; do :; done
case "\$*" in
  *" -o "*) printf 'PKG' >"\${last}" ;;
  *) echo '"browser_download_url": "https://example.invalid/tuicommander_${suffix}"' ;;
esac
EOF
  # The package manager records the file it was given and that dir's mode.
  cat >"${bin}/${manager}" <<EOF
#!/bin/sh
for last; do :; done
[ "\$(cat "\${last}")" = PKG ] || exit 3
echo "\${last}" >'${record}'
ls -ld "\$(dirname "\${last}")" | cut -c1-10 >>'${record}'
${fail:+exit 1}
EOF
  # Only the generated stubs: the tool symlinks above point at system-owned
  # binaries, and chmod follows them (EPERM for a non-owner on GNU chmod).
  chmod +x "${bin}/uname" "${bin}/sudo" "${bin}/curl" "${bin}/${manager}"

  if [ -n "${fail}" ]; then
    # The package manager fails: the installer aborts (set -e) but must still
    # remove the scratch dir holding the downloaded package.
    if env -i PATH="${bin}" HOME="${work}/home" TMPDIR="${tmp}" /bin/sh "${installer}" >"${work}/${manager}.log" 2>&1; then
      fail "${manager}: installer should have failed"
    fi
    [ -f "${record}" ] || fail "${manager} was never called"
    [ -z "$(ls -A "${tmp}")" ] || fail "${manager} (failing install): scratch left behind: $(ls -A "${tmp}")"
    echo "ok: ${manager} cleans its scratch dir when the install fails"
    return
  fi
  env -i PATH="${bin}" HOME="${work}/home" TMPDIR="${tmp}" /bin/sh "${installer}" >"${work}/${manager}.log" 2>&1 ||
    fail "${manager}: installer failed: $(cat "${work}/${manager}.log")"

  [ -f "${record}" ] || fail "${manager} was never called: $(cat "${work}/${manager}.log")"
  local pkg mode
  pkg="$(sed -n 1p "${record}")"
  mode="$(sed -n 2p "${record}")"
  case "${pkg}" in
    "${tmp}"/tuicommander-??????/tuicommander.*) ;;
    *) fail "${manager} got ${pkg}, not a file in a private dir under TMPDIR=${tmp}" ;;
  esac
  [ "${mode}" = "drwx------" ] || fail "${manager}: scratch dir mode ${mode}, want drwx------"
  [ -z "$(ls -A "${tmp}")" ] || fail "${manager}: scratch left behind: $(ls -A "${tmp}")"
  echo "ok: ${manager} installs from ${pkg#"${tmp}"/} under TMPDIR and cleans it up"
}

run_case dpkg 'amd64.deb'
run_case rpm 'x86_64.rpm'
run_case dpkg 'amd64.deb' fail
run_case rpm 'x86_64.rpm' fail

