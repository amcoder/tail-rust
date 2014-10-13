#![feature(if_let)]

extern crate getopts;

use std::os::{ args };
use std::io::{
    File, IoResult, EndOfFile,
    stderr, stdout,
    SeekSet, SeekCur, SeekEnd,
};
use std::cmp::{ min };
use getopts::{ optflag, optopt, getopts, usage, OptGroup };

static VERSION: &'static str = "0.0.1";

static BUFFER_SIZE: uint = 1024;
static DEFAULT_LINES: uint = 10;

// Represents the set of options
struct TailOptions {
    show_help: bool,
    show_version: bool,
    output_headers: bool,
    item_count: uint,
    files: Vec<String>,
}

fn main() {
    let args = args();
    let program = args[0].clone();

    let possible_options = [
        optopt("n", "lines", "output the last K lines", "K"),
        optflag("q", "quiet", "never output file name headers"),
        optflag("", "silent", "same as --quiet"),
        optflag("v", "verbose", "always output file name headers"),
        optflag("h", "help", "display this help and exit"),
        optflag("V", "version", "display version information and exit"),
    ];

    let options = match parse_options(args.tail(), possible_options) {
        Ok(o) => o,
        Err(error) => {
            (writeln!(stderr(), "{}: {}", program, error.to_string())).unwrap();
            return;
        },
    };

    if options.show_help {
        let brief = format!("Usage: {} [OPTION]... [FILE]...", program);
        println!("{}", usage(brief.as_slice(), possible_options));
        return;
    }

    if options.show_version {
        println!("tail-rust v{}", VERSION);
        return;
    }

    for file_name in options.files.iter() {

        // Output the header, but only if we are tailing more than one file
        if options.output_headers {
            println!("==> {} <==", file_name);
        }

        // Open the file and tail it
        match File::open(&Path::new(file_name.as_slice()))
                    .and_then(|f| { tail_file(f, options.item_count) }) {
            Err(error) => {
                (writeln!(stderr(), "{}: {}: {}", program, file_name, error.desc)).unwrap();
            },
            _ => continue,
        }
    }
}

// Given a set of arguments and possible options, parse the arguments and
// return the selected TailOptions
fn parse_options(args: &[String],
                 options: &[OptGroup]) -> Result<TailOptions, String> {

    let option_matches = match getopts(args, options) {
        Ok(o) => o,
        Err(error) => return Err(error.to_string()),
    };

    let options = TailOptions {
        show_help: option_matches.opt_present("help"),
        show_version: option_matches.opt_present("version"),
        output_headers: !option_matches.opt_present("quiet") &&
                            !option_matches.opt_present("silent") &&
                            (option_matches.opt_present("verbose") ||
                             option_matches.free.len() > 1),
        item_count: match option_matches.opt_str("lines") {
            Some(nstr) => {
                match from_str(nstr.as_slice()) {
                    Some(n) => n,
                    None => {
                        return Err(format!("{}: invalid number of lines",
                                           nstr));
                    },
                }
            },
            None => DEFAULT_LINES,
        },
        files: option_matches.free,
    };

    return Ok(options);
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
