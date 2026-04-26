use std::fmt;

/// Represents all possible errors that can occur during transaction processing.
#[derive(Debug)]
pub enum TransactionError {
    /// Insufficient funds for the requested operation
    InsufficientAmount { client: u16, tx: u32 },
    /// Account is locked due to chargeback and cannot be used
    AccountLocked(u16),
    /// Referenced transaction ID not found in history
    TransactionNotFound(u32),
    /// Operation requires transaction to be in disputed state
    TransactionNotDisputed(u32),
    /// Transaction is already disputed and cannot be disputed again
    TransactionAlreadyDisputed(u32),
    /// Client ID mismatch between transaction and operation
    ClientMismatch { tx: u32, expected: u16, got: u16 },
    /// Transaction type requires an amount but none was provided
    MissingAmount(u32),
    /// Amount string could not be parsed or is invalid
    InvalidAmount(String),
    /// Unknown transaction type encountered in CSV
    UnknownTransactionType(String),
    /// CSV record is malformed or missing required fields
    MalformedRecord,
    /// Numeric overflow occurred during calculation
    NumericOverflow(u32),
    /// CSV parsing error from underlying csv crate
    CsvParse(csv::Error),
    /// I/O error during file operations
    Io(std::io::Error),
}

impl fmt::Display for TransactionError {
    /// Provides human-readable error messages for logging.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TransactionError::InsufficientAmount { client, tx } => {
                write!(f, "insufficient amount for client={client} transaction={tx}")
            }
            TransactionError::AccountLocked(client) => write!(f, "client={client} account locked"),
            TransactionError::TransactionNotFound(tx) => write!(f, "transaction={tx} not found"),
            TransactionError::TransactionNotDisputed(tx) => write!(f, "transaction={tx} is not disputed"),
            TransactionError::TransactionAlreadyDisputed(tx) => write!(f, "transaction={tx} is already being disputed"),
            TransactionError::ClientMismatch { tx, expected, got } => {
                write!(f, "client mismatch for transaction={tx}: expected={expected} got={got}")
            }
            TransactionError::MissingAmount(tx) => write!(f, "missing amount for transaction={tx}"),
            TransactionError::InvalidAmount(s) => write!(f, "invalid amount: {s}"),
            TransactionError::UnknownTransactionType(s) => write!(f, "unknown transaction type: {s}"),
            TransactionError::MalformedRecord => write!(f, "malformed csv record"),
            TransactionError::NumericOverflow(tx) => write!(f, "numeric overflow for transaction={tx}"),
            TransactionError::CsvParse(e) => write!(f, "csv parse error: {e}"),
            TransactionError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for TransactionError {
    /// Provides the underlying error source for error chaining.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransactionError::CsvParse(e) => Some(e),
            TransactionError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<csv::Error> for TransactionError {
    /// Converts csv::Error to TransactionError for error handling.
    fn from(e: csv::Error) -> Self { TransactionError::CsvParse(e) }
}

impl From<std::io::Error> for TransactionError {
    /// Converts std::io::Error to TransactionError for error handling.
    fn from(e: std::io::Error) -> Self { TransactionError::Io(e) }
}
