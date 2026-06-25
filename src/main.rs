fn main() {
    match diverge::run(std::env::args().skip(1)) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("diverge: {error}");
            std::process::exit(1);
        }
    }
}
