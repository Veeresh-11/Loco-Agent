//! macOS workspace-root anchored snapshot acquisition using descriptor-relative opens.
use std::fs::File;
use std::io::Read;
use std::path::{Component,Path,PathBuf};
use rustix::fs::{openat,Mode,OFlags};
use crate::snapshot_payload::{build_snapshot_contract,CapturedWorkspaceSnapshot,SnapshotCaptureRequest,SnapshotPayloadError,WorkspaceSnapshotAdapter,MAX_SNAPSHOT_BYTES};

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum MacOsSnapshotAdapterError { InvalidRoot, InvalidWorkspaceId }

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

pub struct MacOsWorkspaceSnapshotAdapter { root:File, root_device:u64, workspace_id:String, canonical_root:PathBuf }
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
  let mut canonical=PathBuf::from("/");
  let mut seen=false;
  for component in path.components(){
   match component {
    Component::RootDir=>{},
    Component::Normal(name)=>{
     let name=name.to_str().ok_or(MacOsSnapshotAdapterError::InvalidRoot)?;
     if name.is_empty()||name=="."||name==".."{return Err(MacOsSnapshotAdapterError::InvalidRoot)}
     current=open_dir(&current,name)?; canonical.push(name); seen=true;
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
  Ok(Self{root:current,root_device:meta.dev(),workspace_id,canonical_root:canonical})
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
   let fd=openat(&parent,*part,OFlags::RDONLY|OFlags::DIRECTORY|OFlags::CLOEXEC|OFlags::NOFOLLOW,Mode::empty())
    .map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
   let child=File::from(fd); let m=child.metadata().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
   if !m.is_dir()||m.dev()!=self.root_device{return Err(SnapshotPayloadError::AcquisitionFailed)}
   parent=child;
  }
  let fd=openat(&parent,parts[parts.len()-1],OFlags::RDONLY|OFlags::CLOEXEC|OFlags::NOFOLLOW|OFlags::NONBLOCK,Mode::empty())
   .map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  let mut file=File::from(fd);
  let before=file.metadata().map_err(|_|SnapshotPayloadError::AcquisitionFailed)?;
  if !before.is_file(){return Err(SnapshotPayloadError::InvalidFileType)}
  if before.dev()!=self.root_device{return Err(SnapshotPayloadError::AcquisitionFailed)}
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
 #[test]
 fn macos_workspace_root_is_on_apfs(){
  let d=tempfile::tempdir().unwrap();
  let root=File::open(d.path()).unwrap();
  assert_eq!(filesystem_name(&root).unwrap(),"apfs");
  assert!(MacOsWorkspaceSnapshotAdapter::new(
   std::fs::canonicalize(d.path()).unwrap(),"workspace"
  ).is_ok());
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
    Err(SnapshotPayloadError::AcquisitionFailed)=>{},
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
