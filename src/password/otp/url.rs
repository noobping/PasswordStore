use std::time::{SystemTime, UNIX_EPOCH};
use totp_rs::TOTP;
use url::Url;

const DEFAULT_OTP_PERIOD: u64 = 30;

pub(super) fn otp_display(url: &str) -> Result<(String, u64, u64), String> {
    let normalized_url = normalized_otp_url(url)?;
    let totp = TOTP::from_url_unchecked(&normalized_url).map_err(|err| err.to_string())?;
    let period = otp_period(&normalized_url);
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
    query_param(url, "secret").map(|secret| normalize_otp_secret(&secret))
}

pub(super) fn replace_otp_secret(url: &str, secret: &str) -> String {
    let normalized_secret = normalize_otp_secret(secret);
    if let Ok(mut parsed) = Url::parse(url.trim()) {
        let mut found_secret = false;
        let mut pairs = parsed
            .query_pairs()
            .map(|(key, value)| {
                if key.eq_ignore_ascii_case("secret") {
                    found_secret = true;
                    (key.into_owned(), normalized_secret.clone())
                } else {
                    (key.into_owned(), value.into_owned())
                }
            })
            .collect::<Vec<_>>();

        if !found_secret {
            pairs.push(("secret".to_string(), normalized_secret));
        }

        {
            let mut query = parsed.query_pairs_mut();
            query.clear();
            query.extend_pairs(
                pairs
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            );
        }

        return parsed.into();
    }

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
                    return format!("{key}={normalized_secret}");
                }
            }
            part.to_string()
        })
        .collect::<Vec<_>>();

    if !found_secret {
        parts.push(format!("secret={normalized_secret}"));
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
    let parsed = Url::parse(url.trim()).ok()?;
    parsed.query_pairs().find_map(|(current_key, value)| {
        current_key
            .eq_ignore_ascii_case(key)
            .then(|| value.into_owned())
    })
}

fn normalized_otp_url(url: &str) -> Result<String, String> {
    let mut parsed = Url::parse(url.trim()).map_err(|err| err.to_string())?;
    let mut found_secret = false;
    let pairs = parsed
        .query_pairs()
        .map(|(key, value)| {
            if key.eq_ignore_ascii_case("secret") {
                found_secret = true;
                (key.into_owned(), normalize_otp_secret(&value))
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect::<Vec<_>>();

    if !found_secret {
        return Ok(parsed.into());
    }

    {
        let mut query = parsed.query_pairs_mut();
        query.clear();
        query.extend_pairs(
            pairs
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
    }

    Ok(parsed.into())
}

fn normalize_otp_secret(secret: &str) -> String {
    let without_spacing = secret
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    without_spacing.trim_end_matches('=').to_ascii_uppercase()
}
