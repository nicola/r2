use std::fs;
use std::io::{Read, Seek, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn main() -> Result<(), failure::Error> {
    let (sender, receiver) = mpsc::channel();

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
        file.write(b"12345");
        let mut buf = [0u8; 32];

        for i in 0..1000 {
            file.seek(std::io::SeekFrom::Start(i * 32)).unwrap();
            file.read(&mut buf[..]).unwrap();
            sender.send(buf.clone()).unwrap();
        }
    });

    let mut res = Vec::new();
    for _i in 0..100 {
        res.push(receiver.recv().unwrap());
    }
    println!("res: {:?}", &res[0]);
    println!("took {:0.4}ms", start.elapsed().as_millis());

    handle.join().unwrap();
    Ok(())
}
