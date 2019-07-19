use crossbeam::channel;
use std::fs;
use std::io::{Read, Seek, Write};
use std::thread;
use std::time::Instant;

fn main() -> Result<(), failure::Error> {
    let (sender, receiver) = channel::bounded(128);
    let (sender_r, receiver_r) = channel::bounded(128);

    let start = Instant::now();

    let handle = thread::spawn(move || {
        let size = 1024 * 1024 * 10;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open("/tmp/a")
            .unwrap();

        file.set_len(size).unwrap();
        file.write(b"12345").unwrap();
        let mut buf = [0u8; 32];

        while let Ok(i) = receiver_r.recv() {
            if i % 2 == 0 {
                file.seek(std::io::SeekFrom::Start((i * 32) as u64))
                    .unwrap();
            } else {
                file.seek(std::io::SeekFrom::End(-((i * 32) as i64)))
                    .unwrap();
            }
            assert_eq!(file.read(&mut buf[..]).unwrap(), 32);
            sender.send(buf).unwrap();
        }
    });

    let mut res = Vec::new();
    for j in 0..10 * 1024 {
        for i in j * 10..(j + 1) * 10 {
            sender_r.send(i).unwrap();
        }

        for _i in j * 10..(j + 1) * 10 {
            res.push(receiver.recv().unwrap());
        }
    }
    println!("res: {:?}, {}", &res[0], res.len());
    drop(sender_r);
    handle.join().unwrap();

    println!("took {:0.4}ms", start.elapsed().as_millis());

    Ok(())
}
