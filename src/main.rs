fn main() {
    if let Err(error) = diverge::run(std::env::args().skip(1)) {
        eprintln!("diverge: {error}");
        std::process::exit(1);
    }
}
