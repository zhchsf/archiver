//! 将本地文件或目录打包为 ZIP 或 tar 系列格式。

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, anyhow};
use bzip2::Compression as Bzip2Compression;
use flate2::Compression as GzipCompression;
use flate2::write::GzEncoder;
use tar::Builder;
use walkdir::WalkDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;
use zip::write::ZipWriter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressFormat {
    Zip,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
}

impl CompressFormat {
    pub fn from_output_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name.ends_with(".zip") {
            return Some(Self::Zip);
        }
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(Self::TarGzip);
        }
        if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
            return Some(Self::TarBzip2);
        }
        if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            return Some(Self::TarXz);
        }
        if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
            return Some(Self::TarZstd);
        }
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::TarGzip => "TAR.GZ",
            Self::TarBzip2 => "TAR.BZ2",
            Self::TarXz => "TAR.XZ",
            Self::TarZstd => "TAR.ZST",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionLevel {
    Fast,
    Balanced,
    Best,
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Balanced
    }
}

impl CompressionLevel {
    fn zip_level(self) -> i64 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 6,
            Self::Best => 9,
        }
    }

    fn gzip_level(self) -> GzipCompression {
        match self {
            Self::Fast => GzipCompression::fast(),
            Self::Balanced => GzipCompression::default(),
            Self::Best => GzipCompression::best(),
        }
    }

    fn bzip2_level(self) -> Bzip2Compression {
        match self {
            Self::Fast => Bzip2Compression::fast(),
            Self::Balanced => Bzip2Compression::default(),
            Self::Best => Bzip2Compression::best(),
        }
    }

    fn xz_level(self) -> u32 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 6,
            Self::Best => 9,
        }
    }

    fn zstd_level(self) -> i32 {
        match self {
            Self::Fast => 1,
            Self::Balanced => 3,
            Self::Best => 19,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompressOptions {
    pub level: CompressionLevel,
    pub keep_top_level: bool,
    pub include_hidden: bool,
    pub exclude_mac_metadata: bool,
    pub exclude_common_dev_dirs: bool,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            level: CompressionLevel::Balanced,
            keep_top_level: true,
            include_hidden: false,
            exclude_mac_metadata: true,
            exclude_common_dev_dirs: true,
            cancel: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompressStats {
    pub file_count: usize,
    pub total_bytes: u64,
}

fn check_cancel(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(anyhow!("任务已取消"));
    }
    Ok(())
}

fn copy_with_cancel<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<u64> {
    let mut buf = [0_u8; 64 * 1024];
    let mut written = 0_u64;
    loop {
        check_cancel(cancel)?;
        let n = reader.read(&mut buf).context("读取数据失败")?;
        if n == 0 {
            return Ok(written);
        }
        writer.write_all(&buf[..n]).context("写入数据失败")?;
        written += n as u64;
    }
}

/// 将 `sources` 中的文件与目录（递归）写入 `output`，路径使用 `/`。
pub fn compress_with_options(
    sources: &[PathBuf],
    output: &Path,
    format: CompressFormat,
    options: CompressOptions,
) -> Result<()> {
    if sources.is_empty() {
        return Err(anyhow!("请至少选择一个文件或文件夹"));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录 {}", parent.display()))?;
    }
    check_cancel(options.cancel.as_ref())?;
    let pairs = collect_file_pairs(sources, &options)?;
    if pairs.is_empty() {
        return Err(anyhow!("没有可压缩的文件（空目录或未选到有效路径）"));
    }
    match format {
        CompressFormat::Zip => compress_zip(&pairs, output, options.level, options.cancel.as_ref()),
        CompressFormat::TarGzip => compress_tar_gz(&pairs, output, options.level, options.cancel.as_ref()),
        CompressFormat::TarBzip2 => compress_tar_bz2(&pairs, output, options.level, options.cancel.as_ref()),
        CompressFormat::TarXz => compress_tar_xz(&pairs, output, options.level, options.cancel.as_ref()),
        CompressFormat::TarZstd => compress_tar_zstd(&pairs, output, options.level, options.cancel.as_ref()),
    }
}

pub fn estimate_sources(sources: &[PathBuf], options: CompressOptions) -> Result<CompressStats> {
    let pairs = collect_file_pairs(sources, &options)?;
    let mut stats = CompressStats {
        file_count: pairs.len(),
        total_bytes: 0,
    };
    for (disk, _) in pairs {
        check_cancel(options.cancel.as_ref())?;
        if let Ok(meta) = std::fs::metadata(&disk) {
            stats.total_bytes = stats.total_bytes.saturating_add(meta.len());
        }
    }
    Ok(stats)
}

fn posix_path(s: &str) -> String {
    s.replace('\\', "/")
}

fn ensure_safe_zip_path(name: &str) -> Result<()> {
    if name.starts_with('/') || name.contains("..") {
        return Err(anyhow!("压缩包内路径非法: {name:?}"));
    }
    Ok(())
}

/// 在多个顶层来源之间为「仅文件」条目分配不重名。
fn unique_top_level_name(orig: &str, used: &mut HashSet<String>) -> String {
    let p = Path::new(orig);
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("file");
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let mut c = 0u32;
    loop {
        let candidate = if c == 0 {
            orig.to_string()
        } else {
            format!("{stem}_{c}{ext}")
        };
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
        c += 1;
    }
}

fn should_skip_name(name: &str, options: &CompressOptions) -> bool {
    if options.exclude_mac_metadata && (name == ".DS_Store" || name == "__MACOSX") {
        return true;
    }
    if options.exclude_common_dev_dirs
        && matches!(
            name,
            ".git" | ".idea" | ".vscode" | "node_modules" | "target" | ".next" | "dist"
        )
    {
        return true;
    }
    !options.include_hidden && name.starts_with('.')
}

fn should_skip_path(path: &Path, options: &CompressOptions) -> bool {
    path.components().any(|comp| {
        let Some(name) = comp.as_os_str().to_str() else {
            return false;
        };
        should_skip_name(name, options)
    })
}

fn collect_file_pairs(sources: &[PathBuf], options: &CompressOptions) -> Result<Vec<(PathBuf, String)>> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let mut used_top: HashSet<String> = HashSet::new();

    for src in sources {
        check_cancel(options.cancel.as_ref())?;
        let src = src.canonicalize().with_context(|| format!("无效路径: {}", src.display()))?;
        if src
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| should_skip_name(name, options))
        {
            continue;
        }
        if src.is_file() {
            let base = src
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow!("文件名需为 UTF-8: {}", src.display()))?;
            let name = unique_top_level_name(base, &mut used_top);
            let name = posix_path(&name);
            ensure_safe_zip_path(&name)?;
            out.push((src, name));
        } else if src.is_dir() {
            let root = src
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow!("文件夹名需为 UTF-8: {}", src.display()))?;
            for entry in WalkDir::new(&src).follow_links(false).into_iter() {
                check_cancel(options.cancel.as_ref())?;
                let entry = entry.with_context(|| format!("遍历 {}", src.display()))?;
                if entry.file_type().is_symlink() {
                    continue;
                }
                let path = entry.path();
                if path.is_file() {
                    let rel = path
                        .strip_prefix(&src)
                        .with_context(|| format!("路径前缀: {}", path.display()))?;
                    if should_skip_path(rel, options) {
                        continue;
                    }
                    let inner = rel.to_string_lossy();
                    if inner.is_empty() {
                        continue;
                    }
                    let inner = posix_path(&inner);
                    let name = if options.keep_top_level {
                        format!("{root}/{inner}")
                    } else {
                        inner
                    };
                    ensure_safe_zip_path(&name)?;
                    out.push((path.to_path_buf(), name));
                }
            }
        }
    }
    Ok(out)
}

fn compress_zip(
    pairs: &[(PathBuf, String)],
    output: &Path,
    level: CompressionLevel,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(level.zip_level()));

    for (disk, name) in pairs {
        check_cancel(cancel)?;
        zip.start_file(name, opts)
            .with_context(|| format!("zip 条目 {name}"))?;
        let mut f = File::open(disk).with_context(|| format!("打开 {}", disk.display()))?;
        copy_with_cancel(&mut f, &mut zip, cancel)
            .with_context(|| format!("写入 zip {name}"))?;
    }
    zip.finish().context("完成 ZIP 写入")?;
    Ok(())
}

fn compress_tar_gz(
    pairs: &[(PathBuf, String)],
    output: &Path,
    level: CompressionLevel,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let enc = GzEncoder::new(file, level.gzip_level());
    let mut builder = Builder::new(enc);
    for (disk, name) in pairs {
        check_cancel(cancel)?;
        builder
            .append_path_with_name(disk, name)
            .with_context(|| format!("tar 条目 {}", name))?;
    }
    let enc = builder.into_inner().context("结束 tar 打包")?;
    enc.finish().context("结束 gzip")?;
    Ok(())
}

fn compress_tar_bz2(
    pairs: &[(PathBuf, String)],
    output: &Path,
    level: CompressionLevel,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let enc = bzip2::write::BzEncoder::new(file, level.bzip2_level());
    let mut builder = Builder::new(enc);
    for (disk, name) in pairs {
        check_cancel(cancel)?;
        builder
            .append_path_with_name(disk, name)
            .with_context(|| format!("tar 条目 {}", name))?;
    }
    let enc = builder.into_inner().context("结束 tar 打包")?;
    enc.finish().context("结束 bzip2")?;
    Ok(())
}

fn compress_tar_xz(
    pairs: &[(PathBuf, String)],
    output: &Path,
    level: CompressionLevel,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let enc = xz2::write::XzEncoder::new(file, level.xz_level());
    let mut builder = Builder::new(enc);
    for (disk, name) in pairs {
        check_cancel(cancel)?;
        builder
            .append_path_with_name(disk, name)
            .with_context(|| format!("tar 条目 {}", name))?;
    }
    let enc = builder.into_inner().context("结束 tar 打包")?;
    enc.finish().context("结束 xz")?;
    Ok(())
}

fn compress_tar_zstd(
    pairs: &[(PathBuf, String)],
    output: &Path,
    level: CompressionLevel,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let enc = zstd::stream::write::Encoder::new(file, level.zstd_level())
        .context("创建 zstd 编码器")?;
    let mut builder = Builder::new(enc);
    for (disk, name) in pairs {
        check_cancel(cancel)?;
        builder
            .append_path_with_name(disk, name)
            .with_context(|| format!("tar 条目 {}", name))?;
    }
    let enc = builder.into_inner().context("结束 tar 打包")?;
    enc.finish().context("结束 zstd")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::ZipArchive;

    #[test]
    fn estimate_sources_respects_default_exclusions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("input");
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("visible.txt"), b"hello").unwrap();
        std::fs::write(root.join(".hidden"), b"secret").unwrap();
        std::fs::write(root.join(".DS_Store"), b"mac").unwrap();
        std::fs::write(root.join("node_modules/pkg.js"), b"pkg").unwrap();

        let stats = estimate_sources(&[root], CompressOptions::default()).unwrap();

        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.total_bytes, 5);
    }

    #[test]
    fn zip_output_excludes_hidden_mac_metadata_and_dev_dirs_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("input");
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("visible.txt"), b"hello").unwrap();
        std::fs::write(root.join(".hidden"), b"secret").unwrap();
        std::fs::write(root.join(".DS_Store"), b"mac").unwrap();
        std::fs::write(root.join("target/build.bin"), b"build").unwrap();

        let out = tmp.path().join("archive.zip");
        compress_with_options(
            &[root],
            &out,
            CompressFormat::Zip,
            CompressOptions::default(),
        )
        .unwrap();

        let file = File::open(&out).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for i in 0..archive.len() {
            names.push(archive.by_index(i).unwrap().name().to_string());
        }

        assert_eq!(names, vec!["input/visible.txt"]);
    }

    #[test]
    fn cancelled_estimate_stops_before_work() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("input");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("visible.txt"), b"hello").unwrap();

        let cancel = Arc::new(AtomicBool::new(true));
        let options = CompressOptions {
            cancel: Some(cancel),
            ..CompressOptions::default()
        };

        let err = estimate_sources(&[root], options).unwrap_err().to_string();
        assert!(err.contains("任务已取消"));
    }
}
