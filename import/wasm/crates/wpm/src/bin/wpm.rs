fn main() {
    let args: Vec<String> = std::env::args().collect();
    match wpm::cli_run(&args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("wpm: {e:#}");
            std::process::exit(1);
        }
    }
}
