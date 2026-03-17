mod countdown;
mod url;

use self::countdown::OtpCountdownCircle;
use self::url::{otp_display, otp_secret_from_url, replace_otp_secret};
use super::file::{structured_otp_line, OtpFieldTemplate, StructuredPassLine};
use crate::logging::log_error;
use adw::glib::{self, ControlFlow};
use adw::gtk::GestureClick;
use adw::prelude::*;
use adw::{PasswordEntryRow, Toast, ToastOverlay};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const EMPTY_OTP_URL: &str = "otpauth://totp/Keycord?issuer=Keycord&secret=&digits=6&period=30";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OtpMode {
    Live,
    Editing,
}

#[derive(Clone)]
pub struct PasswordOtpState {
    pub row: PasswordEntryRow,
    overlay: ToastOverlay,
    template: Rc<RefCell<Option<OtpFieldTemplate>>>,
    url: Rc<RefCell<Option<String>>>,
    mode: Rc<Cell<OtpMode>>,
    refresh_generation: Rc<Cell<u64>>,
    countdown: OtpCountdownCircle,
}

impl PasswordOtpState {
    pub fn new(row: &PasswordEntryRow, overlay: &ToastOverlay) -> Self {
        let countdown = OtpCountdownCircle::new();

        row.set_activatable(true);
        row.add_suffix(countdown.widget());

        let state = Self {
            row: row.clone(),
            overlay: overlay.clone(),
            template: Rc::new(RefCell::new(None)),
            url: Rc::new(RefCell::new(None)),
            mode: Rc::new(Cell::new(OtpMode::Live)),
            refresh_generation: Rc::new(Cell::new(0)),
            countdown,
        };
        state.connect_row_signals();
        state
    }

    pub fn clear(&self) {
        self.bump_refresh_generation();
        self.template.borrow_mut().take();
        self.url.borrow_mut().take();
        self.mode.set(OtpMode::Live);
        self.row.set_title("OTP");
        self.row.set_text("");
        self.row.set_editable(false);
        self.row.set_show_apply_button(false);
        self.row.set_visible(false);
        self.countdown.set_visible(false);
        self.countdown.set_fraction(0.0);
        self.countdown.set_tooltip_text(None);
    }

    pub fn sync_from_parsed_lines(
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
        *self.url.borrow_mut() = Some(url.clone());
        self.row.set_visible(true);
        self.mode.set(OtpMode::Live);
        if otp_secret_is_blank(&url) {
            self.set_edit_mode(false);
            return;
        }
        self.render(show_errors);
    }

    pub fn current_url(&self) -> Option<String> {
        if self.is_editing() {
            self.url_for_current_secret()
        } else {
            self.url.borrow().clone()
        }
    }

    pub fn current_url_for_save(&self) -> Result<Option<String>, &'static str> {
        let Some(url) = self.current_url() else {
            return Ok(None);
        };

        if self.has_otp()
            && otp_secret_from_url(&url)
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return Err("Enter an OTP secret.");
        }

        Ok(Some(url))
    }

    pub fn add_empty_secret(&self) {
        *self.template.borrow_mut() = Some(OtpFieldTemplate::BareUrl);
        *self.url.borrow_mut() = Some(EMPTY_OTP_URL.to_string());
        self.row.set_visible(true);
        self.set_edit_mode(true);
    }

    fn connect_row_signals(&self) {
        if let Some(delegate) = self.row.delegate() {
            let state = self.clone();
            let click = GestureClick::new();
            click.connect_pressed(move |_, _, _, _| {
                state.enter_edit_mode();
            });
            delegate.add_controller(click);
        }

        let state = self.clone();
        self.row.connect_apply(move |_| {
            if !state.is_editing() {
                return;
            }

            let Some(url) = state.url_for_current_secret() else {
                state
                    .overlay
                    .add_toast(Toast::new("Couldn't update the code."));
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
            state.exit_edit_mode(true);
        });
    }

    pub fn has_otp(&self) -> bool {
        self.template.borrow().is_some()
    }

    fn url_for_current_secret(&self) -> Option<String> {
        let current_url = self.url.borrow().clone()?;
        Some(replace_otp_secret(&current_url, &self.row.text()))
    }

    fn is_editing(&self) -> bool {
        self.mode.get() == OtpMode::Editing
    }

    fn enter_edit_mode(&self) {
        if !self.has_otp() || self.is_editing() {
            return;
        }

        self.set_edit_mode(true);
    }

    fn exit_edit_mode(&self, show_errors: bool) {
        self.mode.set(OtpMode::Live);
        self.render(show_errors);
    }

    fn set_edit_mode(&self, focus_editor: bool) {
        self.mode.set(OtpMode::Editing);
        self.render(false);
        if focus_editor {
            self.focus_editor();
        }
    }

    fn bump_refresh_generation(&self) -> u64 {
        let next = self.refresh_generation.get().wrapping_add(1);
        self.refresh_generation.set(next);
        next
    }

    fn render(&self, show_errors: bool) {
        if self.is_editing() {
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
        self.row.set_show_apply_button(true);
        self.row.set_text(&secret);
        self.countdown.set_visible(false);
    }

    fn focus_editor(&self) {
        if let Some(delegate) = self.row.delegate() {
            glib::idle_add_local_once(move || {
                delegate.grab_focus();
                delegate.select_region(0, -1);
            });
        } else {
            self.row.grab_focus_without_selecting();
        }
    }

    fn render_live_mode(&self, show_errors: bool) {
        let Some(url) = self.url.borrow().clone() else {
            self.clear();
            return;
        };

        self.row.set_title("OTP code");
        self.row.set_editable(false);
        self.row.set_show_apply_button(false);
        self.countdown.set_visible(true);

        match otp_display(&url) {
            Ok((code, remaining, period)) => {
                self.set_live_code(&code, remaining, period);
                self.start_live_refresh();
            }
            Err(err) => {
                log_error(format!("Failed to render OTP code: {err}"));
                self.clear_live_code();
                if show_errors {
                    self.overlay
                        .add_toast(Toast::new("Couldn't load the code."));
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
            if state.is_editing() || !state.row.is_visible() {
                return ControlFlow::Break;
            }

            let Some(url) = state.url.borrow().clone() else {
                return ControlFlow::Break;
            };

            match otp_display(&url) {
                Ok((code, remaining, period)) => {
                    state.set_live_code(&code, remaining, period);
                    ControlFlow::Continue
                }
                Err(err) => {
                    log_error(format!("Failed to refresh OTP code: {err}"));
                    state.clear_live_code();
                    ControlFlow::Break
                }
            }
        });
    }

    fn set_live_code(&self, code: &str, remaining: u64, period: u64) {
        let remaining = u32::try_from(remaining).unwrap_or(u32::MAX);
        let period = u32::try_from(period).unwrap_or(u32::MAX);
        self.row.set_text(code);
        self.countdown
            .set_fraction(f64::from(remaining) / f64::from(period));
        self.countdown
            .set_tooltip_text(Some(&format!("{remaining}s remaining")));
    }

    fn clear_live_code(&self) {
        self.row.set_text("");
        self.countdown.set_fraction(0.0);
        self.countdown.set_tooltip_text(None);
    }
}

fn otp_secret_is_blank(url: &str) -> bool {
    otp_secret_from_url(url)
        .unwrap_or_default()
        .trim()
        .is_empty()
}

#[cfg(test)]
mod tests {
    use super::url::{otp_period, otp_secret_from_url, replace_otp_secret};
    use super::{otp_secret_is_blank, EMPTY_OTP_URL};
    use totp_rs::TOTP;

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

    #[test]
    fn empty_or_missing_otp_secret_is_treated_as_blank() {
        assert!(otp_secret_is_blank(EMPTY_OTP_URL));
        assert!(otp_secret_is_blank("otpauth://totp/Test?issuer=Example"));
        assert!(!otp_secret_is_blank(
            "otpauth://totp/Test?issuer=Example&secret=ABC123"
        ));
    }

    #[test]
    fn placeholder_url_becomes_a_valid_totp_url_after_filling_in_the_secret() {
        let url = replace_otp_secret(EMPTY_OTP_URL, "JBSWY3DPEHPK3PXP");
        assert!(TOTP::from_url_unchecked(&url).is_ok());
    }
}
