use crate::logging::log_error;
use crate::pass_file::{structured_otp_line, OtpFieldTemplate, StructuredPassLine};
use adw::glib::{self, ControlFlow};
use adw::prelude::*;
use adw::{PasswordEntryRow, Toast, ToastOverlay};
use adw::gtk::{Align, Button, ProgressBar};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use totp_rs::TOTP;

const DEFAULT_OTP_PERIOD: u64 = 30;

#[derive(Clone)]
pub(crate) struct PasswordOtpState {
    pub(crate) row: PasswordEntryRow,
    overlay: ToastOverlay,
    template: Rc<RefCell<Option<OtpFieldTemplate>>>,
    url: Rc<RefCell<Option<String>>>,
    edit_mode: Rc<Cell<bool>>,
    refresh_generation: Rc<Cell<u64>>,
    toggle_button: Button,
    progress: ProgressBar,
}

impl PasswordOtpState {
    pub(crate) fn new(row: &PasswordEntryRow, overlay: &ToastOverlay) -> Self {
        let toggle_button = Button::from_icon_name("document-edit-symbolic");
        toggle_button.add_css_class("flat");
        toggle_button.set_tooltip_text(Some("Edit OTP secret"));

        let progress = ProgressBar::new();
        progress.set_valign(Align::Center);
        progress.set_halign(Align::Center);
        progress.set_show_text(false);
        progress.set_width_request(54);
        progress.set_visible(false);

        row.add_suffix(&progress);
        row.add_suffix(&toggle_button);

        let state = Self {
            row: row.clone(),
            overlay: overlay.clone(),
            template: Rc::new(RefCell::new(None)),
            url: Rc::new(RefCell::new(None)),
            edit_mode: Rc::new(Cell::new(false)),
            refresh_generation: Rc::new(Cell::new(0)),
            toggle_button,
            progress,
        };
        state.connect_toggle_button();
        state
    }

    pub(crate) fn clear(&self) {
        self.bump_refresh_generation();
        self.template.borrow_mut().take();
        self.url.borrow_mut().take();
        self.edit_mode.set(false);
        self.row.set_title("OTP");
        self.row.set_text("");
        self.row.set_editable(false);
        self.row.set_visible(false);
        self.toggle_button.set_visible(false);
        self.toggle_button
            .set_icon_name("document-edit-symbolic");
        self.toggle_button
            .set_tooltip_text(Some("Edit OTP secret"));
        self.progress.set_visible(false);
        self.progress.set_fraction(0.0);
        self.progress.set_tooltip_text(None);
    }

    pub(crate) fn sync_from_parsed_lines(
        &self,
        lines: &[(StructuredPassLine, Option<String>)],
        show_errors: bool,
    ) {
        self.bump_refresh_generation();

        let Some((template, url)) = structured_otp_line(lines) else {
            self.clear();
            return;
        };

        *self.template.borrow_mut() = Some(template);
        *self.url.borrow_mut() = Some(url);
        self.row.set_visible(true);
        self.toggle_button.set_visible(true);
        self.render(show_errors);
    }

    pub(crate) fn current_url(&self) -> Option<String> {
        if self.edit_mode.get() {
            self.url_for_current_secret()
        } else {
            self.url.borrow().clone()
        }
    }

    pub(crate) fn current_url_for_save(&self) -> Result<Option<String>, &'static str> {
        let Some(url) = self.current_url() else {
            return Ok(None);
        };

        if self.has_otp() && otp_secret_from_url(&url).unwrap_or_default().trim().is_empty() {
            return Err("Enter an OTP secret.");
        }

        Ok(Some(url))
    }

    fn connect_toggle_button(&self) {
        let state = self.clone();
        self.toggle_button.connect_clicked(move |_| {
            if state.edit_mode.get() {
                let Some(url) = state.url_for_current_secret() else {
                    state.overlay.add_toast(Toast::new("Couldn't update the code."));
                    return;
                };

                if otp_secret_from_url(&url)
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    state.overlay.add_toast(Toast::new("Enter an OTP secret."));
                    return;
                }

                *state.url.borrow_mut() = Some(url);
                state.edit_mode.set(false);
            } else {
                state.edit_mode.set(true);
            }

            state.render(true);
        });
    }

    fn has_otp(&self) -> bool {
        self.template.borrow().is_some()
    }

    fn url_for_current_secret(&self) -> Option<String> {
        let current_url = self.url.borrow().clone()?;
        Some(replace_otp_secret(&current_url, &self.row.text()))
    }

    fn bump_refresh_generation(&self) -> u64 {
        let next = self.refresh_generation.get().wrapping_add(1);
        self.refresh_generation.set(next);
        next
    }

    fn render(&self, show_errors: bool) {
        if self.edit_mode.get() {
            self.render_edit_mode();
        } else {
            self.render_live_mode(show_errors);
        }
    }

    fn render_edit_mode(&self) {
        let secret = self
            .url
            .borrow()
            .as_deref()
            .and_then(otp_secret_from_url)
            .unwrap_or_default();
        self.row.set_title("OTP secret");
        self.row.set_editable(true);
        self.row.set_text(&secret);
        self.progress.set_visible(false);
        self.toggle_button
            .set_icon_name("object-select-symbolic");
        self.toggle_button.set_tooltip_text(Some("Show live code"));
    }

    fn render_live_mode(&self, show_errors: bool) {
        let Some(url) = self.url.borrow().clone() else {
            self.clear();
            return;
        };

        self.row.set_title("OTP code");
        self.row.set_editable(false);
        self.toggle_button
            .set_icon_name("document-edit-symbolic");
        self.toggle_button
            .set_tooltip_text(Some("Edit OTP secret"));
        self.progress.set_visible(true);

        match otp_display(&url) {
            Ok((code, remaining, period)) => {
                self.row.set_text(&code);
                self.progress
                    .set_fraction(remaining as f64 / period as f64);
                self.progress
                    .set_tooltip_text(Some(&format!("{remaining}s remaining")));
                self.start_live_refresh();
            }
            Err(err) => {
                log_error(format!("Failed to render OTP code: {err}"));
                self.row.set_text("");
                self.progress.set_fraction(0.0);
                self.progress.set_tooltip_text(None);
                if show_errors {
                    self.overlay.add_toast(Toast::new("Couldn't load the code."));
                }
            }
        }
    }

    fn start_live_refresh(&self) {
        let generation = self.bump_refresh_generation();
        let state = self.clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            if state.refresh_generation.get() != generation {
                return ControlFlow::Break;
            }
            if state.edit_mode.get() || !state.row.is_visible() {
                return ControlFlow::Break;
            }

            let Some(url) = state.url.borrow().clone() else {
                return ControlFlow::Break;
            };

            match otp_display(&url) {
                Ok((code, remaining, period)) => {
                    state.row.set_text(&code);
                    state
                        .progress
                        .set_fraction(remaining as f64 / period as f64);
                    state
                        .progress
                        .set_tooltip_text(Some(&format!("{remaining}s remaining")));
                    ControlFlow::Continue
                }
                Err(err) => {
                    log_error(format!("Failed to refresh OTP code: {err}"));
                    state.row.set_text("");
                    state.progress.set_fraction(0.0);
                    state.progress.set_tooltip_text(None);
                    ControlFlow::Break
                }
            }
        });
    }
}

fn otp_display(url: &str) -> Result<(String, u64, u64), String> {
    let totp = TOTP::from_url_unchecked(url).map_err(|err| err.to_string())?;
    let period = otp_period(url);
    let remaining = otp_remaining_seconds(period);
    let code = totp.generate_current().map_err(|err| err.to_string())?;
    Ok((code, remaining, period))
}

fn otp_period(url: &str) -> u64 {
    query_param(url, "period")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|period| *period > 0)
        .unwrap_or(DEFAULT_OTP_PERIOD)
}

fn otp_remaining_seconds(period: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let remaining = period - (now % period);
    if remaining == 0 { period } else { remaining }
}

fn otp_secret_from_url(url: &str) -> Option<String> {
    query_param(url, "secret")
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

fn replace_otp_secret(url: &str, secret: &str) -> String {
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
            if let Some((key, _value)) = part.split_once('=') {
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

#[cfg(test)]
mod tests {
    use super::{otp_period, otp_secret_from_url, replace_otp_secret};

    #[test]
    fn otp_secret_is_read_from_otpauth_url() {
        assert_eq!(
            otp_secret_from_url("otpauth://totp/Test?secret=ABC123&period=45"),
            Some("ABC123".to_string())
        );
    }

    #[test]
    fn otp_secret_is_replaced_without_touching_other_query_values() {
        assert_eq!(
            replace_otp_secret(
                "otpauth://totp/Test?issuer=Example&secret=OLD&period=45",
                "NEW"
            ),
            "otpauth://totp/Test?issuer=Example&secret=NEW&period=45".to_string()
        );
    }

    #[test]
    fn otp_period_defaults_to_thirty_seconds() {
        assert_eq!(otp_period("otpauth://totp/Test?secret=ABC123"), 30);
    }
}
