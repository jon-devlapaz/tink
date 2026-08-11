#!/usr/bin/env sh
# Install tink from the latest GitHub Release into ~/.local/bin.
# Requires: curl, tar, python3.
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
need python3

api_mode="$(python3 -c '
import sys
from urllib.parse import urlsplit

url = sys.argv[1]
if not url or any(ord(char) <= 32 for char in url) or "\\" in url or "?" in url or "#" in url:
    sys.exit(1)
try:
    parsed = urlsplit(url)
except ValueError:
    sys.exit(1)
if parsed.scheme == "https" and parsed.hostname and parsed.username is None and parsed.password is None:
    print("https")
elif parsed.scheme == "file" and not parsed.netloc and parsed.path.startswith("/"):
    print("file")
else:
    sys.exit(1)
' "${API_URL}")" || die "TINK_RELEASES_API is not an allowed release URL"

curl_fetch() {
  fetch_url="$1"
  fetch_dest="$2"
  fetch_protocol="$3"
  fetch_max_time="$4"
  run_bounded "$((fetch_max_time + 5))" curl -fsSL \
    --proto "=${fetch_protocol}" \
    --proto-redir "=${fetch_protocol}" \
    --connect-timeout 5 \
    --max-time "${fetch_max_time}" \
    --retry 2 \
    --retry-delay 1 \
    "${fetch_url}" -o "${fetch_dest}"
}

run_bounded() {
  process_timeout="$1"
  shift
  python3 - "${process_timeout}" "$@" <<'PY'
import os, signal, subprocess, sys

def terminate_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except OSError:
        pass

def reap_after_kill(process):
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        try:
            process.kill()
        except OSError:
            pass
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass

try:
    process = subprocess.Popen(sys.argv[2:], start_new_session=True)
except OSError:
    sys.exit(126)
def interrupt(_signum, _frame):
    raise KeyboardInterrupt
for watched in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
    signal.signal(watched, interrupt)
try:
    returncode = process.wait(timeout=float(sys.argv[1]))
except subprocess.TimeoutExpired:
    terminate_group(process)
    reap_after_kill(process)
    sys.exit(124)
except BaseException:
    terminate_group(process)
    reap_after_kill(process)
    sys.exit(130)
terminate_group(process)
sys.exit(returncode)
PY
}

probe_exact() {
  probe_path="$1"
  probe_version="$2"
  python3 - "${probe_path}" "${probe_version}" <<'PY'
import os, signal, subprocess, sys, threading, time

CAPTURE_LIMIT = 16 * 1024 * 1024

def terminate_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except OSError:
        pass

def reap_after_kill(process):
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        try:
            process.kill()
        except OSError:
            pass
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass

def close_pipes(process):
    try:
        if process.stdout is not None:
            process.stdout.close()
        if process.stderr is not None:
            process.stderr.close()
    except OSError:
        pass

def drain_capped(name, pipe, results, overflow):
    retained = bytearray()
    exceeded = False
    try:
        while True:
            chunk = pipe.read(64 * 1024)
            if not chunk:
                break
            remaining = max(0, CAPTURE_LIMIT - len(retained))
            retained.extend(chunk[:remaining])
            if len(chunk) > remaining:
                exceeded = True
                overflow.set()
    except (OSError, ValueError):
        exceeded = True
        overflow.set()
    finally:
        results[name] = (bytes(retained), exceeded)

def join_drains(process, threads):
    for thread in threads:
        thread.join(timeout=1)
    if any(thread.is_alive() for thread in threads):
        close_pipes(process)
        for thread in threads:
            thread.join(timeout=1)
    return not any(thread.is_alive() for thread in threads)

try:
    process = subprocess.Popen(
        [sys.argv[1], "--version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
except OSError:
    sys.exit(1)
def interrupt(_signum, _frame):
    raise KeyboardInterrupt
for watched in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
    signal.signal(watched, interrupt)
results = {}
overflow = threading.Event()
threads = [
    threading.Thread(
        target=drain_capped,
        args=("stdout", process.stdout, results, overflow),
        daemon=True,
    ),
    threading.Thread(
        target=drain_capped,
        args=("stderr", process.stderr, results, overflow),
        daemon=True,
    ),
]
for thread in threads:
    thread.start()
timed_out = False
try:
    deadline = time.monotonic() + 5
    while process.poll() is None and not overflow.is_set():
        if time.monotonic() >= deadline:
            timed_out = True
            break
        time.sleep(0.01)
except OSError:
    terminate_group(process)
    reap_after_kill(process)
    close_pipes(process)
    join_drains(process, threads)
    sys.exit(1)
except BaseException:
    terminate_group(process)
    reap_after_kill(process)
    close_pipes(process)
    join_drains(process, threads)
    sys.exit(130)
terminate_group(process)
if process.returncode is None:
    reap_after_kill(process)
drains_finished = join_drains(process, threads)
stdout, stdout_exceeded = results.get("stdout", (b"", True))
_, stderr_exceeded = results.get("stderr", (b"", True))
expected = f"tink {sys.argv[2]}\n".encode()
sys.exit(
    0
    if not timed_out
    and not overflow.is_set()
    and drains_finished
    and not stdout_exceeded
    and not stderr_exceeded
    and process.returncode == 0
    and stdout == expected
    else 1
)
PY
}

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
stage=""
backup=""
cleanup() {
  rm -rf "${tmp}"
  [ -z "${stage}" ] || rm -f "${stage}"
  [ -z "${backup}" ] || rm -f "${backup}"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

curl_fetch "${API_URL}" "${tmp}/release.json" "${api_mode}" 30 \
  2>"${tmp}/metadata-curl-error.txt" || die "could not fetch release metadata"

python3 -c '
import json, re, sys
from urllib.parse import urlsplit

def valid_semver(value):
    pieces = value.split("+")
    if len(pieces) > 2:
        return False
    version_and_pre = pieces[0]
    build = pieces[1] if len(pieces) == 2 else None
    pieces = version_and_pre.split("-")
    core = pieces[0]
    pre = "-".join(pieces[1:]) if len(pieces) > 1 else None
    core_parts = core.split(".")
    if len(core_parts) != 3:
        return False
    if any(not part or not all("0" <= char <= "9" for char in part) or (len(part) > 1 and part.startswith("0")) for part in core_parts):
        return False
    for identifiers, reject_numeric_zero in ((pre, True), (build, False)):
        if identifiers is None:
            continue
        parts = identifiers.split(".")
        if any(not part or not re.fullmatch(r"[0-9A-Za-z-]+", part) for part in parts):
            return False
        if reject_numeric_zero and any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in parts):
            return False
    return True

def url_mode(url, file_allowed):
    if not isinstance(url, str) or not url or any(ord(char) <= 32 for char in url):
        return None
    if "\\" in url or "?" in url or "#" in url:
        return None
    try:
        parsed = urlsplit(url)
    except ValueError:
        return None
    if parsed.scheme == "https" and parsed.hostname and parsed.username is None and parsed.password is None:
        return "https"
    if file_allowed and parsed.scheme == "file" and not parsed.netloc and parsed.path.startswith("/"):
        return "file"
    return None

try:
    target, metadata_path, api_mode = sys.argv[1:]
    with open(metadata_path, encoding="utf-8") as source:
        data = json.load(source)
except (OSError, ValueError, TypeError):
    sys.exit("could not parse release metadata")

tag = data.get("tag_name") if isinstance(data, dict) else None
version = tag[1:] if isinstance(tag, str) and tag.startswith("v") else tag
if not isinstance(version, str) or not valid_semver(version):
    sys.exit("release metadata has invalid semantic version")
want = f"tink-{version}-{target}.tar.gz"
assets = data.get("assets")
if not isinstance(assets, list):
    sys.exit("release metadata has invalid assets")
matches = [asset for asset in assets if isinstance(asset, dict) and asset.get("name") == want]
if len(matches) != 1:
    sys.exit(f"expected exactly one asset named {want}")
asset = matches[0]
url = asset.get("browser_download_url")
asset_mode = url_mode(url, api_mode == "file")
if asset_mode is None:
    sys.exit(f"asset {want} has invalid download URL")
digest = asset.get("digest")
if not isinstance(digest, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
    sys.exit(f"asset {want} has invalid SHA-256 digest")
print(tag)
print(version)
print(url)
print(digest[7:])
print(asset_mode)
' "${target}" "${tmp}/release.json" "${api_mode}" >"${tmp}/asset.txt" 2>"${tmp}/metadata-error.txt" \
  || die "$(cat "${tmp}/metadata-error.txt" 2>/dev/null || echo could not parse release metadata)"

[ "$(wc -l < "${tmp}/asset.txt" | tr -d ' ')" = "5" ] \
  || die "release metadata parser returned invalid asset fields"
tag_name="$(sed -n '1p' "${tmp}/asset.txt")"
version="$(sed -n '2p' "${tmp}/asset.txt")"
asset_url="$(sed -n '3p' "${tmp}/asset.txt")"
expected_digest="$(sed -n '4p' "${tmp}/asset.txt")"
asset_mode="$(sed -n '5p' "${tmp}/asset.txt")"

curl_fetch "${asset_url}" "${tmp}/tink.tgz" "${asset_mode}" 300 \
  2>"${tmp}/asset-curl-error.txt" || die "could not download release asset"
actual_digest="$(python3 -c '
import hashlib, sys
digest = hashlib.sha256()
with open(sys.argv[1], "rb") as source:
    for chunk in iter(lambda: source.read(65536), b""):
        digest.update(chunk)
print(digest.hexdigest())
' "${tmp}/tink.tgz")" || die "could not hash downloaded release archive"
[ "${actual_digest}" = "${expected_digest}" ] || die "release archive SHA-256 digest mismatch"

run_bounded 30 tar -tzf "${tmp}/tink.tgz" >"${tmp}/archive-entries.txt" \
  2>"${tmp}/tar-list-error.txt" || die "failed to inspect release archive"
python3 -c '
import sys
sys.exit(0 if open(sys.argv[1], "rb").read() == b"tink\n" else 1)
' "${tmp}/archive-entries.txt" \
  || die "release archive must contain exactly one top-level tink file"
run_bounded 30 tar -tvzf "${tmp}/tink.tgz" >"${tmp}/archive-details.txt" \
  2>"${tmp}/tar-details-error.txt" || die "failed to inspect release archive entry type"
python3 -c '
import sys
rows = open(sys.argv[1], "rb").read().splitlines()
sys.exit(0 if len(rows) == 1 and rows[0].lstrip().startswith(b"-") else 1)
' "${tmp}/archive-details.txt" \
  || die "release archive tink entry must be a regular file"

mkdir "${tmp}/extract"
run_bounded 30 tar -C "${tmp}/extract" -xzf "${tmp}/tink.tgz" \
  >"${tmp}/tar-extract-output.txt" 2>"${tmp}/tar-extract-error.txt" \
  || die "failed to extract release archive"
candidate="${tmp}/extract/tink"
if [ ! -f "${candidate}" ] || [ -L "${candidate}" ]; then
  die "archive did not contain a regular tink binary"
fi
[ -x "${candidate}" ] || die "release candidate is not executable"
probe_exact "${candidate}" "${version}" \
  || die "release candidate failed version probe"

mkdir -p "${INSTALL_DIR}"
destination="${INSTALL_DIR}/tink"
[ ! -L "${destination}" ] || die "refusing to replace symlink: ${destination}"
[ ! -e "${destination}" ] || [ -f "${destination}" ] \
  || die "refusing to replace non-file: ${destination}"
stage="$(mktemp "${INSTALL_DIR}/.tink-install.XXXXXX")" \
  || die "could not create install staging file in ${INSTALL_DIR}"
cp "${candidate}" "${stage}" || die "could not stage tink in ${INSTALL_DIR}"
chmod 755 "${stage}" || die "could not make staged tink executable"
probe_exact "${stage}" "${version}" || die "staged release candidate failed version probe"

if [ -f "${destination}" ]; then
  backup="$(mktemp "${INSTALL_DIR}/.tink-backup.XXXXXX")" \
    || die "could not create install backup in ${INSTALL_DIR}"
  cp -p "${destination}" "${backup}" || die "could not preserve existing tink binary"
fi
python3 -c 'import os, sys; os.replace(sys.argv[1], sys.argv[2])' "${stage}" "${destination}" \
  || die "could not publish tink to install destination"
stage=""

if ! probe_exact "${destination}" "${version}"; then
  if [ -n "${backup}" ]; then
    if ! python3 -c 'import os, sys; os.replace(sys.argv[1], sys.argv[2])' \
      "${backup}" "${destination}"
    then
      recovery_backup="${backup}"
      backup=""
      die "published tink failed verification; rollback backup remains at ${recovery_backup}"
    fi
    backup=""
  else
    rm -f "${destination}"
  fi
  die "published tink failed exact version verification"
fi
[ -z "${backup}" ] || rm -f "${backup}"
backup=""

trap '' PIPE
printf 'Installed tink %s -> %s/tink\n' "${tag_name:-unknown}" "${INSTALL_DIR}" || :

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf 'Add %s to your PATH, then run: tink --version\n' "${INSTALL_DIR}" || :
    ;;
esac

printf 'tink %s\n' "${version}" || :
