#!/usr/bin/env bash
# ==============================================================================
# COSMIC Screen Translator (cosmic-ext-translator) - Installer
# Author: Hasan Hüseyin Yolcu <hasanhuseyinyolcu25@gmail.com>
# License: GPL-3.0-or-later
# ==============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

TARGET_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
METAINFO_DIR="${HOME}/.local/share/metainfo"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [[ -n "${SCRIPT_PATH}" ]]; then
    REPO_ROOT="$(cd "$(dirname "${SCRIPT_PATH}")" 2>/dev/null && pwd || pwd)"
else
    REPO_ROOT="$(pwd)"
fi
GITHUB_REPO="Hasmolam/cosmic-ext-translator"
RAW_BASE_URL="https://raw.githubusercontent.com/${GITHUB_REPO}/master"
DEFAULT_ARCHIVE_NAME="cosmic-ext-translator-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"

for arg in "$@"; do
    case "$arg" in
        --help|-h)
            echo "Usage: ./install.sh [OPTIONS]"
            echo "Options:"
            echo "  --help, -h    Show this help message"
            exit 0
            ;;
    esac
done

echo -e "${BLUE}${BOLD}=== COSMIC Screen Translator Installer ===${NC}\n"

mkdir -p "${TARGET_DIR}" "${APP_DIR}" "${METAINFO_DIR}" "${ICON_DIR}"

if [[ ":${PATH}:" != *":${TARGET_DIR}:"* ]]; then
    echo -e "${YELLOW}[!] Warning:${NC} ${TARGET_DIR} is not in your PATH."
    echo -e "    Add 'export PATH=\"\$HOME/.local/bin:\$PATH\"' to your ~/.bashrc or ~/.zshrc."
fi

INSTALLED=0

# Clean old destination binaries to avoid ETXTBSY if running
rm -f "${TARGET_DIR}/cosmic-applet-translator" "${TARGET_DIR}/cosmic-translator-overlay"

# Option A: Prebuilt binaries in current package/tarball directory
if [[ -f "${REPO_ROOT}/cosmic-applet-translator" && -f "${REPO_ROOT}/cosmic-translator-overlay" ]]; then
    echo -e "${BLUE}[*] Found binaries in release package.${NC}"
    install -m 0755 "${REPO_ROOT}/cosmic-applet-translator" "${TARGET_DIR}/"
    install -m 0755 "${REPO_ROOT}/cosmic-translator-overlay" "${TARGET_DIR}/"
    INSTALLED=1

# Option B: Local compiled release binaries in repo
elif [[ -f "${REPO_ROOT}/target/release/cosmic-applet-translator" && -f "${REPO_ROOT}/target/release/cosmic-translator-overlay" ]]; then
    echo -e "${BLUE}[*] Found locally compiled release binaries.${NC}"
    install -m 0755 "${REPO_ROOT}/target/release/cosmic-applet-translator" "${TARGET_DIR}/"
    install -m 0755 "${REPO_ROOT}/target/release/cosmic-translator-overlay" "${TARGET_DIR}/"
    INSTALLED=1

# Option C: Download latest prebuilt release archive from GitHub Releases
else
    echo -e "${BLUE}[*] Downloading prebuilt release archive from GitHub Releases...${NC}"
    TMP_DIR=$(mktemp -d)

    # Attempt to resolve latest release tarball URL dynamically
    API_URL="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    DOWNLOAD_URL=""
    if command -v curl &>/dev/null; then
        DOWNLOAD_URL=$(curl -fsSL "${API_URL}" 2>/dev/null | grep "browser_download_url.*x86_64.*\.tar\.gz" | head -n 1 | cut -d '"' -f 4 || true)
    fi
    if [[ -z "${DOWNLOAD_URL}" ]]; then
        DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/latest/download/${DEFAULT_ARCHIVE_NAME}"
    fi

    TMP_ARCHIVE="${TMP_DIR}/release.tar.gz"

    if command -v curl &>/dev/null; then
        curl -fsSL "${DOWNLOAD_URL}" -o "${TMP_ARCHIVE}" || {
            echo -e "${RED}[x] Failed to download prebuilt archive.${NC}"
            rm -rf "${TMP_DIR}"
            exit 1
        }
    elif command -v wget &>/dev/null; then
        wget -qO "${TMP_ARCHIVE}" "${DOWNLOAD_URL}" || {
            echo -e "${RED}[x] Failed to download prebuilt archive.${NC}"
            rm -rf "${TMP_DIR}"
            exit 1
        }
    else
        echo -e "${RED}[x] Neither curl nor wget found.${NC}"
        rm -rf "${TMP_DIR}"
        exit 1
    fi

    tar -xzf "${TMP_ARCHIVE}" -C "${TMP_DIR}"
    # Find extracted directory
    EXTRACTED_DIR=$(find "${TMP_DIR}" -mindepth 1 -maxdepth 1 -type d | head -n 1)
    if [[ -z "${EXTRACTED_DIR}" || ! -f "${EXTRACTED_DIR}/cosmic-applet-translator" ]]; then
        EXTRACTED_DIR="${TMP_DIR}"
    fi

    install -m 0755 "${EXTRACTED_DIR}/cosmic-applet-translator" "${TARGET_DIR}/"
    install -m 0755 "${EXTRACTED_DIR}/cosmic-translator-overlay" "${TARGET_DIR}/"

    # Also install desktop files from archive if available
    if [[ -f "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.desktop" ]]; then
        install -m 0644 "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.desktop" "${APP_DIR}/"
    fi
    if [[ -f "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.metainfo.xml" ]]; then
        install -m 0644 "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.metainfo.xml" "${METAINFO_DIR}/"
    fi
    if [[ -f "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.svg" ]]; then
        install -m 0644 "${EXTRACTED_DIR}/com.system76.CosmicExtTranslator.svg" "${ICON_DIR}/"
    fi

    rm -rf "${TMP_DIR}"
    INSTALLED=1
fi

# Install desktop entry, metainfo, and icon if local
DESKTOP_SRC="${REPO_ROOT}/crates/cosmic-applet-translator/target/xdgen/app.desktop"
METAINFO_SRC="${REPO_ROOT}/crates/cosmic-applet-translator/target/xdgen/app.metainfo.xml"
ICON_SRC="${REPO_ROOT}/crates/cosmic-applet-translator/resources/icon.svg"

if [[ -f "${DESKTOP_SRC}" ]]; then
    install -m 0644 "${DESKTOP_SRC}" "${APP_DIR}/com.system76.CosmicExtTranslator.desktop"
elif [[ ! -f "${APP_DIR}/com.system76.CosmicExtTranslator.desktop" ]]; then
    if command -v curl &>/dev/null; then
        curl -fsSL "${RAW_BASE_URL}/crates/cosmic-applet-translator/target/xdgen/app.desktop" -o "${APP_DIR}/com.system76.CosmicExtTranslator.desktop" || true
    fi
fi

if [[ -f "${METAINFO_SRC}" ]]; then
    install -m 0644 "${METAINFO_SRC}" "${METAINFO_DIR}/com.system76.CosmicExtTranslator.metainfo.xml"
fi

if [[ -f "${ICON_SRC}" ]]; then
    install -m 0644 "${ICON_SRC}" "${ICON_DIR}/com.system76.CosmicExtTranslator.svg"
elif [[ ! -f "${ICON_DIR}/com.system76.CosmicExtTranslator.svg" ]]; then
    if command -v curl &>/dev/null; then
        curl -fsSL "${RAW_BASE_URL}/crates/cosmic-applet-translator/resources/icon.svg" -o "${ICON_DIR}/com.system76.CosmicExtTranslator.svg" || true
    fi
fi

# Refresh desktop and icon database caches
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# Restart panel to register applet
if [[ "${INSTALLED}" -eq 1 ]]; then
    echo -e "\n${BLUE}[*] Restarting cosmic-panel to register applet...${NC}"
    killall cosmic-panel 2>/dev/null || true
    echo -e "${GREEN}${BOLD}✓ Success!${NC} COSMIC Screen Translator is installed."
    echo -e "  - Binaries: ${TARGET_DIR}/cosmic-applet-translator, ${TARGET_DIR}/cosmic-translator-overlay"
    echo -e "  - Add to panel: Settings -> Desktop -> Panel -> Configure panel applets -> Add applet -> Screen Translator"
    echo -e "  - To uninstall anytime, run: ./uninstall.sh\n"
fi
