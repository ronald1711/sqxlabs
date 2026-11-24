use sqx_project::archive::{Archive,content_id};
use serde_json::json;
static TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {fn new()->Self{Self(std::env::temp_dir().join(format!("sqx-archive-test-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),TEMP_ID.fetch_add(1,std::sync::atomic::Ordering::Relaxed))))}}
impl Drop for Temp{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn save(archive:&Archive)->String{archive.save("engine-test".into(),json!({"bars":[[1,1.2345678901234567]]}),json!({"name":"test"}),json!({"seed":42}),json!({"profit":12.5})).unwrap()}
#[test]
fn content_addressed_runs_deduplicate_and_survive_reopening(){
    let temp=Temp::new();let a=Archive::new(&temp.0);let id=save(&a);
    assert_eq!(id,save(&a));assert_eq!(Archive::new(&temp.0).list().unwrap(),vec![id.clone()]);
    let loaded=a.load(&id).unwrap();assert_eq!(loaded.dataset["bars"][0][1].as_f64().unwrap().to_bits(),1.2345678901234567f64.to_bits());
    assert_eq!(content_id(b"abc"),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}
#[test]
fn corrupted_snapshot_and_path_traversal_fail_closed(){
    let temp=Temp::new();let a=Archive::new(&temp.0);let id=save(&a);let bundle=a.load(&id).unwrap();
    std::fs::write(temp.0.join("datasets").join(format!("{}.json",bundle.run.dataset_id)),b"{}").unwrap();
    assert!(a.load(&id).unwrap_err().contains("integrity"));assert!(a.load("../../outside").is_err());
    assert!(save_result(&a).is_err(),"do not overwrite damaged objects");
}
fn save_result(a:&Archive)->Result<String,String>{a.save("engine-test".into(),json!({"bars":[[1,1.2345678901234567]]}),json!({"name":"test"}),json!({"seed":42}),json!({"profit":12.5}))}
#[test]
fn backup_restore_validates_every_component_before_publication(){
    let source=Temp::new();let destination=Temp::new();let a=Archive::new(&source.0);let b=Archive::new(&destination.0);
    let id=save(&a);let bundle=a.load(&id).unwrap();let mut damaged=bundle.clone();damaged.strategy["name"]=json!("changed");
    assert!(b.restore(&damaged).is_err());assert!(b.list().unwrap().is_empty());
    assert_eq!(b.restore(&bundle).unwrap(),id);assert_eq!(b.load(&id).unwrap().run.result,bundle.run.result);
}
#[test]
fn concurrent_writers_publish_one_complete_record(){
    let temp=Temp::new();let a=Archive::new(&temp.0);
    let threads:Vec<_>=(0..8).map(|_|{let a=a.clone();std::thread::spawn(move||save(&a))}).collect();
    let ids:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    assert!(ids.iter().all(|id|id==&ids[0]));assert_eq!(a.list().unwrap(),vec![ids[0].clone()]);
}

#[test]
fn retained_engine_is_immutable_and_not_included_in_json_backup(){
    let temp=Temp::new();let archive=Archive::new(&temp.0);let bytes=b"synthetic engine bytes";
    let id=content_id(bytes);
    assert_eq!(archive.engine_retention(&id)["status"],"missing");
    assert_eq!(archive.engine_retention("../../outside")["status"],"unverified");
    assert!(archive.retain_engine(bytes,&"0".repeat(64)).is_err());
    let workers:Vec<_>=(0..4).map(|_|{let a=archive.clone();let id=id.clone();std::thread::spawn(move||a.retain_engine(bytes,&id))}).collect();
    for worker in workers {assert_eq!(worker.join().unwrap().unwrap(),id);}
    let path=temp.0.join("engines").join(format!("{id}.bin"));
    assert_eq!(std::fs::read(&path).unwrap(),bytes);
    assert_eq!(archive.engine_retention(&id)["status"],"verified");
    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode()&0o111,0);}
    let run=archive.save(id.clone(),json!({}),json!({}),json!({}),json!({})).unwrap();
    let destination=Temp::new();let restored=Archive::new(&destination.0);
    restored.restore(&archive.load(&run).unwrap()).unwrap();
    assert_eq!(restored.engine_retention(&id)["status"],"missing");
    assert!(!destination.0.join("engines").exists(),"untrusted research backups must not transport executable bytes implicitly");
    std::fs::write(&path,b"damaged").unwrap();
    assert_eq!(archive.engine_retention(&id)["status"],"unverified");
    assert!(archive.retain_engine(bytes,&id).unwrap_err().contains("integrity"));
    assert_eq!(std::fs::read(&path).unwrap(),b"damaged");
}

#[cfg(unix)]
#[test]
fn retained_engine_rejects_symlink(){
    let temp=Temp::new();let a=Archive::new(&temp.0);let id=content_id(b"engine");
    std::fs::create_dir_all(temp.0.join("engines")).unwrap();
    let source=temp.0.join("source");std::fs::write(&source,b"engine").unwrap();
    std::os::unix::fs::symlink(&source,temp.0.join("engines").join(format!("{id}.bin"))).unwrap();
    assert!(a.retain_engine(b"engine",&id).unwrap_err().contains("regular file"));
    assert_eq!(a.engine_retention(&id)["status"],"unverified");
}
