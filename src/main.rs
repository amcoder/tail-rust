#![feature(if_let)]

use std::os::{ args };
use std::io::{
    File, IoResult, EndOfFile,
    stderr, stdout,
    SeekSet, SeekCur, SeekEnd,
};
use std::cmp::{ min };

static BUFFER_SIZE: uint = 1024;

fn main() {
    let args = args();
    let multiple_files = args.len() > 2;

    for arg in args[1..].iter() {

        // Output the header, but only if we are tailing more than one file
        if multiple_files {
            println!("==> {} <==", arg);
        }

        // Open the file and tail it
        match File::open(&Path::new(arg.as_slice()))
                    .and_then(|f| { tail_file(f, 10) }) {
            Err(error) => {
                (writeln!(stderr(), "tail: {}: {}", arg, error.desc)).unwrap();
            },
            _ => continue,
        }
    }
}

// Output the last 'n' lines from 'file'
fn tail_file(mut file: File, n: uint) -> IoResult<()> {

    let mut stdout = stdout();

    let status = try!(file.stat());
    let mut bytes_left = status.size;
    let mut bytes_read = 0u;
    let mut buffer = [0u8, ..BUFFER_SIZE];
    let mut newline_count = 0u;

    // Start at the end of the file
    try!(file.seek(0, SeekEnd));

    // Keep reading backwards until we have seen 'n' lines or we get to the
    // beginning of the file, whichever comes first.
    while bytes_left != 0u64 && newline_count <= n {

        let bytes_to_read = min(bytes_left, BUFFER_SIZE as u64) as uint;

        // Read the next block of the file
        try!(file.seek(-((bytes_to_read + bytes_read) as i64), SeekCur));
        bytes_read = try!(file.read(buffer.slice_mut(0, bytes_to_read)));
        bytes_left -= bytes_read as u64;

        // Count the newlines in the chunk
        for (i, c) in buffer[0..bytes_read].iter().enumerate().rev() {
            if c == &('\n' as u8) {
                newline_count += 1;
                if newline_count > n {
                    // We found all the newline, so output the remaining buffer
                    try!(stdout.write(buffer[i+1..bytes_read]));
                    break;
                }
            }
        }
    }

    // Go to the beginning of the file if we couldn't find all the newlines
    if bytes_left == 0 {
        try!(file.seek(0, SeekSet));
    }

    return copy_to_end(&mut file, &mut stdout);
}

// Read from 'in' and write to 'out'
fn copy_to_end<T: Reader, U: Writer>(reader: &mut T, writer: &mut U) -> IoResult<()> {

    let mut buffer = [0u8, ..BUFFER_SIZE];
    loop {
        let bytes_read = match reader.read(buffer) {
            Ok(n) => n,
            Err(why) => {
                if why.kind == EndOfFile {
                    break;
                }
                return Err(why)
            },
        };

        try!(writer.write(buffer[0..bytes_read]));
    }
    return Ok(());
}
