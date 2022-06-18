fn main() {
    use memvec::{MemVec, VecFile};

    #[derive(Copy, Clone)]
    #[repr(C, packed)]
    struct Record {
        time: std::time::Instant,
        event_id: u32,
        _payload: [u8; 50], // we will not use it
    }

    let mut path = std::env::temp_dir();
    path.push("vecfile.memvec");

    let vec_file = VecFile::open_or_create(&path, |_| Ok(())).expect("file open failed");
    let mut vec =
        unsafe { MemVec::<Record, _>::try_from_memory(vec_file) }.expect("vec file is corrupted");

    if vec.len() == 0 {
        // creating a new file
        println!("creating a new file: {path:?}");
        for i in 0..10 {
            vec.push(Record {
                time: std::time::Instant::now(),
                event_id: '1' as u32 + (i % 3),
                _payload: unsafe { std::mem::zeroed() },
            })
        }
    } else {
        // found an existing file
        for (i, item) in vec.iter().enumerate() {
            let time = item.time;
            let event_id = item.event_id;
            println!("idx: {i} time: {time:?} event_id: {event_id}",);
        }
        vec.clear();
        println!("deleted existing file: {path:?}");
    }
}
