use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn run(directories: &[PathBuf]) -> ExitCode {
    let mut code = ExitCode::SUCCESS;
    for path in directories {
        match locus_derivation::locus::delivery::send(path) {
            Ok(count) => println!("{}: {count} Atom(s) confirmed", path.display()),
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                code = ExitCode::FAILURE;
            }
        }
    }
    code
}
