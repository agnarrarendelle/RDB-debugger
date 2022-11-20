use crate::debugger::Breakpoint;
use crate::dwarf_data::DwarfData;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::signal::Signal::SIGCONT;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::mem::size_of;
use std::os::unix::process::CommandExt;
use std::process::Child;
use std::process::Command;

//Status of the child process
pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process.  
/// Use with pre_exec to call this in the child process.

fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(
        target: &str,
        args: &Vec<String>,
        breakpoints: &mut HashMap<usize, Breakpoint>,
    ) -> Option<Inferior> {
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }

        //Set the inferior for the child process
        let mut inferior = Inferior {
            child: cmd.spawn().ok()?,
        };

        //Calls wait on child to get its status(non-blocking)
        let status = inferior.wait(None).ok()?;
        //If child is stopped, write the breakpoints addresses into into its address space
        if let Status::Stopped(signal, _rip) = status {
            if let signal::Signal::SIGTRAP = signal {
                let brks = breakpoints.clone();
                for b in brks.keys() {
                    match inferior.write_byte(*b, 0xcc) {
                        Ok(orig_instr)=>{
                            breakpoints.get_mut(&b).unwrap().orig_byte = orig_instr;

                        },
                        Err(e)=>{
                            println!("cannot set breakpoints at {}. Error: {}", b, e)
                        }
                    }
                }

                
                return Some(inferior);
            }
        }

        None
    }

    //Resume to child process from breakpoints
    //This is a little bit complicated
    //First of all, when the breakpopints is written into child's address space, 
    //the original instruction at that address would be overwritten and lost,
    //so we need to keep track of all the instructions overwritten and their location
    //Second, when the program counter(%rip) is about the resume from the breakpoint instruction,
    //we need to replace the breakpoint instruction with the original instruction
    //so that the next time the next instruction to be executed is the original instruction
    //Last, after the second step, we need to set the program counter(%rip) to the precious insturction
    //so that it can resume execution as if nothing had happened at all 
    pub fn cont(&mut self, breakpoints: &HashMap<usize, Breakpoint>) -> Result<Status, nix::Error> {
        let mut registers = ptrace::getregs(self.pid())?;
        //program counter(PC)
        let rip = registers.rip as usize;
        //address of the instruciton that interrupts the child
        let interrupted_instru_addr = rip-1;
        if let Some(breakpoint) = breakpoints.get(&interrupted_instru_addr){
            //write the original instruction back
            self.write_byte(breakpoint.addr, breakpoint.orig_byte)?;
            //set the program counter to previous instruction
            registers.rip = interrupted_instru_addr as u64;
            ptrace::setregs(self.pid(), registers)?;

            //resume the execution of the child
            ptrace::step(self.pid(), None)?;

            //return the status of the child
            self.wait(None)?;

        }

        match ptrace::cont(self.pid(), SIGCONT) {
            Ok(_) => self.wait(None),
            Err(e) => Err(e),
        }
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn kill_child(&mut self) -> Result<std::process::ExitStatus, std::io::Error> {
        match Child::kill(&mut self.child) {
            Ok(_) => Child::wait(&mut self.child),
            Err(e) => Err(e),
        }
    }

    //Print the call stack backtrace
    //we need two registers: program counter register and current stack frame register
    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let registers = ptrace::getregs(self.pid())?;
        let mut instruction_ptr = registers.rip as usize;
        let mut base_ptr = registers.rbp as usize;
        loop {
            let addr = DwarfData::get_line_from_addr(debug_data, instruction_ptr).unwrap();
            let func_name = DwarfData::get_function_from_addr(debug_data, instruction_ptr).unwrap();
            println!("at fucntion: {}. In {}", func_name, addr);
            if func_name == "main" {
                break;
            }

            instruction_ptr =
                ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType)? as usize;
            base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as usize;
        }

        // println!("at fucntion: {}. In {}", func_name, addr);

        Ok(())
    }

    pub fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }
}
