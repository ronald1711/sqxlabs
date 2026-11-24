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

#[test]
fn listing_shared_inputs_revalidates_each_call_and_rejects_missing_or_invalid_blobs(){
    let temp=Temp::new();let archive=Archive::new(&temp.0);
    let dataset=json!({"bars":(0..5000).map(|i|json!([i,1.1,1.101,1.099,1.1,10])).collect::<Vec<_>>()});
    let mut expected=Vec::new();
    for trial in 0..128{expected.push(archive.save("engine-test".into(),dataset.clone(),json!({"name":"shared control"}),json!({"trial":trial}),json!({"net":trial})).unwrap());}
    expected.sort();
    let started=std::time::Instant::now();assert_eq!(archive.list().unwrap(),expected);
    eprintln!("128-record shared-input inspection: {:?}",started.elapsed());
    let record=archive.load(&expected[0]).unwrap();
    for (kind,id) in [("datasets",&record.run.dataset_id),("strategies",&record.run.strategy_id)]{
        let path=temp.0.join(kind).join(format!("{id}.json"));let original=std::fs::read(&path).unwrap();
        std::fs::write(&path,b"{}").unwrap();assert!(archive.list().unwrap_err().contains("integrity"));
        std::fs::write(&path,&original).unwrap();assert_eq!(archive.list().unwrap(),expected);
        std::fs::remove_file(&path).unwrap();assert!(archive.list().is_err());
        std::fs::write(&path,&original).unwrap();assert_eq!(archive.list().unwrap(),expected);
    }
    // A matching content hash alone must not make invalid JSON acceptable.
    let invalid=b"not JSON";let invalid_id=content_id(invalid);
    std::fs::write(temp.0.join("datasets").join(format!("{invalid_id}.json")),invalid).unwrap();
    let mut bad=record.run;bad.dataset_id=invalid_id;
    let bytes=serde_json::to_vec(&bad).unwrap();
    std::fs::write(temp.0.join("runs").join(format!("{}.json",content_id(&bytes))),bytes).unwrap();
    assert!(archive.list().is_err());
}

#[test]
fn identical_dataset_and_strategy_hashes_still_require_both_objects(){
    let temp=Temp::new();let archive=Archive::new(&temp.0);
    let id=archive.save("engine-test".into(),json!({"same":true}),json!({"same":true}),json!({}),json!({})).unwrap();
    let record=archive.load(&id).unwrap();assert_eq!(record.run.dataset_id,record.run.strategy_id);
    assert_eq!(archive.list().unwrap(),vec![id]);
    std::fs::remove_file(temp.0.join("strategies").join(format!("{}.json",record.run.strategy_id))).unwrap();
    assert!(archive.list().is_err(),"dataset validation must not hide a missing strategy with the same hash");
}
