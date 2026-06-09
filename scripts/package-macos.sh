#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANAGER_DIR="$ROOT_DIR/apps/codex-pilot-manager"
TARGET_TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | awk '/host:/ { print $2 }')}"
PROFILE="${PROFILE:-release}"
VERSION="${VERSION:-$(awk -F\" '/"version"/ { print $4; exit }' "$MANAGER_DIR/src-tauri/tauri.conf.json")}"

if [[ "$TARGET_TRIPLE" != *"apple-darwin" ]]; then
  echo "当前脚本只用于 macOS 打包，检测到 target：$TARGET_TRIPLE" >&2
  exit 1
fi

if ! command -v create-dmg >/dev/null 2>&1; then
  echo "未找到 create-dmg。请先安装：brew install create-dmg" >&2
  exit 1
fi

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin) RELEASE_ARCH="arm64" ;;
  x86_64-apple-darwin) RELEASE_ARCH="x64" ;;
  *) RELEASE_ARCH="${TARGET_TRIPLE%%-*}" ;;
esac

if [[ "$PROFILE" == "release" ]]; then
  CARGO_PROFILE_FLAG="--release"
  TARGET_PROFILE_DIR="release"
else
  CARGO_PROFILE_FLAG=""
  TARGET_PROFILE_DIR="debug"
fi

SIDECAR_DIR="$MANAGER_DIR/src-tauri/binaries"
SIDECAR_PATH="$SIDECAR_DIR/codex-pilot-launcher-$TARGET_TRIPLE"

mkdir -p "$SIDECAR_DIR"

echo "构建 launcher sidecar：$TARGET_TRIPLE ($PROFILE)"
cargo build -p codex-pilot-launcher $CARGO_PROFILE_FLAG --target "$TARGET_TRIPLE"
cp "$ROOT_DIR/target/$TARGET_TRIPLE/$TARGET_PROFILE_DIR/codex-pilot-launcher" "$SIDECAR_PATH"
chmod +x "$SIDECAR_PATH"

echo "构建 Manager 前端"
(
  cd "$MANAGER_DIR"
  npm run vite:build
)

echo "构建 Tauri app/dmg"
APP_PATH="$ROOT_DIR/target/$TARGET_TRIPLE/$TARGET_PROFILE_DIR/bundle/macos/CodexPilot.app"
rm -rf "$APP_PATH"
(
  cd "$MANAGER_DIR"
  npm run tauri -- build --target "$TARGET_TRIPLE" --bundles app
)

sign_app_ad_hoc() {
  local app_path="$1"
  local executable

  executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app_path/Contents/Info.plist")"

  codesign --force --sign - "$app_path/Contents/MacOS/$executable"

  while IFS= read -r sidecar; do
    [[ "$sidecar" == "$app_path/Contents/MacOS/$executable" ]] && continue
    codesign --force --sign - "$sidecar"
  done < <(find "$app_path/Contents/MacOS" -type f -perm -111)

  codesign --force --deep --sign - "$app_path"
  codesign --verify --deep --strict "$app_path"
  codesign -dv "$app_path" >/dev/null 2>&1
}

DIST_DIR="$ROOT_DIR/dist/macos"
STAGE_DIR="$DIST_DIR/stage"
DMG_PATH="$DIST_DIR/CodexPilot-${VERSION}-macos-${RELEASE_ARCH}.dmg"
VOLUME_NAME="CodexPilot"
REPAIR_ICON_GENERATOR="$ROOT_DIR/assets/dmg/generate_repair_icon.py"

if [[ ! -d "$APP_PATH" ]]; then
  echo "未找到 app bundle：$APP_PATH" >&2
  exit 1
fi

echo "ad-hoc 签名 CodexPilot.app"
sign_app_ad_hoc "$APP_PATH"

echo "生成 DMG"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp -R "$APP_PATH" "$STAGE_DIR/"
cat > "$STAGE_DIR/已损坏修复.command" <<'SCRIPT'
#!/usr/bin/env bash
set -u

clear

RED="\033[0;31m"
GREEN="\033[0;32m"
YELLOW="\033[0;33m"
BLUE="\033[0;34m"
NC="\033[0m"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
APP_PATH="$(find "$SCRIPT_DIR" -maxdepth 1 -name '*.app' -print -quit)"

echo ""
echo -e "${BLUE}CodexPilot 已损坏提示修复工具${NC}"
echo ""

if [[ -z "$APP_PATH" ]]; then
  echo -e "${RED}未在当前磁盘映像中找到 app，请重新下载安装包。${NC}"
  echo ""
  read -r -p "按回车键关闭窗口..."
  exit 1
fi

APP_NAME="$(basename "$APP_PATH")"
INSTALLED_APP="/Applications/$APP_NAME"

if [[ ! -d "$INSTALLED_APP" ]]; then
  echo -e "${YELLOW}请先把 ${APP_NAME} 拖入 Applications，再运行本修复工具。${NC}"
  echo ""
  read -r -p "按回车键关闭窗口..."
  exit 1
fi

echo -e "${YELLOW}正在清理 ${APP_NAME} 的下载隔离标记。${NC}"
echo "如果系统要求输入密码，请输入当前 Mac 的开机密码。输入时不会显示字符。"
echo ""

if xattr -dr com.apple.quarantine "$INSTALLED_APP" 2>/dev/null; then
  :
elif sudo xattr -dr com.apple.quarantine "$INSTALLED_APP"; then
  :
else
  echo ""
  echo -e "${RED}修复失败，请确认 ${INSTALLED_APP} 存在且当前用户有权限修改。${NC}"
  echo ""
  read -r -p "按回车键关闭窗口..."
  exit 1
fi

echo ""
echo -e "${GREEN}修复完成，正在打开 ${APP_NAME}。${NC}"
open "$INSTALLED_APP"
echo ""
read -r -p "按回车键关闭窗口..."
SCRIPT
chmod +x "$STAGE_DIR/已损坏修复.command"

if [[ -f "$REPAIR_ICON_GENERATOR" ]] \
  && python3 -c 'import PIL' >/dev/null 2>&1 \
  && command -v sips >/dev/null 2>&1 \
  && command -v DeRez >/dev/null 2>&1 \
  && command -v Rez >/dev/null 2>&1 \
  && command -v SetFile >/dev/null 2>&1; then
  REPAIR_ICON_PNG="$STAGE_DIR/repair-tool-1024.png"
  REPAIR_ICON_RSRC="$STAGE_DIR/repair-tool.rsrc"
  python3 "$REPAIR_ICON_GENERATOR" "$REPAIR_ICON_PNG"
  sips -i "$REPAIR_ICON_PNG" >/dev/null
  DeRez -only icns "$REPAIR_ICON_PNG" > "$REPAIR_ICON_RSRC"
  Rez -append "$REPAIR_ICON_RSRC" -o "$STAGE_DIR/已损坏修复.command" >/dev/null
  SetFile -a C "$STAGE_DIR/已损坏修复.command"
  rm -f "$REPAIR_ICON_PNG" "$REPAIR_ICON_RSRC"
else
  echo "跳过修复工具图标装饰：缺少 Pillow 或 macOS 资源工具。" >&2
fi

mkdir -p "$STAGE_DIR/.background"
cat > "$STAGE_DIR/.background/background.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="900" height="450" viewBox="0 0 900 450">
  <rect width="900" height="450" fill="#f7f8fb"/>
  <path d="M0 315c116-70 226-72 370-6 154 70 314 82 530 26v115H0z" fill="#ffffff"/>
  <text x="335" y="138" text-anchor="middle" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif" font-size="16" fill="#111827">将左侧应用拖入到 Applications 即可安装</text>
  <g transform="translate(300 226) scale(2.7)" fill="none" stroke="#c4cbd5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M5 12h14"/>
    <path d="m12 5 7 7-7 7"/>
  </g>
</svg>
SVG
BACKGROUND_FILE=".background/background.svg"
if command -v sips >/dev/null 2>&1; then
  if sips -s format png "$STAGE_DIR/.background/background.svg" --out "$STAGE_DIR/.background/background.png" >/dev/null 2>&1; then
    BACKGROUND_FILE=".background/background.png"
  fi
fi
rm -f "$DMG_PATH"
create-dmg \
  --volname "$VOLUME_NAME" \
  --window-pos 120 120 \
  --window-size 900 450 \
  --background "$STAGE_DIR/$BACKGROUND_FILE" \
  --text-size 14 \
  --icon-size 100 \
  --icon "CodexPilot.app" 145 265 \
  --app-drop-link 525 265 \
  --icon "已损坏修复.command" 750 265 \
  --hide-extension "CodexPilot.app" \
  --hide-extension "已损坏修复.command" \
  --no-internet-enable \
  "$DMG_PATH" \
  "$STAGE_DIR"
rm -rf "$STAGE_DIR"

echo "macOS 打包完成：$DMG_PATH"
