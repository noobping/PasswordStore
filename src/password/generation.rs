use adw::gtk::SpinButton;
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::rc::Rc;

const LOWERCASE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const NUMBER_CHARS: &[u8] = b"0123456789";
const SYMBOL_CHARS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordGenerationSettings {
    pub length: u32,
    pub min_lowercase: u32,
    pub min_uppercase: u32,
    pub min_numbers: u32,
    pub min_symbols: u32,
}

impl Default for PasswordGenerationSettings {
    fn default() -> Self {
        Self {
            length: 24,
            min_lowercase: 1,
            min_uppercase: 1,
            min_numbers: 1,
            min_symbols: 1,
        }
    }
}

impl PasswordGenerationSettings {
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.length = normalized.length.max(1);

        if normalized.minimum_length() == 0 {
            normalized.min_lowercase = 1;
        }

        normalized.length = normalized.length.max(normalized.minimum_length());
        normalized
    }

    pub const fn minimum_length(&self) -> u32 {
        self.min_lowercase + self.min_uppercase + self.min_numbers + self.min_symbols
    }

    fn enabled_pools(&self) -> Vec<&'static [u8]> {
        let mut pools = Vec::new();

        if self.min_lowercase > 0 {
            pools.push(LOWERCASE_CHARS);
        }
        if self.min_uppercase > 0 {
            pools.push(UPPERCASE_CHARS);
        }
        if self.min_numbers > 0 {
            pools.push(NUMBER_CHARS);
        }
        if self.min_symbols > 0 {
            pools.push(SYMBOL_CHARS);
        }

        pools
    }
}

#[derive(Clone)]
pub struct PasswordGenerationControls {
    pub length: SpinButton,
    pub min_lowercase: SpinButton,
    pub min_uppercase: SpinButton,
    pub min_numbers: SpinButton,
    pub min_symbols: SpinButton,
    syncing: Rc<Cell<bool>>,
}

impl PasswordGenerationControls {
    pub fn new(
        length: &SpinButton,
        min_lowercase: &SpinButton,
        min_uppercase: &SpinButton,
        min_numbers: &SpinButton,
        min_symbols: &SpinButton,
    ) -> Self {
        Self {
            length: length.clone(),
            min_lowercase: min_lowercase.clone(),
            min_uppercase: min_uppercase.clone(),
            min_numbers: min_numbers.clone(),
            min_symbols: min_symbols.clone(),
            syncing: Rc::new(Cell::new(false)),
        }
    }

    pub fn settings(&self) -> PasswordGenerationSettings {
        PasswordGenerationSettings {
            length: self.length.value_as_int().max(1).cast_unsigned(),
            min_lowercase: self.min_lowercase.value_as_int().max(0).cast_unsigned(),
            min_uppercase: self.min_uppercase.value_as_int().max(0).cast_unsigned(),
            min_numbers: self.min_numbers.value_as_int().max(0).cast_unsigned(),
            min_symbols: self.min_symbols.value_as_int().max(0).cast_unsigned(),
        }
    }

    pub fn set_settings(&self, settings: &PasswordGenerationSettings) {
        let settings = settings.normalized();
        self.syncing.set(true);

        set_spin_value(&self.length, settings.length);
        set_spin_value(&self.min_lowercase, settings.min_lowercase);
        set_spin_value(&self.min_uppercase, settings.min_uppercase);
        set_spin_value(&self.min_numbers, settings.min_numbers);
        set_spin_value(&self.min_symbols, settings.min_symbols);

        self.syncing.set(false);
    }

    pub fn connect_changed(&self, changed: &Rc<dyn Fn()>) {
        for spin in [
            self.length.clone(),
            self.min_lowercase.clone(),
            self.min_uppercase.clone(),
            self.min_numbers.clone(),
            self.min_symbols.clone(),
        ] {
            let changed = changed.clone();
            let syncing = self.syncing.clone();
            spin.connect_value_changed(move |_| {
                if !syncing.get() {
                    changed();
                }
            });
        }
    }
}

pub fn generate_password(settings: &PasswordGenerationSettings) -> String {
    let settings = settings.normalized();
    let mut chars = Vec::with_capacity(settings.length as usize);
    let mut rng = OsRng;

    append_random_chars(
        &mut chars,
        LOWERCASE_CHARS,
        settings.min_lowercase as usize,
        &mut rng,
    );
    append_random_chars(
        &mut chars,
        UPPERCASE_CHARS,
        settings.min_uppercase as usize,
        &mut rng,
    );
    append_random_chars(
        &mut chars,
        NUMBER_CHARS,
        settings.min_numbers as usize,
        &mut rng,
    );
    append_random_chars(
        &mut chars,
        SYMBOL_CHARS,
        settings.min_symbols as usize,
        &mut rng,
    );

    let enabled_pools = settings.enabled_pools();
    while chars.len() < settings.length as usize {
        let pool = enabled_pools
            .choose(&mut rng)
            .expect("normalized settings always enable at least one pool");
        chars.push(random_char(pool, &mut rng));
    }

    chars.shuffle(&mut rng);
    chars.into_iter().collect()
}

fn append_random_chars(output: &mut Vec<char>, pool: &[u8], count: usize, rng: &mut OsRng) {
    for _ in 0..count {
        output.push(random_char(pool, rng));
    }
}

fn random_char(pool: &[u8], rng: &mut OsRng) -> char {
    *pool.choose(rng).expect("character pools are never empty") as char
}

fn set_spin_value(spin: &SpinButton, value: u32) {
    let Ok(value) = i32::try_from(value) else {
        return;
    };

    if spin.value_as_int() != value {
        spin.set_value(f64::from(value));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        generate_password, PasswordGenerationSettings, LOWERCASE_CHARS, NUMBER_CHARS, SYMBOL_CHARS,
        UPPERCASE_CHARS,
    };

    fn count_pool_chars(password: &str, pool: &[u8]) -> usize {
        password
            .chars()
            .filter(|ch| pool.contains(&(*ch as u8)))
            .count()
    }

    #[test]
    fn normalization_keeps_generator_usable_when_everything_is_zero() {
        let settings = PasswordGenerationSettings {
            length: 2,
            min_lowercase: 0,
            min_uppercase: 0,
            min_numbers: 0,
            min_symbols: 0,
        }
        .normalized();

        assert_eq!(settings.min_lowercase, 1);
        assert_eq!(settings.length, 2);
    }

    #[test]
    fn normalization_expands_length_to_fit_minimum_counts() {
        let settings = PasswordGenerationSettings {
            length: 6,
            min_lowercase: 3,
            min_uppercase: 3,
            min_numbers: 3,
            min_symbols: 0,
        }
        .normalized();

        assert_eq!(settings.length, 9);
    }

    #[test]
    fn generated_password_respects_enabled_sets_and_minimums() {
        let settings = PasswordGenerationSettings {
            length: 18,
            min_lowercase: 4,
            min_uppercase: 3,
            min_numbers: 5,
            min_symbols: 0,
        };

        let password = generate_password(&settings);

        assert_eq!(password.len(), 18);
        assert!(count_pool_chars(&password, LOWERCASE_CHARS) >= 4);
        assert!(count_pool_chars(&password, UPPERCASE_CHARS) >= 3);
        assert!(count_pool_chars(&password, NUMBER_CHARS) >= 5);
        assert_eq!(count_pool_chars(&password, SYMBOL_CHARS), 0);
    }

    #[test]
    fn zero_minimum_disables_a_character_class() {
        let settings = PasswordGenerationSettings {
            length: 12,
            min_lowercase: 6,
            min_uppercase: 0,
            min_numbers: 6,
            min_symbols: 0,
        };

        let password = generate_password(&settings);

        assert_eq!(count_pool_chars(&password, UPPERCASE_CHARS), 0);
        assert_eq!(count_pool_chars(&password, SYMBOL_CHARS), 0);
    }
}
