use std::fs;
use std::io::{Read, Seek};
use std::time::Instant;

fn main() -> Result<(), failure::Error> {
    let size = 1024 * 1024 * 1;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .read(true)
        .open("/tmp/a")?;

    file.set_len(size)?;

    let start = Instant::now();

    let mut buf = [0u8; 32];

    for i in 0..(size / 32) {
        file.seek(std::io::SeekFrom::Start(i * 32))?;
        file.read(&mut buf[..]).unwrap();
    }

    println!("buf: {:?}", &buf);
    println!("took {:0.4}ms", start.elapsed().as_millis());
    Ok(())
}
