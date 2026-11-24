//! macOS workspace-root anchored snapshot acquisition using descriptor-relative opens.
use std::fs::File;
use std::io::Read;
use std::path::{Component,Path,PathBuf};
use rustix::fs::{openat,Mode,OFlags};
use crate::snapshot_payload::{build_snapshot_contract,CapturedWorkspaceSnapshot,SnapshotCaptureRequest,SnapshotPayloadError,WorkspaceSnapshotAdapter,MAX_SNAPSHOT_BYTES};

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum MacOsSnapshotAdapterError { InvalidRoot, InvalidWorkspaceId }

#[cfg(target_os="macos")]
const F_GETPATH_NOFIRMLINK: libc::c_int = 102;

/// Returns the kernel's descriptor path with firmlink redirection disabled.
/// This is a path observation, not a capability or an atomic ancestry proof.
#[cfg(target_os="macos")]
fn descriptor_path_no_firmlink(file: &File) -> Result<PathBuf, MacOsSnapshotAdapterError> {
 use std::ffi::OsString;
 use std::os::fd::AsRawFd;
 use std::os::unix::ffi::OsStringExt;

 let mut buffer = vec![0u8; libc::PATH_MAX as usize];
 if buffer.is_empty() {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let result = unsafe {
  libc::fcntl(file.as_raw_fd(), F_GETPATH_NOFIRMLINK, buffer.as_mut_ptr())
 };
 if result != 0 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let end = buffer.iter().position(|byte| *byte == 0)
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if end == 0 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let path = PathBuf::from(OsString::from_vec(buffer[..end].to_vec()));
 if !path.is_absolute() {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 Ok(path)
}

#[cfg(target_os="macos")]
const FSOPT_RESOLVE_BENEATH: libc::c_ulong = 0x1000;

#[cfg(all(target_os="macos", test))]
fn parse_nofirmlink_path_record(buffer: &[u8]) -> Result<PathBuf, MacOsSnapshotAdapterError> {
 use std::ffi::OsString;
 use std::os::unix::ffi::OsStringExt;

 if buffer.len() < 4 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let record_len = u32::from_ne_bytes(buffer[..4].try_into().unwrap()) as usize;
 let minimum_len = 4 + std::mem::size_of::<libc::attribute_set_t>() +
                   std::mem::size_of::<libc::attrreference_t>();
 if record_len > buffer.len() || record_len < minimum_len {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let returned = unsafe {
  std::ptr::read_unaligned(buffer.as_ptr().add(4).cast::<libc::attribute_set_t>())
 };
 if returned.forkattr & libc::ATTR_CMNEXT_NOFIRMLINKPATH == 0 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }

 let reference_offset = 4 + std::mem::size_of::<libc::attribute_set_t>();
 let reference = unsafe {
  std::ptr::read_unaligned(
   buffer.as_ptr().add(reference_offset).cast::<libc::attrreference_t>()
  )
 };
 let path_start = reference_offset as isize + reference.attr_dataoffset as isize;
 let path_len = reference.attr_length as usize;
 let minimum_path_start = reference_offset + std::mem::size_of::<libc::attrreference_t>();
 let path_start = usize::try_from(path_start)
  .map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?;
 let path_end = path_start.checked_add(path_len)
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if path_start < minimum_path_start || path_len < 2 ||
    path_end > record_len || path_len > libc::PATH_MAX as usize + 64 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let path_bytes = &buffer[path_start..path_end];
 if path_bytes.last() != Some(&0) || path_bytes[..path_bytes.len() - 1].contains(&0) {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let path = PathBuf::from(OsString::from_vec(path_bytes[..path_bytes.len() - 1].to_vec()));
 if !path.is_absolute() || path.components().any(|part|
  matches!(part, std::path::Component::CurDir | std::path::Component::ParentDir)
 ) {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 Ok(path)
}

#[cfg(target_os="macos")]
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
#[cfg(target_os="macos")]
const MAX_ENTRY_NAME_BYTES: usize = 3 * 255 + 1;

#[cfg(target_os="macos")]
fn attr_reference_range(
 record_len: usize,
 reference_offset: usize,
 reference: libc::attrreference_t,
 minimum_payload_len: usize,
 maximum_payload_len: usize,
) -> Result<std::ops::Range<usize>, MacOsSnapshotAdapterError> {
 let reference_end = reference_offset
  .checked_add(std::mem::size_of::<libc::attrreference_t>())
  .filter(|end| *end <= record_len)
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 let payload_start = (reference_offset as isize)
  .checked_add(reference.attr_dataoffset as isize)
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 let payload_start = usize::try_from(payload_start)
  .map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?;
 let payload_len = reference.attr_length as usize;
 let payload_end = payload_start.checked_add(payload_len)
  .filter(|end| *end <= record_len)
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if payload_start < reference_end
    || payload_len < minimum_payload_len
    || payload_len > maximum_payload_len {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 Ok(payload_start..payload_end)
}

#[cfg(target_os="macos")]
const MAX_BULK_RECORD_BYTES: usize = 64 * 1024;

#[cfg(target_os="macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct BulkEntryRecord {
 name: Vec<u8>,
 flags: u32,
 no_firmlink_path: PathBuf,
}

#[cfg(target_os="macos")]
fn parse_bulk_entry_record(record: &[u8]) -> Result<BulkEntryRecord, MacOsSnapshotAdapterError> {
 use std::ffi::OsString;
 use std::os::unix::ffi::OsStringExt;

 let header_size = 4usize
  .checked_add(std::mem::size_of::<libc::attribute_set_t>())
  .ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if record.len() < header_size || record.len() > MAX_BULK_RECORD_BYTES {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let record_len = u32::from_ne_bytes(
  record[..4].try_into().map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?
 ) as usize;
 if record_len != record.len() {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let returned = unsafe {
  std::ptr::read_unaligned(record.as_ptr().add(4).cast::<libc::attribute_set_t>())
 };
 if returned.commonattr & libc::ATTR_CMN_NAME == 0
  || returned.commonattr & libc::ATTR_CMN_FLAGS == 0
  || returned.forkattr & libc::ATTR_CMNEXT_NOFIRMLINKPATH == 0
 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }

 let mut offset = header_size;
 if returned.commonattr & ATTR_CMN_ERROR != 0 {
  let error_end = offset.checked_add(4).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
  if error_end > record_len {
   return Err(MacOsSnapshotAdapterError::InvalidRoot);
  }
  let entry_error = u32::from_ne_bytes(
   record[offset..error_end].try_into().map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?
  );
  if entry_error != 0 {
   return Err(MacOsSnapshotAdapterError::InvalidRoot);
  }
  offset = error_end;
 }

 let reference_size = std::mem::size_of::<libc::attrreference_t>();
 let name_ref_end = offset.checked_add(reference_size).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if name_ref_end > record_len {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let name_ref = unsafe {
  std::ptr::read_unaligned(record.as_ptr().add(offset).cast::<libc::attrreference_t>())
 };
 let name_range = attr_reference_range(record_len, offset, name_ref, 2, 256)?;
 let name_bytes = &record[name_range];
 if name_bytes.last() != Some(&0) || name_bytes[..name_bytes.len() - 1].contains(&0) {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 offset = name_ref_end;

 let flags_end = offset.checked_add(4).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if flags_end > record_len {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let flags = u32::from_ne_bytes(
  record[offset..flags_end].try_into().map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?
 );
 offset = flags_end;

 let path_ref_end = offset.checked_add(reference_size).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
 if path_ref_end > record_len {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let path_ref = unsafe {
  std::ptr::read_unaligned(record.as_ptr().add(offset).cast::<libc::attrreference_t>())
 };
 let path_range = attr_reference_range(
  record_len, offset, path_ref, 2, libc::PATH_MAX as usize
 )?;
 let path_bytes = &record[path_range];
 if path_bytes.last() != Some(&0) || path_bytes[..path_bytes.len() - 1].contains(&0) {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let no_firmlink_path = PathBuf::from(OsString::from_vec(path_bytes[..path_bytes.len() - 1].to_vec()));
 if !no_firmlink_path.is_absolute()
  || no_firmlink_path.components().any(|part|
   matches!(part, std::path::Component::CurDir | std::path::Component::ParentDir)
  )
 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }

 Ok(BulkEntryRecord {
  name: name_bytes[..name_bytes.len() - 1].to_vec(),
  flags,
  no_firmlink_path,
 })
}

#[cfg(target_os="macos")]
fn bulk_entry_path_no_firmlink(parent: &File, name: &std::ffi::OsStr) -> Result<PathBuf, MacOsSnapshotAdapterError> {
 use std::os::fd::AsRawFd;
 use std::os::unix::ffi::OsStrExt;

 let wanted = name.as_bytes();
 if wanted.is_empty() || wanted.contains(&0) || wanted.contains(&b'/') {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }

 // Each scan needs a new open file description. try_clone/dup would share
 // getattrlistbulk's directory offset across concurrent captures.
 let scan_fd = openat(
  parent, ".",
  OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
  Mode::empty()
 ).map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?;
 let scan_dir = File::from(scan_fd);

 let mut attrs: libc::attrlist = unsafe { std::mem::zeroed() };
 attrs.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
 attrs.commonattr = libc::ATTR_CMN_RETURNED_ATTRS
  | libc::ATTR_CMN_NAME
  | libc::ATTR_CMN_FLAGS
  | ATTR_CMN_ERROR;
 attrs.forkattr = libc::ATTR_CMNEXT_NOFIRMLINKPATH;

 let mut buffer = vec![0u8; MAX_BULK_RECORD_BYTES];
 let mut scanned = 0usize;
 const MAX_DIRECTORY_ENTRIES: usize = 16 * 1024;

 loop {
  let count = unsafe {
   libc::getattrlistbulk(
    scan_dir.as_raw_fd(),
    (&mut attrs as *mut libc::attrlist).cast(),
    buffer.as_mut_ptr().cast(),
    buffer.len(),
    libc::FSOPT_ATTR_CMN_EXTENDED as u64,
   )
  };
  if count <= 0 {
   return Err(MacOsSnapshotAdapterError::InvalidRoot);
  }

  let mut cursor = 0usize;
  for _ in 0..count as usize {
   scanned = scanned.checked_add(1).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
   if scanned > MAX_DIRECTORY_ENTRIES || cursor > buffer.len() || buffer.len() - cursor < 4 {
    return Err(MacOsSnapshotAdapterError::InvalidRoot);
   }
   let record_len = u32::from_ne_bytes(
    buffer[cursor..cursor + 4].try_into().map_err(|_| MacOsSnapshotAdapterError::InvalidRoot)?
   ) as usize;
   let record_end = cursor.checked_add(record_len).ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
   if record_len < 4 + std::mem::size_of::<libc::attribute_set_t>()
    || record_end > buffer.len()
   {
    return Err(MacOsSnapshotAdapterError::InvalidRoot);
   }
   let parsed = parse_bulk_entry_record(&buffer[cursor..record_end])?;
   if parsed.name == wanted {
    // Keep flags for diagnostics only; never equate SF_NOUNLINK with SF_FIRMLINK.
    let _observed_entry_flags = parsed.flags;
    return Ok(parsed.no_firmlink_path);
   }
   cursor = record_end;
  }
 }
}

#[cfg(target_os="macos")]
fn preflight_entry_path(parent: &File, name: &str) -> Result<PathBuf, SnapshotPayloadError> {
 use std::ffi::OsStr;

 let parent_path = descriptor_path_no_firmlink(parent)
  .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
 let entry_path = bulk_entry_path_no_firmlink(parent, OsStr::new(name))
  .map_err(|_| SnapshotPayloadError::AcquisitionFailed)?;
 let expected_path = parent_path.join(name);
 if entry_path != expected_path {
  return Err(SnapshotPayloadError::FirmlinkBoundary);
 }
 Ok(entry_path)
}

#[cfg(target_os="macos")]
pub fn attr_cmnex_nofirmlinkpath_available() -> bool {
 use std::ffi::OsStr;
 File::open("/")
  .ok()
  .and_then(|directory| bulk_entry_path_no_firmlink(&directory, OsStr::new("System")).ok())
  .is_some_and(|path| path == Path::new("/System"))
}

#[cfg(target_os="macos")]
fn path_is_within_root(root: &Path, candidate: &Path) -> bool {
 candidate.starts_with(root)
}

/// Capability probe used by the sidecar handshake. The actual adapter repeats
/// the query for each retained descriptor and fails closed on any error.
#[cfg(target_os="macos")]
pub fn fcntl_getpath_nofirmlink_available() -> bool {
 File::open(".")
  .ok()
  .and_then(|file| descriptor_path_no_firmlink(&file).ok())
  .is_some()
}

#[cfg(target_os="macos")]
// Keep fsid_t opaque: compare the OS-provided volume identifier without decoding its private layout.
fn filesystem_id(file:&File)->Result<[u8;8],MacOsSnapshotAdapterError>{
 use std::os::fd::AsRawFd;
 let mut stat:libc::statfs=unsafe{std::mem::zeroed()};
 if unsafe{libc::fstatfs(file.as_raw_fd(),&mut stat)}!=0 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 // Darwin fsid_t is defined by the SDK as two 32-bit integers. Copy its
 // fixed-width C representation without depending on libc private fields.
 let mut bytes=[0u8;8];
 unsafe { std::ptr::copy_nonoverlapping(
  (&stat.f_fsid as *const libc::fsid_t).cast::<u8>(), bytes.as_mut_ptr(), bytes.len()
 ); }
 Ok(bytes)
}

#[cfg(target_os="macos")]
fn filesystem_name(file:&File)->Result<String,MacOsSnapshotAdapterError>{
 use std::ffi::CStr;
 use std::os::fd::AsRawFd;
 let mut stat:libc::statfs=unsafe{std::mem::zeroed()};
 if unsafe{libc::fstatfs(file.as_raw_fd(),&mut stat)}!=0 {
  return Err(MacOsSnapshotAdapterError::InvalidRoot);
 }
 let name=unsafe{CStr::from_ptr(stat.f_fstypename.as_ptr())};
 name.to_str().map(str::to_owned).map_err(|_|MacOsSnapshotAdapterError::InvalidRoot)
}

#[cfg(target_os="macos")]
fn metadata_matches(a:&std::fs::Metadata,b:&std::fs::Metadata)->bool {
 use std::os::unix::fs::MetadataExt;
 a.dev()==b.dev() && a.ino()==b.ino() && a.len()==b.len() && a.nlink()==b.nlink()
 && a.mtime()==b.mtime() && a.mtime_nsec()==b.mtime_nsec()
 && a.ctime()==b.ctime() && a.ctime_nsec()==b.ctime_nsec()
}

fn open_dir(parent:&File,name:&str)->Result<File,MacOsSnapshotAdapterError>{
 let fd=openat(parent,name,OFlags::RDONLY|OFlags::DIRECTORY|OFlags::CLOEXEC|OFlags::NOFOLLOW,Mode::empty())
  .map_err(|_|MacOsSnapshotAdapterError::InvalidRoot)?;
 let file=File::from(fd);
 if !file.metadata().map_err(|_|MacOsSnapshotAdapterError::InvalidRoot)?.is_dir(){return Err(MacOsSnapshotAdapterError::InvalidRoot)}
 Ok(file)
}

pub struct MacOsWorkspaceSnapshotAdapter { root:File, root_device:u64, root_filesystem_id:[u8;8], workspace_id:String, canonical_root:PathBuf }
impl MacOsWorkspaceSnapshotAdapter {
 /// Opens an absolute workspace path from /, one component at a time.
 /// O_NOFOLLOW rejects symlinks; the retained final descriptor is the capability.
 pub fn new(root_path:impl AsRef<Path>,workspace_id:impl Into<String>)->Result<Self,MacOsSnapshotAdapterError>{
  use std::os::unix::fs::MetadataExt;
  let path=root_path.as_ref();
  if !path.is_absolute(){return Err(MacOsSnapshotAdapterError::InvalidRoot)}
  let workspace_id=workspace_id.into();
  if workspace_id.trim().is_empty(){return Err(MacOsSnapshotAdapterError::InvalidWorkspaceId)}
  let mut current=File::open("/").map_err(|_|MacOsSnapshotAdapterError::InvalidRoot)?;
  let mut seen=false;
  for component in path.components(){
   match component {
    Component::RootDir=>{},
    Component::Normal(name)=>{
     let name=name.to_str().ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
     if name.is_empty()||name=="."||name==".."{return Err(MacOsSnapshotAdapterError::InvalidRoot)}
     current=open_dir(&current,name)?; seen=true;
    },
    _=>return Err(MacOsSnapshotAdapterError::InvalidRoot),
   }
  }
  if !seen{return Err(MacOsSnapshotAdapterError::InvalidRoot)}
  // Admit only APFS roots until other macOS filesystems have dedicated
  // native adversarial coverage. Query the retained descriptor, not a path.
  if filesystem_name(&current)? != "apfs" {
   return Err(MacOsSnapshotAdapterError::InvalidRoot);
  }
  let meta=current.metadata().map_err(|_|MacOsSnapshotAdapterError::InvalidRoot)?;
  let root_filesystem_id=filesystem_id(&current)?;
  if root_filesystem_id.iter().all(|byte| *byte == 0) {
   return Err(MacOsSnapshotAdapterError::InvalidRoot);
  }
  let canonical_root=descriptor_path_no_firmlink(&current)?;
  Ok(Self{root:current,root_device:meta.dev(),root_filesystem_id,workspace_id,canonical_root})
 }
 pub fn root_path(&self)->&Path{&self.canonical_root}
}

impl WorkspaceSnapshotAdapter for MacOsWorkspaceSnapshotAdapter {
 fn capture(&self,request:&SnapshotCaptureRequest)->Result<CapturedWorkspaceSnapshot,SnapshotPayloadError>{
  use std::os::unix::fs::MetadataExt;
  let parts:Vec<&str>=request.relative_path().split('/').collect();
  if parts.is_empty()||parts.iter().any(|p|p.is_empty()||*p=="."||*p==".."){return Err(SnapshotPayloadError::AcquisitionFailed)}
  let mut parent=self.root.try_clone().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  for part in &parts[..parts.len()-1]{
   let expected_child_path=preflight_entry_path(&parent,part)?;
   let fd=openat(&parent,*part,OFlags::RDONLY|OFlags::DIRECTORY|OFlags::CLOEXEC|OFlags::NOFOLLOW,Mode::empty())
    .map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
   let child=File::from(fd);
   let m=child.metadata().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
   let child_path=descriptor_path_no_firmlink(&child).map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
   if !m.is_dir()||m.dev()!=self.root_device
    || filesystem_id(&child).map_err(|_|SnapshotPayloadError::AcquisitionFailed)? != self.root_filesystem_id
    || child_path!=expected_child_path
    || !path_is_within_root(&self.canonical_root,&child_path)
   {return Err(SnapshotPayloadError::AcquisitionFailed)}
   parent=child;
  }
  let file_name=parts[parts.len()-1];
  let expected_file_path=preflight_entry_path(&parent,file_name)?;
  let fd=openat(&parent,file_name,OFlags::RDONLY|OFlags::CLOEXEC|OFlags::NOFOLLOW|OFlags::NONBLOCK,Mode::empty())
   .map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  let mut file=File::from(fd);
  let before=file.metadata().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  let file_path=descriptor_path_no_firmlink(&file).map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  if file_path!=expected_file_path || !path_is_within_root(&self.canonical_root,&file_path){
   return Err(SnapshotPayloadError::AcquisitionFailed);
  }
  if !before.is_file(){return Err(SnapshotPayloadError::InvalidFileType)}
  if before.dev()!=self.root_device
   || filesystem_id(&file).map_err(|_|SnapshotPayloadError::AcquisitionFailed)? != self.root_filesystem_id
  {return Err(SnapshotPayloadError::AcquisitionFailed)}
  if before.nlink()!=1{return Err(SnapshotPayloadError::HardLinkNotAllowed)}
  let limit=MAX_SNAPSHOT_BYTES.min(request.max_bytes());
  if before.len()>limit as u64{return Err(SnapshotPayloadError::LimitExceeded)}
  let mut bytes=Vec::with_capacity(before.len() as usize);
  (&mut file).take(limit as u64+1).read_to_end(&mut bytes).map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  if bytes.len()>limit{return Err(SnapshotPayloadError::LimitExceeded)}
  if bytes.len() as u64!=before.len(){return Err(SnapshotPayloadError::AcquisitionFailed)}
  let after=file.metadata().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  if !metadata_matches(&before,&after){return Err(SnapshotPayloadError::AcquisitionFailed)}
  let identity=format!("dev:{}:ino:{}:captured_len:{}",before.dev(),before.ino(),bytes.len());
  let contract=build_snapshot_contract(&self.workspace_id,request.relative_path(),&identity,&bytes)?;
  CapturedWorkspaceSnapshot::new(contract,bytes)
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::snapshot_payload::SnapshotCaptureRequest;
 use std::os::unix::fs::symlink;

 fn synthetic_nofirmlink_record(path: &[u8]) -> Vec<u8> {
  let returned_offset = 4;
  let reference_offset = returned_offset + std::mem::size_of::<libc::attribute_set_t>();
  let path_offset = reference_offset + std::mem::size_of::<libc::attrreference_t>();
  let record_len = path_offset + path.len();
  let mut buffer = vec![0u8; record_len];
  buffer[..4].copy_from_slice(&(record_len as u32).to_ne_bytes());
  let mut returned: libc::attribute_set_t = unsafe { std::mem::zeroed() };
  returned.forkattr = libc::ATTR_CMNEXT_NOFIRMLINKPATH;
  unsafe {
   std::ptr::write_unaligned(
    buffer.as_mut_ptr().add(returned_offset).cast::<libc::attribute_set_t>(),
    returned,
   );
   std::ptr::write_unaligned(
    buffer.as_mut_ptr().add(reference_offset).cast::<libc::attrreference_t>(),
    libc::attrreference_t {
     attr_dataoffset: std::mem::size_of::<libc::attrreference_t>() as i32,
     attr_length: path.len() as u32,
    },
   );
  }
  buffer[path_offset..].copy_from_slice(path);
  buffer
 }
 fn synthetic_bulk_entry_record(
  name: &[u8],
  path: &[u8],
  flags: u32,
  entry_error: u32,
  include_entry_error: bool,
 ) -> Vec<u8> {
  let header_size = 4 + std::mem::size_of::<libc::attribute_set_t>();
  let error_size = if include_entry_error { 4 } else { 0 };
  let name_ref_offset = header_size + error_size;
  let name_ref_end = name_ref_offset + std::mem::size_of::<libc::attrreference_t>();
  let flags_offset = name_ref_end;
  let path_ref_offset = flags_offset + 4;
  let path_ref_end = path_ref_offset + std::mem::size_of::<libc::attrreference_t>();
  let name_offset = path_ref_end;
  let path_offset = name_offset + name.len();
  let record_len = path_offset + path.len();
  let mut record = vec![0u8; record_len];
  record[..4].copy_from_slice(&(record_len as u32).to_ne_bytes());

  let mut returned: libc::attribute_set_t = unsafe { std::mem::zeroed() };
  returned.commonattr = libc::ATTR_CMN_NAME | libc::ATTR_CMN_FLAGS
   | if include_entry_error { ATTR_CMN_ERROR } else { 0 };
  returned.forkattr = libc::ATTR_CMNEXT_NOFIRMLINKPATH;
  unsafe {
   std::ptr::write_unaligned(
    record.as_mut_ptr().add(4).cast::<libc::attribute_set_t>(),
    returned,
   );
   std::ptr::write_unaligned(
    record.as_mut_ptr().add(name_ref_offset).cast::<libc::attrreference_t>(),
    libc::attrreference_t {
     attr_dataoffset: (name_offset - name_ref_offset) as i32,
     attr_length: name.len() as u32,
    },
   );
   std::ptr::write_unaligned(
    record.as_mut_ptr().add(path_ref_offset).cast::<libc::attrreference_t>(),
    libc::attrreference_t {
     attr_dataoffset: (path_offset - path_ref_offset) as i32,
     attr_length: path.len() as u32,
    },
   );
  }
  if include_entry_error {
   record[header_size..header_size + 4].copy_from_slice(&entry_error.to_ne_bytes());
  }
  record[flags_offset..flags_offset + 4].copy_from_slice(&flags.to_ne_bytes());
  record[name_offset..name_offset + name.len()].copy_from_slice(name);
  record[path_offset..path_offset + path.len()].copy_from_slice(path);
  record
 }

 fn set_bulk_returned_attributes(record: &mut [u8], commonattr: u32, forkattr: u32) {
  let mut returned: libc::attribute_set_t = unsafe { std::mem::zeroed() };
  returned.commonattr = commonattr;
  returned.forkattr = forkattr;
  unsafe {
   std::ptr::write_unaligned(
    record.as_mut_ptr().add(4).cast::<libc::attribute_set_t>(),
    returned,
   );
  }
 }

 #[test]
 fn bulk_entry_record_parser_accepts_valid_entry_and_preserves_raw_flags() {
  let record = synthetic_bulk_entry_record(
   b"Users\0", b"/System/Volumes/Data/Users\0", 0x0010_0000, 0, true,
  );
  let parsed = parse_bulk_entry_record(&record).unwrap();
  assert_eq!(parsed.name, b"Users");
  assert_eq!(parsed.flags, 0x0010_0000);
  assert_eq!(parsed.no_firmlink_path, PathBuf::from("/System/Volumes/Data/Users"));
 }

 #[test]
 fn bulk_entry_record_parser_accepts_record_without_optional_entry_error() {
  let record = synthetic_bulk_entry_record(
   b"System\0", b"/System\0", 0x0008_0000, 0, false,
  );
  let parsed = parse_bulk_entry_record(&record).unwrap();
  assert_eq!(parsed.name, b"System");
  assert_eq!(parsed.no_firmlink_path, PathBuf::from("/System"));
 }

 #[test]
 fn bulk_entry_record_parser_rejects_truncated_or_oversized_records() {
  let valid = synthetic_bulk_entry_record(b"System\0", b"/System\0", 0, 0, true);
  assert!(parse_bulk_entry_record(&valid[..3]).is_err());

  let mut truncated = valid.clone();
  truncated.truncate(4 + std::mem::size_of::<libc::attribute_set_t>() - 1);
  let len = truncated.len() as u32;
  truncated[..4].copy_from_slice(&len.to_ne_bytes());
  assert!(parse_bulk_entry_record(&truncated).is_err());

  let mut oversized = valid.clone();
  oversized[..4].copy_from_slice(&((MAX_BULK_RECORD_BYTES as u32) + 1).to_ne_bytes());
  assert!(parse_bulk_entry_record(&oversized).is_err());
 }

 #[test]
 fn bulk_entry_record_parser_rejects_malformed_name_and_path_references() {
  let valid = synthetic_bulk_entry_record(b"System\0", b"/System\0", 0, 0);
  let header_size = 4 + std::mem::size_of::<libc::attribute_set_t>();
  let name_ref_offset = header_size + 4;
  let mut malformed_name = valid.clone();
  let mut name_ref = unsafe {
   std::ptr::read_unaligned(
    malformed_name.as_ptr().add(name_ref_offset).cast::<libc::attrreference_t>()
   )
  };
  name_ref.attr_dataoffset = -1;
  unsafe {
   std::ptr::write_unaligned(
    malformed_name.as_mut_ptr().add(name_ref_offset).cast::<libc::attrreference_t>(),
    name_ref,
   );
  }
  assert!(parse_bulk_entry_record(&malformed_name).is_err());

  let path_ref_offset = name_ref_offset + std::mem::size_of::<libc::attrreference_t>() + 4;
  let mut malformed_path = valid;
  let mut path_ref = unsafe {
   std::ptr::read_unaligned(
    malformed_path.as_ptr().add(path_ref_offset).cast::<libc::attrreference_t>()
   )
  };
  path_ref.attr_dataoffset = 0;
  unsafe {
   std::ptr::write_unaligned(
    malformed_path.as_mut_ptr().add(path_ref_offset).cast::<libc::attrreference_t>(),
    path_ref,
   );
  }
  assert!(parse_bulk_entry_record(&malformed_path).is_err());
 }

 #[test]
 fn bulk_entry_record_parser_requires_terminators_absolute_paths_and_zero_entry_error() {
  let mut bad_name = synthetic_bulk_entry_record(b"System\0", b"/System\0", 0, 0);
  let header_size = 4 + std::mem::size_of::<libc::attribute_set_t>();
  let name_ref_offset = header_size + 4;
  let name_ref = unsafe {
   std::ptr::read_unaligned(
    bad_name.as_ptr().add(name_ref_offset).cast::<libc::attrreference_t>()
   )
  };
  let name_start = (name_ref_offset as isize + name_ref.attr_dataoffset as isize) as usize;
  bad_name[name_start + name_ref.attr_length as usize - 1] = b'X';
  assert!(parse_bulk_entry_record(&bad_name).is_err());

  let relative = synthetic_bulk_entry_record(b"System\0", b"System\0", 0, 0, true);
  assert!(parse_bulk_entry_record(&relative).is_err());

  let nonzero_error = synthetic_bulk_entry_record(b"System\0", b"/System\0", 0, 5, true);
  assert!(parse_bulk_entry_record(&nonzero_error).is_err());
 }

 #[test]
 fn bulk_entry_record_parser_rejects_missing_requested_attributes() {
  let valid = synthetic_bulk_entry_record(b"System\0", b"/System\0", 0, 0);
  let common = libc::ATTR_CMN_NAME | libc::ATTR_CMN_FLAGS | ATTR_CMN_ERROR;
  for (commonattr, forkattr) in [
   (libc::ATTR_CMN_FLAGS | ATTR_CMN_ERROR, libc::ATTR_CMNEXT_NOFIRMLINKPATH),
   (libc::ATTR_CMN_NAME | ATTR_CMN_ERROR, libc::ATTR_CMNEXT_NOFIRMLINKPATH),
   (common, 0),
  ] {
   let mut missing = valid.clone();
   set_bulk_returned_attributes(&mut missing, commonattr, forkattr);
   assert!(parse_bulk_entry_record(&missing).is_err());
  }
 }

 #[test]
 fn macos_workspace_root_is_on_apfs(){
  let d=tempfile::tempdir().unwrap();
  let root=File::open(d.path()).unwrap();
  assert_eq!(filesystem_name(&root).unwrap(),"apfs");
  assert!(MacOsWorkspaceSnapshotAdapter::new(
   std::fs::canonicalize(d.path()).unwrap(),"workspace"
  ).is_ok());
 }

 #[test]
 fn descriptor_path_resolves_users_firmlink_without_following_display_path() {
  let users=File::open("/Users").unwrap();
  let resolved=descriptor_path_no_firmlink(&users).unwrap();
  assert!(resolved.is_absolute());
  assert_ne!(resolved,PathBuf::from("/Users"));
 }

 #[test]
 fn captures_workspace_root_through_users_firmlink() {
  let home=PathBuf::from(std::env::var("HOME").expect("macOS CI home directory"));
  assert!(home.starts_with("/Users/"));
  let d=tempfile::tempdir_in(&home).unwrap();
  std::fs::write(d.path().join("inside.txt"),b"inside-users-firmlink").unwrap();
  let adapter=MacOsWorkspaceSnapshotAdapter::new(d.path(),"users-workspace").unwrap();
  assert!(adapter.root_path().starts_with("/System/Volumes/Data/Users"));
  let captured=adapter.capture(&SnapshotCaptureRequest::new("inside.txt",64).unwrap()).unwrap();
  assert_eq!(captured.bytes(),b"inside-users-firmlink");
 }

 #[test]
 fn extended_nofirmlink_attribute_distinguishes_firmlink_entries_before_open() {
  let root=File::open("/").unwrap();
  assert_eq!(
   preflight_entry_path(&root,"Users"),
   Err(SnapshotPayloadError::FirmlinkBoundary)
  );
  assert_eq!(
   preflight_entry_path(&root,"Applications"),
   Err(SnapshotPayloadError::FirmlinkBoundary)
  );
  assert_eq!(
   preflight_entry_path(&root,"Library"),
   Err(SnapshotPayloadError::FirmlinkBoundary)
  );
  assert_eq!(
   preflight_entry_path(&root,"private"),
   Err(SnapshotPayloadError::FirmlinkBoundary)
  );
  assert_eq!(
   preflight_entry_path(&root,"System").unwrap(),
   PathBuf::from("/System")
  );
  assert_eq!(
   preflight_entry_path(&root,"usr").unwrap(),
   PathBuf::from("/usr")
  );
 }

 #[test]
 fn bulk_attrreference_ranges_reject_overlaps_and_out_of_bounds_payloads() {
  let valid = libc::attrreference_t { attr_dataoffset: 8, attr_length: 4 };
  assert_eq!(
   attr_reference_range(64, 24, valid, 2, 32).unwrap(),
   32..36
  );

  for malformed in [
   libc::attrreference_t { attr_dataoffset: -8, attr_length: 4 },
   libc::attrreference_t { attr_dataoffset: 0, attr_length: 4 },
   libc::attrreference_t { attr_dataoffset: i32::MAX, attr_length: 4 },
   libc::attrreference_t { attr_dataoffset: 8, attr_length: 0 },
   libc::attrreference_t { attr_dataoffset: 8, attr_length: 33 },
  ] {
   assert!(attr_reference_range(64, 24, malformed, 1, 32).is_err());
  }

  assert!(attr_reference_range(
   32, 28, libc::attrreference_t { attr_dataoffset: 8, attr_length: 4 }, 1, 32
  ).is_err());
 }

 #[test]
 fn nofirmlink_path_record_parser_fails_closed_on_malformed_records() {
  let valid=synthetic_nofirmlink_record(b"/System/Volumes/Data/Users\0");
  assert_eq!(
   parse_nofirmlink_path_record(&valid).unwrap(),
   PathBuf::from("/System/Volumes/Data/Users")
  );

  let mut truncated=valid.clone();
  truncated.truncate(truncated.len()-1);
  assert!(parse_nofirmlink_path_record(&truncated).is_err());

  let mut missing_attr=valid.clone();
  let mut returned=unsafe {
   std::ptr::read_unaligned(
    missing_attr.as_ptr().add(4).cast::<libc::attribute_set_t>()
   )
  };
  returned.forkattr=0;
  unsafe {
   std::ptr::write_unaligned(
    missing_attr.as_mut_ptr().add(4).cast::<libc::attribute_set_t>(),
    returned,
   );
  }
  assert!(parse_nofirmlink_path_record(&missing_attr).is_err());

  let relative=synthetic_nofirmlink_record(b"relative/path\0");
  assert!(parse_nofirmlink_path_record(&relative).is_err());

  let unterminated=synthetic_nofirmlink_record(b"/tmp");
  assert!(parse_nofirmlink_path_record(&unterminated).is_err());
 }

 #[test]
 fn concurrent_preflight_path_queries_do_not_share_directory_offsets() {
  use std::sync::Arc;
  use std::thread;

  let root=Arc::new(File::open("/").unwrap());
  let workers=(0..6).map(|_| {
   let root=Arc::clone(&root);
   thread::spawn(move || {
    for _ in 0..20 {
     assert_eq!(
      preflight_entry_path(&root,"System").unwrap(),
      PathBuf::from("/System")
     );
     assert_eq!(
      preflight_entry_path(&root,"Users"),
      Err(SnapshotPayloadError::FirmlinkBoundary)
     );
    }
   })
  }).collect::<Vec<_>>();
  for worker in workers {
   worker.join().unwrap();
  }
 }

 #[test]
 fn bulk_preflight_preserves_symlink_alias_and_openat_rejects_it() {
  let d=tempfile::tempdir().unwrap();
  std::fs::create_dir(d.path().join("target")).unwrap();
  symlink("target",d.path().join("alias")).unwrap();
  let root=File::open(d.path()).unwrap();
  let expected=descriptor_path_no_firmlink(&root).unwrap().join("alias");
  assert_eq!(preflight_entry_path(&root,"alias").unwrap(),expected);
  assert!(open_dir(&root,"alias").is_err());
 }

 #[test]
 fn concurrent_bulk_preflights_use_independent_directory_offsets() {
  let d=tempfile::tempdir().unwrap();
  for index in 0..12 {
   std::fs::write(d.path().join(format!("entry-{index:02}")),b"x").unwrap();
  }
  let root=File::open(d.path()).unwrap();
  std::thread::scope(|scope| {
   let mut workers=Vec::new();
   for worker in 0..4 {
    let root=&root;
    workers.push(scope.spawn(move || {
     for index in 0..12 {
      let name=format!("entry-{:02}",(index+worker)%12);
      let expected=descriptor_path_no_firmlink(root).unwrap().join(&name);
      assert_eq!(preflight_entry_path(root,&name).unwrap(),expected);
     }
    }));
   }
   for worker in workers { worker.join().unwrap(); }
  });
 }

 #[test]
 fn root_and_descendant_use_nofirmlink_descriptor_paths() {
  let d=tempfile::tempdir().unwrap();
  std::fs::create_dir(d.path().join("nested")).unwrap();
  std::fs::write(d.path().join("nested/file.txt"),b"contained").unwrap();
  let root_file=File::open(d.path()).unwrap();
  let expected_root=descriptor_path_no_firmlink(&root_file).unwrap();
  let adapter=MacOsWorkspaceSnapshotAdapter::new(
   std::fs::canonicalize(d.path()).unwrap(),"workspace"
  ).unwrap();
  assert_eq!(adapter.root_path(),expected_root);
  let captured=adapter.capture(&SnapshotCaptureRequest::new("nested/file.txt",64).unwrap()).unwrap();
  assert_eq!(captured.bytes(),b"contained");
  let file=File::open(d.path().join("nested/file.txt")).unwrap();
  let resolved_file=descriptor_path_no_firmlink(&file).unwrap();
  assert!(path_is_within_root(adapter.root_path(),&resolved_file));
 }

 #[test]
 fn nofirmlink_path_containment_uses_path_components_not_string_prefixes() {
  let root=Path::new("/System/Volumes/Data/Users/runner/work");
  assert!(path_is_within_root(root,Path::new("/System/Volumes/Data/Users/runner/work/src/main.rs")));
  assert!(!path_is_within_root(root,Path::new("/System/Volumes/Data/Users/runner/work-other/secret")));
  assert!(!path_is_within_root(root,Path::new("/System/Volumes/Data/Applications/secret")));
 }

 #[test]
 #[ignore = "requires a real nested APFS volume mounted by the macOS CI workflow"]
 fn rejects_nested_apfs_volume_mount(){
  use std::os::unix::fs::MetadataExt;
  let workspace=PathBuf::from(std::env::var("LOCO_MACOS_MOUNT_WORKSPACE")
   .expect("macOS mount fixture workflow must set LOCO_MACOS_MOUNT_WORKSPACE"));
  let mounted=workspace.join("mounted");
  let root=File::open(&workspace).unwrap();
  let volume=File::open(&mounted).unwrap();
  assert_eq!(filesystem_name(&root).unwrap(),"apfs");
  assert_eq!(filesystem_name(&volume).unwrap(),"apfs");
  assert_ne!(filesystem_id(&root).unwrap(),filesystem_id(&volume).unwrap(),
   "fixture must expose a distinct APFS volume identity through retained handles");
  assert_ne!(
   root.metadata().unwrap().dev(),
   volume.metadata().unwrap().dev(),
   "fixture must also preserve the existing distinct st_dev boundary"
  );
  std::fs::write(mounted.join("probe.txt"),b"mounted-volume-sentinel").unwrap();
  let adapter=MacOsWorkspaceSnapshotAdapter::new(
   std::fs::canonicalize(&workspace).unwrap(),"workspace"
  ).unwrap();
  assert_eq!(
   adapter.capture(&SnapshotCaptureRequest::new("mounted/probe.txt",128).unwrap()),
   Err(SnapshotPayloadError::AcquisitionFailed),
   "capture must reject traversal into a separately mounted APFS volume"
  );
 }

 // On current macOS, the hosted System and Data APFS volumes share st_dev,
 // but fstatfs on retained handles reports distinct f_fsid values. Firmlinks
 // bridge those volumes, so reject a descendant whose handle-derived volume
 // identity differs from the retained workspace root. This is a volume
 // boundary check, not a claim to identify every firmlink independently.
 #[test]
 fn rejects_system_to_data_firmlink_by_handle_volume_identity() {
  use std::os::unix::fs::MetadataExt;

  let home=PathBuf::from(std::env::var("HOME").expect("macOS CI home directory"));
  assert!(home.starts_with("/Users/"),"test requires hosted macOS user home on Data volume");
  let data_dir=File::open("/Users").unwrap();
  let system_root=File::open("/").unwrap();
  assert_eq!(filesystem_name(&system_root).unwrap(),"apfs");
  assert_eq!(filesystem_name(&data_dir).unwrap(),"apfs");
  assert_eq!(system_root.metadata().unwrap().dev(),data_dir.metadata().unwrap().dev(),
   "fixture should preserve the known same-st_dev condition");
  assert_ne!(filesystem_id(&system_root).unwrap(),filesystem_id(&data_dir).unwrap(),
   "fixture must expose distinct System and Data volume identities");

  let probe=tempfile::tempdir_in(&home).unwrap();
  std::fs::write(probe.path().join("sentinel.txt"),b"data-volume-sentinel").unwrap();
  let relative=probe.path().join("sentinel.txt").strip_prefix("/").unwrap()
   .to_str().unwrap().to_owned();

  // Construct a root-anchored adapter only inside this test. Production root
  // admission still rejects /; this exercises the actual /Users mapping.
  let root_meta=system_root.metadata().unwrap();
  let root_filesystem_id=filesystem_id(&system_root).unwrap();
  let adapter=MacOsWorkspaceSnapshotAdapter {
   root:system_root,
   root_device:root_meta.dev(),
   root_filesystem_id,
   workspace_id:"system-root-test".into(),
   canonical_root:PathBuf::from("/"),
  };
  assert_eq!(adapter.capture(&SnapshotCaptureRequest::new(&relative,128).unwrap()),
   Err(SnapshotPayloadError::FirmlinkBoundary),
   "capture must reject the System-to-Data firmlink before opening its target");
 }

 #[test] fn captures_regular_and_empty_files(){
  let d=tempfile::tempdir().unwrap(); std::fs::write(d.path().join("a"),b"macOS capture").unwrap(); std::fs::write(d.path().join("e"),b"").unwrap();
  let a=MacOsWorkspaceSnapshotAdapter::new(std::fs::canonicalize(d.path()).unwrap(),"workspace").unwrap();
  let x=a.capture(&SnapshotCaptureRequest::new("a",64).unwrap()).unwrap(); assert_eq!(x.bytes(),b"macOS capture");
  assert_eq!(crate::snapshot_payload::validate_snapshot_payload(x.snapshot(),x.bytes()),Ok(()));
  assert!(a.capture(&SnapshotCaptureRequest::new("e",1).unwrap()).unwrap().bytes().is_empty());
 }
 #[test] fn rejects_symlinked_root_and_descendant(){
  let p=tempfile::tempdir().unwrap(); let actual=tempfile::tempdir().unwrap(); std::fs::write(actual.path().join("secret"),b"x").unwrap();
  let link=p.path().join("link"); symlink(actual.path(),&link).unwrap();
  assert!(MacOsWorkspaceSnapshotAdapter::new(&link,"w").is_err());
  let w=tempfile::tempdir().unwrap(); symlink(actual.path(),w.path().join("linked")).unwrap();
  let a=MacOsWorkspaceSnapshotAdapter::new(std::fs::canonicalize(w.path()).unwrap(),"w").unwrap();
  assert!(a.capture(&SnapshotCaptureRequest::new("linked/secret",32).unwrap()).is_err());
 }
 #[test] fn rejects_hard_links_directories_and_over_limit(){
  let w=tempfile::tempdir().unwrap(); std::fs::create_dir(w.path().join("dir")).unwrap();
  std::fs::write(w.path().join("large"),b"12345").unwrap(); std::fs::write(w.path().join("origin"),b"shared").unwrap();
  std::fs::hard_link(w.path().join("origin"),w.path().join("alias")).unwrap();
  let a=MacOsWorkspaceSnapshotAdapter::new(std::fs::canonicalize(w.path()).unwrap(),"w").unwrap();
  assert!(matches!(a.capture(&SnapshotCaptureRequest::new("dir",64).unwrap()),Err(SnapshotPayloadError::InvalidFileType)));
  assert!(matches!(a.capture(&SnapshotCaptureRequest::new("large",4).unwrap()),Err(SnapshotPayloadError::LimitExceeded)));
  assert!(matches!(a.capture(&SnapshotCaptureRequest::new("alias",64).unwrap()),Err(SnapshotPayloadError::HardLinkNotAllowed)));
 }
 #[test] fn rejects_missing_and_relative_roots(){
  let w=tempfile::tempdir().unwrap(); let a=MacOsWorkspaceSnapshotAdapter::new(std::fs::canonicalize(w.path()).unwrap(),"w").unwrap();
  assert_eq!(a.capture(&SnapshotCaptureRequest::new("missing",64).unwrap()),Err(SnapshotPayloadError::AcquisitionFailed));
  assert!(MacOsWorkspaceSnapshotAdapter::new("relative/path","w").is_err());
  assert!(MacOsWorkspaceSnapshotAdapter::new(std::fs::canonicalize(w.path()).unwrap()," ").is_err());
 }
 #[test] fn metadata_comparison_detects_mutation(){
  let d=tempfile::tempdir().unwrap(); let p=d.path().join("x"); std::fs::write(&p,b"before").unwrap();
  let before=std::fs::metadata(&p).unwrap(); std::fs::write(&p,b"after and longer").unwrap();
  assert!(!metadata_matches(&before,&std::fs::metadata(&p).unwrap()));
 }

 #[test]
 fn intermediate_directory_swap_never_escapes_retained_workspace_root() {
  use std::sync::{Arc,Barrier};
  use std::thread;

  let parent=tempfile::tempdir().unwrap();
  let workspace=parent.path().join("workspace");
  let outside=parent.path().join("outside");
  let nested=workspace.join("nested");
  std::fs::create_dir_all(&nested).unwrap();
  std::fs::create_dir(&outside).unwrap();
  std::fs::write(nested.join("target.txt"),b"inside-workspace").unwrap();
  std::fs::write(outside.join("target.txt"),b"OUTSIDE-SENTINEL").unwrap();

  let adapter=Arc::new(MacOsWorkspaceSnapshotAdapter::new(
   std::fs::canonicalize(&workspace).unwrap(),"workspace"
  ).unwrap());
  let barrier=Arc::new(Barrier::new(2));
  let writer_workspace=workspace.clone();
  let writer_outside=outside.clone();
  let writer_barrier=barrier.clone();

  let writer=thread::spawn(move||{
   writer_barrier.wait();
   for index in 0..200 {
    let held=writer_workspace.join(format!("nested-held-{index}"));
    let active=writer_workspace.join("nested");
    // Replace the intermediate pathname with an outside symlink while the
    // reader repeatedly opens it relative to the retained workspace handle.
    std::fs::rename(&active,&held).unwrap();
    symlink(&writer_outside,&active).unwrap();
    thread::yield_now();
    std::fs::remove_file(&active).unwrap();
    std::fs::rename(&held,&active).unwrap();
   }
  });

  barrier.wait();
  let request=SnapshotCaptureRequest::new("nested/target.txt",128).unwrap();
  // This checks path containment under namespace replacement only; it does
  // not establish an atomic snapshot of concurrently modified file contents.
  for _ in 0..400 {
   match adapter.capture(&request) {
    Ok(captured)=>{
     assert_eq!(
      captured.bytes(),b"inside-workspace",
      "capture escaped through the swapped intermediate directory"
     );
     assert_eq!(
      crate::snapshot_payload::validate_snapshot_payload(
       captured.snapshot(),captured.bytes()
      ),
      Ok(())
     );
    },
    Err(SnapshotPayloadError::AcquisitionFailed | SnapshotPayloadError::FirmlinkBoundary)=>{},
    Err(other)=>panic!("unexpected capture error during path swap: {other:?}"),
   }
  }
  writer.join().unwrap();

  // Once the attacker-controlled swap sequence ends, the original relative
  // target must still be readable through the retained workspace root.
  let final_capture=adapter.capture(&request).unwrap();
  assert_eq!(final_capture.bytes(),b"inside-workspace");
 }

}
