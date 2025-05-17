use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct Entry {
    pub password: SecretString,
    /// Remaining lines (metadata) are kept verbatim to preserve compatibility.
    pub extra: Vec<SecretString>,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            password: SecretString::new("".into()),
            extra: vec![],
        }
    }
}

impl ToString for Entry {
    fn to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.password.expose_secret());
        out.push('\n');
        for l in &self.extra {
            out.push_str(l.expose_secret());
            out.push('\n');
        }
        out
    }
}

impl Entry {
    pub fn from_secret(secret: impl ExposeSecret<str>) -> Self {
        let mut lines: std::str::Lines<'_> = secret.expose_secret().lines();
        let password = SecretString::from(lines.next().unwrap_or_default().to_string());
        let extra = lines.map(|l| SecretString::from(l.to_string())).collect();
        Self { password, extra }
    }
}
