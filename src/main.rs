#![feature(if_let)]

extern crate getopts;

use std::os::{ args };
use std::io::{
    File, IoResult, IoError, EndOfFile,
    stderr, stdout, stdin,
    SeekSet, SeekCur, SeekEnd,
    BufferedReader,
};
use std::cmp::{ min };
use getopts::{ optflag, optopt, getopts, usage, OptGroup };
use std::collections::Deque;
use std::collections::ringbuf::RingBuf;

static VERSION: &'static str = "0.0.1";

static BUFFER_SIZE: uint = 1024;
static DEFAULT_LINES: uint = 10;

// Represents the direction to start counting items
enum Direction {
    FromTop,
    FromBottom,
}

// Represents the set of options
struct TailOptions {
    show_help: bool,
    show_version: bool,
    output_headers: bool,
    item_count: uint,
    direction: Direction,
    files: Vec<String>,
}

fn main() {
    let args = args();
    let program = args[0].clone();

    let possible_options = [
        optopt("n", "lines",
               "output the last K lines, or use -n +K to output lines starting with the Kth", "K"),
        optflag("q", "quiet", "never output file name headers"),
        optflag("", "silent", "same as --quiet"),
        optflag("v", "verbose", "always output file name headers"),
        optflag("h", "help", "display this help and exit"),
        optflag("V", "version", "display version information and exit"),
    ];

    // Parse the provided options
    let options = match parse_options(args.tail(), possible_options) {
        Ok(o) => o,
        Err(error) => {
            (writeln!(stderr(), "{}: {}", program, error.to_string())).unwrap();
            return;
        },
    };

    // Show help
    if options.show_help {
        let brief = format!("Usage: {} [OPTION]... [FILE]...", program);
        println!("{}", usage(brief.as_slice(), possible_options));
        return;
    }

    // Show version
    if options.show_version {
        println!("tail-rust v{}", VERSION);
        return;
    }

    // If no files are specified, tail stdin
    let stdin_name = vec!["-".to_string()];
    let files = match options.files.len() {
        0 => &stdin_name,
        _ => &options.files,
    };

    // Tail each file
    for file_name in files.iter() {
        match tail(file_name.as_slice(), &options) {
            Err(error) => {
                (writeln!(stderr(), "{}: {}: {}", program, file_name, error.desc)).unwrap();
            },
            _ => continue,
        }
    }
}

// Given a set of arguments and possible options, parse the arguments and
// return the selected TailOptions
fn parse_options(args: &[String], options: &[OptGroup]) -> Result<TailOptions, String> {

    let matches = match getopts(args, options) {
        Ok(o) => o,
        Err(error) => return Err(error.to_string()),
    };

    let parse_item_count = |nstr: &str| {
        let (nstr, direction) = match nstr.char_at(0) {
                                    '+' => (nstr[1..], FromTop),
                                    _ => (nstr, FromBottom),
                                };
        match from_str(nstr.as_slice()) {
            Some(n) => Ok((n, direction)),
            None => Err(format!("{}: invalid number of lines", nstr)),
        }
    };

    let (item_count, direction) =
        match matches.opt_str("lines") {
            Some(nstr) => try!(parse_item_count(nstr.as_slice())),
            None => (DEFAULT_LINES, FromBottom),
        };

    let options = TailOptions {
        show_help: matches.opt_present("help"),
        show_version: matches.opt_present("version"),
        output_headers: !matches.opt_present("quiet") && !matches.opt_present("silent")
                            && (matches.opt_present("verbose") || matches.free.len() > 1),
        item_count: item_count,
        direction: direction,
        files: matches.free,
    };

    return Ok(options);
}

// Tail the given filename
fn tail(file_name: &str, options: &TailOptions) -> IoResult<()> {

    // Output the header, but only if we are tailing more than one file
    if options.output_headers {
        println!("==> {} <==", match file_name {
            "-" => "standard input",
            s => s,
        });
    }

    match file_name {
        // Tail stdin
        "-" => match options.direction {
            FromBottom => tail_reader(&mut stdin(), options),
            FromTop => tail_reader_top(&mut stdin(), options.item_count),
        },
        // Open the file and tail it
        file_name => File::open(&Path::new(file_name)).and_then(|mut file| {
            match options.direction {
                FromBottom => tail_file(&mut file, options.item_count),
                FromTop => tail_reader_top(&mut BufferedReader::new(file), options.item_count),
            }
        }),
    }
}

// Output the last 'n' lines from 'file'
fn tail_file(file: &mut File, n: uint) -> IoResult<()> {

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

    return copy_to_end(file, &mut stdout);
}

// Output the last 'n' lines from 'reader'
fn tail_reader<R: Reader>(reader: &mut BufferedReader<R>, options: &TailOptions) -> IoResult<()> {
    let mut stdout = stdout();
    let mut lines: RingBuf<String> = RingBuf::new();

    for line in reader.lines() {
        let line = try!(line);
        if lines.len() == options.item_count {
            lines.pop_front();
        }
        lines.push(line);
    }

    for line in lines.iter() {
        try!(stdout.write_str(line.as_slice()));
    }

    return Ok(());
}

// Tail a reader by skipping 'n' lines from the top and outputting the rest
fn tail_reader_top<R: Reader>(reader: &mut BufferedReader<R>, n: uint) -> IoResult<()> {
    let mut stdout = stdout();

    for line in reader.lines().skip(n) {
        let line = try!(line);
        try!(stdout.write_str(line.as_slice()));
    }

    return Ok(());
}

// Read from 'in' and write to 'out'
fn copy_to_end<T: Reader, U: Writer>(reader: &mut T, writer: &mut U) -> IoResult<()> {

    let mut buffer = [0u8, ..BUFFER_SIZE];
    loop {
        let bytes_read = match reader.read(buffer) {
            Ok(n) => n,
            Err(IoError { kind: EndOfFile, .. }) => break,
            Err(why) => return Err(why),
        };

        try!(writer.write(buffer[0..bytes_read]));
    }
    return Ok(());
}
