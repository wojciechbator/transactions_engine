use crate::amount::Amount;
use crate::error::TransactionError;

/// Represents the type of transaction that can be processed.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionType {
    /// Adds funds to a client account
    Deposit,
    /// Removes funds from a client account
    Withdrawal,
    /// Places a transaction under dispute, moving funds to held status
    Dispute,
    /// Resolves a disputed transaction, releasing held funds
    Resolve,
    /// Charges back a disputed transaction, removing funds and locking account
    Chargeback,
}

/// Represents the current state of a transaction in the system.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionState {
    /// Transaction is in normal state, funds are available
    Normal,
    /// Transaction is under dispute, funds are held
    Disputed,
    /// Transaction has been charged back, funds removed and account locked
    ChargedBack,
}

impl TransactionType {
    /// Parses transaction type from byte slice for CSV processing.
    fn from_bytes(bytes_array: &[u8]) -> Result<Self, TransactionError> {
        match bytes_array {
            b"deposit" => Ok(Self::Deposit),
            b"withdrawal" => Ok(Self::Withdrawal),
            b"dispute" => Ok(Self::Dispute),
            b"resolve" => Ok(Self::Resolve),
            b"chargeback" => Ok(Self::Chargeback),
            other => Err(TransactionError::UnknownTransactionType(
                String::from_utf8_lossy(other).into_owned(),
            )),
        }
    }
}

/// Represents a single transaction row from CSV input.
/// 
/// Contains the transaction type, client ID, transaction ID, and optional amount.
/// Used for parsing raw CSV data before processing.
#[derive(Debug)]
pub struct TransactionRow {
    pub tx_type: TransactionType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Amount>,
}

impl TransactionRow {
    /// Parses a CSV ByteRecord into a TransactionRow.
    /// 
    /// Handles whitespace trimming and optional amount fields.
    pub fn from_byte_record(record: &csv::ByteRecord) -> Result<Self, TransactionError> {
        let tx_type = TransactionType::from_bytes(trim(record.get(0)
            .ok_or(TransactionError::MalformedRecord)?)
        )?;

        let client = parse_u16(trim(record.get(1)
            .ok_or(TransactionError::MalformedRecord)?)
        )?;

        let tx = parse_u32(trim(record.get(2)
            .ok_or(TransactionError::MalformedRecord)?)
        )?;

        let amount = match record.get(3).map(trim) {
            Some(b"") | None => None,
            Some(raw) => Some(Amount::from_bytes(raw)?),
        };

        Ok(Self { tx_type, client, tx, amount })
    }
}

/// Trims whitespace from both ends of a byte slice.
fn trim(bytes: &[u8]) -> &[u8] {
    // left
    let bytes = match bytes.iter().position(|char| !char.is_ascii_whitespace()) {
        Some(idx) => &bytes[idx..],
        None => return b"",
    };
    // right
    match bytes.iter().rposition(|char| !char.is_ascii_whitespace()) {
        Some(idx) => &bytes[..=idx],
        None => b"",
    }
}

/// Parses byte slice into u16 for client ID field.
fn parse_u16(bytes_num: &[u8]) -> Result<u16, TransactionError> {
    std::str::from_utf8(bytes_num)
        .ok()
        .and_then(|str_num| str_num.parse().ok())
        .ok_or(TransactionError::MalformedRecord)
}

/// Parses byte slice into u32 for transaction ID field.
fn parse_u32(bytes_num: &[u8]) -> Result<u32, TransactionError> {
    std::str::from_utf8(bytes_num)
        .ok()
        .and_then(|str_num| str_num.parse().ok())
        .ok_or(TransactionError::MalformedRecord)
}

/// Represents a stored transaction record in the ledger.
/// 
/// Tracks the client, amount, and current state of a transaction
/// for dispute resolution and chargeback processing.
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub client: u16,
    pub amount: Amount,
    pub state: TransactionState,
}

impl TransactionRecord {
    /// Creates a new transaction record in Normal state.
    pub fn new(client: u16, amount: Amount) -> Self {
        Self { client, amount, state: TransactionState::Normal }
    }

    /// Returns true if the transaction is currently disputed.
    pub fn is_disputed(&self) -> bool {
        self.state == TransactionState::Disputed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(fields: &[&[u8]]) -> csv::ByteRecord {
        csv::ByteRecord::from(fields.iter().map(|f| *f).collect::<Vec<_>>())
    }

    #[test]
    fn test_parse_deposit() {
        let record = make_record(&[b"deposit", b"1", b"42", b"10.5000"]);
        let row = TransactionRow::from_byte_record(&record).unwrap();
        assert_eq!(row.tx_type, TransactionType::Deposit);
        assert_eq!(row.client, 1);
        assert_eq!(row.tx, 42);
        assert_eq!(row.amount.unwrap(), "10.5000".parse().unwrap());
    }

    #[test]
    fn test_parse_chargeback_no_amount() {
        let record = make_record(&[b"chargeback", b"3", b"7", b""]);
        let row = TransactionRow::from_byte_record(&record).unwrap();
        assert_eq!(row.tx_type, TransactionType::Chargeback);
        assert!(row.amount.is_none());
    }

    #[test]
    fn test_parse_dispute_missing_amount_field() {
        let record = make_record(&[b"dispute", b"2", b"5"]);
        let row = TransactionRow::from_byte_record(&record).unwrap();
        assert_eq!(row.tx_type, TransactionType::Dispute);
        assert!(row.amount.is_none());
    }

    #[test]
    fn test_parse_whitespace_trimmed() {
        let record = make_record(&[b"  deposit  ", b" 1 ", b" 10 ", b" 5.0000 "]);
        let row = TransactionRow::from_byte_record(&record).unwrap();
        assert_eq!(row.tx_type, TransactionType::Deposit);
        assert_eq!(row.client, 1);
        assert_eq!(row.tx, 10);
    }

    #[test]
    fn test_unknown_tx_type_is_error() {
        assert!(matches!(
            TransactionRow::from_byte_record(&make_record(&[b"refund", b"1", b"1", b"10.0"])),
            Err(TransactionError::UnknownTransactionType(_))
        ));
    }

    #[test]
    fn test_missing_fields_is_error() {
        assert!(matches!(
            TransactionRow::from_byte_record(&make_record(&[b"deposit", b"1"])),
            Err(TransactionError::MalformedRecord)
        ));
    }

    #[test]
    fn test_invalid_client_id_is_error() {
        assert!(matches!(
            TransactionRow::from_byte_record(&make_record(&[b"deposit", b"notanumber", b"1", b"10.0"])),
            Err(TransactionError::MalformedRecord)
        ));
    }

    #[test]
    fn test_client_id_overflow_is_error() {
        assert!(matches!(
            TransactionRow::from_byte_record(&make_record(&[b"deposit", b"99999", b"1", b"10.0"])),
            Err(TransactionError::MalformedRecord)
        ));
    }
}
