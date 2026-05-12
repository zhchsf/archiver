# archiver — 免费 macOS 解压缩软件

基于 [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 的桌面小工具：在「解压」与「压缩」两种模式间切换，用系统文件对话框选择路径，支持拖入文件、文件夹和多个压缩包。

## 功能概览

### 解压

根据压缩包扩展名识别格式，支持单个或批量解压到指定目录（可选默认目录）。解压时对包内路径做校验，**拒绝绝对路径与 `..` 穿越**，降低 Zip Slip 类风险。

解压模式支持：

- 批量选择或拖入多个压缩包。
- 解压前后台预览压缩包内容、文件数量、大小与加密状态，避免大包读取时卡住界面。
- 进度条与当前处理文件名，任务执行中可取消（已开始写入的大文件通常会在当前文件处理完后停止）。
- 覆盖策略：自动重命名、跳过、覆盖。
- 加密 ZIP / RAR / 7z 的密码输入（取决于底层格式与库支持）。
- 完成后打开输出位置、打开父目录、复制输出路径；批量解压会展示所有输出目录。
- 默认输出目录使用压缩包原名；如果目录已存在，会自动追加序号避免混入旧文件。

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
  - **tar.bz2**（`.tar.bz2`、`.tbz2`、`.tbz`）
  - **tar.xz**（`.tar.xz`、`.txz`）
  - **tar.zst**（`.tar.zst`、`.tzst`）
- 压缩选项：
  - 快速 / 均衡 / 最高压缩级别。
  - 是否保留文件夹顶层目录。
  - 是否包含隐藏文件（默认不包含）。
  - 是否排除 `.DS_Store` 与 `__MACOSX`。
  - 是否排除常见开发目录，例如 `.git`、`target`、`node_modules`、`.idea`。
- 压缩前可统计待压缩文件数量和原始大小，超过较大体积时会提示耗时风险。

压缩与解压均在后台线程执行，界面可显示日志；进行中会禁用相关操作，并提供取消入口。

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
| `src/compress/mod.rs` | ZIP / tar.gz / tar.bz2 / tar.xz / tar.zst 压缩实现 |

## 依赖说明（简要）

- **ZIP / TAR 及压缩算法**：`zip`、`tar`、`flate2`、`bzip2`、`xz2`、`zstd` 等。
- **7z**：`sevenz-rust`。
- **RAR**：`unrar` crate（与上游 RAR 解压能力及许可相关；分发二进制前请自行确认合规）。

## macOS 发布说明

当前项目可通过 `cargo build --release` 得到可执行文件。若要作为完整 macOS 应用分发，建议继续补充：

- `.app` Bundle、应用图标、`Info.plist`。
- 开发者签名与 Apple 公证。
- DMG 安装包。
- Finder 右键“用本软件解压/压缩”的系统服务或扩展。

## 许可

仓库内未附带默认许可证文件；使用前请根据你的用途补充许可证或自行约定。
