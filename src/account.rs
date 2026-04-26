use serde::Serialize;

use crate::amount::Amount;
use crate::error::TransactionError;

/// Represents a client account with available funds, held funds, and lock status.
/// 
/// Tracks the financial state of a single client including available balance,
/// funds held in disputes, and whether the account is locked due to chargebacks.
#[derive(Debug, Clone)]
pub struct Account {
    pub client: u16,
    pub available: Amount,
    pub held: Amount,
    pub locked: bool,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Self { client, available: Amount::ZERO, held: Amount::ZERO, locked: false }
    }

    /// Returns the total funds (available + held) in the account.
    pub fn total(&self) -> Amount {
        self.available
            .checked_add(self.held)
            .unwrap_or(Amount::ZERO)
    }

    /// Adds funds to the available balance if account is not locked.
    pub fn deposit(&mut self, amount: Amount, tx: u32) -> Result<(), TransactionError> {
        if self.locked {
            return Err(TransactionError::AccountLocked(self.client));
        }

        self.available = self.available
            .checked_add(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;

        Ok(())
    }

    /// Removes funds from available balance if sufficient funds and account not locked.
    pub fn withdraw(&mut self, amount: Amount, tx: u32) -> Result<(), TransactionError> {
        if self.locked {
            return Err(TransactionError::AccountLocked(self.client));
        }

        if self.available < amount {
            return Err(TransactionError::InsufficientAmount { client: self.client, tx });
        }

        self.available = self.available
            .checked_sub(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;

        Ok(())
    }

    /// Moves funds from available to held status for disputed transactions.
    /// Simulates ATM/bank case - does not count held money as available and rejects.
    pub fn hold(&mut self, amount: Amount, tx: u32) -> Result<(), TransactionError> {
        if self.available < amount {
            return Err(TransactionError::InsufficientAmount {client: self.client, tx });
        }

        self.available = self.available
                .checked_sub(amount)
                .ok_or(TransactionError::NumericOverflow(tx))?;
        self.held = self.held
            .checked_add(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;

        Ok(())
    }

    /// Moves funds from held back to available when dispute is resolved.
    pub fn release(&mut self, amount: Amount, tx: u32) -> Result<(), TransactionError> {
        self.held = self.held
            .checked_sub(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;
        self.available = self.available
            .checked_add(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;

        Ok(())
    }

    /// Removes held funds and locks the account due to chargeback.
    pub fn chargeback(&mut self, amount: Amount, tx: u32) -> Result<(), TransactionError> {
        self.held = self.held
            .checked_sub(amount)
            .ok_or(TransactionError::NumericOverflow(tx))?;
        self.locked = true;

        Ok(())
    }
}

/// Serializable version of Account for CSV output with calculated total.
#[derive(Debug, Serialize)]
pub struct SerializedAccount {
    pub client: u16,
    pub available: Amount,
    pub held: Amount,
    pub total: Amount,
    pub locked: bool,
}

impl From<&Account> for SerializedAccount {
    fn from(a: &Account) -> Self {
        Self {
            client: a.client,
            available: a.available,
            held: a.held,
            total: a.total(),
            locked: a.locked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_amount(s: &str) -> Amount { s.parse().unwrap() }

    #[test]
    fn test_locked_account_rejects_withdrawal() {
        let mut account = Account::new(1);
        account.available = str_amount("100.0");
        account.locked = true;
        assert!(matches!(account.withdraw(str_amount("10.0"), 1), Err(TransactionError::AccountLocked(1))));
        assert_eq!(account.available, str_amount("100.0"));
    }

    #[test]
    fn test_hold_moves_available_to_held() {
        let mut account = Account::new(1);
        account.available = str_amount("100.0");
        account.hold(str_amount("40.0"), 1).unwrap();
        assert_eq!(account.available, str_amount("60.0"));
        assert_eq!(account.held, str_amount("40.0"));
        assert_eq!(account.total(), str_amount("100.0"));
    }

    #[test]
    fn test_release_moves_held_to_available() {
        let mut account = Account::new(1);
        account.available = str_amount("60.0");
        account.held = str_amount("40.0");
        account.release(str_amount("40.0"), 1).unwrap();
        assert_eq!(account.available, str_amount("100.0"));
        assert_eq!(account.held, str_amount("0.0"));
    }

    #[test]
    fn test_chargeback_removes_held_and_locks() {
        let mut account = Account::new(1);
        account.held = str_amount("50.0");
        account.chargeback(str_amount("50.0"), 1).unwrap();
        assert_eq!(account.held, str_amount("0.0"));
        assert!(account.locked);
    }

    #[test]
    fn test_hold_prevents_negative_available_balance() {
        let mut account = Account::new(1);
        account.available = str_amount("30.0");
        account.held = str_amount("20.0");
        
        // Try to hold more than available - should fail
        let result = account.hold(str_amount("50.0"), 1);
        assert!(matches!(result, Err(TransactionError::InsufficientAmount { client: 1, tx: 1 })));
        
        // Account state should remain unchanged
        assert_eq!(account.available, str_amount("30.0"));
        assert_eq!(account.held, str_amount("20.0"));
        
        // Holding exactly available amount should work
        account.hold(str_amount("30.0"), 2).unwrap();
        assert_eq!(account.available, str_amount("0.0"));
        assert_eq!(account.held, str_amount("50.0"));
    }

    #[test]
    fn test_total_is_available_plus_held() {
        let mut account = Account::new(1);
        account.available = str_amount("30.0");
        account.held = str_amount("70.0");
        assert_eq!(account.total(), str_amount("100.0"));
    }
}
