//! 将常见压缩包解压到目标目录，并做路径穿越防护。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwritePolicy {
    Skip,
    Overwrite,
    Rename,
}

impl Default for OverwritePolicy {
    fn default() -> Self {
        Self::Rename
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExtractOptions {
    pub overwrite: OverwritePolicy,
    pub password: Option<String>,
    pub cancel: Option<Arc<AtomicBool>>,
}

#[derive(Clone, Debug)]
pub struct ExtractProgress {
    pub current: usize,
    pub total: Option<usize>,
    pub file: String,
}

#[derive(Clone, Debug)]
pub struct ArchiveEntryPreview {
    pub name: String,
    pub size: Option<u64>,
    pub is_dir: bool,
    pub encrypted: bool,
}

fn check_cancel(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(anyhow!("任务已取消"));
    }
    Ok(())
}

fn copy_with_cancel<R: Read + ?Sized, W: Write>(
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

pub fn extract_with_options(
    archive: &Path,
    out_dir: &Path,
    kind: ArchiveKind,
    options: ExtractOptions,
    mut on_progress: impl FnMut(ExtractProgress),
) -> Result<()> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("无法创建输出目录: {}", out_dir.display()))?;
    check_cancel(options.cancel.as_ref())?;
    match kind {
        ArchiveKind::Zip => extract_zip(archive, out_dir, options, &mut on_progress),
        ArchiveKind::SevenZ => extract_7z(archive, out_dir, options, &mut on_progress),
        ArchiveKind::Rar => extract_rar(archive, out_dir, options, &mut on_progress),
        ArchiveKind::TarPlain => {
            // 未压缩 tar：可额外扫一遍条目数，进度条显示 n/total，成本可接受。
            let total = count_tar_entries_plain(archive, options.cancel.as_ref()).ok();
            let f = File::open(archive)?;
            extract_tar(f, out_dir, options, total, &mut on_progress)
        }
        ArchiveKind::TarGzip => {
            let f = File::open(archive)?;
            extract_tar(GzDecoder::new(f), out_dir, options, None, &mut on_progress)
        }
        ArchiveKind::TarBzip2 => {
            let f = File::open(archive)?;
            let dec = bzip2::read::BzDecoder::new(f);
            extract_tar(dec, out_dir, options, None, &mut on_progress)
        }
        ArchiveKind::TarXz => {
            let f = File::open(archive)?;
            let dec = xz2::read::XzDecoder::new(f);
            extract_tar(dec, out_dir, options, None, &mut on_progress)
        }
        ArchiveKind::TarZstd => {
            let f = File::open(archive)?;
            let dec = zstd::stream::read::Decoder::new(f)
                .context("打开 zstd 流失败")?;
            extract_tar(dec, out_dir, options, None, &mut on_progress)
        }
    }
}

/// 仅用于 `.tar`：统计条目数（不解压正文），便于进度条显示总数。
fn count_tar_entries_plain(path: &Path, cancel: Option<&Arc<AtomicBool>>) -> Result<usize> {
    let f = File::open(path)?;
    let mut archive = Archive::new(f);
    let mut n = 0usize;
    for entry in archive.entries().context("读取 TAR 条目失败")? {
        check_cancel(cancel)?;
        let _ = entry.context("读取 TAR 条目失败")?;
        n += 1;
    }
    Ok(n)
}

fn sevenz_entry_count(path: &Path, password: &sevenz_rust::Password) -> Result<usize> {
    let mut file = File::open(path).with_context(|| format!("打开 {}", path.display()))?;
    let len = file
        .seek(SeekFrom::End(0))
        .context("读取 7z 大小失败")?;
    file.seek(SeekFrom::Start(0)).context("读取 7z 失败")?;
    let reader = sevenz_rust::SevenZReader::new(file, len, password.clone())
        .map_err(|e| anyhow!("读取 7z 目录失败: {e}"))?;
    Ok(reader.archive().files.len())
}

fn rar_entry_count(path: &Path, password: Option<&str>) -> Result<usize> {
    let rar = if let Some(password) = password {
        RarArchive::with_password(path, password)
    } else {
        RarArchive::new(path)
    };
    let mut archive = rar
        .as_first_part()
        .open_for_listing()
        .map_err(|e| anyhow!("打开 RAR 失败: {e}"))?;
    let mut n = 0usize;
    while let Some(header) = archive
        .read_header()
        .map_err(|e| anyhow!("读取 RAR 条目失败: {e}"))?
    {
        n += 1;
        archive = header
            .skip()
            .map_err(|e| anyhow!("跳过 RAR 条目失败: {e}"))?;
    }
    Ok(n)
}

fn next_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for n in 2u32.. {
        let candidate = parent.join(format!("{stem} {n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn resolve_conflict(path: &Path, policy: OverwritePolicy) -> Option<PathBuf> {
    if !path.exists() {
        return Some(path.to_path_buf());
    }
    match policy {
        OverwritePolicy::Skip => None,
        OverwritePolicy::Overwrite => Some(path.to_path_buf()),
        OverwritePolicy::Rename => Some(next_available_path(path)),
    }
}

fn extract_zip(
    path: &Path,
    out: &Path,
    options: ExtractOptions,
    on_progress: &mut impl FnMut(ExtractProgress),
) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("读取 ZIP 失败")?;
    let total = archive.len();
    for i in 0..total {
        check_cancel(options.cancel.as_ref())?;
        let mut entry = {
            let probe = archive.by_index(i).with_context(|| format!("ZIP 条目 {i}"))?;
            let encrypted = probe.encrypted();
            drop(probe);
            if encrypted {
                let password = options
                    .password
                    .as_deref()
                    .ok_or_else(|| anyhow!("ZIP 条目需要密码，请先填写密码"))?;
                archive
                    .by_index_decrypt(i, password.as_bytes())
                    .with_context(|| format!("ZIP 加密条目 {i}"))?
            } else {
                archive.by_index(i).with_context(|| format!("ZIP 条目 {i}"))?
            }
        };
        let name = entry.name().to_owned();
        on_progress(ExtractProgress {
            current: i + 1,
            total: Some(total),
            file: name.clone(),
        });
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
        let Some(outpath) = resolve_conflict(&outpath, options.overwrite) else {
            continue;
        };
        if let Some(p) = outpath.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut outfile = File::create(&outpath).with_context(|| format!("创建文件 {}", outpath.display()))?;
        copy_with_cancel(&mut entry, &mut outfile, options.cancel.as_ref())
            .with_context(|| format!("写入 {}", outpath.display()))?;
    }
    Ok(())
}

fn extract_tar<R: Read>(
    reader: R,
    out: &Path,
    options: ExtractOptions,
    total_entries: Option<usize>,
    on_progress: &mut impl FnMut(ExtractProgress),
) -> Result<()> {
    let mut archive = Archive::new(reader);
    let mut current = 0usize;
    for entry in archive.entries().context("读取 TAR 条目失败")? {
        check_cancel(options.cancel.as_ref())?;
        let mut entry = entry.context("读取 TAR 条目失败")?;
        let path = entry.path().context("读取 TAR 条目路径失败")?;
        let name = path.to_string_lossy().replace('\\', "/");
        current += 1;
        on_progress(ExtractProgress {
            current,
            total: total_entries,
            file: name.clone(),
        });
        let outpath = safe_join(out, &name)?;
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&outpath)
                .with_context(|| format!("创建目录 {}", outpath.display()))?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Some(outpath) = resolve_conflict(&outpath, options.overwrite) else {
            continue;
        };
        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录 {}", parent.display()))?;
        }
        let mut outfile =
            File::create(&outpath).with_context(|| format!("创建文件 {}", outpath.display()))?;
        copy_with_cancel(&mut entry, &mut outfile, options.cancel.as_ref())
            .with_context(|| format!("解压 TAR 条目 {}", outpath.display()))?;
    }
    Ok(())
}

fn extract_7z(
    path: &Path,
    out: &Path,
    options: ExtractOptions,
    on_progress: &mut impl FnMut(ExtractProgress),
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("打开 {}", path.display()))?;
    let password = options
        .password
        .as_deref()
        .map(sevenz_rust::Password::from)
        .unwrap_or_else(sevenz_rust::Password::empty);
    let total = sevenz_entry_count(path, &password).ok();
    let mut current = 0usize;
    sevenz_rust::decompress_with_extract_fn_and_password(
        file,
        out,
        password,
        |entry, reader, _dest| {
            check_cancel(options.cancel.as_ref())
                .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
            let name = entry.name().to_string();
            current += 1;
            on_progress(ExtractProgress {
                current,
                total,
                file: name,
            });
            let safe_dest = safe_join(out, entry.name())
                .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
            if entry.is_directory() {
                std::fs::create_dir_all(&safe_dest).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            }
            let Some(dest) = resolve_conflict(&safe_dest, options.overwrite) else {
                return Ok(true);
            };
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
            }
            let mut writer = std::io::BufWriter::new(
                File::create(&dest).map_err(|e| {
                    sevenz_rust::Error::io_msg(e, format!("打开 {}", dest.display()))
                })?,
            );
            copy_with_cancel(reader, &mut writer, options.cancel.as_ref())
                .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
            Ok(true)
        },
    )
    .map_err(|e| anyhow!("7z 解压失败: {e}"))
}

/// RAR：使用 `unrar` crate，在编译期静态链入 RarLab 的 UnRAR 源码，无需系统安装解压工具。
/// （非纯 Rust；分发时需遵守 `unrar_sys` 自带的 UnRAR 许可条款。）
fn extract_rar(
    archive: &Path,
    out: &Path,
    options: ExtractOptions,
    on_progress: &mut impl FnMut(ExtractProgress),
) -> Result<()> {
    let total = rar_entry_count(archive, options.password.as_deref()).ok();
    let rar = if let Some(password) = options.password.as_deref() {
        RarArchive::with_password(archive, password)
    } else {
        RarArchive::new(archive)
    };
    let mut archive = rar
        .as_first_part()
        .open_for_processing()
        .map_err(|e| anyhow!("打开 RAR 失败: {e}"))?;
    let mut current = 0usize;
    while let Some(header) = archive
        .read_header()
        .map_err(|e| anyhow!("读取 RAR 条目失败: {e}"))?
    {
        check_cancel(options.cancel.as_ref())?;
        current += 1;
        let name = header.entry().filename.to_string_lossy().replace('\\', "/");
        on_progress(ExtractProgress {
            current,
            total,
            file: name.clone(),
        });
        archive = if header.entry().is_file() {
            let outpath = safe_join(out, &name)?;
            if let Some(outpath) = resolve_conflict(&outpath, options.overwrite) {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("创建目录 {}", parent.display()))?;
                }
                header
                    .extract_to(&outpath)
                    .map_err(|e| anyhow!("解压 RAR 内文件失败: {e}"))?
            } else {
                header
                    .skip()
                    .map_err(|e| anyhow!("跳过 RAR 条目失败: {e}"))?
            }
        } else {
            let dir = safe_join(out, &name)?;
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("创建目录 {}", dir.display()))?;
            header
                .skip()
                .map_err(|e| anyhow!("跳过 RAR 条目失败: {e}"))?
        };
    }
    Ok(())
}

pub fn list_archive(archive: &Path, kind: ArchiveKind, password: Option<&str>) -> Result<Vec<ArchiveEntryPreview>> {
    match kind {
        ArchiveKind::Zip => list_zip(archive),
        ArchiveKind::Rar => list_rar(archive, password),
        ArchiveKind::SevenZ => list_7z(archive, password),
        ArchiveKind::TarPlain => {
            let f = File::open(archive)?;
            list_tar(f)
        }
        ArchiveKind::TarGzip => {
            let f = File::open(archive)?;
            list_tar(GzDecoder::new(f))
        }
        ArchiveKind::TarBzip2 => {
            let f = File::open(archive)?;
            list_tar(bzip2::read::BzDecoder::new(f))
        }
        ArchiveKind::TarXz => {
            let f = File::open(archive)?;
            list_tar(xz2::read::XzDecoder::new(f))
        }
        ArchiveKind::TarZstd => {
            let f = File::open(archive)?;
            list_tar(zstd::stream::read::Decoder::new(f).context("打开 zstd 流失败")?)
        }
    }
}

fn list_zip(path: &Path) -> Result<Vec<ArchiveEntryPreview>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("读取 ZIP 失败")?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).with_context(|| format!("ZIP 条目 {i}"))?;
        out.push(ArchiveEntryPreview {
            name: entry.name().to_owned(),
            size: Some(entry.size()),
            is_dir: entry.is_dir(),
            encrypted: entry.encrypted(),
        });
    }
    Ok(out)
}

fn list_tar<R: Read>(reader: R) -> Result<Vec<ArchiveEntryPreview>> {
    let mut archive = Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries().context("读取 TAR 条目失败")? {
        let entry = entry.context("读取 TAR 条目失败")?;
        let path = entry.path().context("读取 TAR 条目路径失败")?;
        out.push(ArchiveEntryPreview {
            name: path.to_string_lossy().replace('\\', "/"),
            size: Some(entry.size()),
            is_dir: entry.header().entry_type().is_dir(),
            encrypted: false,
        });
    }
    Ok(out)
}

fn list_rar(path: &Path, password: Option<&str>) -> Result<Vec<ArchiveEntryPreview>> {
    let rar = if let Some(password) = password {
        RarArchive::with_password(path, password)
    } else {
        RarArchive::new(path)
    };
    let mut archive = rar
        .as_first_part()
        .open_for_listing()
        .map_err(|e| anyhow!("打开 RAR 失败: {e}"))?;
    let mut out = Vec::new();
    while let Some(header) = archive
        .read_header()
        .map_err(|e| anyhow!("读取 RAR 条目失败: {e}"))?
    {
        let entry = header.entry();
        out.push(ArchiveEntryPreview {
            name: entry.filename.to_string_lossy().replace('\\', "/"),
            size: Some(entry.unpacked_size),
            is_dir: entry.is_directory(),
            encrypted: entry.is_encrypted(),
        });
        archive = header
            .skip()
            .map_err(|e| anyhow!("跳过 RAR 条目失败: {e}"))?;
    }
    Ok(out)
}

fn list_7z(path: &Path, password: Option<&str>) -> Result<Vec<ArchiveEntryPreview>> {
    let mut file = File::open(path).with_context(|| format!("打开 {}", path.display()))?;
    let len = file.seek(SeekFrom::End(0)).context("读取 7z 大小失败")?;
    file.seek(SeekFrom::Start(0)).context("读取 7z 失败")?;
    let password = password
        .map(sevenz_rust::Password::from)
        .unwrap_or_else(sevenz_rust::Password::empty);
    let reader = sevenz_rust::SevenZReader::new(file, len, password)
        .map_err(|e| anyhow!("读取 7z 目录失败: {e}"))?;
    Ok(reader
        .archive()
        .files
        .iter()
        .map(|entry| ArchiveEntryPreview {
            name: entry.name.clone(),
            size: Some(entry.size),
            is_dir: entry.is_directory,
            encrypted: false,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn safe_join_rejects_parent_and_absolute_paths() {
        let dir = tempdir().unwrap();

        assert!(safe_join(dir.path(), "../evil.txt").is_err());
        assert!(safe_join(dir.path(), "/tmp/evil.txt").is_err());
        assert!(safe_join(dir.path(), "nested/file.txt").is_ok());
    }

    #[test]
    fn resolve_conflict_renames_existing_file() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("file.txt");
        std::fs::write(&existing, b"old").unwrap();

        let renamed = resolve_conflict(&existing, OverwritePolicy::Rename).unwrap();

        assert_eq!(renamed.file_name().unwrap(), "file 2.txt");
        assert_eq!(resolve_conflict(&existing, OverwritePolicy::Skip), None);
        assert_eq!(
            resolve_conflict(&existing, OverwritePolicy::Overwrite),
            Some(existing)
        );
    }

    #[test]
    fn copy_with_cancel_stops_between_chunks() {
        struct CancelAfterFirstRead {
            data: Cursor<Vec<u8>>,
            cancel: Arc<AtomicBool>,
            reads: usize,
        }

        impl Read for CancelAfterFirstRead {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.reads += 1;
                if self.reads > 1 {
                    self.cancel.store(true, Ordering::Relaxed);
                }
                self.data.read(buf)
            }
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let mut reader = CancelAfterFirstRead {
            data: Cursor::new(vec![1_u8; 192 * 1024]),
            cancel: cancel.clone(),
            reads: 0,
        };
        let mut output = Vec::new();

        let err = copy_with_cancel(&mut reader, &mut output, Some(&cancel)).unwrap_err();

        assert!(err.to_string().contains("任务已取消"));
        assert!(output.len() < 192 * 1024);
    }
}
