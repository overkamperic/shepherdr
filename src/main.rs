//! The one program: `herdr-projects` and its subcommands. Argument routing only: everything lives in the library.
fn main() {
    if let Err(error) = sw::budget_binding::bootstrap() { eprintln!("bounded runtime refused: {error}"); std::process::exit(2); }
    std::process::exit(sw::cli::main(std::env::args().skip(1).collect())); }
