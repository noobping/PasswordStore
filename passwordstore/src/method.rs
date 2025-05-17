#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Passphrase,
    Pinantry,
}

impl Default for Method {
    fn default() -> Self {
        Self::Passphrase
    }
}
