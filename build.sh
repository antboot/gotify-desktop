#!/bin/bash

# ==============================================================================
# Gotify Notifier 统一构建与发布脚本
# 支持自动解决 macOS 27 本地网络权限与代码签名问题 (os error 65)
# 用法:
#   ./build.sh          -> 标准 Release 生产构建、签名、打包 DMG
#   ./build.sh --run    -> 构建、签名并立即启动运行测试
#   ./build.sh --debug  -> 快速 Debug 增量构建、签名
# ==============================================================================

set -e

DO_RUN=false
BUILD_MODE="release"
BUILD_FLAG=""

for arg in "$@"; do
    case $arg in
        --run|-r)
            DO_RUN=true
            shift
            ;;
        --debug|-d)
            BUILD_MODE="debug"
            BUILD_FLAG="--debug"
            shift
            ;;
    esac
done

echo "🚀 开始构建 Gotify Notifier [模式: $BUILD_MODE]..."

# 1. 检查 Rust 环境
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust 未安装，请先安装 Rust:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# 2. 检查 Tauri CLI
if ! command -v cargo-tauri &> /dev/null; then
    echo "📦 安装 Tauri CLI..."
    cargo install tauri-cli
fi

# 3. 如果当前有正在运行的实例，友好退出
if pgrep -x "gotify-notifier" > /dev/null; then
    echo "⚠️ 检测到正在运行的 Gotify Notifier，正在关闭以完成更新..."
    killall "gotify-notifier" 2>/dev/null || true
    sleep 1
fi

# 4. 执行 Tauri 构建
echo "🔨 正在编译应用..."
if [ "$BUILD_MODE" = "debug" ]; then
    cargo tauri build --debug --bundles app
    APP_PATH="src-tauri/target/debug/bundle/macos/Gotify Notifier.app"
    DMG_DIR="src-tauri/target/debug/bundle/dmg"
else
    cargo tauri build --bundles app
    APP_PATH="src-tauri/target/release/bundle/macos/Gotify Notifier.app"
    DMG_DIR="src-tauri/target/release/bundle/dmg"
fi

# 5. 核心：macOS 规范代码签名、Entitlements 与资源密封（彻底根治 os error 65: No route to host）
if [ -d "$APP_PATH" ]; then
    echo "🔏 正在检查本地代码签名身份..."
    SIGN_IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null | grep -E "Apple Development|Developer ID Application" | head -n 1 | awk -F'"' '{print $2}' || true)
    ENTITLEMENTS="src-tauri/Entitlements.plist"

    if [ -n "$SIGN_IDENTITY" ]; then
        echo "🔑 找到有效开发者证书: $SIGN_IDENTITY"
        echo "🔏 正在使用开发者证书执行深度签名 (包含网络 Entitlements)..."
        codesign --force --deep --sign "$SIGN_IDENTITY" --entitlements "$ENTITLEMENTS" --identifier "com.gotify.notifier" "$APP_PATH"
    else
        echo "⚠️ 未找到开发者证书，使用 Ad-hoc 签名并固化指定需求 (Designated Requirement)..."
        codesign --force --deep --sign - --entitlements "$ENTITLEMENTS" --identifier "com.gotify.notifier" -r='=designated => identifier "com.gotify.notifier"' "$APP_PATH"
    fi
    
    # 验证签名绑定
    SIGN_ID=$(codesign -dvvv "$APP_PATH" 2>&1 | grep "Identifier=com.gotify.notifier" || true)
    if [ -n "$SIGN_ID" ]; then
        echo "✅ 代码签名绑定成功: com.gotify.notifier"
    fi

    # 生成包含签名后 App 的 DMG (如果是 Release 构建)
    if [ "$BUILD_MODE" = "release" ]; then
        echo "📦 正在打包包含已签名 App 的 DMG 安装包..."
        mkdir -p "$DMG_DIR"
        TMP_DMG_DIR=$(mktemp -d)
        cp -R "$APP_PATH" "$TMP_DMG_DIR/"
        ln -s /Applications "$TMP_DMG_DIR/Applications"
        DMG_FILE="$DMG_DIR/Gotify Notifier_0.4.0_aarch64.dmg"
        rm -f "$DMG_FILE"
        hdiutil create -volname "Gotify Notifier" -srcfolder "$TMP_DMG_DIR" -ov -format UDZO "$DMG_FILE" > /dev/null 2>&1
        echo "✅ DMG 安装包打包完成: $DMG_FILE"
        rm -rf "$TMP_DMG_DIR"
    fi

    # 自动同步更新到系统的 /Applications/Gotify Notifier.app
    echo "📲 正在同步安装到 /Applications..."
    rm -rf "/Applications/Gotify Notifier.app"
    cp -R "$APP_PATH" "/Applications/"
    if [ -n "$SIGN_IDENTITY" ]; then
        codesign --force --deep --sign "$SIGN_IDENTITY" --entitlements "$ENTITLEMENTS" --identifier "com.gotify.notifier" "/Applications/Gotify Notifier.app"
    else
        codesign --force --deep --sign - --entitlements "$ENTITLEMENTS" --identifier "com.gotify.notifier" -r='=designated => identifier "com.gotify.notifier"' "/Applications/Gotify Notifier.app"
    fi
    # 清理 quarantine 隔离属性并刷新 LaunchServices 缓存
    xattr -cr "/Applications/Gotify Notifier.app"
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "/Applications/Gotify Notifier.app" 2>/dev/null || true
    echo "✅ /Applications/Gotify Notifier.app 更新、签名并注册完成"
fi

echo ""
echo "🎉 构建与签名全部完成！"
echo "📱 生产应用位置: /Applications/Gotify Notifier.app"

# 6. 若传入了 --run，则立即启动
if [ "$DO_RUN" = true ]; then
    echo "🚀 正在启动 Gotify Notifier..."
    open "/Applications/Gotify Notifier.app"
fi
