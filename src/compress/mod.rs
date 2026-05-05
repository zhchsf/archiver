//! 将本地文件或目录打包为 ZIP 或 tar.gz。

use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use flate2::Compression;
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
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::TarGzip => "TAR.GZ",
        }
    }
}

/// 将 `sources` 中的文件与目录（递归）写入 `output`，路径使用 `/`。
pub fn compress(sources: &[PathBuf], output: &Path, format: CompressFormat) -> Result<()> {
    if sources.is_empty() {
        return Err(anyhow!("请至少选择一个文件或文件夹"));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录 {}", parent.display()))?;
    }
    let pairs = collect_file_pairs(sources)?;
    if pairs.is_empty() {
        return Err(anyhow!("没有可压缩的文件（空目录或未选到有效路径）"));
    }
    match format {
        CompressFormat::Zip => compress_zip(&pairs, output),
        CompressFormat::TarGzip => compress_tar_gz(&pairs, output),
    }
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

fn collect_file_pairs(sources: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let mut used_top: HashSet<String> = HashSet::new();

    for src in sources {
        let src = src.canonicalize().with_context(|| format!("无效路径: {}", src.display()))?;
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
                let entry = entry.with_context(|| format!("遍历 {}", src.display()))?;
                if entry.file_type().is_symlink() {
                    continue;
                }
                let path = entry.path();
                if path.is_file() {
                    if path.file_name() == Some(std::ffi::OsStr::new(".DS_Store")) {
                        continue;
                    }
                    let rel = path
                        .strip_prefix(&src)
                        .with_context(|| format!("路径前缀: {}", path.display()))?;
                    let inner = rel.to_string_lossy();
                    if inner.is_empty() {
                        continue;
                    }
                    let inner = posix_path(&inner);
                    let name = format!("{root}/{inner}");
                    ensure_safe_zip_path(&name)?;
                    out.push((path.to_path_buf(), name));
                }
            }
        }
    }
    Ok(out)
}

fn compress_zip(pairs: &[(PathBuf, String)], output: &Path) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (disk, name) in pairs {
        zip.start_file(name, opts)
            .with_context(|| format!("zip 条目 {name}"))?;
        let mut f = File::open(disk).with_context(|| format!("打开 {}", disk.display()))?;
        std::io::copy(&mut f, &mut zip).with_context(|| format!("写入 zip {name}"))?;
    }
    zip.finish().context("完成 ZIP 写入")?;
    Ok(())
}

fn compress_tar_gz(pairs: &[(PathBuf, String)], output: &Path) -> Result<()> {
    let file = File::create(output).with_context(|| format!("创建 {}", output.display()))?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(enc);
    for (disk, name) in pairs {
        builder
            .append_path_with_name(disk, name)
            .with_context(|| format!("tar 条目 {}", name))?;
    }
    let enc = builder.into_inner().context("结束 tar 打包")?;
    enc.finish().context("结束 gzip")?;
    Ok(())
}
