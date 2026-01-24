//! Price validation (§10.3).
//!
//! This module implements price validation against protocol constraints.

use nodalync_types::{Amount, MAX_PRICE, MIN_PRICE};

use crate::error::{EconError, EconResult};

/// Validate that a price is within protocol constraints.
///
/// # Arguments
/// * `price` - The price to validate (in smallest unit, 10^-8 NDL)
///
/// # Returns
/// * `Ok(())` if the price is valid
/// * `Err(EconError::PriceTooLow)` if price < MIN_PRICE
/// * `Err(EconError::PriceTooHigh)` if price > MAX_PRICE
///
/// # Example
/// ```
/// use nodalync_econ::validate_price;
///
/// assert!(validate_price(100).is_ok());
/// assert!(validate_price(0).is_err());
/// ```
pub fn validate_price(price: Amount) -> EconResult<()> {
    if price < MIN_PRICE {
        return Err(EconError::PriceTooLow {
            price,
            min: MIN_PRICE,
        });
    }
    if price > MAX_PRICE {
        return Err(EconError::PriceTooHigh {
            price,
            max: MAX_PRICE,
        });
    }
    Ok(())
}

/// Check if a price is within valid range without returning an error.
///
/// # Arguments
/// * `price` - The price to check
///
/// # Returns
/// `true` if MIN_PRICE <= price <= MAX_PRICE
pub fn is_valid_price(price: Amount) -> bool {
    (MIN_PRICE..=MAX_PRICE).contains(&price)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_price() {
        // Normal valid prices
        assert!(validate_price(1).is_ok());
        assert!(validate_price(100).is_ok());
        assert!(validate_price(1_000_000).is_ok());
        assert!(validate_price(100_000_000).is_ok()); // 1 NDL
    }

    #[test]
    fn test_min_price() {
        // Exactly at MIN_PRICE should be valid
        assert!(validate_price(MIN_PRICE).is_ok());
    }

    #[test]
    fn test_max_price() {
        // Exactly at MAX_PRICE should be valid
        assert!(validate_price(MAX_PRICE).is_ok());
    }

    #[test]
    fn test_price_too_low() {
        // Price below MIN_PRICE (which is 1)
        let result = validate_price(0);
        assert!(result.is_err());

        match result {
            Err(EconError::PriceTooLow { price, min }) => {
                assert_eq!(price, 0);
                assert_eq!(min, MIN_PRICE);
            }
            _ => panic!("Expected PriceTooLow error"),
        }
    }

    #[test]
    fn test_price_too_high() {
        // Price above MAX_PRICE
        let too_high = MAX_PRICE + 1;
        let result = validate_price(too_high);
        assert!(result.is_err());

        match result {
            Err(EconError::PriceTooHigh { price, max }) => {
                assert_eq!(price, too_high);
                assert_eq!(max, MAX_PRICE);
            }
            _ => panic!("Expected PriceTooHigh error"),
        }
    }

    #[test]
    fn test_is_valid_price() {
        assert!(is_valid_price(MIN_PRICE));
        assert!(is_valid_price(MAX_PRICE));
        assert!(is_valid_price(100));
        assert!(!is_valid_price(0));
        assert!(!is_valid_price(MAX_PRICE + 1));
    }
}
