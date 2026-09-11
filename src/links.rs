//! Project and donation links shown in the tray menu and the settings window.

/// Project website.
pub const SITE_URL: &str = "https://winbend.me";

/// Source repository.
pub const GITHUB_URL: &str = "https://github.com/MineGL/winbend";

/// Primary donation page (GitHub Sponsors pays out through Stripe).
pub const DONATE_URL: &str = "https://github.com/sponsors/MineGL";

/// Crypto wallets listed on the About page with a copy button. Leave empty to hide the section.
pub struct Wallet {
    /// Coin and network, e.g. "Bitcoin", "Ethereum (ERC-20)", "USDT (TRC-20)".
    pub name: &'static str,
    pub address: &'static str,
}

pub const WALLETS: &[Wallet] = &[
    Wallet { name: "Ethereum (ERC-20)", address: "0xf177d181a287f43e45b14235640693d5daac125a" },
    Wallet { name: "BNB Smart Chain (BEP-20)", address: "0xf177d181a287f43e45b14235640693d5daac125a" },
    Wallet { name: "Tron (TRC-20)", address: "TWMBxrG7ERvYYejd2PMuLJS3jnjQCrArAx" },
];
