use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::collections::HashMap;
use std::fs::{File};
use std::io::{BufRead, BufReader};

//struct to represent the breakpoints set in the program
#[derive(Clone)]
pub struct Breakpoint {
    //the address of the breakpoint
    pub addr: usize,
    //the original byte replaced by the breakpoint
    pub orig_byte: u8,
}

//Debugger struct
pub struct Debugger {
    //path to the C executable file
    target: String,
    //all lines in the original C program files
    target_lines: Vec<String>,
    //history file
    history_path: String,
    //utility to read line entered to the debugger
    readline: Editor<()>,
    //utility to change the status of the child process being examined by the debugger
    inferior: Option<Inferior>,
    //meta data about the child process
    debug_data: DwarfData,
    //breakpoints in the child process
    breakpoints: HashMap<usize, Breakpoint>,
}

impl Debugger {
    /// Create the debugger.
    pub fn new(target: &str) -> Debugger {
        //read the metadata from the file to be examined
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
        let target_lines = get_file_lines(&format!("{}.c", target));
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
            target_lines,
        }
    }

    //Create the debugger and run it 
    pub fn run(&mut self) {
        //Inside the infinety loop, the debugger will repeatedly take user command and perform the tasks requested
        loop {
            match self.get_next_command() {
                //Input the args into the child process and run it
                DebuggerCommand::Run(args) => {
                    //When a new child process is created and run by the debugger, 
                    //there might be another child process that is previously paused
                    //that needs to be dealt with, otherwise it will become a zombie process.
                    //So at the beginning of the Run command, 
                    //the debugger will check if there is any child process that has not been reaped and reap it
                    if let Some(_) = &self.inferior {
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.kill_child() {
                            Ok(_) => println!("Child {} killed", inf.pid()),
                            Err(_) => println!("No chlld to be killed"),
                        }
                    }
                    //Create the inferior to manipulate the child process
                    if let Some(inferior) =
                        Inferior::new(&self.target, &args, &mut self.breakpoints)
                    {
                        
                        self.inferior = Some(inferior);
                        let inf = self.inferior.as_mut().unwrap();
                        //Wait for child process to stop or exit and print its status
                        match inf.cont(&self.breakpoints) {
                            Ok(s) => self.print_child_status(s),
                            Err(e) => panic!("Cannot run child process. Error: {}", e),
                        }
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                //Quit the debugger. Againg it needs to check if there is any child process that has not been reaped and reap it
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
                //Continue from the breakpoints
                DebuggerCommand::Continue => {
                    if let None = self.inferior {
                        println!("No process is currently being run");
                        continue;
                    }
                    let inf = self.inferior.as_mut().unwrap();
                    //resume the child process until it is paused or exists and print its status
                    match inf.cont(&self.breakpoints) {
                        Ok(s) => self.print_child_status(s),
                        Err(e) => println!("Cannot run child process. Error: {}", e),
                    }
                }
                //Print the call stack backtrace
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
                //Set the breakpoint in the child process
                //As the breakpoints can be set before the child process is run and while the child process is running
                //this function needs to handle two different cases
                DebuggerCommand::Break(addr) => {
                    //parse the address string to usize 
                    let parsed_addr = self.parse_address(&addr);
                    if let None = parsed_addr {
                        println!("Invalid breakpoint address");
                        continue;
                    }

                    let parsed_addr = parsed_addr.unwrap();
                    //Case 1: The child process has been started and is currently paused
                    //In this case, the breakpoints instruction needs to be written direcly into child process's address space
                    if self.inferior.is_some() {
                        let inf = self.inferior.as_mut().unwrap();
                        match inf.write_byte(parsed_addr, 0xcc) {
                            Ok(orig_byte) => {
                                println!("Set breakpoint at {} while stopped", addr);
                                self.breakpoints.insert(
                                    parsed_addr,
                                    Breakpoint {
                                        addr: parsed_addr,
                                        orig_byte,
                                    },
                                );
                            }
                            Err(_) => println!("Cannot set breakpoint at {}", addr),
                        }
                    //Case 2: The child process has not been started yet
                    //In this case, push the breakpoints into the breakpoints hashtable,
                    //and the breakpoints will be written into the child process once the debugger starts running    
                    } else {
                        println!("Set a breakpoint at {}", addr);
                        self.breakpoints.insert(
                            parsed_addr,
                            Breakpoint {
                                addr: parsed_addr,
                                orig_byte: 0,
                            },
                        );
                    }
                }
            }
        }
    }

   
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit"
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

    //the breakpoints can be set on address, line number and function name
    fn parse_address(&self, addr: &str) -> Option<usize> {
        //Case1: The breakpoint is an address in the child's address space
        if addr.to_lowercase().starts_with("*0x") {
            return usize::from_str_radix(&addr[3..], 16).ok();
        //Case2: The breakpoint is a line number
        } else if addr.parse::<usize>().is_ok() {
            return self
                .debug_data
                .get_addr_for_line(None, addr.parse::<usize>().unwrap());
        //Case3: The breakpoint is a function name
        } else {
            return self.debug_data.get_addr_for_function(None, addr.trim());
        }
    }
    fn print_nearby_line(&self, line_num: usize) {
        println!("nearby lines-------");
        let line_nums = [line_num - 1, line_num, line_num + 1];
        for l in line_nums {
            if let Some(line) = self.target_lines.get(l) {
                println!("{}", line)
            }
        }
    }

    //Print the status of the child process being examined, and there are 3 statuses
    //1. Existed
    //2. Stopped
    //3. Signaled
    fn print_child_status(&self, s: Status) {
        match s {
            Status::Exited(code) => println!("Child existed (status {})", code),
            //Child process is stopped because of some signals sent by debugger 
            Status::Stopped(sig, rip) => {
                println!("Child stopped (signal: {})", sig);
                if let (Some(line), Some(func_name)) = (
                    DwarfData::get_line_from_addr(&self.debug_data, rip),
                    DwarfData::get_function_from_addr(&self.debug_data, rip),
                ) {
                    println!("Stopped at {}", line);
                    println!("Inside function {}", func_name);
                    self.print_nearby_line(line.number);
                    self.debug_data
                        .print_local_variable_from_func(None, &func_name)
                }
            }
            //Child process is stopped because it has executed some instruction that causes itself to be stopped 
            Status::Signaled(sig) => {
                println!("Program stopped due to signal {}", sig)
            }
        }
    }
}

fn get_file_lines(target: &str) -> Vec<String> {
    let file = File::open(target).expect(&format!("Cannot read lines in file {}", target));
    let reader = BufReader::new(file);
    let mut lines = vec![];
    for line in reader.lines() {
        lines.push(line.expect(&format!("Cannot read lines in file {}", target)));
    }

    lines
}
