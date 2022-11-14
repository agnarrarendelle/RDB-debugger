use std::collections::HashMap;

use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;

#[derive(Clone)]
pub struct Breakpoint {
    pub addr: usize,
    pub orig_byte: u8,
}
pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: HashMap<usize, Breakpoint>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };
        debug_data.print();
        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);
        let breakpoints = HashMap::new();
        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints,
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(_) = &self.inferior {
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.kill_child() {
                            Ok(_) => println!("Child {} killed", inf.pid()),
                            Err(_) => println!("No chlld to be killed"),
                        }
                    }

                    if let Some(inferior) = Inferior::new(&self.target, &args, &mut self.breakpoints) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // TODO (milestone 1): make the inferior run
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.cont(&self.breakpoints) {
                            Ok(s) => match s {
                                Status::Exited(code) => println!("Child existed (status {})", code),
                                Status::Stopped(sig, rip) => {
                                    println!("Child stopped (signal: {})", sig);
                                    if let Some(line) =
                                        DwarfData::get_line_from_addr(&self.debug_data, rip)
                                    {
                                        println!("Stopped at {}", line);
                                    }
                                }
                                Status::Signaled(sig) => println!("Program stopped due to signal {}", sig),
                            },
                            Err(e) => panic!("Cannot run child process. Error: {}", e),
                        }
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    if let Some(_) = &self.inferior {
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.kill_child() {
                            Ok(_) => println!("Child {} killed", inf.pid()),
                            Err(_) => (),
                        }
                    }
                    return;
                }
                DebuggerCommand::Continue => {
                    if let None = self.inferior {
                        println!("No process is currently being run");
                        continue;
                    }
                    let inf = self.inferior.as_mut().unwrap();
                    match inf.cont(&self.breakpoints) {
                        Ok(s) => match s {
                            Status::Exited(code) => println!("Child existed (status {})", code),
                            Status::Stopped(sig, rip) => {
                                println!("Child stopped (signal: {})", sig);
                                if let Some(line) =
                                    DwarfData::get_line_from_addr(&self.debug_data, rip)
                                {
                                    println!("Stopped at {}", line);
                                }
                            }
                            Status::Signaled(sig)  => println!("Program stopped due to signal {}", sig),
                        },
                        Err(e) => println!("Cannot run child process. Error: {}", e),
                    }
                }
                DebuggerCommand::Backtrace => {
                    if let None = self.inferior {
                        println!("No process is currently being run");
                        continue;
                    }
                    let inf = self.inferior.as_mut().unwrap();
                    if let Err(e) = inf.print_backtrace(&self.debug_data) {
                        println!("Cannot print backtrace. Error: {}", e);
                    }
                }
                DebuggerCommand::Break(addr) => {
                    let parsed_addr = parse_address(&addr);
                    if let None = parsed_addr {
                        println!("Invalid breakpoint address");
                        continue;
                    }

                    let parsed_addr = parsed_addr.unwrap();
                    if self.inferior.is_some() {
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.write_byte(parsed_addr, 0xcc) {
                            Ok(orig_byte) => {
                                println!("Set breakpoint at {} while stopped", addr);
                                self.breakpoints.insert(parsed_addr, Breakpoint { addr: parsed_addr, orig_byte});
                            }
                            Err(_) => println!("Cannot set breakpoint at {}", addr),
                        }
                    } else {
                        println!("Set a breakpoint at {}", addr);
                        self.breakpoints.insert(parsed_addr, Breakpoint { addr: parsed_addr, orig_byte:0});
                    }
                    // Some(parsed_addr) => {
                    //     let addr = &addr[1..];
                    //     if self.inferior.is_some() {
                    //         let inf = self.inferior.as_mut().unwrap();
                    //         match inf.write_byte(parsed_addr, 0xcc) {
                    //             Ok(_) => {
                    //                 println!("Set breakpoint while stopped");
                    //                 self.breakpoints.push(parsed_addr);
                    //             },
                    //             Err(_)=>println!("Cannot set breakpoint at {}",addr)
                    //         }
                    //     } else {
                    //         println!("Set a breakpoint at {}", addr);
                    //         self.breakpoints.push(parsed_addr);
                    //     }
                    // }
                    // None => println!("Invalid Breakpoint"),
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

fn parse_address(addr: &str) -> Option<usize> {
    let addr_without_0x = if addr.to_lowercase().starts_with("*0x") {
        &addr[3..]
    } else {
        &addr
    };
    usize::from_str_radix(addr_without_0x, 16).ok()
}
