// All types of command that the debugger support
pub enum DebuggerCommand {
    Quit,//quite the debugger
    Run(Vec<String>),//Run the debugger. The argument is a vector of strings that serve as the arguments to the program being run by the debugger
    Continue,//Continue from the breakpoints 
    Backtrace,//print the call stack at the current breakpoint
    Break(String)//set the breakpoint in the program. The argument is the address of the breakpoint to be set
}

impl DebuggerCommand {
    pub fn from_tokens(tokens: &Vec<&str>) -> Option<DebuggerCommand> {
        match tokens[0] {
            "q" | "quit" => Some(DebuggerCommand::Quit),
            "r" | "run" => {
                let args = tokens[1..].to_vec();
                Some(DebuggerCommand::Run(
                    args.iter().map(|s| s.to_string()).collect(),
                ))
            },
            "c" | "cont"=>{
                Some(DebuggerCommand::Continue)
            },
            "bt" | "back" | "backtrace"=>{
                Some(DebuggerCommand::Backtrace)
            },
            "br" | "break"=>{
                Some(DebuggerCommand::Break(tokens[1].to_string()))
            },

            // Default case:
            _ => None,
        }
    }
}

