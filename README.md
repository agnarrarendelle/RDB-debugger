# RDB Debugger: A simple clone of GDB debugger writen in Rust

## How to Use

1. Compiler your C program
   - Create a C program and compile it with
     `-O0 -g -no-pie -fno-omit-frame-pointer -o`
     to get an executable with debugger info in it. Or you can also use the sameple files provided in `samples` directory
2. Run the debugger
   - Use the command `cargo run <path to your compiled executable>` to start the debugger

## Commands

1. Start the debugger:

```
r <optional arguments to your C program>
```

2. Set breakpoints:

```
br <address, line number or function name in your C program>
```

3. Pause the debugger: `ctrl + c`
4. Print backtrace from the current breakpoints:

```
bt
```

5. Continue from breakpoints:

```
c
```

6. Quit the debugger:

```
q
```

## TODO

1. print the value of variables
   - When the program is compiled, only the value of global/static variables are stored at a fixed location in the final executable(Please refer to [ELF](https://en.wikipedia.org/wiki/Executable_and_Linkable_Format) format for more details). and the value of local variables are not determined until runtime where the the stack frames of the functions where local variables are declared are created. Hence, it makes it more difficult to obtain the values for the variables as the debugger is not able to directly access them from the final executable and has to wait until the program is being run
