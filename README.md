# archiver — 免费 macOS 解压缩软件

基于 [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 的桌面小工具：在「解压」与「压缩」两种模式间切换，用系统文件对话框选择路径，压缩模式支持拖入文件或文件夹。

## 功能概览

### 解压

根据压缩包扩展名识别格式，解压到指定目录（可选默认目录）。解压时对包内路径做校验，**拒绝绝对路径与 `..` 穿越**，降低 Zip Slip 类风险。

| 格式 | 扩展名示例 |
|------|------------|
| ZIP | `.zip` |
| 7-Zip | `.7z` |
| RAR | `.rar`、`.cbr` |
| TAR | `.tar` |
| gzip 打包 | `.tar.gz`、`.tgz` |
| bzip2 打包 | `.tar.bz2`、`.tbz2`、`.tbz` |
| xz 打包 | `.tar.xz`、`.txz` |
| Zstandard 打包 | `.tar.zst`、`.tzst` |

### 压缩

- 添加多个**文件**或**文件夹**（递归打包）；列表中**相同路径不会重复加入**（含规范化路径后的相同项）。
- 输出格式由保存文件名决定：
  - **ZIP**（Deflate 等，见 `Cargo.toml` 中 `zip` 特性）
  - **tar.gz**（`.tar.gz` 或 `.tgz`）

压缩与解压均在后台线程执行，界面可显示日志；进行中会禁用相关操作。

## 环境要求

- **macOS**（窗口与交互按当前平台优化；其他平台未专门测试）
- **Rust**（`edition = "2021"`，建议使用当前稳定版）

## 编译与运行

```bash
cd archiver
cargo run          # 调试运行
cargo build --release
# 可执行文件通常在 target/release/archiver
```

## 项目结构

| 路径 | 说明 |
|------|------|
| `src/main.rs` | 窗口、模式切换、文件选择与 UI |
| `src/theme.rs` | 字体与浅色视觉主题 |
| `src/extract/mod.rs` | 解压实现与各归档格式分支 |
| `src/compress/mod.rs` | ZIP / tar.gz 压缩实现 |

## 依赖说明（简要）

- **ZIP / TAR 及压缩算法**：`zip`、`tar`、`flate2`、`bzip2`、`xz2`、`zstd` 等。
- **7z**：`sevenz-rust`。
- **RAR**：`unrar` crate（与上游 RAR 解压能力及许可相关；分发二进制前请自行确认合规）。

## 许可

仓库内未附带默认许可证文件；使用前请根据你的用途补充许可证或自行约定。
