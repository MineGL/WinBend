//! Project and donation links shown in the tray menu and the settings window.
//! Fill these in before publishing (see README → Publishing checklist).

/// Source repository.
pub const GITHUB_URL: &str = "https://github.com/YOUR_GITHUB_USER/winbend";

/// Primary donation page (GitHub Sponsors pays out through Stripe).
pub const DONATE_URL: &str = "https://github.com/sponsors/YOUR_GITHUB_USER";

/// Crypto wallets listed on the About page with a copy button. Leave empty to hide the section.
pub struct Wallet {
    /// Coin and network, e.g. "Bitcoin", "Ethereum (ERC-20)", "USDT (TRC-20)".
    pub name: &'static str,
    pub address: &'static str,
}

pub const WALLETS: &[Wallet] = &[
    // Wallet { name: "Bitcoin", address: "bc1..." },
    // Wallet { name: "Ethereum (ERC-20)", address: "0x..." },
];
