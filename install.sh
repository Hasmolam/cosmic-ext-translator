#!/usr/bin/env bash
# ==============================================================================
# COSMIC Screen Translator (cosmic-ext-translator) - Installer
# Author: Hasan Hüseyin Yolcu <hasanhuseyinyolcu25@gmail.com>
# License: GPL-3.0-or-later
#
# Installs to ~/.local without root. Binary sources, in order:
#   1. binaries next to this script (extracted release tarball)
#   2. target/release in a source checkout
#   3. the GitHub release (latest, or --version vX.Y.Z), verified against SHA256SUMS
# ==============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

APP_ID="io.github.hasmolam.CosmicExtTranslator"
LEGACY_APP_ID="com.system76.CosmicExtTranslator"
GITHUB_REPO="Hasmolam/cosmic-ext-translator"
TARGET_TRIPLE="x86_64-unknown-linux-gnu"

TARGET_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
METAINFO_DIR="${HOME}/.local/share/metainfo"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [[ -n "${SCRIPT_PATH}" && -f "${SCRIPT_PATH}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
else
    # Piped through `curl | bash`: no local files to use.
    SCRIPT_DIR=""
fi

REQUESTED_TAG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            REQUESTED_TAG="${2:?--version needs a tag, e.g. v0.2.0}"
            shift 2
            ;;
        --help|-h)
            echo "Usage: install.sh [--version vX.Y.Z]"
            echo "  --version TAG   Install a specific release instead of the latest one"
            echo "  --help, -h      Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}[x] Unknown option: $1${NC}" >&2
            exit 2
            ;;
    esac
done

die() {
    echo -e "${RED}[x] $*${NC}" >&2
    exit 1
}

fetch() {
    # fetch <url> <output>
    if command -v curl &>/dev/null; then
        curl -fsSL --retry 3 "$1" -o "$2"
    elif command -v wget &>/dev/null; then
        wget -qO "$2" "$1"
    else
        die "Neither curl nor wget found."
    fi
}

latest_tag() {
    # Follows the releases/latest redirect; avoids the rate-limited REST API.
    local url
    if command -v curl &>/dev/null; then
        url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${GITHUB_REPO}/releases/latest")
    else
        url=$(wget -q --max-redirect=5 -S --spider "https://github.com/${GITHUB_REPO}/releases/latest" 2>&1 |
            sed -n 's/^ *Location: //p' | tail -n 1)
    fi
    url="${url%$'\r'}"
    [[ "${url}" == */tag/* ]] || return 1
    echo "${url##*/tag/}"
}

# Installs desktop entry, AppStream metadata and icon from a directory that contains
# <APP_ID>.desktop, <APP_ID>.metainfo.xml and <APP_ID>.svg.
install_metadata_from() {
    local dir="$1"
    [[ -f "${dir}/${APP_ID}.desktop" ]] || die "Missing ${APP_ID}.desktop in ${dir}"
    install -m 0644 "${dir}/${APP_ID}.desktop" "${APP_DIR}/"
    [[ -f "${dir}/${APP_ID}.metainfo.xml" ]] && install -m 0644 "${dir}/${APP_ID}.metainfo.xml" "${METAINFO_DIR}/"
    [[ -f "${dir}/${APP_ID}.svg" ]] && install -m 0644 "${dir}/${APP_ID}.svg" "${ICON_DIR}/"
    return 0
}

# Moves a panel applet entry added under the 0.1.x application ID to the current one.
migrate_panel_entry() {
    local config_root="${XDG_CONFIG_HOME:-${HOME}/.config}/cosmic"
    local backup_root="${XDG_STATE_HOME:-${HOME}/.local/state}/cosmic-ext-translator/panel-backup"
    local list rel
    [[ -d "${config_root}" ]] || return 0
    shopt -s nullglob
    for list in "${config_root}"/com.system76.CosmicPanel.*/v*/plugins_*; do
        if grep -qF "\"${LEGACY_APP_ID}\"" "${list}"; then
            rel="${list#"${config_root}"/}"
            mkdir -p "${backup_root}/$(dirname "${rel}")"
            cp -p "${list}" "${backup_root}/${rel}"
            sed -i "s/\"${LEGACY_APP_ID//./\\.}\"/\"${APP_ID}\"/g" "${list}"
            echo -e "${BLUE}[*] Updated panel applet entry in ${list}${NC}"
        fi
    done
    shopt -u nullglob
}

echo -e "${BLUE}${BOLD}=== COSMIC Screen Translator Installer ===${NC}\n"

mkdir -p "${TARGET_DIR}" "${APP_DIR}" "${METAINFO_DIR}" "${ICON_DIR}"

if [[ ":${PATH}:" != *":${TARGET_DIR}:"* ]]; then
    echo -e "${YELLOW}[!] Warning:${NC} ${TARGET_DIR} is not in your PATH."
    echo -e "    Add 'export PATH=\"\$HOME/.local/bin:\$PATH\"' to your ~/.bashrc or ~/.zshrc."
fi

TMP_DIR=""
cleanup() {
    [[ -n "${TMP_DIR}" ]] && rm -rf "${TMP_DIR}"
    return 0
}
trap cleanup EXIT

if [[ -n "${SCRIPT_DIR}" && -z "${REQUESTED_TAG}" &&
    -f "${SCRIPT_DIR}/cosmic-applet-translator" && -f "${SCRIPT_DIR}/cosmic-translator-overlay" ]]; then
    echo -e "${BLUE}[*] Installing from the extracted release archive.${NC}"
    BIN_DIR="${SCRIPT_DIR}"
    META_DIR="${SCRIPT_DIR}"

elif [[ -n "${SCRIPT_DIR}" && -z "${REQUESTED_TAG}" &&
    -f "${SCRIPT_DIR}/target/release/cosmic-applet-translator" &&
    -f "${SCRIPT_DIR}/target/release/cosmic-translator-overlay" &&
    -f "${SCRIPT_DIR}/crates/cosmic-applet-translator/target/xdgen/app.desktop" ]]; then
    echo -e "${BLUE}[*] Installing locally compiled release binaries.${NC}"
    BIN_DIR="${SCRIPT_DIR}/target/release"
    TMP_DIR=$(mktemp -d)
    META_DIR="${TMP_DIR}"
    cp "${SCRIPT_DIR}/crates/cosmic-applet-translator/target/xdgen/app.desktop" "${META_DIR}/${APP_ID}.desktop"
    cp "${SCRIPT_DIR}/crates/cosmic-applet-translator/target/xdgen/app.metainfo.xml" "${META_DIR}/${APP_ID}.metainfo.xml"
    cp "${SCRIPT_DIR}/crates/cosmic-applet-translator/resources/icon.svg" "${META_DIR}/${APP_ID}.svg"

else
    TAG="${REQUESTED_TAG}"
    if [[ -z "${TAG}" ]]; then
        TAG=$(latest_tag) || die "Cannot determine the latest release of ${GITHUB_REPO}."
    fi
    ARCHIVE="cosmic-ext-translator-${TAG}-${TARGET_TRIPLE}.tar.gz"
    BASE_URL="https://github.com/${GITHUB_REPO}/releases/download/${TAG}"
    echo -e "${BLUE}[*] Downloading ${ARCHIVE}...${NC}"

    TMP_DIR=$(mktemp -d)
    fetch "${BASE_URL}/${ARCHIVE}" "${TMP_DIR}/${ARCHIVE}" || die "Failed to download ${BASE_URL}/${ARCHIVE}"
    fetch "${BASE_URL}/SHA256SUMS" "${TMP_DIR}/SHA256SUMS" ||
        die "Release ${TAG} has no SHA256SUMS; refusing to install an unverified archive."

    command -v sha256sum &>/dev/null || die "sha256sum not found; cannot verify the download."
    (cd "${TMP_DIR}" && grep -F " ${ARCHIVE}" SHA256SUMS | sha256sum -c --status -) ||
        die "Checksum verification failed for ${ARCHIVE}."
    echo -e "${GREEN}[*] Checksum verified.${NC}"

    tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "${TMP_DIR}"
    BIN_DIR="${TMP_DIR}/${ARCHIVE%.tar.gz}"
    [[ -f "${BIN_DIR}/cosmic-applet-translator" ]] || die "Unexpected archive layout in ${ARCHIVE}."
    META_DIR="${BIN_DIR}"
fi

# Remove first to avoid ETXTBSY when the applet is running.
rm -f "${TARGET_DIR}/cosmic-applet-translator" "${TARGET_DIR}/cosmic-translator-overlay"
install -m 0755 "${BIN_DIR}/cosmic-applet-translator" "${TARGET_DIR}/"
install -m 0755 "${BIN_DIR}/cosmic-translator-overlay" "${TARGET_DIR}/"
install_metadata_from "${META_DIR}"

# 0.1.x used a different application ID; drop its files and keep the panel entry.
rm -f "${APP_DIR}/${LEGACY_APP_ID}.desktop" \
    "${METAINFO_DIR}/${LEGACY_APP_ID}.metainfo.xml" \
    "${ICON_DIR}/${LEGACY_APP_ID}.svg"
migrate_panel_entry

if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo -e "\n${BLUE}[*] Restarting cosmic-panel to register the applet...${NC}"
killall cosmic-panel 2>/dev/null || true
echo -e "${GREEN}${BOLD}Installed.${NC}"
echo -e "  - Binaries: ${TARGET_DIR}/cosmic-applet-translator, ${TARGET_DIR}/cosmic-translator-overlay"
echo -e "  - Add to panel: Settings -> Desktop -> Panel -> Configure panel applets -> Add applet -> Screen Translator"
echo -e "  - Uninstall: run uninstall.sh from the repository or release archive\n"

# Japanese and Chinese models are downloaded on first use; Tesseract is a fallback and
# the only engine for Korean.
if ! command -v tesseract &>/dev/null; then
    echo -e "${YELLOW}[!] Optional:${NC} Japanese and Chinese OCR models (about 21 MB) are downloaded on first use."
    echo -e "    Tesseract is used as a fallback and is required for Korean:"
    echo -e "    Debian/Ubuntu/Pop!_OS: sudo apt install tesseract-ocr tesseract-ocr-jpn tesseract-ocr-jpn-vert"
    echo -e "    Arch:                  sudo pacman -S tesseract tesseract-data-jpn tesseract-data-jpn_vert\n"
fi
