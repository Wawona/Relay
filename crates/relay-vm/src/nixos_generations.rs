//! NixOS system-profile generations on a stopped guest disk.
//!
//! A generation is `system-N-link` in `/nix/var/nix/profiles`. The `system`
//! symlink is the generation NixOS will boot. `/init` points at
//! `system/init`, so the command line stays `init=/init`.
//!
//! This module lists those links and rewrites only the fast `system` symlink.
//! It does not create a second init name, and it does not load another kernel.
//! The bundled NixOS kernel and initrd still come from the guest manifest.

use relay_core::RelayError;
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const SUPERBLOCK_OFFSET: u64 = 1024;
const EXT4_MAGIC: u16 = 0xEF53;
const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
const INCOMPAT_FILETYPE: u32 = 0x2;
const INCOMPAT_RECOVER: u32 = 0x4;
const INCOMPAT_EXTENTS: u32 = 0x40;
const INCOMPAT_64BIT: u32 = 0x80;
const INCOMPAT_CSUM_SEED: u32 = 0x2000;
const RO_METADATA_CSUM: u32 = 0x400;
const EXTENTS_FL: u32 = 0x80000;
const S_IFMT: u16 = 0o170000;
const S_IFDIR: u16 = 0o040000;
const S_IFLNK: u16 = 0o120000;
const FAST_SYMLINK_MAX: usize = 60;
const PROFILE_PATH: &[&str] = &["nix", "var", "nix", "profiles"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NixGeneration {
    pub number: u32,
    pub label: String,
    pub current: bool,
}

#[derive(Debug, Serialize)]
struct GenerationDocument {
    present: bool,
    generations: Vec<NixGeneration>,
}

pub fn generations_json(path: &Path) -> Result<String, RelayError> {
    let document = if !path.is_file() {
        GenerationDocument {
            present: false,
            generations: Vec::new(),
        }
    } else {
        let mut file = File::open(path).map_err(|error| {
            RelayError::Failed(format!("cannot open NixOS disk for generations: {error}"))
        })?;
        GenerationDocument {
            present: true,
            generations: list_file(&mut file)?,
        }
    };
    serde_json::to_string(&document)
        .map_err(|error| RelayError::Failed(format!("cannot encode NixOS generations: {error}")))
}

pub fn activate_generation(file: &mut File, generation: u32) -> Result<(), RelayError> {
    if generation == 0 {
        return Err(RelayError::Failed(
            "NixOS generation numbers start at 1".into(),
        ));
    }
    let mut fs = Ext4::open(file)?;
    if fs.superblock.incompat & INCOMPAT_RECOVER != 0 {
        return Err(RelayError::Failed(
            "NixOS disk journal is dirty. Generation selection waits until the disk is clean."
                .into(),
        ));
    }
    let profiles = walk_profile(&mut fs)?;
    let link_name = profile_link_name(generation);
    if lookup_dir(&mut fs, profiles, link_name.as_bytes())?.is_none() {
        return Err(RelayError::Failed(format!(
            "NixOS generation {generation} is not on this disk"
        )));
    }
    let Some(system_ino) = lookup_dir(&mut fs, profiles, b"system")? else {
        return Err(RelayError::Failed(
            "This disk has no NixOS system profile.".into(),
        ));
    };
    let mut inode = fs.read_inode(system_ino)?;
    if !inode.is_fast_symlink() {
        return Err(RelayError::Failed(
            "NixOS system profile is not a short symlink.".into(),
        ));
    }
    if inode.symlink_bytes() == link_name.as_bytes() {
        return Ok(());
    }
    inode.set_fast_symlink(link_name.as_bytes())?;
    fs.write_inode(system_ino, &inode)
}

fn list_file(file: &mut File) -> Result<Vec<NixGeneration>, RelayError> {
    let mut fs = Ext4::open(file)?;
    let Ok(profiles) = walk_profile(&mut fs) else {
        return Ok(Vec::new());
    };
    let entries = read_directory(&mut fs, profiles)?;
    let system_target = if let Some((_, ino)) = entries.iter().find(|(name, _)| name == "system") {
        let inode = fs.read_inode(*ino)?;
        Some(inode.read_target(&mut fs)?)
    } else {
        None
    };
    let mut generations = Vec::new();
    for (name, ino) in entries {
        let Some(number) = generation_from_profile_name(name.as_bytes()) else {
            continue;
        };
        let inode = fs.read_inode(ino)?;
        let target = inode.read_target(&mut fs)?;
        let current = system_target.as_deref() == Some(name.as_str());
        generations.push(NixGeneration {
            number,
            label: label_from_target(&target),
            current,
        });
    }
    generations.sort_by_key(|generation| generation.number);
    Ok(generations)
}

fn walk_profile(fs: &mut Ext4<'_>) -> Result<u32, RelayError> {
    let mut ino = 2u32;
    for name in PROFILE_PATH {
        let next = lookup_dir(fs, ino, name.as_bytes())?
            .ok_or_else(|| RelayError::Failed("This disk has no NixOS system profile.".into()))?;
        let inode = fs.read_inode(next)?;
        if inode.mode & S_IFMT == S_IFLNK {
            return Err(RelayError::Failed(
                "NixOS profile path contains a symlink. Relay boots the directory profile.".into(),
            ));
        }
        if inode.mode & S_IFMT != S_IFDIR {
            return Err(RelayError::Failed(
                "NixOS profile path is not a directory.".into(),
            ));
        }
        ino = next;
    }
    Ok(ino)
}

fn lookup_dir(fs: &mut Ext4<'_>, dir: u32, name: &[u8]) -> Result<Option<u32>, RelayError> {
    for (entry, ino) in read_directory(fs, dir)? {
        if entry.as_bytes() == name {
            return Ok(Some(ino));
        }
    }
    Ok(None)
}

fn read_directory(fs: &mut Ext4<'_>, ino: u32) -> Result<Vec<(String, u32)>, RelayError> {
    let inode = fs.read_inode(ino)?;
    if inode.mode & S_IFMT != S_IFDIR {
        return Err(RelayError::Failed(
            "NixOS profile lookup hit a non-directory".into(),
        ));
    }
    let data = inode.read_extent_bytes(fs, 64 * 1024)?;
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= data.len() && offset < inode.size as usize {
        let entry_ino = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let rec_len = u16::from_le_bytes(data[offset + 4..offset + 6].try_into().unwrap()) as usize;
        if rec_len < 8 || offset + rec_len > data.len() {
            break;
        }
        let name_len = data[offset + 6] as usize;
        if entry_ino != 0 && offset + 8 + name_len <= data.len() {
            let name =
                String::from_utf8_lossy(&data[offset + 8..offset + 8 + name_len]).into_owned();
            if name != "." && name != ".." {
                entries.push((name, entry_ino));
            }
        }
        offset += rec_len;
    }
    Ok(entries)
}

fn label_from_target(target: &str) -> String {
    target
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(target)
        .to_string()
}

pub(crate) fn generation_from_profile_name(name: &[u8]) -> Option<u32> {
    let prefix = b"system-";
    let suffix = b"-link";
    if name.len() <= prefix.len() + suffix.len() {
        return None;
    }
    if !name.starts_with(prefix) || !name.ends_with(suffix) {
        return None;
    }
    let digits = &name[prefix.len()..name.len() - suffix.len()];
    if digits.is_empty() || digits.len() > 9 || digits[0] == b'0' {
        return None;
    }
    let mut number: u32 = 0;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        number = number
            .checked_mul(10)?
            .checked_add(u32::from(byte - b'0'))?;
    }
    if number == 0 {
        None
    } else {
        Some(number)
    }
}

fn profile_link_name(generation: u32) -> String {
    format!("system-{generation}-link")
}

struct Superblock {
    block_size: u64,
    inodes_per_group: u32,
    inode_size: u64,
    incompat: u32,
    desc_size: u64,
    csum_seed: Option<u32>,
}

struct Ext4<'a> {
    file: &'a mut File,
    superblock: Superblock,
}

struct Inode {
    raw: Vec<u8>,
    mode: u16,
    size: u64,
    flags: u32,
    blocks: u64,
}

impl Inode {
    fn is_fast_symlink(&self) -> bool {
        self.mode & S_IFMT == S_IFLNK
            && self.blocks == 0
            && self.flags & EXTENTS_FL == 0
            && self.size > 0
            && self.size as usize <= FAST_SYMLINK_MAX
    }

    fn symlink_bytes(&self) -> &[u8] {
        let len = self.size as usize;
        &self.raw[40..40 + len]
    }

    fn set_fast_symlink(&mut self, target: &[u8]) -> Result<(), RelayError> {
        if target.is_empty() || target.len() > FAST_SYMLINK_MAX {
            return Err(RelayError::Failed(
                "NixOS generation name does not fit in the system symlink".into(),
            ));
        }
        self.raw[40..40 + FAST_SYMLINK_MAX].fill(0);
        self.raw[40..40 + target.len()].copy_from_slice(target);
        self.raw[4..8].copy_from_slice(&(target.len() as u32).to_le_bytes());
        self.size = target.len() as u64;
        Ok(())
    }

    fn read_target(&self, fs: &mut Ext4<'_>) -> Result<String, RelayError> {
        let bytes = if self.is_fast_symlink() {
            self.symlink_bytes().to_vec()
        } else if self.mode & S_IFMT == S_IFLNK {
            self.read_extent_bytes(fs, FAST_SYMLINK_MAX.max(self.size as usize).min(4096))?
        } else {
            return Err(RelayError::Failed(
                "NixOS profile entry is not a symlink".into(),
            ));
        };
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn read_extent_bytes(&self, fs: &mut Ext4<'_>, cap: usize) -> Result<Vec<u8>, RelayError> {
        if self.flags & EXTENTS_FL == 0 || self.raw.len() < 52 {
            return Err(RelayError::Failed(
                "NixOS profile is not an extent-mapped file".into(),
            ));
        }
        let header = &self.raw[40..52];
        let magic = u16::from_le_bytes(header[0..2].try_into().unwrap());
        if magic != EXT4_EXTENT_MAGIC {
            return Err(RelayError::Failed(
                "NixOS profile extent header is invalid".into(),
            ));
        }
        let entries = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        let depth = u16::from_le_bytes(header[6..8].try_into().unwrap());
        if depth != 0 {
            return Err(RelayError::Failed(
                "NixOS profile extent tree is too deep".into(),
            ));
        }
        let want = (self.size as usize).min(cap);
        let mut out = Vec::new();
        for index in 0..entries {
            let start = 52 + index * 12;
            if start + 12 > self.raw.len() {
                break;
            }
            let extent = &self.raw[start..start + 12];
            let len = u16::from_le_bytes(extent[4..6].try_into().unwrap()) & 0x7fff;
            let start_hi = u16::from_le_bytes(extent[6..8].try_into().unwrap());
            let start_lo = u32::from_le_bytes(extent[8..12].try_into().unwrap());
            let physical = (u64::from(start_hi) << 32) | u64::from(start_lo);
            for block in 0..u64::from(len) {
                if out.len() >= want {
                    return Ok(out);
                }
                let mut chunk = fs.read_block(physical + block)?;
                let remain = want - out.len();
                if chunk.len() > remain {
                    chunk.truncate(remain);
                }
                out.extend_from_slice(&chunk);
            }
        }
        if out.len() < want {
            return Err(RelayError::Failed(
                "NixOS profile extent does not cover the file".into(),
            ));
        }
        Ok(out)
    }
}

impl<'a> Ext4<'a> {
    fn open(file: &'a mut File) -> Result<Self, RelayError> {
        let mut superblock = [0u8; 1024];
        file.seek(SeekFrom::Start(SUPERBLOCK_OFFSET))
            .and_then(|_| file.read_exact(&mut superblock))
            .map_err(|error| RelayError::Failed(format!("cannot read ext4 superblock: {error}")))?;
        let magic = u16::from_le_bytes(superblock[0x38..0x3a].try_into().unwrap());
        if magic != EXT4_MAGIC {
            return Err(RelayError::Failed(
                "NixOS disk is not an ext4 filesystem".into(),
            ));
        }
        let log_block = u32::from_le_bytes(superblock[0x18..0x1c].try_into().unwrap());
        if log_block > 2 {
            return Err(RelayError::Failed(
                "NixOS disk block size is not 1, 2, or 4 KiB".into(),
            ));
        }
        let block_size = 1024u64 << log_block;
        let inodes_per_group = u32::from_le_bytes(superblock[0x28..0x2c].try_into().unwrap());
        let inode_size = u16::from_le_bytes(superblock[0x58..0x5a].try_into().unwrap()) as u64;
        if inodes_per_group == 0 || !(128..=512).contains(&inode_size) {
            return Err(RelayError::Failed(
                "NixOS disk inode layout is unusable".into(),
            ));
        }
        let incompat = u32::from_le_bytes(superblock[0x60..0x64].try_into().unwrap());
        let ro_compat = u32::from_le_bytes(superblock[0x64..0x68].try_into().unwrap());
        if incompat & INCOMPAT_EXTENTS == 0 || incompat & INCOMPAT_FILETYPE == 0 {
            return Err(RelayError::Failed(
                "NixOS disk is not an extent ext4 filesystem".into(),
            ));
        }
        let desc_size = if incompat & INCOMPAT_64BIT != 0 {
            let recorded = u16::from_le_bytes(superblock[0xfe..0x100].try_into().unwrap());
            u64::from(if recorded == 0 { 64 } else { recorded })
        } else {
            32
        };
        if !(32..1024).contains(&desc_size) {
            return Err(RelayError::Failed(
                "NixOS disk group descriptor size is unusable".into(),
            ));
        }
        let csum_seed = if ro_compat & RO_METADATA_CSUM == 0 {
            None
        } else if incompat & INCOMPAT_CSUM_SEED != 0 {
            Some(u32::from_le_bytes(
                superblock[0x24c..0x250].try_into().unwrap(),
            ))
        } else {
            Some(crc32c(!0, &superblock[0x68..0x78]))
        };
        Ok(Self {
            file,
            superblock: Superblock {
                block_size,
                inodes_per_group,
                inode_size,
                incompat,
                desc_size,
                csum_seed,
            },
        })
    }

    fn read_block(&mut self, block: u64) -> Result<Vec<u8>, RelayError> {
        let mut buf = vec![0u8; self.superblock.block_size as usize];
        self.file
            .seek(SeekFrom::Start(block * self.superblock.block_size))
            .and_then(|_| self.file.read_exact(&mut buf))
            .map_err(|error| {
                RelayError::Failed(format!("cannot read NixOS disk block: {error}"))
            })?;
        Ok(buf)
    }

    fn inode_offset(&mut self, ino: u32) -> Result<u64, RelayError> {
        if ino < 1 {
            return Err(RelayError::Failed("NixOS inode number is invalid".into()));
        }
        let group = u64::from((ino - 1) / self.superblock.inodes_per_group);
        let index = u64::from((ino - 1) % self.superblock.inodes_per_group);
        let desc_offset = self.superblock.block_size + group * self.superblock.desc_size;
        let mut desc = [0u8; 12];
        self.file
            .seek(SeekFrom::Start(desc_offset + 8))
            .and_then(|_| self.file.read_exact(&mut desc[..4]))
            .map_err(|error| {
                RelayError::Failed(format!("cannot read NixOS inode table pointer: {error}"))
            })?;
        let mut table = u64::from(u32::from_le_bytes(desc[0..4].try_into().unwrap()));
        if self.superblock.desc_size >= 0x2c {
            self.file
                .seek(SeekFrom::Start(desc_offset + 0x28))
                .and_then(|_| self.file.read_exact(&mut desc[4..8]))
                .map_err(|error| {
                    RelayError::Failed(format!("cannot read NixOS inode table pointer: {error}"))
                })?;
            table |= u64::from(u32::from_le_bytes(desc[4..8].try_into().unwrap())) << 32;
        }
        Ok(table * self.superblock.block_size + index * self.superblock.inode_size)
    }

    fn read_inode(&mut self, ino: u32) -> Result<Inode, RelayError> {
        let offset = self.inode_offset(ino)?;
        let mut raw = vec![0u8; self.superblock.inode_size as usize];
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.read_exact(&mut raw))
            .map_err(|error| RelayError::Failed(format!("cannot read NixOS inode: {error}")))?;
        let mode = u16::from_le_bytes(raw[0..2].try_into().unwrap());
        let size_lo = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        let flags = u32::from_le_bytes(raw[32..36].try_into().unwrap());
        let blocks_lo = u32::from_le_bytes(raw[28..32].try_into().unwrap());
        let blocks_hi = u16::from_le_bytes(raw[116..118].try_into().unwrap());
        Ok(Inode {
            raw,
            mode,
            size: u64::from(size_lo),
            flags,
            blocks: (u64::from(blocks_hi) << 32) | u64::from(blocks_lo),
        })
    }

    fn write_inode(&mut self, ino: u32, inode: &Inode) -> Result<(), RelayError> {
        let mut raw = inode.raw.clone();
        if let Some(seed) = self.superblock.csum_seed {
            if raw.len() < 128 {
                return Err(RelayError::Failed(
                    "NixOS inode is too small for a metadata checksum".into(),
                ));
            }
            raw[124..126].fill(0);
            let extra = if raw.len() > 128 {
                u16::from_le_bytes(raw[128..130].try_into().unwrap())
            } else {
                0
            };
            let has_hi = raw.len() >= 132 && extra >= 4;
            if has_hi {
                raw[130..132].fill(0);
            }
            let mut crc = crc32c(seed, &ino.to_le_bytes());
            crc = crc32c(crc, &raw[100..104]);
            crc = crc32c(crc, &raw);
            raw[124..126].copy_from_slice(&(crc as u16).to_le_bytes());
            if has_hi {
                raw[130..132].copy_from_slice(&((crc >> 16) as u16).to_le_bytes());
            }
        }
        let offset = self.inode_offset(ino)?;
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.write_all(&raw))
            .map_err(|error| {
                RelayError::Failed(format!("cannot update NixOS system profile: {error}"))
            })?;
        Ok(())
    }
}

fn crc32c(mut crc: u32, data: &[u8]) -> u32 {
    crc = !crc;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
        }
    }
    !crc
}

#[cfg(kani)]
mod proofs {
    use super::generation_from_profile_name;

    #[kani::proof]
    fn parsed_generation_is_nonzero() {
        let byte: u8 = kani::any();
        let name = [
            b's', b'y', b's', b't', b'e', b'm', b'-', byte, b'-', b'l', b'i', b'n', b'k',
        ];
        if let Some(number) = generation_from_profile_name(&name) {
            assert!(number > 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn crc32c_matches_castagnoli_vector() {
        assert_eq!(crc32c(0, b"123456789"), 0xe306_9283);
        assert_eq!(
            crc32c(0, b"123456789"),
            crc32c(crc32c(0, b"12345"), b"6789")
        );
    }

    #[test]
    fn profile_names_are_generation_numbers() {
        assert_eq!(generation_from_profile_name(b"system-1-link"), Some(1));
        assert_eq!(generation_from_profile_name(b"system-12-link"), Some(12));
        assert_eq!(generation_from_profile_name(b"system-01-link"), None);
        assert_eq!(generation_from_profile_name(b"init"), None);
        assert_eq!(generation_from_profile_name(b"wawona-init"), None);
    }

    #[test]
    fn lists_and_activates_a_generation() {
        let path = std::env::temp_dir().join(format!("relay-generations-{}", std::process::id()));
        let bytes = sample_image(false, false);
        File::create(&path).unwrap().write_all(&bytes).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&generations_json(&path).unwrap()).unwrap();
        assert_eq!(json["present"], true);
        assert_eq!(json["generations"][0]["number"], 1);
        assert_eq!(json["generations"][0]["current"], true);
        assert_eq!(json["generations"][1]["number"], 2);
        assert_eq!(json["generations"][1]["current"], false);
        let mut file = File::options().read(true).write(true).open(&path).unwrap();
        activate_generation(&mut file, 2).unwrap();
        drop(file);
        let json: serde_json::Value =
            serde_json::from_str(&generations_json(&path).unwrap()).unwrap();
        assert_eq!(json["generations"][1]["current"], true);
        assert_eq!(json["generations"][0]["current"], false);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dirty_journal_is_not_rewritten() {
        let path =
            std::env::temp_dir().join(format!("relay-generations-dirty-{}", std::process::id()));
        File::create(&path)
            .unwrap()
            .write_all(&sample_image(false, true))
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut file = File::options().read(true).write(true).open(&path).unwrap();
        let error = activate_generation(&mut file, 2).unwrap_err();
        assert!(error.to_string().contains("journal"));
        drop(file);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn metadata_checksum_round_trips() {
        let path =
            std::env::temp_dir().join(format!("relay-generations-csum-{}", std::process::id()));
        File::create(&path)
            .unwrap()
            .write_all(&sample_image(true, false))
            .unwrap();
        let mut file = File::options().read(true).write(true).open(&path).unwrap();
        activate_generation(&mut file, 2).unwrap();
        let mut fs = Ext4::open(&mut file).unwrap();
        let profiles = walk_profile(&mut fs).unwrap();
        let system = lookup_dir(&mut fs, profiles, b"system").unwrap().unwrap();
        let inode = fs.read_inode(system).unwrap();
        assert_eq!(inode.symlink_bytes(), b"system-2-link");
        let stored = u32::from(u16::from_le_bytes(inode.raw[124..126].try_into().unwrap()))
            | (u32::from(u16::from_le_bytes(inode.raw[130..132].try_into().unwrap())) << 16);
        let mut raw = inode.raw.clone();
        raw[124..126].fill(0);
        raw[130..132].fill(0);
        let seed = fs.superblock.csum_seed.unwrap();
        let mut crc = crc32c(seed, &system.to_le_bytes());
        crc = crc32c(crc, &raw[100..104]);
        crc = crc32c(crc, &raw);
        assert_eq!(stored, crc);
        drop(fs);
        let _ = std::fs::remove_file(path);
    }

    fn sample_image(checksums: bool, dirty: bool) -> Vec<u8> {
        const BLOCK: usize = 4096;
        let mut image = vec![0u8; BLOCK * 12];
        let sb = 1024;
        put32(&mut image, sb + 0x00, 16);
        put32(&mut image, sb + 0x04, 12);
        put32(&mut image, sb + 0x18, 2);
        put32(&mut image, sb + 0x20, 32768);
        put32(&mut image, sb + 0x28, 16);
        put16(&mut image, sb + 0x38, EXT4_MAGIC);
        put32(&mut image, sb + 0x4c, 1);
        put16(&mut image, sb + 0x58, 256);
        let mut incompat = INCOMPAT_FILETYPE | INCOMPAT_EXTENTS | INCOMPAT_64BIT;
        if dirty {
            incompat |= INCOMPAT_RECOVER;
        }
        if checksums {
            incompat |= INCOMPAT_CSUM_SEED;
        }
        put32(&mut image, sb + 0x60, incompat);
        if checksums {
            put32(&mut image, sb + 0x64, RO_METADATA_CSUM);
            put32(&mut image, sb + 0x24c, 0x1234_5678);
        }
        put16(&mut image, sb + 0xfe, 64);
        put32(&mut image, BLOCK + 8, 2);

        let long = b"/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-nixos-system-wawona-mobile-guest";
        write_dir(
            &mut image,
            3,
            &[(2, ".", true), (2, "..", true), (3, "nix", true)],
        );
        write_dir(
            &mut image,
            4,
            &[(3, ".", true), (2, "..", true), (4, "var", true)],
        );
        write_dir(
            &mut image,
            5,
            &[(4, ".", true), (3, "..", true), (5, "nix", true)],
        );
        write_dir(
            &mut image,
            6,
            &[(5, ".", true), (4, "..", true), (6, "profiles", true)],
        );
        write_dir(
            &mut image,
            7,
            &[
                (6, ".", true),
                (5, "..", true),
                (7, "system", false),
                (8, "system-1-link", false),
                (9, "system-2-link", false),
            ],
        );
        image[8 * BLOCK..8 * BLOCK + long.len()].copy_from_slice(long);

        write_inode(&mut image, 2, dir_inode(3, checksums));
        write_inode(&mut image, 3, dir_inode(4, checksums));
        write_inode(&mut image, 4, dir_inode(5, checksums));
        write_inode(&mut image, 5, dir_inode(6, checksums));
        write_inode(&mut image, 6, dir_inode(7, checksums));
        write_inode(&mut image, 7, fast_symlink(b"system-1-link", checksums));
        write_inode(&mut image, 8, fast_symlink(b"/nix/store/abc", checksums));
        write_inode(&mut image, 9, slow_symlink(long.len(), 8, checksums));
        if checksums {
            for ino in 2..=9 {
                seal_inode(&mut image, ino as u32);
            }
        }
        image
    }

    fn dir_inode(block: u32, checksums: bool) -> Vec<u8> {
        let mut raw = vec![0u8; 256];
        put16(&mut raw, 0, S_IFDIR | 0o755);
        put32(&mut raw, 4, 4096);
        put16(&mut raw, 26, 2);
        put32(&mut raw, 28, 8);
        put32(&mut raw, 32, EXTENTS_FL);
        put16(&mut raw, 40, EXT4_EXTENT_MAGIC);
        put16(&mut raw, 42, 1);
        put16(&mut raw, 44, 4);
        put16(&mut raw, 56, 1);
        put32(&mut raw, 60, block);
        if checksums {
            put16(&mut raw, 128, 32);
        }
        raw
    }

    fn fast_symlink(target: &[u8], checksums: bool) -> Vec<u8> {
        let mut raw = vec![0u8; 256];
        put16(&mut raw, 0, S_IFLNK | 0o777);
        put32(&mut raw, 4, target.len() as u32);
        put16(&mut raw, 26, 1);
        raw[40..40 + target.len()].copy_from_slice(target);
        if checksums {
            put16(&mut raw, 128, 32);
        }
        raw
    }

    fn slow_symlink(len: usize, block: u32, checksums: bool) -> Vec<u8> {
        let mut raw = vec![0u8; 256];
        put16(&mut raw, 0, S_IFLNK | 0o777);
        put32(&mut raw, 4, len as u32);
        put16(&mut raw, 26, 1);
        put32(&mut raw, 28, 8);
        put32(&mut raw, 32, EXTENTS_FL);
        put16(&mut raw, 40, EXT4_EXTENT_MAGIC);
        put16(&mut raw, 42, 1);
        put16(&mut raw, 44, 4);
        put16(&mut raw, 56, 1);
        put32(&mut raw, 60, block);
        if checksums {
            put16(&mut raw, 128, 32);
        }
        raw
    }

    fn write_inode(image: &mut [u8], ino: u32, raw: Vec<u8>) {
        let offset = 2 * 4096 + (ino as usize - 1) * 256;
        image[offset..offset + 256].copy_from_slice(&raw);
    }

    fn seal_inode(image: &mut [u8], ino: u32) {
        let offset = 2 * 4096 + (ino as usize - 1) * 256;
        let mut raw = image[offset..offset + 256].to_vec();
        raw[124..126].fill(0);
        raw[130..132].fill(0);
        let mut crc = crc32c(0x1234_5678, &ino.to_le_bytes());
        crc = crc32c(crc, &raw[100..104]);
        crc = crc32c(crc, &raw);
        raw[124..126].copy_from_slice(&(crc as u16).to_le_bytes());
        raw[130..132].copy_from_slice(&((crc >> 16) as u16).to_le_bytes());
        image[offset..offset + 256].copy_from_slice(&raw);
    }

    fn write_dir(image: &mut [u8], block: usize, entries: &[(u32, &str, bool)]) {
        let start = block * 4096;
        let mut offset = 0;
        for (index, (ino, name, dir)) in entries.iter().enumerate() {
            let name_len = name.len();
            let natural = (8 + name_len + 3) & !3;
            let rec = if index + 1 == entries.len() {
                4096 - offset
            } else {
                natural
            };
            let at = start + offset;
            put32(image, at, *ino);
            put16(image, at + 4, rec as u16);
            image[at + 6] = name_len as u8;
            image[at + 7] = if *dir { 2 } else { 7 };
            image[at + 8..at + 8 + name_len].copy_from_slice(name.as_bytes());
            offset += rec;
        }
    }

    fn put16(buf: &mut [u8], at: usize, value: u16) {
        buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put32(buf: &mut [u8], at: usize, value: u32) {
        buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}
