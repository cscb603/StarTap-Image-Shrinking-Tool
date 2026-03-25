# 图片高速压缩工具 v4.0 - AI 接入说明

## 📍 可执行文件位置
```
f:\trae-cn\图片高速压缩 rust 版\target\release\rust_image_compressor.exe
```

---

## 🤖 AI 调用方式

### 方式一：JSON 模式（推荐，最适合 AI）

```bash
rust_image_compressor.exe --json
```

通过 stdin 传入 JSON 配置：

```json
{
  "version": "1.0",
  "files": [
    "C:/图片/test1.jpg",
    "C:/图片/test2.png"
  ],
  "mode": "custom",
  "max_dim": 1920,
  "quality": 85,
  "target_kb": 0,
  "output_dir": "C:/输出目录",
  "overwrite": false,
  "keep_original_name": false,
  "output_format": "jpeg"
}
```

**返回结果示例：**
```json
{
  "success": true,
  "total": 2,
  "completed": 2,
  "failed": 0,
  "results": [
    {
      "input": "C:/图片/test1.jpg",
      "output": "C:/输出目录/test1_custom.jpg",
      "success": true,
      "error": null,
      "original_size": 22692140,
      "compressed_size": 332214,
      "compression_ratio": 68.3
    }
  ]
}
```

### 方式二：CLI 参数模式

**单文件处理：**
```bash
rust_image_compressor.exe --input "图片路径.jpg" --output-dir "输出目录" --max-dim 1920 --quality 85
```

**批量处理目录：**
```bash
rust_image_compressor.exe --input "图片目录" --output-dir "输出目录" --max-dim 1920 --quality 85
```

---

## 📋 参数说明

### 输入参数

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `--input` | 文件或目录路径 | 必需 | 支持单个文件或整个目录 |
| `--output-dir` | 目录路径 | 原目录 | 输出目录路径 |
| `--mode` | custom/wechat/hd | custom | 处理模式 |
| `--max-dim` | 像素数 | 3000 | 最大尺寸（宽或高） |
| `--quality` | 1-100 | 95 | JPEG 质量 |
| `--target-kb` | KB数 | 0 | 目标文件大小（0=不限制） |
| `--output-format` | jpeg/keep-original | jpeg | 输出格式 |
| `--json` | 标志 | false | 启用 JSON 输入/输出模式 |
| `--quiet` | 标志 | false | 静默模式，减少输出 |

### JSON 模式额外参数

| 参数 | 类型 | 说明 |
|------|------|------|
| `version` | string | 版本号，如 "1.0" |
| `keep_original_name` | bool | 保留原始文件名 |
| `overwrite` | bool | 覆盖原文件 |

---

## 🎯 OpenClaw 调用示例

### Python 调用示例
```python
import subprocess
import json

json_input = {
    "version": "1.0",
    "files": ["F:/图片/test.jpg"],
    "mode": "custom",
    "max_dim": 1920,
    "quality": 85,
    "output_dir": "F:/输出目录"
}

result = subprocess.run(
    ["rust_image_compressor.exe", "--json"],
    input=json.dumps(json_input),
    capture_output=True,
    text=True
)

result_data = json.loads(result.stdout)
print(f"成功: {result_data['completed']}/{result_data['total']}")
```

### JavaScript 调用示例
```javascript
const { spawn } = require('child_process');

const jsonInput = {
    version: "1.0",
    files: ["F:/图片/test.jpg"],
    mode: "custom",
    max_dim: 1920,
    quality: 85,
    output_dir: "F:/输出目录"
};

const proc = spawn('rust_image_compressor.exe', ['--json']);
proc.stdin.write(JSON.stringify(jsonInput));
proc.stdin.end();

proc.stdout.on('data', (data) => {
    const result = JSON.parse(data.toString());
    console.log(`成功: ${result.completed}/${result.total}`);
});
```

### curl 模拟调用（通过 PowerShell）
```powershell
$jsonInput = @{
    version = "1.0"
    files = @("F:/图片/test.jpg")
    mode = "custom"
    max_dim = 1920
    quality = 85
    output_dir = "F:/输出目录"
} | ConvertTo-Json

$result = $jsonInput | .\rust_image_compressor.exe --json
$result | ConvertFrom-Json | Select-Object success, completed, failed
```

---

## 🔧 路径处理特性

程序内置智能路径处理，AI 调用时无需担心：

✅ **自动处理空格路径**： `"C:/Program Files/test.jpg"`  
✅ **中文路径支持**：`"C:/用户/图片/test.jpg"`  
✅ **大小写修复**：自动匹配文件系统实际大小写  
✅ **跨驱动器路径**：自动检测并修复  
✅ **相对/绝对路径**：自动转换为绝对路径  

---

## 📊 性能数据

| 场景 | 数据 |
|------|------|
| 批量处理成功率 | **17/17 (100%)** |
| 处理耗时 | **33.64秒 / 17张** |
| 大图压缩比 | **52-79倍** |
| 内存占用 | **自动优化，大文件使用 mmap** |
| 支持格式 | JPG, PNG, WebP, ICO, RAW (DNG/CR2/NEF等) |

---

## ⚠️ 注意事项

1. **EXIF 保留**：JPEG 图片自动保留 EXIF 元数据
2. **色彩空间**：默认保持原始色彩空间
3. **大文件**：>200MB 的文件自动使用内存映射优化
4. **临时文件**：RAW 格式处理会在临时目录创建中间文件
5. **错误处理**：失败自动跳过，不影响批量处理继续

---

## 🐛 故障排除

### 问题：路径包含空格导致失败
**解决**：程序已自动处理，无需额外操作

### 问题：中文文件名显示乱码
**解决**：GUI 字体已配置 Microsoft YaHei，如仍有问题检查系统字体

### 问题：处理大图时内存不足
**解决**：程序自动使用内存映射，但仍建议关闭其他大型程序

### 问题：RAW 格式处理失败
**解决**：仅支持 macOS 的 RAW 格式（通过 sips/qlmanage），Windows 用户请使用预置的 JPG/PNG/WebP

---

## 📞 技术支持

如遇问题，请提供：
1. 完整的命令行输出
2. 输入文件信息（格式、大小）
3. 错误日志（stderr 输出）

---

**版本**: v4.0.0
**构建时间**: 2026-03-25
**Rust 版本**: 现代化编译优化
