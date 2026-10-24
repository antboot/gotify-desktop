# GitHub Actions 自动打包指南

## 📋 概述

这个 GitHub Actions 工作流可以自动为你的 Tauri 应用构建多平台安装包：
- **Windows**: MSI 和 NSIS 安装包
- **macOS**: DMG 和 APP 包（支持 Intel 和 Apple Silicon）
- **Linux**: DEB 和 AppImage 包

> ⚠️ **当前状态（2026-10-24）**：该工作流在 GitHub 上被手动禁用（`disabled_manually`），推送 tag **不会**触发构建。需要先在仓库 **Actions → Build and Release → Enable workflow** 中启用。当前 macOS 正式包由本地 `./build.sh` 签名构建后上传到已有 Release。

## 🚀 使用方法

### 方法 1: 手动触发构建（推荐用于测试）

1. 确保你的代码已推送到 GitHub
2. 进入你的 GitHub 仓库页面
3. 点击顶部的 **Actions** 标签
4. 在左侧选择 **Build and Release** 工作流
5. 点击右侧的 **Run workflow** 按钮
6. 选择分支（通常是 main 或 master）
7. 点击绿色的 **Run workflow** 按钮

构建完成后，你可以在工作流运行页面下载构建产物。

### 方法 2: 通过 Git Tag 自动触发（用于正式发布）

1. **在本地创建并推送 tag**:
   ```bash
   # 创建 tag（版本号根据实际情况修改；全平台共用一个版本号）
   git tag v0.5.0
   
   # 推送 tag 到 GitHub
   git push origin v0.5.0
   ```

2. **自动构建和发布**:
   - GitHub Actions 会自动检测到新的 tag
   - 开始构建所有平台的安装包
   - 构建完成后自动创建 GitHub Release（草稿状态）
   - 你可以在 Releases 页面编辑并发布

## 🍎 macOS 单平台发布（当前实际流程）

当只有 macOS 端需要更新、而 Windows / Linux 仍停留在旧版本时，**不要升版本号**。全平台保持同一个版本（当前为 `0.5.0`），只替换该 Release 下的 macOS 产物：

```bash
VERSION=0.5.0
ARCH=$(uname -m)   # Apple Silicon 为 arm64

# 1. 构建：编译 + 签名 + 打包 DMG + 安装到 /Applications + 启动
./build.sh --run

# 2. 按 Release 约定重命名产物（避免 --clobber 覆盖不到旧文件）
cp "src-tauri/target/release/bundle/dmg/Gotify Notifier_${VERSION}_${ARCH}.dmg" \
   "/tmp/Gotify_Notifier_${VERSION}_aarch64.dmg"
ditto -c -k --sequesterRsrc --keepParent \
  "src-tauri/target/release/bundle/macos/Gotify Notifier.app" \
  "/tmp/Gotify_Notifier_${VERSION}_aarch64.zip"

# 3. 覆盖已有 Release 的 macOS 资源（Windows 资源保持不动）
gh release upload "v${VERSION}" \
  "/tmp/Gotify_Notifier_${VERSION}_aarch64.dmg" \
  "/tmp/Gotify_Notifier_${VERSION}_aarch64.zip" --clobber

# 4. 在发布说明里追加本次刷新记录
gh release edit "v${VERSION}" --notes-file release-notes.md
```

注意事项：
- CI 构建的 macOS 包没有开发者证书与 Entitlements，无法通过本地网络权限（`os error 65`）。macOS 包应使用本地 `./build.sh` 构建。
- 只覆盖对应架构的资源文件名；不要删除 Windows / Linux 产物。
- 版本号同步写在 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`，改版本时必须三处一致。

## 📦 下载构建产物

### 手动触发的构建

1. 进入 **Actions** 页面
2. 点击对应的工作流运行记录
3. 滚动到页面底部的 **Artifacts** 部分
4. 下载你需要的平台安装包：
   - `windows-installer`: Windows 安装包
   - `macos-x86_64-apple-darwin-installer`: macOS Intel 版本
   - `macos-aarch64-apple-darwin-installer`: macOS Apple Silicon 版本
   - `linux-installer`: Linux 安装包

### 通过 Tag 触发的构建

1. 进入仓库的 **Releases** 页面
2. 找到对应版本的 Release（初始为草稿状态）
3. 点击编辑，添加更新说明
4. 点击 **Publish release** 发布
5. 用户可以直接从 Releases 页面下载安装包

## 🔧 配置说明

### 触发条件

工作流在以下情况下会触发：
- 推送以 `v` 开头的 tag（如 v1.0.0, v2.1.3）
- 手动触发（workflow_dispatch）

### 构建平台

当前配置会构建以下平台：
- Windows (x86_64)
- macOS Intel (x86_64)
- macOS Apple Silicon (ARM64)
- Linux (x86_64)

### 构建产物类型

- **Windows**: MSI 和 NSIS 安装程序
- **macOS**: DMG 磁盘映像和 APP 包
- **Linux**: DEB 包和 AppImage

## 📝 注意事项

1. **首次使用**: 
   - 确保你的 GitHub 仓库已启用 Actions（Settings → Actions → General → Allow all actions）
   - 确保 GITHUB_TOKEN 有写入权限（Settings → Actions → General → Workflow permissions → Read and write permissions）

2. **构建时间**: 
   - 完整构建所有平台大约需要 20-40 分钟
   - macOS 构建通常最慢（需要构建两个架构）

3. **产物保留**: 
   - Artifacts 默认保留 90 天
   - Release 中的文件永久保留

4. **私有仓库**: 
   - 如果仓库是私有的，Actions 有使用限制（免费账户每月 2000 分钟）
   - 公开仓库没有限制

## 🎯 快速开始示例

完整的发布流程示例：

```bash
# 1. 确保所有改动已提交
git add .
git commit -m "准备发布 v0.5.0"

# 2. 推送到 GitHub
git push origin main

# 3. 创建并推送 tag
git tag v0.5.0
git push origin v0.5.0

# 4. 等待 GitHub Actions 完成构建（可以在 Actions 页面查看进度）

# 5. 前往 Releases 页面编辑并发布
```

## 🔍 故障排查

如果构建失败：

1. **查看日志**: 进入 Actions 页面，点击失败的工作流，查看详细日志
2. **常见问题**:
   - Rust 版本不兼容：检查 `rust-toolchain` 版本
   - 依赖缺失：检查 Linux 依赖安装步骤
   - 图标文件缺失：确保 `src-tauri/icons/` 目录下有所需图标

3. **手动触发重试**: 在 Actions 页面可以重新运行失败的工作流

## 📚 更多资源

- [Tauri 官方文档](https://tauri.app/v2/guides/building/)
- [GitHub Actions 文档](https://docs.github.com/en/actions)
- [Tauri Action](https://github.com/tauri-apps/tauri-action)

## 🎉 完成！

现在你可以轻松地为多个平台构建和发布应用了！
