use std::time::{SystemTime, UNIX_EPOCH};
use totp_rs::TOTP;

const DEFAULT_OTP_PERIOD: u64 = 30;

pub(super) fn otp_display(url: &str) -> Result<(String, u64, u64), String> {
    let totp = TOTP::from_url_unchecked(url).map_err(|err| err.to_string())?;
    let period = otp_period(url);
    let remaining = otp_remaining_seconds(period);
    let code = totp.generate_current().map_err(|err| err.to_string())?;
    Ok((code, remaining, period))
}

pub(super) fn otp_period(url: &str) -> u64 {
    query_param(url, "period")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|period| *period > 0)
        .unwrap_or(DEFAULT_OTP_PERIOD)
}

pub(super) fn otp_secret_from_url(url: &str) -> Option<String> {
    query_param(url, "secret")
}

pub(super) fn replace_otp_secret(url: &str, secret: &str) -> String {
    let (without_fragment, fragment) = match url.split_once('#') {
        Some((prefix, fragment)) => (prefix, Some(fragment)),
        None => (url, None),
    };
    let (base, query) = match without_fragment.split_once('?') {
        Some((base, query)) => (base, query),
        None => (without_fragment, ""),
    };

    let mut found_secret = false;
    let mut parts = query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if let Some((key, _)) = part.split_once('=') {
                if key.eq_ignore_ascii_case("secret") {
                    found_secret = true;
                    return format!("{key}={secret}");
                }
            }
            part.to_string()
        })
        .collect::<Vec<_>>();

    if !found_secret {
        parts.push(format!("secret={secret}"));
    }

    let mut rebuilt = if parts.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", parts.join("&"))
    };
    if let Some(fragment) = fragment {
        rebuilt.push('#');
        rebuilt.push_str(fragment);
    }
    rebuilt
}

fn otp_remaining_seconds(period: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let remaining = period - (now % period);
    if remaining == 0 {
        period
    } else {
        remaining
    }
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next().unwrap_or_default();
    query.split('&').find_map(|pair| {
        let (current_key, value) = pair.split_once('=')?;
        if current_key.eq_ignore_ascii_case(key) {
            Some(value.to_string())
        } else {
            None
        }
    })
}
