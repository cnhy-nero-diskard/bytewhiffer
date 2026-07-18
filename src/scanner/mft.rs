//! Phase 2 scanning engine: the NTFS `$MFT`-reading "turbo" engine. Reads a
//! local NTFS volume's Master File Table in one sequential I/O pass, parses the
//! fixed-size records in parallel, and rolls them up bottom-up into the same
//! [`Entry`](super::Entry) tree the directory walker produces — the technique
//! WizTree uses to beat a call-by-call directory walk on large volumes.
//!
//! # Cross-platform split
//!
//! Everything that turns *bytes* into a tree — boot-sector parsing, `$MFT`
//! record parsing (with Update-Sequence-Array fixups), data-run decoding, and
//! the parent→children reconstruction — is pure and platform-independent, so it
//! is exercised by `cargo test` on any host against synthetic `$MFT` byte
//! layouts (see the tests at the bottom of this file). Only the parts that
//! *cannot* run without real Windows hardware and an elevated token — opening a
//! raw `\\.\C:` volume handle, detecting the volume filesystem and process
//! elevation, and the self-relaunch that triggers UAC — are `#[cfg(windows)]`,
//! with inert stubs on other platforms so the module still compiles and the UI
//! can be driven (the toggle simply reports [`Availability::UnsupportedFilesystem`]
//! off-Windows). This mirrors the no-GUI-dependency rule the rest of `scanner/`
//! follows and the design's explicit testability contract.
//!
//! The `ntfs` crate was evaluated for the record parsing and rejected: it has
//! no public entry point for parsing a record out of an in-memory buffer (only
//! volume-relative reads through a single `&mut` reader), which is incompatible
//! with the flat-buffer + parallel-parse architecture that is the whole reason
//! this engine exists. The subset of the FILE-record format needed here
//! (`$STANDARD_INFORMATION` flags, `$FILE_NAME` name/parent, unnamed `$DATA`
//! size, and the record header) is small and well-specified, so it is
//! hand-rolled below.

// The `$MFT` parsing/reconstruction core is compiled-in and exercised only on
// Windows (the `platform` module consumes it) and under `cfg(test)` (the
// synthetic-layout tests). A plain non-Windows release build links none of it,
// so it would otherwise flag every parser function as dead. Suppress dead-code
// *only* in that build; on Windows and test builds detection stays fully active,
// so a genuinely-unused helper is still caught where the code actually runs.
#![cfg_attr(not(any(windows, test)), allow(dead_code))]

use std::path::Path;

use super::{Availability, Entry};

// ---------------------------------------------------------------------------
// On-disk format constants (NTFS FILE record + attributes)
// ---------------------------------------------------------------------------

/// FILE-record header field offsets.
mod rec {
    pub const SIGNATURE: usize = 0x00; // b"FILE" (b"BAAD" = corrupt)
    pub const USA_OFFSET: usize = 0x04; // u16: offset to the Update Sequence Array
    pub const USA_COUNT: usize = 0x06; // u16: USA entry count (1 USN + N fixups)
    pub const HARD_LINK_COUNT: usize = 0x12; // u16
    pub const FIRST_ATTR_OFFSET: usize = 0x14; // u16
    pub const FLAGS: usize = 0x16; // u16
    pub const BASE_RECORD_REF: usize = 0x20; // u64 (0 => this is a base record)
}

/// Attribute header field offsets (common prefix, then resident/non-resident).
mod attr {
    pub const TYPE: usize = 0x00; // u32
    pub const LENGTH: usize = 0x04; // u32: total attribute length (advance by this)
    pub const NON_RESIDENT: usize = 0x08; // u8: 0 = resident, 1 = non-resident
    pub const NAME_LENGTH: usize = 0x09; // u8: name length in UTF-16 units
    // Resident:
    pub const RES_VALUE_LENGTH: usize = 0x10; // u32
    pub const RES_VALUE_OFFSET: usize = 0x14; // u16
    // Non-resident:
    pub const NONRES_REAL_SIZE: usize = 0x30; // u64: actual data size
}

/// `$FILE_NAME` attribute-content field offsets.
mod fname {
    pub const PARENT_REF: usize = 0x00; // u64: parent dir file reference
    pub const FLAGS: usize = 0x38; // u32: file attribute flags
    pub const NAME_LENGTH: usize = 0x40; // u8: length in UTF-16 units
    pub const NAMESPACE: usize = 0x41; // u8
    pub const NAME: usize = 0x42; // UTF-16LE, NAME_LENGTH units
}

/// `$STANDARD_INFORMATION` attribute-content field offsets.
mod stdinfo {
    pub const FILE_ATTRIBUTES: usize = 0x20; // u32
}

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

const FLAG_IN_USE: u16 = 0x0001;
const FLAG_DIRECTORY: u16 = 0x0002;

/// `FILE_ATTRIBUTE_REPARSE_POINT`. A reparse point / junction carries this in
/// its `$STANDARD_INFORMATION` and `$FILE_NAME` flags; the tree must not
/// traverse *through* it as an ordinary directory.
const FILE_ATTR_REPARSE_POINT: u32 = 0x0400;

/// DOS-only 8.3 short name namespace. These are duplicates of a real Win32
/// name and must never be used as the entry's display name.
const NAMESPACE_DOS: u8 = 2;

/// NTFS reserves the first 16 records for metadata files (`$MFT`, `$MFTMirr`,
/// `$LogFile`, …). A normal directory listing never surfaces these, so
/// excluding them keeps the reconstructed tree equivalent to the walker's,
/// which also never sees them. Record 5 is the root directory — it is the tree
/// anchor, not an emitted child.
const FIRST_USER_RECORD: u64 = 16;
/// The root directory's fixed record number.
pub(crate) const ROOT_RECORD: u64 = 5;

/// The standard hardware sector size fixups are computed against when a record
/// length isn't cleanly divisible into its sector count.
const DEFAULT_SECTOR_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// Little-endian readers (bounds-checked; None on truncation)
// ---------------------------------------------------------------------------

fn le_u16(b: &[u8], off: usize) -> Option<u16> {
    b.get(off..off + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn le_u32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le_u64(b: &[u8], off: usize) -> Option<u64> {
    b.get(off..off + 8).map(|s| {
        u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
    })
}

/// A file reference is a 64-bit value whose low 48 bits are the record number
/// and whose high 16 bits are a reuse sequence number. Only the record number
/// matters for tree reconstruction.
fn record_number_of(reference: u64) -> u64 {
    reference & 0x0000_FFFF_FFFF_FFFF
}

// ---------------------------------------------------------------------------
// Boot sector
// ---------------------------------------------------------------------------

/// The geometry read from an NTFS volume's boot sector, enough to locate and
/// size the `$MFT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootInfo {
    pub cluster_size: u64,
    /// First-cluster number (LCN) of the `$MFT`.
    pub mft_lcn: u64,
    /// Size of one FILE record in bytes (typically 1024).
    pub record_size: usize,
}

impl BootInfo {
    pub fn mft_byte_offset(&self) -> u64 {
        self.mft_lcn * self.cluster_size
    }
}

/// Parses an NTFS boot sector. Returns `None` if it is not an NTFS boot sector
/// or is truncated. Pure — the caller supplies the first sector's bytes.
pub(crate) fn parse_boot_sector(b: &[u8]) -> Option<BootInfo> {
    // OEM ID "NTFS    " at offset 3 is the cheap filesystem sanity check.
    if b.get(3..11)? != b"NTFS    " {
        return None;
    }
    let bytes_per_sector = le_u16(b, 0x0B)? as u64;
    if bytes_per_sector == 0 {
        return None;
    }
    // Sectors-per-cluster is a signed power-of-two code when > 0x80 (used for
    // clusters larger than 64 KiB), otherwise a literal count.
    let spc_raw = *b.get(0x0D)? as i8;
    let sectors_per_cluster: u64 = if spc_raw >= 0 {
        spc_raw as u64
    } else {
        1u64 << ((-(spc_raw as i32)) as u32)
    };
    let cluster_size = bytes_per_sector * sectors_per_cluster;
    let mft_lcn = le_u64(b, 0x30)?;

    // Clusters-per-file-record-segment uses the same signed convention: a
    // negative value is a byte size of 2^|v|, a positive value is a cluster
    // count.
    let crps = *b.get(0x40)? as i8;
    let record_size = if crps >= 0 {
        (crps as u64 * cluster_size) as usize
    } else {
        1usize << ((-(crps as i32)) as u32)
    };
    if record_size == 0 || cluster_size == 0 {
        return None;
    }
    Some(BootInfo {
        cluster_size,
        mft_lcn,
        record_size,
    })
}

// ---------------------------------------------------------------------------
// Update Sequence Array fixups
// ---------------------------------------------------------------------------

/// Applies the FILE record's Update Sequence Array fixups in place: NTFS
/// replaces the last two bytes of every sector with a per-record sequence
/// number for torn-write detection and stores the originals in the USA. Every
/// consumer must restore them before reading attribute data that spans a sector
/// boundary. Returns `false` (leaving the buffer untouched past what it
/// checked) if the record is malformed or the sequence numbers don't match, so
/// the caller can skip a corrupt record rather than trust garbage.
pub(crate) fn apply_fixups(buf: &mut [u8]) -> bool {
    let Some(usa_off) = le_u16(buf, rec::USA_OFFSET).map(|v| v as usize) else {
        return false;
    };
    let Some(count) = le_u16(buf, rec::USA_COUNT).map(|v| v as usize) else {
        return false;
    };
    if count < 1 {
        return false;
    }
    let sectors = count - 1; // entry 0 is the USN itself
    if sectors == 0 {
        return true;
    }
    // Prefer the sector size implied by the record length; fall back to the
    // hardware default if it doesn't divide evenly.
    let sector_size = if buf.len() % sectors == 0 {
        buf.len() / sectors
    } else {
        DEFAULT_SECTOR_SIZE
    };

    let Some(usn) = le_u16(buf, usa_off) else {
        return false;
    };
    for i in 0..sectors {
        let fixup_entry_off = usa_off + 2 * (i + 1);
        let Some(fixup) = le_u16(buf, fixup_entry_off) else {
            return false;
        };
        let tail = (i + 1) * sector_size;
        if tail < 2 || tail > buf.len() {
            return false;
        }
        // The two bytes NTFS overwrote with the USN must currently read as the
        // USN; a mismatch means a torn write or a bad record.
        let cur = le_u16(buf, tail - 2);
        if cur != Some(usn) {
            return false;
        }
        buf[tail - 2..tail].copy_from_slice(&fixup.to_le_bytes());
    }
    true
}

// ---------------------------------------------------------------------------
// Record parsing
// ---------------------------------------------------------------------------

/// One `$FILE_NAME` attribute of a record: which directory it lives in and
/// under what name. A hard-linked file has several of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileNameEntry {
    pub parent: u64,
    pub name: String,
    pub namespace: u8,
}

/// The parsed, engine-relevant subset of one `$MFT` FILE record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRecord {
    pub record_number: u64,
    pub in_use: bool,
    pub is_dir: bool,
    /// A base record has no base-record reference; extension records (which
    /// only continue a base's attribute list) are not files in their own right.
    pub is_base: bool,
    pub hard_link_count: u16,
    /// A reparse point / junction: recorded so the tree never traverses through
    /// it as an ordinary directory.
    pub is_reparse: bool,
    /// Actual size of the unnamed `$DATA` stream (0 for directories or when
    /// absent). Named alternate data streams are intentionally not summed, to
    /// match what the directory walker's `metadata().len()` reports per file.
    pub data_size: u64,
    pub names: Vec<FileNameEntry>,
}

impl ParsedRecord {
    /// The name to show for this record: the first Win32 / POSIX name,
    /// falling back to any non-DOS name. DOS 8.3 short names are skipped —
    /// they're duplicates of a real name, and using one would mislabel the
    /// entry. `None` if the record has no usable name (e.g. an orphan).
    pub fn best_name(&self) -> Option<&FileNameEntry> {
        self.names
            .iter()
            .find(|n| n.namespace != NAMESPACE_DOS)
            .or_else(|| self.names.first())
    }
}

/// Parses one FILE record out of `buf` (which must already have had
/// [`apply_fixups`] applied), tagging it with `record_number` (its index in the
/// `$MFT`, which is authoritative regardless of what the header claims).
/// Returns `None` for a non-FILE / corrupt buffer. Pure and allocation-light.
pub(crate) fn parse_record(record_number: u64, buf: &[u8]) -> Option<ParsedRecord> {
    if buf.get(rec::SIGNATURE..rec::SIGNATURE + 4)? != b"FILE" {
        return None;
    }
    let flags = le_u16(buf, rec::FLAGS)?;
    let in_use = flags & FLAG_IN_USE != 0;
    let is_dir = flags & FLAG_DIRECTORY != 0;
    let hard_link_count = le_u16(buf, rec::HARD_LINK_COUNT)?;
    let is_base = record_number_of(le_u64(buf, rec::BASE_RECORD_REF)?) == 0;

    let mut names = Vec::new();
    let mut data_size: u64 = 0;
    let mut is_reparse = false;

    let mut off = le_u16(buf, rec::FIRST_ATTR_OFFSET)? as usize;
    // Walk the attribute list until the end marker or a malformed entry. The
    // length field of each attribute is how we advance; a zero length would
    // loop forever, so it terminates the walk.
    loop {
        let attr_type = le_u32(buf, off + attr::TYPE)?;
        if attr_type == ATTR_END {
            break;
        }
        let attr_len = le_u32(buf, off + attr::LENGTH)? as usize;
        if attr_len == 0 || off + attr_len > buf.len() {
            break;
        }
        let non_resident = *buf.get(off + attr::NON_RESIDENT)? != 0;

        match attr_type {
            ATTR_STANDARD_INFORMATION if !non_resident => {
                if let Some(content) = resident_content(buf, off) {
                    if let Some(fa) = le_u32(content, stdinfo::FILE_ATTRIBUTES) {
                        if fa & FILE_ATTR_REPARSE_POINT != 0 {
                            is_reparse = true;
                        }
                    }
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                if let Some(content) = resident_content(buf, off) {
                    if let Some(entry) = parse_file_name(content) {
                        if let Some(fa) = le_u32(content, fname::FLAGS) {
                            if fa & FILE_ATTR_REPARSE_POINT != 0 {
                                is_reparse = true;
                            }
                        }
                        names.push(entry);
                    }
                }
            }
            ATTR_DATA => {
                // Only the *unnamed* `$DATA` stream is the file's size; named
                // streams (ADS) are separate and not counted, matching the
                // walker. Use the largest unnamed stream seen (there is only
                // one, but be defensive).
                let name_len = *buf.get(off + attr::NAME_LENGTH)?;
                if name_len == 0 {
                    let size = if non_resident {
                        le_u64(buf, off + attr::NONRES_REAL_SIZE).unwrap_or(0)
                    } else {
                        le_u32(buf, off + attr::RES_VALUE_LENGTH).unwrap_or(0) as u64
                    };
                    data_size = data_size.max(size);
                }
            }
            _ => {}
        }

        off += attr_len;
        if off + 4 > buf.len() {
            break;
        }
    }

    Some(ParsedRecord {
        record_number,
        in_use,
        is_dir,
        is_base,
        hard_link_count,
        is_reparse,
        data_size,
        names,
    })
}

/// Returns the byte slice of a resident attribute's value, given the attribute
/// header's start offset.
fn resident_content(buf: &[u8], attr_off: usize) -> Option<&[u8]> {
    let value_off = attr_off + le_u16(buf, attr_off + attr::RES_VALUE_OFFSET)? as usize;
    let value_len = le_u32(buf, attr_off + attr::RES_VALUE_LENGTH)? as usize;
    buf.get(value_off..value_off + value_len)
}

/// Parses a `$FILE_NAME` attribute's content into its parent reference and
/// (UTF-16LE) name.
fn parse_file_name(content: &[u8]) -> Option<FileNameEntry> {
    let parent = record_number_of(le_u64(content, fname::PARENT_REF)?);
    let name_len = *content.get(fname::NAME_LENGTH)? as usize;
    let namespace = *content.get(fname::NAMESPACE)?;
    let name_bytes = content.get(fname::NAME..fname::NAME + name_len * 2)?;
    let units: Vec<u16> = name_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let name = String::from_utf16_lossy(&units);
    Some(FileNameEntry {
        parent,
        name,
        namespace,
    })
}

// ---------------------------------------------------------------------------
// Data runs (only needed to locate the $MFT's own fragments during the read)
// ---------------------------------------------------------------------------

/// One extent of a non-resident attribute: a run of `cluster_count` clusters
/// starting at logical cluster `lcn` (already resolved to an absolute LCN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DataRun {
    pub lcn: u64,
    pub cluster_count: u64,
}

/// Decodes an NTFS data-run list. Each run is a header byte (low nibble =
/// length-field byte count, high nibble = offset-field byte count) followed by
/// the little-endian length and a *signed* offset delta relative to the
/// previous run's LCN. A zero header byte ends the list. Sparse runs (zero
/// offset length) are skipped for our purposes since the `$MFT` is never
/// sparse. Pure.
pub(crate) fn parse_data_runs(b: &[u8]) -> Vec<DataRun> {
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut prev_lcn: i64 = 0;
    while i < b.len() {
        let header = b[i];
        if header == 0 {
            break;
        }
        let len_bytes = (header & 0x0F) as usize;
        let off_bytes = (header >> 4) as usize;
        i += 1;
        if len_bytes == 0 || i + len_bytes + off_bytes > b.len() {
            break;
        }

        let mut cluster_count: u64 = 0;
        for (k, &byte) in b[i..i + len_bytes].iter().enumerate() {
            cluster_count |= (byte as u64) << (8 * k);
        }
        i += len_bytes;

        if off_bytes == 0 {
            // Sparse run: no on-disk clusters. Skip it but keep decoding.
            continue;
        }
        // Sign-extend the offset delta from its byte length.
        let mut delta: i64 = 0;
        for (k, &byte) in b[i..i + off_bytes].iter().enumerate() {
            delta |= (byte as i64) << (8 * k);
        }
        let sign_bit = 1i64 << (8 * off_bytes - 1);
        if delta & sign_bit != 0 {
            delta -= 1i64 << (8 * off_bytes);
        }
        i += off_bytes;

        prev_lcn += delta;
        if prev_lcn < 0 {
            break;
        }
        runs.push(DataRun {
            lcn: prev_lcn as u64,
            cluster_count,
        });
    }
    runs
}

/// Finds the unnamed non-resident `$DATA` attribute in a (fixed-up) FILE record
/// and returns its decoded data runs. Used to locate the `$MFT`'s own fragments
/// from record 0. Returns an empty vec if `$DATA` is resident or absent.
pub(crate) fn data_runs_of(buf: &[u8]) -> Vec<DataRun> {
    let Some(mut off) = le_u16(buf, rec::FIRST_ATTR_OFFSET).map(|v| v as usize) else {
        return Vec::new();
    };
    loop {
        let Some(attr_type) = le_u32(buf, off + attr::TYPE) else {
            break;
        };
        if attr_type == ATTR_END {
            break;
        }
        let Some(attr_len) = le_u32(buf, off + attr::LENGTH).map(|v| v as usize) else {
            break;
        };
        if attr_len == 0 || off + attr_len > buf.len() {
            break;
        }
        let non_resident = buf.get(off + attr::NON_RESIDENT).copied().unwrap_or(0) != 0;
        let name_len = buf.get(off + attr::NAME_LENGTH).copied().unwrap_or(0);
        if attr_type == ATTR_DATA && non_resident && name_len == 0 {
            // Non-resident data-run list offset lives at 0x20 in the header.
            if let Some(runs_off) = le_u16(buf, off + 0x20).map(|v| v as usize) {
                if let Some(run_bytes) = buf.get(off + runs_off..off + attr_len) {
                    return parse_data_runs(run_bytes);
                }
            }
        }
        off += attr_len;
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// Tree reconstruction (pure)
// ---------------------------------------------------------------------------

/// Rolls a flat set of parsed records up into an [`Entry`] subtree rooted at
/// `root_record`, with `root_name`/`root_path` naming that anchor. This is the
/// bottom-up pass: it builds a parent→children map, then walks down from the
/// root summing sizes, matching the walker's contract (recursive directory
/// sizes, no double-counting, no ghost entries). Children are left unsorted
/// here; the caller sorts largest-first once at the end, exactly like the
/// walker. Pure — no I/O, no `ScanContext`.
///
/// Correctness rules baked in, per the `disk-scanning` spec:
/// - deleted-but-unreclaimed records (`in_use == false`) are dropped, so no
///   ghost entries;
/// - extension records (`is_base == false`) and reserved metadata records
///   (`record_number < FIRST_USER_RECORD`, except the root) are dropped;
/// - a file's size is counted once regardless of `hard_link_count` (each record
///   contributes its single `data_size`, placed under one parent);
/// - reparse points / junctions become zero-child leaves — never traversed —
///   avoiding double-counted sizes or infinite recursion on a cyclic link;
/// - a `visited` set guards against a malformed parent cycle.
pub(crate) fn reconstruct(
    records: &[ParsedRecord],
    root_record: u64,
    root_name: String,
    root_path: std::path::PathBuf,
) -> Entry {
    use std::collections::HashMap;

    // Index the usable records and bucket child record numbers by parent.
    let mut by_number: HashMap<u64, &ParsedRecord> = HashMap::new();
    let mut children_of: HashMap<u64, Vec<u64>> = HashMap::new();
    for r in records {
        if !r.in_use || !r.is_base {
            continue;
        }
        if r.record_number < FIRST_USER_RECORD && r.record_number != root_record {
            continue;
        }
        by_number.insert(r.record_number, r);
        if r.record_number == root_record {
            continue; // the root's own parent link is irrelevant
        }
        if let Some(name) = r.best_name() {
            children_of.entry(name.parent).or_default().push(r.record_number);
        }
    }

    fn build(
        num: u64,
        name: String,
        path: std::path::PathBuf,
        is_dir: bool,
        own_size: u64,
        is_reparse: bool,
        by_number: &HashMap<u64, &ParsedRecord>,
        children_of: &HashMap<u64, Vec<u64>>,
        visited: &mut std::collections::HashSet<u64>,
    ) -> Entry {
        let mut total = own_size;
        let mut children = Vec::new();

        // A reparse point / junction is a leaf: don't descend, so a cyclic
        // junction can't loop and a linked target isn't double-counted.
        if is_dir && !is_reparse && visited.insert(num) {
            if let Some(kids) = children_of.get(&num) {
                for &child_num in kids {
                    let Some(rec) = by_number.get(&child_num) else {
                        continue;
                    };
                    let Some(fname) = rec.best_name() else {
                        continue;
                    };
                    let child_path = path.join(&fname.name);
                    let child = build(
                        child_num,
                        fname.name.clone(),
                        child_path,
                        rec.is_dir,
                        rec.data_size,
                        rec.is_reparse,
                        by_number,
                        children_of,
                        visited,
                    );
                    total += child.size;
                    children.push(child);
                }
            }
        }

        Entry {
            name,
            path,
            size: total,
            is_dir,
            children,
        }
    }

    let mut visited = std::collections::HashSet::new();
    let root_is_dir = by_number
        .get(&root_record)
        .map(|r| r.is_dir)
        .unwrap_or(true);
    let root_own = by_number
        .get(&root_record)
        .map(|r| r.data_size)
        .unwrap_or(0);
    build(
        root_record,
        root_name,
        root_path,
        root_is_dir,
        root_own,
        false,
        &by_number,
        &children_of,
        &mut visited,
    )
}

// ---------------------------------------------------------------------------
// Availability (pure branching + cfg-gated probes)
// ---------------------------------------------------------------------------

/// The pure three-way capability decision the `disk-scanning` spec describes,
/// factored out from the OS probes so it is unit-testable without a real volume
/// or an elevated token: non-NTFS → `UnsupportedFilesystem`; NTFS but
/// unelevated → `RequiresElevation`; NTFS and elevated → `Available`.
pub(crate) fn resolve_availability(is_ntfs: bool, elevated: bool) -> Availability {
    if !is_ntfs {
        Availability::UnsupportedFilesystem
    } else if !elevated {
        Availability::RequiresElevation
    } else {
        Availability::Available
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// The NTFS `$MFT`-reading turbo engine. Only usable on Windows with an
/// elevated token against a local NTFS volume; everywhere else it reports
/// [`Availability::UnsupportedFilesystem`] and the orchestration layer falls
/// back to the walker.
pub struct MftEngine;

impl super::ScanEngine for MftEngine {
    fn name(&self) -> &'static str {
        "mft-turbo"
    }

    fn is_available(&self, target: &Path) -> Availability {
        // Re-evaluated on every call (the UI calls this on each scan-target
        // change) so switching volumes correctly re-derives the state rather
        // than reusing a stale result.
        resolve_availability(volume_is_ntfs(target), process_is_elevated())
    }

    fn scan(
        &self,
        target: &Path,
        ctx: &super::ScanContext,
    ) -> Result<Entry, super::ScanError> {
        platform::scan(target, ctx)
    }
}

/// Whether the current process holds an elevated token. Exposed so the UI can
/// record "elevated for this process's lifetime" once at startup.
pub fn process_is_elevated() -> bool {
    platform::process_is_elevated()
}

/// Whether `target`'s volume is NTFS.
pub fn volume_is_ntfs(target: &Path) -> bool {
    platform::volume_is_ntfs(target)
}

/// Relaunches this executable elevated (triggering UAC), asking the new process
/// to scan `scan_root`. Returns `Ok(true)` if a new elevated process was
/// launched (this one should exit), `Ok(false)` if the user declined the UAC
/// prompt (this one keeps running unelevated), or `Err` on an unexpected
/// failure. A no-op returning `Ok(false)` off-Windows.
pub fn relaunch_elevated(scan_root: &Path) -> std::io::Result<bool> {
    platform::relaunch_elevated(scan_root)
}

/// Wraps `arg` in double quotes for a Windows command line, following the
/// `CommandLineToArgvW` backslash/quote rules so the value round-trips through
/// `std::env::args` in the relaunched process unchanged. The rule that bites
/// here: a run of backslashes immediately before a closing quote must be
/// doubled, otherwise `\"` reads as an escaped literal quote. A drive root like
/// `D:\` naively quoted as `"D:\"` would arrive as `D:"`; this yields `"D:\\"`,
/// which arrives as `D:\`. Pure and testable on any host.
pub fn quote_windows_arg(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut pending_backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => pending_backslashes += 1,
            '"' => {
                // Escape every backslash run that precedes a quote, then the
                // quote itself.
                for _ in 0..(pending_backslashes * 2 + 1) {
                    out.push('\\');
                }
                pending_backslashes = 0;
                out.push('"');
            }
            _ => {
                for _ in 0..pending_backslashes {
                    out.push('\\');
                }
                pending_backslashes = 0;
                out.push(c);
            }
        }
    }
    // Double any trailing backslashes so they don't escape the closing quote.
    for _ in 0..(pending_backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Platform layer
// ---------------------------------------------------------------------------

/// Windows implementation: raw volume access, real elevation/filesystem probes,
/// and the `runas` self-relaunch. None of this can run in the Linux dev
/// environment, so it is verified by a human on real hardware (see the change's
/// tasks.md §8); the pure code above carries the unit-tested correctness.
#[cfg(windows)]
mod platform {
    use super::*;
    use rayon::prelude::*;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;

    use crate::scanner::{Entry, ScanContext, ScanError};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::Storage::FileSystem::{
        GetVolumeInformationW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// UTF-16, NUL-terminated, for the `W` Win32 APIs.
    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// The `C:` (drive + colon) prefix of a path, if it has one.
    fn drive_of(target: &Path) -> Option<String> {
        let s = target.to_string_lossy();
        let bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            Some(format!("{}:", (bytes[0] as char).to_ascii_uppercase()))
        } else {
            None
        }
    }

    pub fn process_is_elevated() -> bool {
        unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION::default();
            let mut ret_len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            )
            .is_ok();
            let _ = CloseHandle(token);
            ok && elevation.TokenIsElevated != 0
        }
    }

    pub fn volume_is_ntfs(target: &Path) -> bool {
        let Some(drive) = drive_of(target) else {
            return false;
        };
        let root = wide(&format!("{drive}\\"));
        let mut fs_name = [0u16; 32];
        unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut fs_name),
            )
            .is_ok()
        }
        .then(|| {
            let end = fs_name.iter().position(|&c| c == 0).unwrap_or(fs_name.len());
            String::from_utf16_lossy(&fs_name[..end])
        })
        .map(|name| name.eq_ignore_ascii_case("NTFS"))
        .unwrap_or(false)
    }

    pub fn relaunch_elevated(scan_root: &Path) -> std::io::Result<bool> {
        let exe = std::env::current_exe()?;
        let exe_w = wide(&exe.to_string_lossy());
        // `--elevated-scan <root>` is the hidden flag main.rs parses to resume
        // scanning the same root in the fresh elevated process. The root is
        // quoted per the Windows `CommandLineToArgvW` rules so a drive root like
        // `D:\` survives the round-trip instead of arriving as `D:"`.
        let params = format!(
            "--elevated-scan {}",
            super::quote_windows_arg(&scan_root.to_string_lossy())
        );
        let params_w = wide(&params);
        let verb_w = wide("runas");

        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb_w.as_ptr()),
            lpFile: PCWSTR(exe_w.as_ptr()),
            lpParameters: PCWSTR(params_w.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        match unsafe { ShellExecuteExW(&mut info) } {
            Ok(()) => Ok(true),
            Err(err) => {
                // ERROR_CANCELLED (1223) means the user declined the UAC
                // prompt — a normal outcome, not an error to surface.
                const ERROR_CANCELLED: i32 = 1223;
                if err.code().0 & 0xFFFF == ERROR_CANCELLED {
                    Ok(false)
                } else {
                    Err(std::io::Error::other(err))
                }
            }
        }
    }

    /// Opens a raw volume handle (e.g. `\\.\C:`) for sequential reading.
    /// Backup semantics + full sharing are required to read a mounted volume.
    fn open_volume(drive: &str) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(format!(r"\\.\{drive}"))
    }

    /// Reads exactly `len` bytes at `offset`, both of which the caller keeps
    /// sector-aligned (raw volume handles reject misaligned reads).
    fn read_at(file: &mut std::fs::File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// The design's flat single pass: read the boot sector, follow the `$MFT`'s
    /// own data runs, and read every fragment into one contiguous buffer.
    fn read_mft(file: &mut std::fs::File) -> std::io::Result<(Vec<u8>, usize)> {
        let boot = read_at(file, 0, DEFAULT_SECTOR_SIZE)?;
        let info = parse_boot_sector(&boot)
            .ok_or_else(|| std::io::Error::other("not an NTFS boot sector"))?;

        // Read $MFT record 0 to learn where the rest of the $MFT lives.
        let mut rec0 = read_at(file, info.mft_byte_offset(), info.record_size)?;
        apply_fixups(&mut rec0);
        let runs = data_runs_of(&rec0);

        let mut mft = Vec::new();
        if runs.is_empty() {
            // Resident/degenerate $MFT (shouldn't happen on a real volume);
            // fall back to just record 0's buffer so the caller still gets the
            // records it can.
            mft.extend_from_slice(&rec0);
            return Ok((mft, info.record_size));
        }
        for run in runs {
            let offset = run.lcn * info.cluster_size;
            let len = (run.cluster_count * info.cluster_size) as usize;
            let chunk = read_at(file, offset, len)?;
            mft.extend_from_slice(&chunk);
        }
        Ok((mft, info.record_size))
    }

    /// Resolves `target`'s directory subtree within the fully reconstructed
    /// volume tree by walking its path components down from the volume root.
    fn extract_subtree(volume_root: Entry, target: &Path) -> Entry {
        // Components after the drive letter, skipping the root `\`.
        let comps: Vec<String> = target
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let mut node = volume_root;
        for comp in comps {
            let Some(pos) = node
                .children
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&comp))
            else {
                // Target not found in the reconstructed tree: hand back an empty
                // node at the requested path rather than the whole volume.
                return Entry {
                    name: comp,
                    path: target.to_path_buf(),
                    size: 0,
                    is_dir: true,
                    children: Vec::new(),
                };
            };
            node = node.children.swap_remove(pos);
        }
        node
    }

    pub fn scan(target: &Path, ctx: &ScanContext) -> Result<Entry, ScanError> {
        // Guard the capability contract: only run on elevated + NTFS.
        match resolve_availability(volume_is_ntfs(target), process_is_elevated()) {
            Availability::Available => {}
            other => {
                ctx.progress.mark_complete();
                return Err(ScanError::Unavailable(other));
            }
        }
        let Some(drive) = drive_of(target) else {
            ctx.progress.mark_complete();
            return Err(ScanError::Unavailable(Availability::UnsupportedFilesystem));
        };

        let read_result = (|| {
            let mut file = open_volume(&drive)?;
            read_mft(&mut file)
        })();
        let (mft, record_size) = match read_result {
            Ok(v) => v,
            Err(err) => {
                ctx.progress.mark_complete();
                return Err(ScanError::RootUnreadable(err));
            }
        };

        if ctx.cancel.load(Ordering::Relaxed) {
            ctx.progress.mark_complete();
            return Ok(empty_root(target));
        }

        // Parse every fixed-size record in parallel. Records are independent
        // once sliced, so this is embarrassingly parallel; the sequential read
        // above is the unavoidable floor.
        let count = mft.len() / record_size;
        let records: Vec<ParsedRecord> = (0..count)
            .into_par_iter()
            .filter_map(|i| {
                if ctx.cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let start = i * record_size;
                let mut buf = mft[start..start + record_size].to_vec();
                if !apply_fixups(&mut buf) {
                    return None;
                }
                parse_record(i as u64, &buf)
            })
            .collect();

        if ctx.cancel.load(Ordering::Relaxed) {
            ctx.progress.mark_complete();
            return Ok(empty_root(target));
        }

        // Reconstruct the whole volume from the root directory, then carve out
        // the requested subtree so the returned tree matches the walker's
        // (which is rooted at the scan target, not the volume).
        let volume_root = reconstruct(
            &records,
            ROOT_RECORD,
            format!("{drive}\\"),
            PathBuf::from(format!("{drive}\\")),
        );
        let mut subtree = extract_subtree(volume_root, target);
        subtree.sort_children_recursive();

        tally(&subtree, ctx);
        ctx.progress.mark_complete();
        Ok(subtree)
    }

    /// An empty directory node for `target`, used for a cancelled scan's
    /// partial result.
    fn empty_root(target: &Path) -> Entry {
        let name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.to_string_lossy().into_owned());
        Entry {
            name,
            path: target.to_path_buf(),
            size: 0,
            is_dir: true,
            children: Vec::new(),
        }
    }

    /// Populates the progress counters from the finished tree in one walk. This
    /// engine can't stream discovery, so counters step from 0 to their final
    /// values once — still monotonic, as the progress contract requires.
    fn tally(entry: &Entry, ctx: &ScanContext) {
        for child in &entry.children {
            if child.is_dir {
                ctx.progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
                tally(child, ctx);
            } else {
                ctx.progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                ctx.progress
                    .bytes_scanned
                    .fetch_add(child.size, Ordering::Relaxed);
            }
        }
    }
}

/// Non-Windows stub layer: the turbo engine can't run here, so it reports
/// unavailable and refuses to scan, keeping the whole crate compilable and the
/// UI drivable (the toggle stays greyed out) on the Linux dev machine.
#[cfg(not(windows))]
mod platform {
    use super::*;
    use crate::scanner::{Entry, ScanContext, ScanError};
    use std::path::Path;

    pub fn process_is_elevated() -> bool {
        false
    }

    pub fn volume_is_ntfs(_target: &Path) -> bool {
        false
    }

    pub fn relaunch_elevated(_scan_root: &Path) -> std::io::Result<bool> {
        Ok(false)
    }

    pub fn scan(_target: &Path, ctx: &ScanContext) -> Result<Entry, ScanError> {
        ctx.progress.mark_complete();
        Err(ScanError::Unavailable(Availability::UnsupportedFilesystem))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Synthetic FILE-record builder ------------------------------------
    //
    // These helpers hand-assemble the minimal on-disk layout the parser reads,
    // so the pure parsing/reconstruction logic can be exercised with no real
    // volume or elevation — the module's whole testability premise.

    const REC_SIZE: usize = 1024;
    const SECTOR: usize = 512;

    /// A resident attribute: 0x18-byte header + value, padded to 8 bytes.
    fn resident_attr(attr_type: u32, name_len: u8, value: &[u8]) -> Vec<u8> {
        let header_len = 0x18usize;
        let content_off = header_len;
        let raw = header_len + value.len();
        let padded = (raw + 7) & !7;
        let mut a = vec![0u8; padded];
        a[attr::TYPE..attr::TYPE + 4].copy_from_slice(&attr_type.to_le_bytes());
        a[attr::LENGTH..attr::LENGTH + 4].copy_from_slice(&(padded as u32).to_le_bytes());
        a[attr::NON_RESIDENT] = 0;
        a[attr::NAME_LENGTH] = name_len;
        a[attr::RES_VALUE_LENGTH..attr::RES_VALUE_LENGTH + 4]
            .copy_from_slice(&(value.len() as u32).to_le_bytes());
        a[attr::RES_VALUE_OFFSET..attr::RES_VALUE_OFFSET + 2]
            .copy_from_slice(&(content_off as u16).to_le_bytes());
        a[content_off..content_off + value.len()].copy_from_slice(value);
        a
    }

    /// A non-resident $DATA attribute carrying only a real-size field (enough
    /// for the size parser; no real data runs).
    fn nonresident_data_attr(name_len: u8, real_size: u64) -> Vec<u8> {
        let len = 0x48usize;
        let mut a = vec![0u8; len];
        a[attr::TYPE..attr::TYPE + 4].copy_from_slice(&ATTR_DATA.to_le_bytes());
        a[attr::LENGTH..attr::LENGTH + 4].copy_from_slice(&(len as u32).to_le_bytes());
        a[attr::NON_RESIDENT] = 1;
        a[attr::NAME_LENGTH] = name_len;
        a[attr::NONRES_REAL_SIZE..attr::NONRES_REAL_SIZE + 8]
            .copy_from_slice(&real_size.to_le_bytes());
        a
    }

    fn std_info_attr(file_attributes: u32) -> Vec<u8> {
        let mut content = vec![0u8; 0x48];
        content[stdinfo::FILE_ATTRIBUTES..stdinfo::FILE_ATTRIBUTES + 4]
            .copy_from_slice(&file_attributes.to_le_bytes());
        resident_attr(ATTR_STANDARD_INFORMATION, 0, &content)
    }

    fn file_name_attr(parent: u64, name: &str, namespace: u8, file_attributes: u32) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut content = vec![0u8; fname::NAME + units.len() * 2];
        content[fname::PARENT_REF..fname::PARENT_REF + 8].copy_from_slice(&parent.to_le_bytes());
        content[fname::FLAGS..fname::FLAGS + 4].copy_from_slice(&file_attributes.to_le_bytes());
        content[fname::NAME_LENGTH] = units.len() as u8;
        content[fname::NAMESPACE] = namespace;
        for (i, u) in units.iter().enumerate() {
            content[fname::NAME + i * 2..fname::NAME + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        resident_attr(ATTR_FILE_NAME, 0, &content)
    }

    /// Assembles a full 1024-byte FILE record from attribute blobs, writing a
    /// valid USA (fixups) so [`apply_fixups`] accepts it.
    fn build_record(flags: u16, hard_links: u16, base_ref: u64, attrs: &[Vec<u8>]) -> Vec<u8> {
        let mut buf = vec![0u8; REC_SIZE];
        buf[rec::SIGNATURE..rec::SIGNATURE + 4].copy_from_slice(b"FILE");
        let usa_off = 0x30usize;
        let sectors = REC_SIZE / SECTOR;
        let usa_count = (sectors + 1) as u16;
        buf[rec::USA_OFFSET..rec::USA_OFFSET + 2].copy_from_slice(&(usa_off as u16).to_le_bytes());
        buf[rec::USA_COUNT..rec::USA_COUNT + 2].copy_from_slice(&usa_count.to_le_bytes());
        buf[rec::HARD_LINK_COUNT..rec::HARD_LINK_COUNT + 2].copy_from_slice(&hard_links.to_le_bytes());
        let first_attr = 0x38usize;
        buf[rec::FIRST_ATTR_OFFSET..rec::FIRST_ATTR_OFFSET + 2]
            .copy_from_slice(&(first_attr as u16).to_le_bytes());
        buf[rec::FLAGS..rec::FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        buf[rec::BASE_RECORD_REF..rec::BASE_RECORD_REF + 8].copy_from_slice(&base_ref.to_le_bytes());

        // Lay attributes down, then the end marker.
        let mut off = first_attr;
        for a in attrs {
            buf[off..off + a.len()].copy_from_slice(a);
            off += a.len();
        }
        buf[off..off + 4].copy_from_slice(&ATTR_END.to_le_bytes());

        // Write the USN into the USA and into the tail of each sector, then
        // stash the "real" tail bytes as fixup entries so a round-trip through
        // apply_fixups restores them.
        let usn: u16 = 0xBEEF;
        buf[usa_off..usa_off + 2].copy_from_slice(&usn.to_le_bytes());
        for i in 0..sectors {
            let tail = (i + 1) * SECTOR;
            let real = [0xAAu8, 0xBB];
            let fixup_entry = usa_off + 2 * (i + 1);
            buf[fixup_entry..fixup_entry + 2].copy_from_slice(&real);
            buf[tail - 2..tail].copy_from_slice(&usn.to_le_bytes());
        }
        buf
    }

    fn dir_record(hard_links: u16, parent: u64, name: &str) -> Vec<u8> {
        build_record(
            FLAG_IN_USE | FLAG_DIRECTORY,
            hard_links,
            0,
            &[std_info_attr(0), file_name_attr(parent, name, 1, 0)],
        )
    }

    fn file_record(hard_links: u16, parent: u64, name: &str, size: u64) -> Vec<u8> {
        build_record(
            FLAG_IN_USE,
            hard_links,
            0,
            &[
                std_info_attr(0),
                file_name_attr(parent, name, 1, 0),
                nonresident_data_attr(0, size),
            ],
        )
    }

    #[test]
    fn parses_a_file_records_name_parent_and_size() {
        let raw = file_record(1, 5, "hello.txt", 4096);
        let mut buf = raw.clone();
        assert!(apply_fixups(&mut buf));
        let rec = parse_record(20, &buf).expect("valid FILE record");
        assert!(rec.in_use);
        assert!(!rec.is_dir);
        assert!(rec.is_base);
        assert_eq!(rec.data_size, 4096);
        let name = rec.best_name().unwrap();
        assert_eq!(name.name, "hello.txt");
        assert_eq!(name.parent, 5);
    }

    #[test]
    fn resident_data_size_is_read_from_the_value_length() {
        // Task 2.2: a small file whose $DATA is resident in the record.
        let raw = build_record(
            FLAG_IN_USE,
            1,
            0,
            &[
                std_info_attr(0),
                file_name_attr(5, "tiny.txt", 1, 0),
                resident_attr(ATTR_DATA, 0, &[0u8; 42]),
            ],
        );
        let mut buf = raw.clone();
        assert!(apply_fixups(&mut buf));
        let rec = parse_record(21, &buf).unwrap();
        assert_eq!(rec.data_size, 42);
    }

    #[test]
    fn named_data_stream_does_not_count_as_file_size() {
        // An alternate data stream (name_len > 0) must not inflate the size.
        let raw = build_record(
            FLAG_IN_USE,
            1,
            0,
            &[
                file_name_attr(5, "f.bin", 1, 0),
                nonresident_data_attr(0, 1000), // unnamed
                nonresident_data_attr(4, 9_999_999), // ADS
            ],
        );
        let mut buf = raw.clone();
        assert!(apply_fixups(&mut buf));
        let rec = parse_record(22, &buf).unwrap();
        assert_eq!(rec.data_size, 1000);
    }

    #[test]
    fn best_name_prefers_win32_over_dos_short_name() {
        // Task 2.3 support: a hard-linked/short-named file must not surface its
        // 8.3 DOS alias as the display name.
        let raw = build_record(
            FLAG_IN_USE,
            1,
            0,
            &[
                file_name_attr(5, "PROGRA~1", NAMESPACE_DOS, 0),
                file_name_attr(5, "Program Files", 1, 0),
            ],
        );
        let mut buf = raw.clone();
        assert!(apply_fixups(&mut buf));
        let rec = parse_record(23, &buf).unwrap();
        assert_eq!(rec.best_name().unwrap().name, "Program Files");
    }

    #[test]
    fn detects_reparse_point_flag() {
        // Task 2.4: reparse detection via file-attribute flags.
        let raw = build_record(
            FLAG_IN_USE | FLAG_DIRECTORY,
            1,
            0,
            &[
                std_info_attr(FILE_ATTR_REPARSE_POINT),
                file_name_attr(5, "junction", 1, FILE_ATTR_REPARSE_POINT),
            ],
        );
        let mut buf = raw.clone();
        assert!(apply_fixups(&mut buf));
        let rec = parse_record(24, &buf).unwrap();
        assert!(rec.is_reparse);
    }

    #[test]
    fn non_file_signature_is_rejected() {
        let mut buf = vec![0u8; REC_SIZE];
        buf[0..4].copy_from_slice(b"BAAD");
        assert!(parse_record(0, &buf).is_none());
    }

    #[test]
    fn fixups_restore_sector_tails_and_reject_mismatch() {
        let raw = file_record(1, 5, "x", 1);
        let mut good = raw.clone();
        assert!(apply_fixups(&mut good));
        // The last two bytes of sector 0 were the USN in the raw buffer; after
        // fixups they must be the stashed "real" bytes.
        assert_eq!(&good[SECTOR - 2..SECTOR], &[0xAA, 0xBB]);

        // Corrupt a sector tail so it no longer matches the USN → rejected.
        let mut bad = raw;
        bad[SECTOR - 1] = 0x00;
        assert!(!apply_fixups(&mut bad));
    }

    #[test]
    fn boot_sector_parses_geometry() {
        let mut boot = vec![0u8; SECTOR];
        boot[3..11].copy_from_slice(b"NTFS    ");
        boot[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes()); // bytes/sector
        boot[0x0D] = 8; // sectors/cluster -> 4096 cluster
        boot[0x30..0x38].copy_from_slice(&786_432u64.to_le_bytes()); // MFT LCN
        boot[0x40] = 246u8 as i8 as u8; // -10 => 2^10 = 1024-byte records
        let info = parse_boot_sector(&boot).expect("valid NTFS boot sector");
        assert_eq!(info.cluster_size, 4096);
        assert_eq!(info.mft_lcn, 786_432);
        assert_eq!(info.record_size, 1024);
        assert_eq!(info.mft_byte_offset(), 786_432 * 4096);
    }

    #[test]
    fn non_ntfs_boot_sector_is_rejected() {
        let mut boot = vec![0u8; SECTOR];
        boot[3..11].copy_from_slice(b"MSDOS5.0");
        assert!(parse_boot_sector(&boot).is_none());
    }

    #[test]
    fn data_runs_decode_length_and_signed_offset() {
        // Two runs: 0x21 0x18 0x00 0x01 (len=0x18 clusters @ +0x0100),
        // then    0x11 0x08 0x02       (len=0x08 clusters @ +0x02 relative).
        let bytes = [0x21, 0x18, 0x00, 0x01, 0x11, 0x08, 0x02, 0x00];
        let runs = parse_data_runs(&bytes);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], DataRun { lcn: 0x0100, cluster_count: 0x18 });
        assert_eq!(runs[1], DataRun { lcn: 0x0102, cluster_count: 0x08 });
    }

    #[test]
    fn data_runs_handle_negative_offset_delta() {
        // 0x11 0x10 0x0A  then  0x11 0x10 0xF6 (0xF6 = -10) -> lcn 10 then 0.
        let bytes = [0x11, 0x10, 0x0A, 0x11, 0x10, 0xF6, 0x00];
        let runs = parse_data_runs(&bytes);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].lcn, 10);
        assert_eq!(runs[1].lcn, 0);
    }

    // -- Reconstruction ---------------------------------------------------

    fn parsed(raw: Vec<u8>, number: u64) -> ParsedRecord {
        let mut buf = raw;
        apply_fixups(&mut buf);
        parse_record(number, &buf).unwrap()
    }

    /// Root(5) -> Docs(16) -> a.txt(17, 100), report.pdf(18, 400); plus
    /// b.txt(19, 50) directly under root.
    fn sample_volume() -> Vec<ParsedRecord> {
        vec![
            parsed(dir_record(1, 5, "."), ROOT_RECORD), // root's own record 5
            parsed(dir_record(1, 5, "Docs"), 16),
            parsed(file_record(1, 16, "a.txt", 100), 17),
            parsed(file_record(1, 16, "report.pdf", 400), 18),
            parsed(file_record(1, 5, "b.txt", 50), 19),
        ]
    }

    #[test]
    fn reconstruct_computes_recursive_sizes() {
        let recs = sample_volume();
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        // Root = Docs(500) + b.txt(50) = 550.
        assert_eq!(tree.size, 550);
        let docs = tree.children.iter().find(|c| c.name == "Docs").unwrap();
        assert_eq!(docs.size, 500);
        assert_eq!(docs.children.len(), 2);
    }

    #[test]
    fn reconstruct_matches_a_walker_style_rollup() {
        // Task 3.6 invariant: each directory's size equals the sum of its
        // descendants, at every level.
        let recs = sample_volume();
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        fn check(e: &Entry) -> u64 {
            if e.is_dir {
                let sum: u64 = e.children.iter().map(check).sum();
                assert_eq!(e.size, sum + 0, "dir {} size == sum of children", e.name);
                e.size
            } else {
                e.size
            }
        }
        check(&tree);
    }

    #[test]
    fn reconstruct_then_sort_orders_children_largest_first() {
        // Task 3.6: after the same largest-first sort the walker applies, every
        // level is ordered largest-to-smallest.
        let recs = sample_volume();
        let mut tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        tree.sort_children_recursive();
        // Root: Docs(500) before b.txt(50).
        assert_eq!(tree.children[0].name, "Docs");
        assert_eq!(tree.children[1].name, "b.txt");
        // Docs: report.pdf(400) before a.txt(100).
        let docs = &tree.children[0];
        assert_eq!(docs.children[0].name, "report.pdf");
        assert_eq!(docs.children[1].name, "a.txt");
    }

    #[test]
    fn deleted_records_are_excluded() {
        let mut recs = sample_volume();
        // Mark b.txt (record 19) deleted-but-unreclaimed.
        recs.iter_mut().find(|r| r.record_number == 19).unwrap().in_use = false;
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        assert!(tree.children.iter().all(|c| c.name != "b.txt"));
        assert_eq!(tree.size, 500); // only Docs now
    }

    #[test]
    fn reparse_point_directory_is_not_traversed() {
        let mut recs = sample_volume();
        // Turn Docs into a reparse point pointing back at root — its children
        // must be dropped and its subtree not counted / not recursed.
        let mut recs2 = recs.clone();
        let docs = recs2.iter_mut().find(|r| r.record_number == 16).unwrap();
        docs.is_reparse = true;
        let tree = reconstruct(&recs2, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        let docs_node = tree.children.iter().find(|c| c.name == "Docs").unwrap();
        assert!(docs_node.children.is_empty());
        assert_eq!(docs_node.size, 0);
        // Root now = Docs(0) + b.txt(50).
        assert_eq!(tree.size, 50);
        let _ = &mut recs; // silence unused mut on the original
    }

    #[test]
    fn hard_link_size_counted_once() {
        // A file with two names (hard link) in two directories should still be
        // counted once. Our model places it under one parent (its best_name),
        // contributing its single data_size once to the tree total.
        let mut recs = sample_volume();
        // Give a.txt (17) a second name under root as well; hard_link_count=2.
        let a = recs.iter_mut().find(|r| r.record_number == 17).unwrap();
        a.hard_link_count = 2;
        a.names.push(FileNameEntry { parent: 5, name: "a-link.txt".into(), namespace: 1 });
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        // Total unchanged (a.txt still 100, counted once): Docs(500)+b(50)=550.
        assert_eq!(tree.size, 550);
    }

    #[test]
    fn reserved_metadata_records_are_not_emitted() {
        let mut recs = sample_volume();
        // A metadata record ($MFT-like) at number 0 parented to root must never
        // appear as a child.
        recs.push(parsed(file_record(1, 5, "$MFT", 999_999), 0));
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        assert!(tree.children.iter().all(|c| c.name != "$MFT"));
        assert_eq!(tree.size, 550);
    }

    #[test]
    fn parent_cycle_does_not_infinite_loop() {
        // Two directories that claim each other as parent (corrupt MFT). The
        // visited guard must terminate reconstruction.
        let recs = vec![
            parsed(dir_record(1, 5, "root"), ROOT_RECORD),
            parsed(dir_record(1, 17, "A"), 16),
            parsed(dir_record(1, 16, "B"), 17),
        ];
        // A(16) parent B(17), B(17) parent A(16): a cycle not reachable from
        // root, so root is just empty — the point is it returns at all.
        let tree = reconstruct(&recs, ROOT_RECORD, "C:\\".into(), "C:\\".into());
        assert_eq!(tree.name, "C:\\");
    }

    // -- Availability -----------------------------------------------------

    #[test]
    fn availability_branches_three_ways() {
        assert_eq!(
            resolve_availability(false, false),
            Availability::UnsupportedFilesystem
        );
        assert_eq!(
            resolve_availability(false, true),
            Availability::UnsupportedFilesystem
        );
        assert_eq!(
            resolve_availability(true, false),
            Availability::RequiresElevation
        );
        assert_eq!(resolve_availability(true, true), Availability::Available);
    }

    /// Emulates the `CommandLineToArgvW` unquoting the relaunched process's
    /// `std::env::args` performs, so we can assert `quote_windows_arg` round-
    /// trips. Only handles the double-quoted single-argument case we produce.
    fn unquote_windows_arg(s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars.first(), Some(&'"'), "must start quoted");
        let mut out = String::new();
        let mut backslashes = 0usize;
        for &c in &chars[1..] {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    // n backslashes then a quote: floor(n/2) literal
                    // backslashes, then an odd count escapes a literal quote
                    // while an even count marks the closing delimiter.
                    for _ in 0..(backslashes / 2) {
                        out.push('\\');
                    }
                    if backslashes % 2 == 1 {
                        out.push('"');
                        backslashes = 0;
                    } else {
                        break; // closing quote
                    }
                }
                _ => {
                    for _ in 0..backslashes {
                        out.push('\\');
                    }
                    backslashes = 0;
                    out.push(c);
                }
            }
        }
        out
    }

    #[test]
    fn quote_windows_arg_round_trips_drive_roots_and_paths() {
        for original in [
            r"D:\",
            r"C:\",
            r"C:\Users\me\Documents",
            r"C:\path with spaces\",
            r#"C:\weird"name\"#,
            r"D:\trailing\\",
            "plain",
        ] {
            let quoted = quote_windows_arg(original);
            assert_eq!(
                unquote_windows_arg(&quoted),
                original,
                "quoting of {original:?} produced {quoted:?}, which did not round-trip"
            );
        }
        // The specific bug this guards: `D:\` must not degrade to `D:"`.
        assert_eq!(quote_windows_arg(r"D:\"), r#""D:\\""#);
    }
}
