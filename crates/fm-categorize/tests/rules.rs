//! Categorization tests against real-shape transaction descriptions
//! (synthetic merchants only — no PII).

use fm_categorize::{categorize, default_rules};

fn cat(desc: &str) -> Option<String> {
    categorize(&default_rules(), desc).map(|h| h.category)
}

#[test]
fn salary_credit() {
    assert_eq!(
        cat("NEFT CR-SCBL0036001-EMPLOYER PVT LTD-JAI-SALARY APR 2026"),
        Some("Salary".into())
    );
    assert_eq!(cat("PAYROLL CR FROM ACME CORP"), Some("Salary".into()));
}

#[test]
fn interest_credit() {
    assert_eq!(cat("INTEREST CREDIT BY HDFC BANK"), Some("Interest".into()));
    assert_eq!(cat("Savings interest cr"), Some("Interest".into()));
}

#[test]
fn food_delivery_via_upi() {
    assert_eq!(
        cat("UPI-SWIGGY INSTAMART-639203@OKAXIS-...-...-PAID VIA CRED"),
        Some("Groceries".into())
    );
    assert_eq!(
        cat("UPI-ZOMATO LTD-zomato.payments@ybl"),
        Some("Food Delivery".into())
    );
}

#[test]
fn rent_payment() {
    assert_eq!(cat("UPI/RENT/APR2026"), Some("Rent".into()));
    assert_eq!(cat("RENT PAYMENT TO LANDLORD"), Some("Rent".into()));
}

#[test]
fn fuel() {
    assert_eq!(
        cat("UPI-HPCL PETROL PUMP-HPCL@HDFCBANK"),
        Some("Fuel".into())
    );
    assert_eq!(cat("IOC FUEL STATION CHEMBUR"), Some("Fuel".into()));
}

#[test]
fn cab_ride() {
    assert_eq!(
        cat("UPI-UBER INDIA SYSTEMS P-UBER1.RZP@HDFCBANK-UBERRIDE"),
        Some("Cab/Ride".into())
    );
    // "ola" alone shouldn't catch random merchants like "Solapur".
    // We don't catch this as anything specific — it falls to UPI Transfer
    // because the description has the realistic "UPI-" prefix.
    assert_eq!(
        cat("UPI-SOLAPUR FRUITS-MERCHANT"),
        Some("UPI Transfer".into())
    );
    assert_eq!(cat("OLA CABS BOOKING"), Some("Cab/Ride".into()));
    assert_eq!(cat("RAPIDO BIKE BOOKING"), Some("Cab/Ride".into()));
}

#[test]
fn online_shopping() {
    assert_eq!(
        cat("UPI-AMAZON SELLER SERVICES"),
        Some("Online Shopping".into())
    );
    assert_eq!(cat("UPI-FLIPKART INTERNET"), Some("Online Shopping".into()));
}

#[test]
fn cc_payment_and_emi() {
    assert_eq!(
        cat("BPPY CC PAYMENT DP016124192045 PAYMENT ON CRED"),
        Some("Credit Card Payment".into())
    );
    assert_eq!(
        cat("OFFUS EMI,PRIN NB:02,00000138162352"),
        Some("Loan EMI".into())
    );
}

#[test]
fn investments() {
    assert_eq!(
        cat("UPI-UPSTOX SECURITIES PV-UPSTOX.BRK@VALIDHDFC"),
        Some("Investments".into())
    );
    assert_eq!(
        cat("UPI-FINZOOM INVESTMENT A-INDMONEY3.PAYU@"),
        Some("Investments".into())
    );
}

#[test]
fn atm_cash() {
    assert_eq!(
        cat("ATM WDL CARD 1234 AT BRANCH"),
        Some("ATM / Cash".into())
    );
    assert_eq!(
        cat("CASH DEPOSIT BY - SELF - MIRA ROAD"),
        Some("ATM / Cash".into())
    );
}

#[test]
fn ach_generic() {
    assert_eq!(
        cat("ACH D- INDIAN CLEARING CORP-D6800438X028"),
        Some("Bank Transfer".into())
    );
    assert_eq!(
        cat("CEMTEX DEP ACHCr NACH00000000021008 INDIAN ENERGY"),
        Some("Bank Transfer".into())
    );
}

#[test]
fn upi_to_person_falls_through_to_generic() {
    // No specific merchant match → catch-all.
    assert_eq!(
        cat("UPI-MEETALI PRAVIN PATEL-PMMEETALIPATEL"),
        Some("UPI Transfer".into())
    );
}

#[test]
fn completely_unknown_returns_none() {
    assert_eq!(cat("RANDOMTHING WITHOUT ANY KEYWORD"), None);
    assert_eq!(cat(""), None);
}

#[test]
fn rule_set_is_sorted_by_priority_desc() {
    let rs = default_rules();
    let mut prev = i32::MAX;
    for r in &rs.rules {
        assert!(
            r.priority <= prev,
            "rule {:?} priority {} should be <= prev {}",
            r.id,
            r.priority,
            prev
        );
        prev = r.priority;
    }
}

#[test]
fn higher_priority_wins_over_lower() {
    let h = categorize(&default_rules(), "UPI-SWIGGY-INSTAMART").unwrap();
    // Specific food/instamart rule (priority 750) should win over the
    // generic upi/generic catch-all (priority 100).
    assert_eq!(h.category, "Groceries");
    assert_ne!(h.rule_id, "upi/generic");
}
