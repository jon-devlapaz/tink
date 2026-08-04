#!/usr/bin/env sh
# Install tink from the latest GitHub Release into ~/.local/bin.
# Requires: curl, tar.
set -eu

REPO="jon-devlapaz/tink"
INSTALL_DIR="${TINK_INSTALL_DIR:-${HOME}/.local/bin}"
API_URL="${TINK_RELEASES_API:-https://api.github.com/repos/${REPO}/releases/latest}"

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "$1 is required to install tink"
}

need curl
need tar

os="$(uname -s)"
arch="$(uname -m)"
case "${os}/${arch}" in
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  Linux/x86_64|Linux/amd64) target="x86_64-unknown-linux-gnu" ;;
  Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-gnu" ;;
  *) die "unsupported platform: ${os}/${arch}" ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

curl -fsSL "${API_URL}" -o "${tmp}/release.json" \
  || die "could not fetch release metadata from ${API_URL}"

if command -v python3 >/dev/null 2>&1; then
  asset_url="$(python3 -c '
import json, sys
target = sys.argv[1]
data = json.load(open(sys.argv[2]))
tag = data.get("tag_name") or ""
version = tag[1:] if tag.startswith("v") else tag
want = f"tink-{version}-{target}.tar.gz"
for asset in data.get("assets") or []:
    if asset.get("name") == want:
        print(asset.get("browser_download_url") or "")
        print(tag, file=sys.stderr)
        sys.exit(0)
names = ", ".join(a.get("name", "") for a in (data.get("assets") or []))
sys.stderr.write(f"no asset named {want} (have: {names})\n")
sys.exit(1)
' "${target}" "${tmp}/release.json" 2>"${tmp}/tag.txt")" \
    || die "$(cat "${tmp}/tag.txt" 2>/dev/null || echo could not parse release metadata)"
  tag_name="$(cat "${tmp}/tag.txt")"
else
  tag_name="$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "${tmp}/release.json" | head -n1)"
  version="${tag_name#v}"
  want="tink-${version}-${target}.tar.gz"
  asset_url="$(tr ',' '\n' < "${tmp}/release.json" | sed -n "s/.*\"browser_download_url\": *\"\\([^\"]*${want}\\)\".*/\\1/p" | head -n1)"
  [ -n "${asset_url}" ] || die "no asset named ${want}"
fi

[ -n "${asset_url}" ] || die "empty download URL"

curl -fsSL "${asset_url}" -o "${tmp}/tink.tgz" || die "download failed: ${asset_url}"
tar -C "${tmp}" -xzf "${tmp}/tink.tgz"
[ -f "${tmp}/tink" ] || die "archive did not contain a tink binary"

mkdir -p "${INSTALL_DIR}"
install -m 755 "${tmp}/tink" "${INSTALL_DIR}/tink"

printf 'Installed tink %s -> %s/tink\n' "${tag_name:-unknown}" "${INSTALL_DIR}"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf 'Add %s to your PATH, then run: tink --version\n' "${INSTALL_DIR}"
    ;;
esac

"${INSTALL_DIR}/tink" --version || true
