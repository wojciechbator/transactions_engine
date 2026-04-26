use std::io::{Read, Write};

use rustc_hash::FxHashMap;

use crate::account::{Account, SerializedAccount};
use crate::error::TransactionError;
use crate::transaction::{TransactionRow, TransactionRecord, TransactionState, TransactionType};

/// A transaction processing ledger that manages client accounts and transaction history.
/// 
/// The Ledger handles all transaction types including deposits, withdrawals, disputes,
/// resolutions, and chargebacks. It maintains account balances and tracks the state
/// of each transaction throughout its lifecycle.
#[derive(Clone)]
pub struct Ledger {
    accounts: FxHashMap<u16, Account>,
    tx_history: FxHashMap<u32, TransactionRecord>,
}

impl Ledger {
    /// Creates a new empty ledger with no accounts or transaction history.
    pub fn new() -> Self {
        Self {
            accounts: FxHashMap::default(),
            tx_history: FxHashMap::default(),
        }
    }

    /// Processes a transaction and updates the ledger state.
    pub fn apply(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        match row.tx_type {
            TransactionType::Deposit => self.handle_deposit(row),
            TransactionType::Withdrawal => self.handle_withdrawal(row),
            TransactionType::Dispute => self.handle_dispute(row),
            TransactionType::Resolve => self.handle_resolve(row),
            TransactionType::Chargeback => self.handle_chargeback(row),
        }
    }

    /// Retrieves an existing account or creates a new one.
    fn get_or_create_account(&mut self, client: u16) -> &mut Account {
        self.accounts.entry(client).or_insert_with(|| Account::new(client))
    }

    /// Processes a deposit transaction, adding funds to client account.
    fn handle_deposit(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        let amount = row.amount
            .ok_or(TransactionError::MissingAmount(row.tx))?;

        self
            .get_or_create_account(row.client)
            .deposit(amount, row.tx)?;

        self
            .tx_history
            .insert(row.tx, TransactionRecord::new(row.client, amount));

        Ok(())
    }

    /// Processes a withdrawal transaction, removing funds from client account.
    /// Withdrawals are not included in tx_history as this engine does not support disputing withdrawal.
    fn handle_withdrawal(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        let amount = row.amount
            .ok_or(TransactionError::MissingAmount(row.tx))?;

        self
            .get_or_create_account(row.client)
            .withdraw(amount, row.tx)?;

        Ok(())
    }

    /// Handles a dispute, moving disputed funds to held status.
    fn handle_dispute(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        let tx_record = self.tx_history
            .get_mut(&row.tx)
            .ok_or(TransactionError::TransactionNotFound(row.tx))?;

        if tx_record.client != row.client {
            return Err(TransactionError::ClientMismatch {
                tx: row.tx,
                expected: tx_record.client,
                got: row.client
            });
        }

        if tx_record.state != TransactionState::Normal {
            return Err(TransactionError::TransactionAlreadyDisputed(row.tx));
        }

        let amount = tx_record.amount;
        tx_record.state = TransactionState::Disputed;

        self
            .get_or_create_account(row.client)
            .hold(amount, row.tx)?;

        Ok(())
    }

    /// Resolves a disputed transaction, releasing held funds back to available.
    fn handle_resolve(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        let tx_record = self.tx_history
            .get_mut(&row.tx)
            .ok_or(TransactionError::TransactionNotFound(row.tx))?;

        if tx_record.client != row.client {
            return Err(TransactionError::ClientMismatch {
                tx: row.tx,
                expected: tx_record.client,
                got: row.client
            });
        }

        if !tx_record.is_disputed() {
            return Err(TransactionError::TransactionNotDisputed(row.tx));
        }

        let amount = tx_record.amount;
        tx_record.state = TransactionState::Normal;

        self
            .get_or_create_account(row.client)
            .release(amount, row.tx)?;

        Ok(())
    }

    /// Processes a chargeback, removing funds and locking the client account.
    fn handle_chargeback(&mut self, row: TransactionRow) -> Result<(), TransactionError> {
        let tx_record = self.tx_history
            .get_mut(&row.tx)
            .ok_or(TransactionError::TransactionNotFound(row.tx))?;

        if tx_record.client != row.client {
            return Err(TransactionError::ClientMismatch {
                tx: row.tx,
                expected: tx_record.client,
                got: row.client
            });
        }

        if !tx_record.is_disputed() {
            return Err(TransactionError::TransactionNotDisputed(row.tx));
        }

        let amount = tx_record.amount;
        tx_record.state = TransactionState::ChargedBack;

        self
            .get_or_create_account(row.client)
            .chargeback(amount, row.tx)?;

        Ok(())
    }

    /// Writes the current state of accounts to a CSV writer.
    pub fn write_output<W: Write>(&self, writer: W) -> Result<(), TransactionError> {
        let mut write_stream = csv::WriterBuilder::new().from_writer(writer);
        for account in self.accounts.values() {
            write_stream.serialize(SerializedAccount::from(account))
                .map_err(TransactionError::CsvParse)?;
        }
        write_stream.flush().map_err(TransactionError::Io)?;

        Ok(())
    }
}

/// Processes a CSV stream of transactions and writes the final account states to a CSV writer.
pub fn process_csv<R: Read, W: Write>(reader: R, writer: W) -> Result<(), TransactionError> {
    let mut read_stream = csv::ReaderBuilder::new()
        .trim(csv::Trim::None)
        .flexible(true)
        .from_reader(reader);
    let mut ledger = Ledger::new();
    let mut raw_record = csv::ByteRecord::new();

    while read_stream.read_byte_record(&mut raw_record).map_err(TransactionError::CsvParse)? {
        let record = match TransactionRow::from_byte_record(&raw_record) {
            Ok(row) => row,
            Err(err) => { eprintln!("skipped malformed record: {err}"); continue; }
        };
        if let Err(err) = ledger.apply(record) {
            eprintln!("skipped transaction: {err}");
        }
    }

    ledger.write_output(writer)
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_input(input: &str) -> Ledger {
        let mut ledger = Ledger::new();
        let mut read_stream = csv::ReaderBuilder::new()
            .trim(csv::Trim::None)
            .flexible(true)
            .from_reader(input.as_bytes());
        let mut raw = csv::ByteRecord::new();
        while read_stream.read_byte_record(&mut raw).unwrap() {
            if let Ok(record) = TransactionRow::from_byte_record(&raw) {
                let _ = ledger.apply(record);
            }
        }
        ledger
    }

    fn amount(s: &str) -> crate::amount::Amount {
        s.parse().unwrap()
    }

    #[test]
    fn test_deposit_increases_available_and_total() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert_eq!(account.total(), amount("100.0"));
        assert_eq!(account.held, crate::amount::Amount::ZERO);
    }

    #[test]
    fn test_withdrawal_decreases_available_and_total() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\nwithdrawal,1,2,40.0\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("60.0"));
        assert_eq!(account.total(), amount("60.0"));
    }

    #[test]
    fn test_withdrawal_insufficient_funds_ignored() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,10.0\nwithdrawal,1,2,50.0\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("10.0"));
    }

    #[test]
    fn test_dispute_moves_funds_to_held() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("0.0"));
        assert_eq!(account.held, amount("100.0"));
        assert_eq!(account.total(), amount("100.0"));
    }

    #[test]
    fn test_resolve_releases_held_funds() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nresolve,1,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert_eq!(account.held, amount("0.0"));
        assert_eq!(account.total(), amount("100.0"));
    }

    #[test]
    fn test_chargeback_removes_held_and_locks_account() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nchargeback,1,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert!(account.locked);
        assert_eq!(account.available, amount("0.0"));
        assert_eq!(account.held, amount("0.0"));
        assert_eq!(account.total(), amount("0.0"));
    }

    #[test]
    fn test_locked_account_rejects_deposits() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nchargeback,1,1,\ndeposit,1,2,50.0\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.total(), amount("0.0"));
        assert!(account.locked);
    }

    #[test]
    fn test_multiple_clients_are_independent() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,50.0\ndeposit,2,2,80.0\nwithdrawal,1,3,20.0\n");
        assert_eq!(ledger.accounts.get(&1).unwrap().available, amount("30.0"));
        assert_eq!(ledger.accounts.get(&2).unwrap().available, amount("80.0"));
    }

    #[test]
    fn test_dispute_nonexistent_tx_ignored() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,999,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert_eq!(account.held, amount("0.0"));
    }

    #[test]
    fn test_resolve_without_dispute_ignored() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\nresolve,1,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert_eq!(account.held, amount("0.0"));
    }

    #[test]
    fn test_chargeback_without_dispute_ignored() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\nchargeback,1,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert!(!account.locked);
    }

    #[test]
    fn test_client_mismatch_on_dispute_ignored() {
        let ledger = run_input("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,2,1,\n");
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("100.0"));
        assert_eq!(account.held, amount("0.0"));
    }

    #[test]
    fn test_double_dispute_ignored() {
        let ledger = run_input(
            "type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\ndispute,1,1,\n",
        );
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.available, amount("0.0"));
        assert_eq!(account.held, amount("100.0"));
    }

    #[test]
    fn test_locked_account_rejects_withdrawal() {
        let ledger = run_input(
            "type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nchargeback,1,1,\ndeposit,1,2,50.0\nwithdrawal,1,3,10.0\n",
        );
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.total(), amount("0.0"));
        assert!(account.locked);
    }

    #[test]
    fn test_client_mismatch_on_resolve_ignored() {
        let ledger = run_input(
            "type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nresolve,2,1,\n",
        );
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.held, amount("100.0"));
        assert_eq!(account.available, amount("0.0"));
    }

    #[test]
    fn test_client_mismatch_on_chargeback_ignored() {
        let ledger = run_input(
            "type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nchargeback,2,1,\n",
        );
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.held, amount("100.0"));
        assert!(!account.locked);
    }

    #[test]
    fn test_chargeback_then_resolve_ignored() {
        let ledger = run_input(
            "type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nchargeback,1,1,\nresolve,1,1,\n",
        );
        let account = ledger.accounts.get(&1).unwrap();
        assert_eq!(account.total(), amount("0.0"));
        assert!(account.locked);
    }

    #[test]
    fn test_write_output_correct_csv() {
        let mut output = Vec::new();
        let input = b"type,client,tx,amount\ndeposit,1,1,10.5000\n";
        process_csv(std::io::Cursor::new(input), &mut output).unwrap();
        let csv = String::from_utf8(output).unwrap();
        assert!(csv.contains("client"));
        assert!(csv.contains("available"));
        assert!(csv.contains("10.5000"));
        assert!(csv.contains("false"));
    }
}