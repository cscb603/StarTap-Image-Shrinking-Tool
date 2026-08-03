#!/bin/bash
# 星TAP 高清缩图 v4.4.2 - macOS App 打包脚本
# 星 TAP 实验室出品

set -e

APP_NAME="图片高速压缩"
VERSION="4.4.2"
APP_DIR="${APP_NAME}.app"
BINARY_NAME="ImageCompressor"

echo "🚀 开始构建 ${APP_NAME} v${VERSION}..."

# 1. 清理旧的构建
echo "🧹 清理旧的构建..."
rm -rf "${APP_DIR}"

# 2. 总是重新编译 Release 版本（源码可能已改，不能因 target 存在就跳过，否则会把旧二进制原样打进 app）
# 注意：binary 需要 gui+cli features，否则 cargo build --release 默认只编译 lib，不会生成 rust_image_compressor
echo "🦀 编译 Release 版本..."
cargo build --release --features "gui,cli"

# 2.5 防御：确保二进制真的生成了
if [ ! -f "target/release/rust_image_compressor" ]; then
    echo "❌ 错误：未找到 target/release/rust_image_compressor，编译失败"
    exit 1
fi

# 3. 创建 App Bundle 结构
echo "📦 创建 App Bundle 结构..."
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

# 4. 拷贝二进制文件
echo "📋 拷贝二进制文件..."
cp "target/release/rust_image_compressor" "${APP_DIR}/Contents/MacOS/${BINARY_NAME}"
chmod +x "${APP_DIR}/Contents/MacOS/${BINARY_NAME}"

# 5. 拷贝图标资源
echo "🎨 拷贝图标资源..."

if [ -f "icon.icns" ]; then
    cp "icon.icns" "${APP_DIR}/Contents/Resources/AppIcon.icns"
    echo "✅ 使用 icon.icns 图标"
else
    echo "⚠️ 警告：未找到 icon.icns 文件"
fi

# 拷贝 icon.png (GUI 二进制嵌入用)
if [ -f "icon.png" ]; then
    cp "icon.png" "${APP_DIR}/Contents/Resources/"
fi

# 6. 创建 Info.plist
echo "📝 创建 Info.plist..."
cat > "${APP_DIR}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.xtap.image-compressor</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

# 7. 先清除扩展属性（避免 codesign 因 com.apple.ResourceFork 等脏 xattr 失败）
echo "🧹 清除扩展属性..."
xattr -cr "${APP_DIR}" || true

# 8. 代码签名（必须在清 xattr 之后，否则签名落在脏属性上无效）
echo "🔐 执行代码签名..."
codesign --force --deep --sign - "${APP_DIR}" || true

# 9. 验证 App 结构
echo "✅ 验证 App 结构..."
echo "App Bundle 内容:"
ls -la "${APP_DIR}/Contents/MacOS/"
ls -la "${APP_DIR}/Contents/Resources/"

echo ""
echo "======================================"
echo "✅ 构建完成！"
echo "======================================"
echo "📱 App 位置：${APP_DIR}"
echo "🎉 可以使用了！"
echo ""
