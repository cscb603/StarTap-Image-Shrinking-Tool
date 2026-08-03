# AGENTS.md — 星TAP 高清缩图 给 AI 的说明书

本工具常被 AI/LLM 调用，支持人用 GUI + AI 用 CLI 双模式。

## 常用命令

- **压缩单文件**：`图片高速压缩 "照片.jpg"`
- **压缩目录（递归）**：`图片高速压缩 "目录"`（子文件夹自动扫，`--no-recursive` 只扫当前层）
- **JSON 输入（管道）**：`echo '{"files":["照片.jpg","a b.jpg"]}' | 图片高速压缩 --json`
- **JSON 输入（直接参数）**（推荐 AI 用）：`图片高速压缩 --json-in '{"files":["照片.jpg"],"quality":85}'`
- **带全部参数（含摄影级优化）**：`图片高速压缩 --json-in '{"files":["照片.jpg"],"quality":85,"max_dim":2000,"enable_sharpening":true,"color_space":"srgb"}'`
- **仅看 JSON 输出**：`图片高速压缩 -i "照片.jpg" --quiet --json`
- **预演模式（安全！先试后压）**：`图片高速压缩 --json-in '{"files":["照片.jpg"],"dry_run":true}'`
- **能力探测**：`图片高速压缩 --capabilities`
- **环境自检**：`图片高速压缩 --self-check`（内置测试图走完整管线，输出健康报告；失败退出码 1）
- **强制重压**：`图片高速压缩 --json-in '{"files":["照片.jpg"],"force":true}'`（默认幂等续跑，输出已存在则跳过，结果标记 `skipped:true`）
- **JSONL 流式输出**：`图片高速压缩 --json-in '{"files":["照片.jpg"],"jsonl":true}'`（每处理完一个文件输出一行 JSON，末尾追加汇总信封；中断也保留已处理记录）
- **限流并发**：`图片高速压缩 --json-in '{"files":["照片.jpg"],"max_workers":4}'`（最大并行 worker，默认=CPU 核心数；服务器共享负载建议减半）
- **查看帮助**：`图片高速压缩 --help`

## 标准 JSON 信封（Agent-First 规范）

stdout **只输出以下 JSON**，stderr 放日志/进度/警告。退出码：0=正常（含隐藏文件跳过/透传），1=有真正失败（不支持且未透传/解码损坏/权限），2=参数错。

```json
{
  "schema_version": "1.0",
  "command": "compress",
  "status": "succeeded"|"partial",
  "data": {
    "total": 5,
    "completed": 5,
    "failed": 0,
    "results": [
      {
        "input": "照片.jpg",
        "output": "照片_da.jpg",
        "success": true,
        "error": null,
        "error_type": null,
        "skipped": null,
        "passthrough": null,
        "original_size": 5000000,
        "compressed_size": 1200000,
        "compression_ratio": 4.17
      }
    ],
    "skipped": 0,
    "manifest": [
      {"input": "照片.jpg", "output": "照片_da.jpg", "status": "compressed"}
    ]
  },
  "warnings": [],
  "errors": [],
  "metrics": {
    "original_bytes": 5000000,
    "compressed_bytes": 1200000,
    "bytes_saved": 3800000,
    "avg_ratio": 4.17,
    "total_time_ms": 1234
  }
}
```

## 不变量（请遵守）

1. **路径含空格/中文必须用引号**：传 shell 时需 `"路径"` 或 `'路径'`。AI 最稳的方式是 `--json-in` 传 JSON 字符串（零 shell 分词损失）。
2. **RAW 格式仅在 macOS 支持**：CR3/NEF/ARW/DNG 等在 Windows 上报错（`status: "partial"`，`errors` 含提示）。请先在 Mac 处理或转 JPG。
3. **目录默认递归**：最深 20 层。不想递归加 `--no-recursive`。
4. **`--dry-run` 现在支持**：预演模式只扫文件不压缩，输出文件列表和配置。AI 调用前置校验任务配置，避免大规模误操作。
5. **`output_dir` 在 `--json-in` 模式下已生效**：指定输出目录（绝对/相对路径均可），不指定时默认输出到 `./compressed/`。
6. **`--json` / `--json-in` 模式失败时退出码 1**：让 AI 脚本能检测处理结果。

## JSON 输入格式（--json / --json-in）

```json
{
  "files": ["照片1.jpg", "照片2.png"],
  "quality": 85,
  "max_dim": 3000,
  "target_kb": 0,
  "mode": "custom",
  "output_format": "jpeg",
  "overwrite": false,
  "keep_original_name": false,
  "output_dir": "/输出目录",
  "enable_sharpening": false,
  "sharpening_radius": 1.0,
  "sharpening_amount": 0.8,
  "use_custom_quantization": false,
  "preserve_high_frequency": false,
  "color_space": "keep",
  "recursive": true,
  "include_pattern": null,
  "exclude_pattern": null,
  "flatten": false,
  "dry_run": false,
  "force": false,
  "jsonl": false,
  "max_workers": null,
  "preserve_structure": false,
  "output_suffix": null,
  "passthrough_unsupported": false
}
```

所有字段可选（除 `files`），缺省用默认值：

### 基本参数
| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `files` | `string[]` | **必填** | 待处理的文件路径列表 |
| `mode` | `string` | `"custom"` | `"wechat"` / `"hd"` / `"custom"` |
| `quality` | `number` | `85` | JPEG 质量 1-100（95+ 重压小图可能反胀） |
| `max_dim` | `number` | `3000` | 最长边像素，0=不缩放 |
| `target_kb` | `number` | `0` | 目标体积 KB，0=不限 |
| `overwrite` | `boolean` | `false` | 覆盖原文件 |
| `keep_original_name` | `boolean` | `false` | 保留原文件名（不加 `_da` 后缀） |
| `output_format` | `string` | `"jpeg"` | `"jpeg"` / `"original"` / `"webp"` |
| `output_dir` | `string` | `null` | 输出目录路径，不指定时默认 `./compressed/` |

### 摄影级优化参数
| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `enable_sharpening` | `boolean` | `false` | 智能自适应 USM 锐化 |
| `sharpening_radius` | `number` | `1.0` | 锐化半径 |
| `sharpening_amount` | `number` | `0.8` | 锐化强度 |
| `use_custom_quantization` | `boolean` | `false` | 自定义量化表 |
| `preserve_high_frequency` | `boolean` | `false` | 保留高频细节 |
| `color_space` | `string` | `"keep"` | `"keep"` / `"srgb"` |

### 目录遍历参数
| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `recursive` | `boolean` | `true` | 递归处理子目录 |
| `include_pattern` | `string` | `null` | 包含 Glob 模式，如 `"*.jpg,*.png"` |
| `exclude_pattern` | `string` | `null` | 排除 Glob 模式，如 `"*thumb*"` |
| `flatten` | `boolean` | `false` | 输出时拍平目录结构 |
| `dry_run` | `boolean` | `false` | 预演模式，只扫文件不压缩 |

### 工业级调度参数（v4.1.0 新增）
| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `force` | `boolean` | `false` | 强制重压：输出已存在也重新压缩（默认幂等续跑，跳过并标记 `skipped:true`） |
| `jsonl` | `boolean` | `false` | 流式 JSONL：每个文件一行 JSON，末尾追加汇总信封 |
| `max_workers` | `number` | `null` | 最大并行 worker 数（默认=CPU 核心数），大批量/共享服务器可限流 |

### v4.3.1 工程成熟度参数（新增）

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `preserve_structure` | `boolean` | `false` | 输出时保留源目录相对路径（默认拍平到 `output_dir`）；批量搬运多层级相册时复刻层级 |
| `output_suffix` | `string` | `null` | 自定义输出文件名后缀（覆盖默认 `_wx/_hd/_da`）；空串 `""` = 无后缀（`keep_original_name` 优先级更高） |
| `passthrough_unsupported` | `boolean` | `false` | 不支持的格式（如 SVG）原样透传复制到输出目录，不压缩、不报失败（结果 `status:"passthrough"`） |

> 对应 CLI 旗标：`--preserve-structure` / `--output-suffix` / `--passthrough-unsupported`。

## 错误语义

- **退出码 0** = 全部成功，可正常继续（含 `._*` 系统隐藏文件归类为 `skipped`、不支持格式在 `--passthrough-unsupported` 下透传，均不计入失败）
- **退出码 1** = 部分或全部真正失败，检查 `errors` 数组
- **退出码 2** = 参数错误，应修正参数后重试（不要盲目重试）
- 非 retryable 错误不要重试（如"RAW 格式不支持"、"路径不存在"）
- 网络/IO 类临时错误可重试（如"Failed to load image"）
- **`error_type` 细分**（失败时）：`unsupported`(格式不支持) / `corrupt`(解码损坏) / `permission`(权限) / `skipped`(隐藏文件跳过) / `passthrough`(透传) / `error`(其他)。便于 agent 决策重试还是跳过。
- **`data.skipped`**：跳过 / 透传的数量（均不计入 `failed`）；**`data.manifest`**：输入→输出映射清单（含未压缩项），便于 agent 回映射源目录。

## 能力探测（--capabilities）

AI 调用前可用 `--capabilities` 获取当前版本支持的完整参数 schema：

```bash
图片高速压缩 --capabilities | jq '.json_input_schema'
```

返回 JSON 包含：支持的全部参数名、类型、默认值、枚举值、说明。AI 启动时可自动探测，避免硬编码参数后版本升级引发兼容故障。

## CLI 参数速查

| 参数 | 说明 |
|------|------|
| `-i / --input` | 输入文件或目录（可多次） |
| `--output-dir` | 输出目录（默认 `./compressed/`） |
| `--mode` | `wechat` / `hd` / `custom` |
| `--max-dim` | 最长边像素 |
| `--quality` | JPEG 质量 1-100 |
| `--target-kb` | 目标体积 KB |
| `--overwrite` | 覆盖原文件 |
| `--keep-original-name` | 保留原文件名 |
| `--output-format` | `jpeg` / `keep-original` / `webp`（webp 更省体积、支持透明） |
| `--json` | JSON 模式（stdin 输入 / 输出 JSON 信封） |
| `--json-in` | 直接传 JSON 字符串（AI 最稳） |
| `--dry-run` | 预演模式，不执行压缩 |
| `--capabilities` | 输出完整参数 schema |
| `--recursive / --no-recursive` | 目录递归控制 |
| `--include` | Glob 包含模式，如 `*.jpg,*.png` |
| `--exclude` | Glob 排除模式，如 `*thumb*` |
| `--flatten` | 输出时拍平目录结构 |
| `--enable-sharpening` | 启用智能自适应锐化 |
| `--color-space` | `keep` / `srgb` |
| `-q / --quiet` | 静默模式 |
| `--force` | 强制重压（默认幂等续跑，输出已存在则跳过） |
| `--jsonl` | 流式 JSONL：逐行 JSON + 末尾汇总信封 |
| `--max-workers` | 最大并行 worker 数（默认=CPU 核心数） |
| `--self-check` | 环境自检，内置测试图走完整管线，输出健康报告 |
| `--preserve-structure` | 输出时保留源目录相对路径（默认拍平） |
| `--output-suffix` | 自定义输出文件名后缀（覆盖默认 `_wx/_hd/_da`；空串=无后缀） |
| `--passthrough-unsupported` | 不支持格式（如 SVG）原样透传，不报失败 |
| `--output-format webp` | 输出 WebP（更省体积、支持透明） |

## 工业级调度（v4.1.0 新增）

- **幂等续跑**：输出文件已存在且未 `--force` / `force:true` 时自动跳过，结果标记 `skipped: true`（不重算、不报错）。大批量中断重跑零重复劳动。
- **流式 JSONL（`--jsonl` / `"jsonl": true`）**：每处理完一个文件立即输出一行 JSON，全部处理完再追加标准汇总信封。适合实时进度采集、长任务中断续跑。
- **并发限流（`--max-workers` / `"max_workers"`）**：默认并行数 = CPU 核心数；共享服务器建议设为 `cpu_count / 2`。
- **stderr 分级日志**：日志走 stderr，统一前缀 `[INFO]` / `[WARN]` / `[ERROR]`；stdout 只放 JSON / JSONL 数据，互不污染，AI 可零分支解析。
- **环境自检（`--self-check`）**：内置生成测试图 → 完整压缩管线 → 逐项校验（pipeline / output_size / decode_output）→ 输出健康报告。接入新机器/新版本前先跑一遍验证二进制健康。

> 当前版本：**v4.4.4**（schema_version `1.0` 信封 / `1.1` capabilities）。AI 接入前建议先 `--capabilities` 探测，再 `--self-check` 验证。

## 下载

- 国内蓝奏云镜像（推荐，下载更快）：
  - 🍎 Mac：https://wwbfk.lanzoub.com/ifCJB40c6e7e
  - 🪟 Win：https://wwbfk.lanzoub.com/ia0Nm40c6e9g
- GitHub Release（备用）：https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases/tag/v4.4.4
