use clap::{command, Arg, ArgAction};
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
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
    count: u64,
}

impl Decapper {
    fn new() -> Decapper {
        Decapper {
            state: State::Idle,
            count: 0,
        }
    }
    // Return:
    // * Some(true) if EOF found.
    // * Some(false) if we should carry on.
    // * None if we should stop. Bad input.
    fn add(&mut self, mut next: &std::process::ChildStdin, x: &[u8]) -> Option<bool> {
        let mut out = Vec::new();
        let mut ret = false;
        for ch in x {
            self.count += 1;
            match self.state {
                State::Idle => match *ch {
                    EOF => {
                        ret = true;
                        break;
                    }
                    SOB => {
                        self.state = State::Data;
                    }
                    other => {
                        let u = &[other];
                        let s = std::str::from_utf8(u).unwrap_or("<binary>");
                        eprintln!(
                            "wp: got invalid command character in input at index {}: {} ({})",
                            self.count - 1,
                            other,
                            s
                        );
                        return None;
                    }
                },
                State::Data => match *ch {
                    EOB => {
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
                        let u = &[other];
                        let s = std::str::from_utf8(u).unwrap_or("<binary>");
                        eprintln!(
                            "wp: invalid escape input at index {}: {} ({})",
                            self.count - 1,
                            other,
                            s
                        );
                        return None;
                    }
                },
            };
        }
        if out.len() == 0 {
            return Some(ret);
        }
        match next.write(out.as_slice()) {
            Ok(_) => Some(ret),
            Err(e) => {
                eprintln!("wp: Error writing to stdout: {}", e);
                None
            }
        }
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

    let flag_o = matches
        .get_one::<bool>("output")
        .expect("Failed to get output flag");
    let flag_i = matches
        .get_one::<bool>("input")
        .expect("Failed to get input flag");

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
            let mut childout = child
                .stdout
                .take()
                .expect("failed to take ownership of child stdout");
            return thread::spawn(move || {
                loop {
                    let mut buffer = vec![0; 128 as usize];
                    let n = childout
                        .read(&mut buffer)
                        .expect("failed to read from child stdout");
                    if n == 0 {
                        break;
                    }
                    io::stdout()
                        .write(&encap(&buffer[0..n]))
                        .expect("error writing to stdoutr");
                }
                if ok_out_rx
                    .recv()
                    .expect("othread failed to receive if it should send EOF")
                {
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
            let childin = child
                .stdin
                .take()
                .expect("failed to take ownership of child stdin");
            return thread::spawn(move || {
                let mut dec = Decapper::new();
                loop {
                    let mut buffer = vec![0; 128 as usize];
                    let n = io::stdin()
                        .read(&mut buffer)
                        .expect("failed to read from stdin");
                    if n == 0 {
                        break;
                    }
                    let buf = &buffer[0..n];
                    match dec.add(&childin, buf) {
                        Some(true) => {
                            // Got EOF.
                            drop(childin);
                            ctx.send(child.wait())
                                .expect("failed to send wait status from ithread");
                            return true;
                        }
                        Some(false) => (),
                        None => break,
                    }
                }
                child.kill().expect("failed to kill child");
                let ws = child.wait();
                if let Ok(ecode) = ws {
                    if ecode.success() {
                        eprintln!("wp: Killed child, but it died a happy process");
                    }
                }
                ctx.send(ws)
                    .expect("failed to send wait status from ithread after kill");
                // TODO: Ideally we would send a fake error if
                // kill results in exit code 0, but I can't find
                // how to do that.
                //
                // Instead we're sending the success, but having
                // the thread return false.
                false
            });
        }
        thread::spawn(move || {
            ctx.send(child.wait())
                .expect("failed to send wait status from fake ithread");
            true
        })
    })();

    let ecode = crx
        .recv()
        .expect("main thread getting back client object")
        .expect("wait success");
    if !ecode.success() {
        std::process::exit({
            if let Some(code) = ecode.code() {
                eprintln!("wp: Subprocess died with exit code {}", code);
                code
            } else if let Some(sig) = ecode.signal() {
                eprintln!("wp: died due to signal {}", sig);
                1
            } else {
                eprintln!("wp: with no exit code and no signal");
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
    if !ithread.join().expect("failed to join ithread") {
        std::process::exit(1);
    }
}
