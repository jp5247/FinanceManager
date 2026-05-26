//! Default rule set tuned for common Indian-banking transaction patterns.
//!
//! Rules are organised by category band; higher priority numbers match
//! first. Specific merchant rules (e.g. Swiggy, Zomato) sit above generic
//! catch-alls (e.g. "UPI Transfer"). When in doubt, prefer a specific
//! merchant rule at a higher priority than a regex catch-all.

use crate::rule::{contains_rule, regex_rule, Rule, RuleSet};

pub fn default_rules() -> RuleSet {
    RuleSet::new(rules())
}

fn rules() -> Vec<Rule> {
    vec![
        // --- Income (highest priority, very specific patterns) ---
        regex_rule(
            "income/salary",
            1000,
            r"(?i)\b(salary|payroll|sal\s+cr)\b",
            "Salary",
        ),
        regex_rule(
            "income/interest",
            950,
            r"(?i)interest\s+(credit|cr)",
            "Interest",
        ),
        regex_rule("income/dividend", 950, r"(?i)\bdividend\b", "Dividend"),
        regex_rule(
            "income/refund",
            450,
            r"(?i)\b(refund|rfnd|reversal)\b",
            "Refund",
        ),
        // --- Housing ---
        regex_rule(
            "housing/rent",
            900,
            r"(?i)(\bUPI/RENT\b|\brent\s+payment\b|\bhouse\s+rent\b)",
            "Rent",
        ),
        regex_rule(
            "housing/maintenance",
            900,
            r"(?i)\b(society\s+maintenance|maintenance\s+charge)\b",
            "Maintenance",
        ),
        // --- Utilities ---
        regex_rule(
            "utilities/electricity",
            850,
            r"(?i)\b(BEST|MSEB|MAHADISCOM|TPCL|tata\s+power|torrent\s+power|adani\s+electric|electricity\s+bill)\b",
            "Electricity",
        ),
        regex_rule(
            "utilities/gas",
            850,
            r"(?i)\b(MGL|IGL|adani\s+gas|HPCL\s+gas|gas\s+bill)\b",
            "Gas",
        ),
        contains_rule("utilities/water", 850, "water bill", "Water"),
        // --- Bills ---
        regex_rule(
            "bills/mobile",
            800,
            r"(?i)\b(airtel|jio|vodafone\s+idea|reliance\s+jio|bsnl|VI\s+mobile)\b",
            "Mobile",
        ),
        regex_rule(
            "bills/internet",
            800,
            r"(?i)\b(act\s+fibernet|hathway|spectra|you\s+broadband|tikona|excitel)\b",
            "Internet",
        ),
        regex_rule(
            "bills/insurance",
            800,
            r"(?i)\b(insurance\s+premium|LIC\s+premium|\bHDFC\s+Life\b|policybazaar)\b",
            "Insurance",
        ),
        regex_rule(
            "bills/billpay",
            550,
            r"(?i)\b(billpay|billdesk|bharat\s+billpay)\b",
            "Bills",
        ),
        // --- Food delivery & groceries ---
        // Compound first: "Swiggy Instamart" is the groceries service, even
        // though "swiggy" alone is food delivery. Match either space or
        // hyphen between the words because real narrations use both.
        regex_rule(
            "food/swiggy-instamart",
            800,
            r"(?i)swiggy[\s\-]+instamart",
            "Groceries",
        ),
        contains_rule("food/swiggy", 750, "swiggy", "Food Delivery"),
        contains_rule("food/zomato", 750, "zomato", "Food Delivery"),
        contains_rule("food/instamart", 750, "instamart", "Groceries"),
        contains_rule("food/blinkit", 750, "blinkit", "Groceries"),
        contains_rule("food/zepto", 750, "zepto", "Groceries"),
        contains_rule("food/bigbasket", 750, "bigbasket", "Groceries"),
        regex_rule(
            "food/dmart",
            750,
            r"(?i)\b(dmart|d-mart|d\.mart)\b",
            "Groceries",
        ),
        // --- Transport ---
        regex_rule("transport/uber", 700, r"(?i)\buber\b", "Cab/Ride"),
        regex_rule(
            "transport/ola",
            700,
            r"(?i)\bola\s+(cabs|money|electric|outstation)\b",
            "Cab/Ride",
        ),
        contains_rule("transport/rapido", 700, "rapido", "Cab/Ride"),
        regex_rule(
            "transport/train",
            700,
            r"(?i)\b(irctc|indian\s+railway|\brailway\b)\b",
            "Train Travel",
        ),
        regex_rule(
            "transport/fuel",
            700,
            r"(?i)\b(HPCL|IOC(L|\s)|BPCL|indianoil|indian\s+oil|hindustan\s+petroleum|bharat\s+petroleum|shell\s+(india|fuel)|reliance\s+petroleum)\b",
            "Fuel",
        ),
        regex_rule(
            "transport/airline",
            700,
            r"(?i)\b(indigo|spicejet|vistara|air\s+india|akasa|goair)\b",
            "Air Travel",
        ),
        // --- Shopping ---
        regex_rule(
            "shopping/amazon",
            700,
            r"(?i)\b(amazon|AMZN)\b",
            "Online Shopping",
        ),
        contains_rule("shopping/flipkart", 700, "flipkart", "Online Shopping"),
        contains_rule("shopping/myntra", 700, "myntra", "Online Shopping"),
        contains_rule("shopping/ajio", 700, "ajio", "Online Shopping"),
        contains_rule("shopping/meesho", 700, "meesho", "Online Shopping"),
        contains_rule("shopping/tata-cliq", 700, "tata cliq", "Online Shopping"),
        // --- Investments ---
        contains_rule("invest/upstox", 650, "upstox", "Investments"),
        contains_rule("invest/zerodha", 650, "zerodha", "Investments"),
        contains_rule("invest/groww", 650, "groww", "Investments"),
        contains_rule("invest/indmoney", 650, "indmoney", "Investments"),
        contains_rule("invest/kuvera", 650, "kuvera", "Investments"),
        contains_rule("invest/scripbox", 650, "scripbox", "Investments"),
        regex_rule(
            "invest/sip",
            650,
            r"(?i)\b(mutual\s+fund|SIP\s+(payment|credit|debit))\b",
            "Investments",
        ),
        // --- Loans / credit ---
        regex_rule(
            "loans/emi",
            600,
            r"(?i)(EMI[\s,]+PRIN|\bEMI\s+payment|loan\s+emi|OFFUS\s+EMI)",
            "Loan EMI",
        ),
        regex_rule(
            "loans/cc-payment",
            600,
            r"(?i)(CRED\s+CLUB|CC\s+PAYMENT|credit\s+card\s+payment|PAYMENT\s+ON\s+CRED)",
            "Credit Card Payment",
        ),
        // --- Tax / Govt ---
        regex_rule(
            "govt/tax",
            400,
            r"(?i)\b(income\s+tax|TDS|GST|IGST|advance\s+tax)\b",
            "Tax",
        ),
        // --- Cash ---
        regex_rule(
            "cash/atm",
            500,
            r"(?i)(ATM\s+(WDL|CASH|CW)|cash\s+(withdrawal|deposit))",
            "ATM / Cash",
        ),
        // --- Lowest-priority generic fallbacks ---
        contains_rule("upi/generic", 100, "UPI-", "UPI Transfer"),
        contains_rule("upi/generic-2", 100, "UPI/", "UPI Transfer"),
        regex_rule(
            "ach/generic",
            100,
            r"(?i)\b(ACH\s+(D|CR)\b|NACH\d|ECS)",
            "Bank Transfer",
        ),
    ]
}
