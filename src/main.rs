mod account;
mod amount;
mod engine;
mod error;
mod transaction;

use std::env;
use std::fs::File;
use std::io;
use std::process;

use error::TransactionError;

fn main() {
    if let Err(e) = process_csv_file() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn process_csv_file() -> Result<(), TransactionError> {
    let path = env::args().nth(1).ok_or_else(|| {
        TransactionError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "make sure to provide a path to a transactions.csv file",
        ))
    })?;

    let file = File::open(&path).map_err(TransactionError::Io)?;
    let stdout = io::stdout();
    engine::process_csv(file, stdout.lock())
}
