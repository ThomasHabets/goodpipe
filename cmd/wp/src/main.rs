use clap::{command, Arg, ArgAction};
use std::io;
use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;

const ESC: u8 = '_' as u8;
const EESC: u8 = '-' as u8;

const EOF: u8 = 'Z' as u8;
const SOB: u8 = '<' as u8;
const EOB: u8 = '>' as u8;

const ESOB: u8 = '[' as u8;
const EEOB: u8 = ']' as u8;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blatest() -> Result<(), String> {
        assert_eq!(12, 12);
        Ok(())
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Data,
    DataEsc,
}

#[derive(Debug)]
struct Decapper {
    state: State,
}

impl Decapper {
    fn new() -> Decapper {
        Decapper { state: State::Idle }
    }
    fn add(&mut self, mut next: &std::process::ChildStdin, x: &[u8]) -> bool {
        let mut out = Vec::new();
        let mut do_write = |out: &mut Vec<u8>| {
            if out.len() > 0 {
                next.write(out.as_slice()).expect("write error");
                out.clear();
            }
        };
        for ch in x {
            match self.state {
                State::Idle => match *ch {
                    EOF => return true,
                    SOB => {
                        self.state = State::Data;
                    }
                    other => {
                        panic!("TODO: Invalid character in input: {}", other);
                    }
                },
                State::Data => match *ch {
                    EOB => {
                        do_write(&mut out);
                        self.state = State::Idle;
                    }
                    ESC => {
                        self.state = State::DataEsc;
                    }
                    _ => {
                        out.push(*ch);
                    }
                },
                State::DataEsc => match *ch {
                    EESC => {
                        out.push(ESC);
                        self.state = State::Data;
                    }
                    ESOB => {
                        out.push(SOB);
                        self.state = State::Data;
                    }
                    EEOB => {
                        out.push(EOB);
                        self.state = State::Data;
                    }
                    other => {
                        panic!("TODO: invalid escape input: {}", other);
                    }
                },
            };
        }
        do_write(&mut out);
        false
    }
}

fn encap(inp: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(SOB);
    for ch in inp {
        match *ch {
            ESC => {
                out.push(ESC);
                out.push(EESC);
            }
            SOB => {
                out.push(ESC);
                out.push(ESOB);
            }
            EOB => {
                out.push(ESC);
                out.push(EEOB);
            }
            _ => {
                out.push(*ch);
            }
        }
    }
    out.push(EOB);
    return out;
}

fn main() {
    let matches = command!()
        .trailing_var_arg(true)
        .arg(
            Arg::new("input")
                .help("Enable input processing")
                .short('i')
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("output")
                .help("Enable output processing")
                .short('o')
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("command")
                .required(true)
                .takes_value(true)
                .multiple_values(true),
        )
        .get_matches();

    let args = matches
        .get_many::<String>("command")
        .unwrap_or_default()
        .map(|v| v.as_str())
        .collect::<Vec<_>>();

    let flag_o = matches.get_one::<bool>("output").expect("blah");
    let flag_i = matches.get_one::<bool>("input").expect("blah");

    // TODO: move all but flag parsing to lib.
    let mut prep = Command::new(args[0]);
    if *flag_o {
        prep.stdout(Stdio::piped());
    }
    if *flag_i {
        prep.stdin(Stdio::piped());
    }

    let mut child = prep
        .args(&args[1..])
        .spawn()
        .expect("failed to execute child");

    let (ok_out_tx, ok_out_rx) = mpsc::channel();

    let othread = (|| {
        if *flag_o {
            let mut childout = child.stdout.take().unwrap();
            return thread::spawn(move || {
                loop {
                    let mut buffer = vec![0; 128 as usize];
                    let n = childout.read(&mut buffer).expect("buffer overflow");
                    if n == 0 {
                        break;
                    }
                    io::stdout()
                        .write(&encap(&buffer[0..n]))
                        .expect("write error");
                }
                if ok_out_rx.recv().unwrap() {
                    io::stdout()
                        .write(&vec![EOF])
                        .expect("write error writing eof");
                }
            });
        }
        thread::spawn(move || {})
    })();

    let (ctx, crx) = mpsc::channel();

    let ithread = (|| {
        if *flag_i {
            //let childin = child.stdin.take().unwrap();
            return thread::spawn(move || {
                let childin = child.stdin.as_mut().unwrap();
                let mut dec = Decapper::new();
                loop {
                    let mut buffer = vec![0; 128 as usize];
                    let n = io::stdin().read(&mut buffer).expect("buffer overflow");
                    if n == 0 {
                        break;
                    }
                    let buf = &buffer[0..n];
                    if dec.add(&childin, buf) {
                        // Got EOF.
                        drop(childin);
                        ctx.send(child.wait()).unwrap();
                        return;
                    }
                }
                child.kill().expect("failed to kill child");
                let ws = child.wait();
                if let Ok(ecode) = ws {
                    if !ecode.success() {
                        ctx.send(ws).unwrap();
                        return;
                    }
                    // TODO: send() a Result error instead.
                    panic!("Killed child, but it died a happy process");
                }
                ctx.send(ws).unwrap();
            });
        }
        thread::spawn(move || {
            ctx.send(child.wait()).unwrap();
        })
    })();

    let ecode = crx
        .recv()
        .expect("main thread getting back client object")
        .expect("wait success");
    if !ecode.success() {
        std::process::exit(match ecode.code() {
            Some(code) => {
                eprintln!("Subprocess died with exit code {}", code);
                code
            }
            None => {
                eprintln!("Subprocess died with NO exit code (probably signal)");
                1
            }
        });
    }
    if *flag_o {
        ok_out_tx
            .send(true)
            .expect("failed to send ok to stdout thread");
    }
    othread.join().expect("failed to join othread");
    ithread.join().expect("failed to join ithread");
}
