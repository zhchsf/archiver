//! 将常见压缩包解压到目标目录，并做路径穿越防护。

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;
use unrar::Archive as RarArchive;
use zip::ZipArchive;

/// 根据扩展名识别的归档类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
    Rar,
    TarPlain,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
}

impl ArchiveKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if name.ends_with(".zip") {
            return Some(Self::Zip);
        }
        if name.ends_with(".7z") {
            return Some(Self::SevenZ);
        }
        if name.ends_with(".rar") || name.ends_with(".cbr") {
            return Some(Self::Rar);
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
        if name.ends_with(".tar") {
            return Some(Self::TarPlain);
        }
        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZ => "7-Zip",
            Self::Rar => "RAR",
            Self::TarPlain => "TAR",
            Self::TarGzip => "TAR.GZ",
            Self::TarBzip2 => "TAR.BZ2",
            Self::TarXz => "TAR.XZ",
            Self::TarZstd => "TAR.ZST",
        }
    }
}

/// 将 `member` 安全地拼到 `base` 下，拒绝绝对路径与 `..`。
pub fn safe_join(base: &Path, member: &str) -> Result<PathBuf> {
    let member = member.replace('\\', "/");
    let mut rel = PathBuf::new();
    for comp in Path::new(&member).components() {
        match comp {
            Component::Normal(c) => rel.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(anyhow!("压缩包内包含非法路径（..）: {member:?}"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("压缩包内包含绝对路径: {member:?}"));
            }
        }
    }
    let base = base
        .canonicalize()
        .with_context(|| format!("输出目录无效: {}", base.display()))?;
    let out = base.join(&rel);
    out.strip_prefix(&base)
        .map_err(|_| anyhow!("检测到路径穿越: {member:?}"))?;
    Ok(out)
}

pub fn extract(archive: &Path, out_dir: &Path, kind: ArchiveKind) -> Result<()> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("无法创建输出目录: {}", out_dir.display()))?;
    match kind {
        ArchiveKind::Zip => extract_zip(archive, out_dir),
        ArchiveKind::SevenZ => extract_7z(archive, out_dir),
        ArchiveKind::Rar => extract_rar(archive, out_dir),
        ArchiveKind::TarPlain => {
            let f = File::open(archive)?;
            extract_tar(f, out_dir)
        }
        ArchiveKind::TarGzip => {
            let f = File::open(archive)?;
            extract_tar(GzDecoder::new(f), out_dir)
        }
        ArchiveKind::TarBzip2 => {
            let f = File::open(archive)?;
            let dec = bzip2::read::BzDecoder::new(f);
            extract_tar(dec, out_dir)
        }
        ArchiveKind::TarXz => {
            let f = File::open(archive)?;
            let dec = xz2::read::XzDecoder::new(f);
            extract_tar(dec, out_dir)
        }
        ArchiveKind::TarZstd => {
            let f = File::open(archive)?;
            let dec = zstd::stream::read::Decoder::new(f)
                .context("打开 zstd 流失败")?;
            extract_tar(dec, out_dir)
        }
    }
}

fn extract_zip(path: &Path, out: &Path) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("读取 ZIP 失败")?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).with_context(|| format!("ZIP 条目 {i}"))?;
        let name = entry.name().to_owned();
        if name.is_empty() {
            continue;
        }
        if name.ends_with('/') {
            let trimmed = name.trim_end_matches('/');
            if trimmed.is_empty() {
                continue;
            }
            let dir = safe_join(out, trimmed)?;
            std::fs::create_dir_all(&dir).with_context(|| format!("创建目录 {}", dir.display()))?;
            continue;
        }
        let outpath = safe_join(out, &name)?;
        if let Some(p) = outpath.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut outfile = File::create(&outpath).with_context(|| format!("创建文件 {}", outpath.display()))?;
        std::io::copy(&mut entry, &mut outfile).with_context(|| format!("写入 {}", outpath.display()))?;
    }
    Ok(())
}

fn extract_tar<R: Read>(reader: R, out: &Path) -> Result<()> {
    let mut archive = Archive::new(reader);
    archive
        .unpack(out)
        .with_context(|| format!("解压 TAR 到 {}", out.display()))?;
    Ok(())
}

fn extract_7z(path: &Path, out: &Path) -> Result<()> {
    let src = path.to_str().with_context(|| format!("路径需为 UTF-8: {}", path.display()))?;
    let dst = out
        .to_str()
        .with_context(|| format!("输出路径需为 UTF-8: {}", out.display()))?;
    sevenz_rust::decompress_file(src, dst).map_err(|e| anyhow!("7z 解压失败: {e}"))
}

/// RAR：使用 `unrar` crate，在编译期静态链入 RarLab 的 UnRAR 源码，无需系统安装解压工具。
/// （非纯 Rust；分发时需遵守 `unrar_sys` 自带的 UnRAR 许可条款。）
fn extract_rar(archive: &Path, out: &Path) -> Result<()> {
    let mut archive = RarArchive::new(archive)
        .as_first_part()
        .open_for_processing()
        .map_err(|e| anyhow!("打开 RAR 失败: {e}"))?;
    while let Some(header) = archive
        .read_header()
        .map_err(|e| anyhow!("读取 RAR 条目失败: {e}"))?
    {
        archive = if header.entry().is_file() {
            header
                .extract_with_base(out)
                .map_err(|e| anyhow!("解压 RAR 内文件失败: {e}"))?
        } else {
            header
                .skip()
                .map_err(|e| anyhow!("跳过 RAR 条目失败: {e}"))?
        };
    }
    Ok(())
}
