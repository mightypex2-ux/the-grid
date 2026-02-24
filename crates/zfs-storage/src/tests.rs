use tempfile::TempDir;
use zfs_core::{Cid, Head, ProgramId, SectorId};

use crate::{BlockStore, HeadStore, ProgramIndex, RocksStorage, StorageBackend, StorageConfig};

fn open_temp_db() -> (RocksStorage, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let config = StorageConfig::new(dir.path());
    let storage = RocksStorage::open(config).expect("open");
    (storage, dir)
}

#[test]
fn block_put_get_delete() {
    let (db, _dir) = open_temp_db();
    let cid = Cid::from_ciphertext(b"test block data");

    assert!(db.get(&cid).expect("get").is_none());

    db.put(&cid, b"test block data").expect("put");
    let got = db.get(&cid).expect("get").expect("some");
    assert_eq!(got, b"test block data");

    db.delete(&cid).expect("delete");
    assert!(db.get(&cid).expect("get").is_none());
}

#[test]
fn block_overwrite() {
    let (db, _dir) = open_temp_db();
    let cid = Cid::from([1u8; 32]);

    db.put(&cid, b"v1").expect("put v1");
    db.put(&cid, b"v2").expect("put v2");
    let got = db.get(&cid).expect("get").expect("some");
    assert_eq!(got, b"v2");
}

#[test]
fn head_put_get() {
    let (db, _dir) = open_temp_db();
    let sector_id = SectorId::from_bytes(b"sector-1".to_vec());

    assert!(db.get_head(&sector_id).expect("get").is_none());

    let head = Head {
        sector_id: sector_id.clone(),
        cid: Cid::from([0xAA; 32]),
        version: 1,
        program_id: ProgramId::from([0xBB; 32]),
        prev_head_cid: None,
        timestamp_ms: 1234567890,
        signature: None,
    };

    db.put_head(&sector_id, &head).expect("put_head");
    let got = db.get_head(&sector_id).expect("get_head").expect("some");
    assert_eq!(got.cid, head.cid);
    assert_eq!(got.version, 1);
    assert_eq!(got.program_id, head.program_id);
    assert_eq!(got.timestamp_ms, 1234567890);
}

#[test]
fn head_update() {
    let (db, _dir) = open_temp_db();
    let sector_id = SectorId::from_bytes(b"update-test".to_vec());

    let head_v1 = Head {
        sector_id: sector_id.clone(),
        cid: Cid::from([1u8; 32]),
        version: 1,
        program_id: ProgramId::from([0xCC; 32]),
        prev_head_cid: None,
        timestamp_ms: 100,
        signature: None,
    };
    db.put_head(&sector_id, &head_v1).expect("put v1");

    let head_v2 = Head {
        sector_id: sector_id.clone(),
        cid: Cid::from([2u8; 32]),
        version: 2,
        program_id: ProgramId::from([0xCC; 32]),
        prev_head_cid: Some(head_v1.cid),
        timestamp_ms: 200,
        signature: None,
    };
    db.put_head(&sector_id, &head_v2).expect("put v2");

    let got = db.get_head(&sector_id).expect("get").expect("some");
    assert_eq!(got.version, 2);
    assert_eq!(got.prev_head_cid, Some(Cid::from([1u8; 32])));
}

#[test]
fn program_index_add_list_remove() {
    let (db, _dir) = open_temp_db();
    let pid = ProgramId::from([0x11; 32]);

    assert!(db.list_cids(&pid).expect("list").is_empty());

    let cid1 = Cid::from([1u8; 32]);
    let cid2 = Cid::from([2u8; 32]);
    let cid3 = Cid::from([3u8; 32]);

    db.add_cid(&pid, &cid1).expect("add cid1");
    db.add_cid(&pid, &cid2).expect("add cid2");
    db.add_cid(&pid, &cid3).expect("add cid3");

    let cids = db.list_cids(&pid).expect("list");
    assert_eq!(cids.len(), 3);
    assert!(cids.contains(&cid1));
    assert!(cids.contains(&cid2));
    assert!(cids.contains(&cid3));

    db.remove_cid(&pid, &cid2).expect("remove cid2");
    let cids = db.list_cids(&pid).expect("list");
    assert_eq!(cids.len(), 2);
    assert!(!cids.contains(&cid2));
}

#[test]
fn program_index_deduplication() {
    let (db, _dir) = open_temp_db();
    let pid = ProgramId::from([0x22; 32]);
    let cid = Cid::from([0x33; 32]);

    db.add_cid(&pid, &cid).expect("add");
    db.add_cid(&pid, &cid).expect("add dup");
    let cids = db.list_cids(&pid).expect("list");
    assert_eq!(cids.len(), 1);
}

#[test]
fn program_index_multiple_programs() {
    let (db, _dir) = open_temp_db();
    let pid1 = ProgramId::from([0x44; 32]);
    let pid2 = ProgramId::from([0x55; 32]);
    let cid_a = Cid::from([0xA0; 32]);
    let cid_b = Cid::from([0xB0; 32]);

    db.add_cid(&pid1, &cid_a).expect("add");
    db.add_cid(&pid2, &cid_b).expect("add");

    let list1 = db.list_cids(&pid1).expect("list");
    let list2 = db.list_cids(&pid2).expect("list");
    assert_eq!(list1, vec![cid_a]);
    assert_eq!(list2, vec![cid_b]);
}

#[test]
fn stats_reporting() {
    let (db, _dir) = open_temp_db();

    let cid1 = Cid::from([1u8; 32]);
    let cid2 = Cid::from([2u8; 32]);
    db.put(&cid1, b"block1").expect("put");
    db.put(&cid2, b"block2").expect("put");

    let sector_id = SectorId::from_bytes(b"stats-sector".to_vec());
    let head = Head {
        sector_id: sector_id.clone(),
        cid: cid1,
        version: 1,
        program_id: ProgramId::from([0x99; 32]),
        prev_head_cid: None,
        timestamp_ms: 0,
        signature: None,
    };
    db.put_head(&sector_id, &head).expect("put_head");

    let pid = ProgramId::from([0x99; 32]);
    db.add_cid(&pid, &cid1).expect("add_cid");

    let stats = db.stats().expect("stats");
    assert_eq!(stats.block_count, 2);
    assert_eq!(stats.head_count, 1);
    assert_eq!(stats.program_count, 1);
}

#[test]
fn reopen_persists_data() {
    let dir = TempDir::new().expect("tempdir");

    let cid = Cid::from([0xFE; 32]);
    {
        let config = StorageConfig::new(dir.path());
        let db = RocksStorage::open(config).expect("open");
        db.put(&cid, b"persistent").expect("put");
    }

    {
        let config = StorageConfig::new(dir.path());
        let db = RocksStorage::open(config).expect("reopen");
        let got = db.get(&cid).expect("get").expect("some");
        assert_eq!(got, b"persistent");
    }
}
