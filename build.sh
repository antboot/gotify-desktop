#!/bin/bash

# Gotify Notifier 构建脚本

echo "🚀 开始构建 Gotify Notifier..."

# 检查 Rust 是否安装
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust 未安装，请先安装 Rust:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "✅ Rust 已安装"

# 检查 Tauri CLI
if ! command -v cargo-tauri &> /dev/null; then
    echo "📦 安装 Tauri CLI..."
    cargo install tauri-cli
fi

echo "✅ Tauri CLI 已准备就绪"

# 构建应用
echo "🔨 开始构建应用（这可能需要几分钟）..."
cargo tauri build

if [ $? -eq 0 ]; then
    APP_PATH="src-tauri/target/release/bundle/macos/Gotify Notifier.app"
    if [ -d "$APP_PATH" ]; then
        echo "🔏 正在对应用进行签名 (绑定 identifier com.gotify.notifier)..."
        codesign --force --deep --sign - --identifier "com.gotify.notifier" "$APP_PATH"
        echo "✅ 签名完成"
    fi

    echo ""
    echo "✅ 构建成功！"
    echo ""
    echo "📱 应用位置:"
    echo "   $APP_PATH"
    echo ""
    echo "🎉 你可以将应用拖动到 Applications 文件夹中使用"
else
    echo "❌ 构建失败，请检查错误信息"
    exit 1
fi
