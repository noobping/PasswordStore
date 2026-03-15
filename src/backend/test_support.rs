use crate::backend::integrated::clear_cached_unlocked_ripasso_private_keys;
use ripasso::crypto::{Crypto, Sequoia};
use sequoia_openpgp::{cert::CertBuilder, serialize::Serialize, Cert};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_lock() -> &'static Mutex<()> {
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    TEST_LOCK.get_or_init(|| Mutex::new(()))
}

fn reset_backend_test_state() {
    clear_cached_unlocked_ripasso_private_keys();
}

fn command_error(action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !stderr.is_empty() {
        format!("{action} failed: {stderr}")
    } else if !stdout.is_empty() {
        format!("{action} failed: {stdout}")
    } else {
        format!("{action} failed: {}", output.status)
    }
}

fn ensure_success(action: &str, output: Output) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_error(action, &output))
    }
}

fn init_git_repository(path: &Path) -> Result<(), String> {
    ensure_success(
        "git init",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .arg("init")
            .output()
            .map_err(|err| format!("Failed to start git init: {err}"))?,
    )?;
    ensure_success(
        "git config user.name",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["config", "user.name", "PasswordStore Tests"])
            .output()
            .map_err(|err| format!("Failed to start git config user.name: {err}"))?,
    )?;
    ensure_success(
        "git config user.email",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["config", "user.email", "tests@example.com"])
            .output()
            .map_err(|err| format!("Failed to start git config user.email: {err}"))?,
    )?;
    Ok(())
}

fn git_commit_subjects(path: &Path) -> Result<Vec<String>, String> {
    let output = ensure_success(
        "git log",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["log", "--format=%s"])
            .output()
            .map_err(|err| format!("Failed to start git log: {err}"))?,
    )?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

fn import_public_key(bytes: &[u8]) -> Result<(), String> {
    let mut child = Command::new("gpg")
        .args(["--batch", "--yes", "--import"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start gpg public-key import: {err}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "gpg public-key import did not provide stdin".to_string())?;
        stdin
            .write_all(bytes)
            .map_err(|err| format!("Failed to write imported public key bytes: {err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("Failed to wait for gpg public-key import: {err}"))?;
    if !output.status.success() {
        return Err(command_error("gpg --import", &output));
    }

    Ok(())
}

fn git_head_author(path: &Path) -> Result<String, String> {
    let output = ensure_success(
        "git log",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["log", "-1", "--format=%an <%ae>"])
            .output()
            .map_err(|err| format!("Failed to start git log author format: {err}"))?,
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_head_commit_has_signature(path: &Path) -> Result<bool, String> {
    let output = ensure_success(
        "git cat-file",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["cat-file", "-p", "HEAD"])
            .output()
            .map_err(|err| format!("Failed to start git cat-file: {err}"))?,
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).contains("\ngpgsig "))
}

fn verify_git_head_signature(path: &Path) -> Result<(), String> {
    ensure_success(
        "git verify-commit",
        Command::new("git")
            .args(["-C"])
            .arg(path)
            .args(["verify-commit", "HEAD"])
            .output()
            .map_err(|err| format!("Failed to start git verify-commit: {err}"))?,
    )?;
    Ok(())
}

fn trust_public_key(fingerprint_hex: &str) -> Result<(), String> {
    let mut child = Command::new("gpg")
        .args(["--batch", "--yes", "--import-ownertrust"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to start gpg ownertrust import: {err}"))?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "gpg ownertrust import did not provide stdin".to_string())?;
        stdin
            .write_all(format!("{fingerprint_hex}:6:\n").as_bytes())
            .map_err(|err| format!("Failed to write ownertrust data: {err}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("Failed to wait for gpg ownertrust import: {err}"))?;
    if !output.status.success() {
        return Err(command_error("gpg --import-ownertrust", &output));
    }

    Ok(())
}

pub struct GeneratedSecretKey {
    pub cert: Arc<Cert>,
    pub fingerprint: [u8; 20],
    pub fingerprint_hex: String,
    pub public_key_bytes: Vec<u8>,
}

pub struct SystemBackendTestEnv {
    _guard: MutexGuard<'static, ()>,
    original_home: Option<OsString>,
    original_xdg_config_home: Option<OsString>,
    original_xdg_data_home: Option<OsString>,
    original_gnupg_home: Option<OsString>,
    original_gpg_agent_info: Option<OsString>,
    root: PathBuf,
    store: PathBuf,
}

impl SystemBackendTestEnv {
    pub fn new() -> Self {
        let guard = match test_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("passwordstore-system-backend-test-{nanos}"));
        let store = root.join("store");
        fs::create_dir_all(&store).expect("create test store root");

        let env = Self {
            _guard: guard,
            original_home: env::var_os("HOME"),
            original_xdg_config_home: env::var_os("XDG_CONFIG_HOME"),
            original_xdg_data_home: env::var_os("XDG_DATA_HOME"),
            original_gnupg_home: env::var_os("GNUPGHOME"),
            original_gpg_agent_info: env::var_os("GPG_AGENT_INFO"),
            root,
            store,
        };
        env.activate_profile("base");
        env
    }

    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    pub fn store_root(&self) -> &Path {
        &self.store
    }

    pub fn activate_profile(&self, name: &str) {
        let home = self.root.join(name);
        let config = home.join(".config");
        let data = home.join(".local/share");
        let gnupg = home.join(".gnupg");
        fs::create_dir_all(&config).expect("create test config dir");
        fs::create_dir_all(&data).expect("create test data dir");
        fs::create_dir_all(&gnupg).expect("create test gnupg dir");
        #[cfg(unix)]
        fs::set_permissions(&gnupg, fs::Permissions::from_mode(0o700))
            .expect("set test gnupg permissions");
        reset_backend_test_state();
        env::set_var("HOME", &home);
        env::set_var("XDG_CONFIG_HOME", &config);
        env::set_var("XDG_DATA_HOME", &data);
        env::set_var("GNUPGHOME", &gnupg);
        env::remove_var("GPG_AGENT_INFO");
    }

    pub fn init_store_git_repository(&self) -> Result<(), String> {
        init_git_repository(self.store_root())
    }

    pub fn store_git_commit_subjects(&self) -> Result<Vec<String>, String> {
        git_commit_subjects(self.store_root())
    }

    pub fn store_git_head_author(&self) -> Result<String, String> {
        git_head_author(self.store_root())
    }

    pub fn store_head_commit_has_signature(&self) -> Result<bool, String> {
        git_head_commit_has_signature(self.store_root())
    }

    pub fn verify_store_head_commit_signature(&self) -> Result<(), String> {
        verify_git_head_signature(self.store_root())
    }

    pub fn generate_secret_key(user_id: &str) -> Result<GeneratedSecretKey, String> {
        let (cert, _) = CertBuilder::general_purpose(Some(user_id))
            .generate()
            .map_err(|err| format!("Failed to generate test certificate: {err}"))?;
        let mut public_key_bytes = Vec::new();
        cert.serialize(&mut public_key_bytes)
            .map_err(|err| format!("Failed to serialize test public certificate: {err}"))?;
        let fingerprint = cert
            .fingerprint()
            .as_bytes()
            .try_into()
            .map_err(|_| "Test certificate fingerprint should be 20 bytes.".to_string())?;
        let fingerprint_hex = cert.fingerprint().to_hex();

        Ok(GeneratedSecretKey {
            cert: Arc::new(cert),
            fingerprint,
            fingerprint_hex,
            public_key_bytes,
        })
    }

    pub fn import_public_key(bytes: &[u8]) -> Result<(), String> {
        import_public_key(bytes)
    }

    pub fn trust_public_key(fingerprint_hex: &str) -> Result<(), String> {
        trust_public_key(fingerprint_hex)
    }
}

impl Drop for SystemBackendTestEnv {
    fn drop(&mut self) {
        reset_backend_test_state();
        if let Some(home) = self.original_home.as_ref() {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }

        if let Some(config) = self.original_xdg_config_home.as_ref() {
            env::set_var("XDG_CONFIG_HOME", config);
        } else {
            env::remove_var("XDG_CONFIG_HOME");
        }

        if let Some(data) = self.original_xdg_data_home.as_ref() {
            env::set_var("XDG_DATA_HOME", data);
        } else {
            env::remove_var("XDG_DATA_HOME");
        }

        if let Some(gnupg) = self.original_gnupg_home.as_ref() {
            env::set_var("GNUPGHOME", gnupg);
        } else {
            env::remove_var("GNUPGHOME");
        }

        if let Some(info) = self.original_gpg_agent_info.as_ref() {
            env::set_var("GPG_AGENT_INFO", info);
        } else {
            env::remove_var("GPG_AGENT_INFO");
        }

        let _ = fs::remove_dir_all(&self.root);
    }
}

fn decrypt_entry_with_generated_key(
    key: &GeneratedSecretKey,
    ciphertext: &[u8],
) -> Result<String, String> {
    let mut key_ring = std::collections::HashMap::new();
    key_ring.insert(key.fingerprint, key.cert.clone());
    Sequoia::from_values(key.fingerprint, key_ring, Path::new("/"))
        .decrypt_string(ciphertext)
        .map_err(|err| err.to_string())
}

#[expect(
    clippy::significant_drop_tightening,
    reason = "SystemBackendTestEnv intentionally keeps temp dirs and test env vars alive for the full helper execution."
)]
pub fn assert_entry_is_encrypted_for_each_recipient(
    initialize_store: impl Fn(&str, &[String]) -> Result<(), String>,
    save_entry: impl Fn(&str, &str, &str) -> Result<(), String>,
) {
    let env = SystemBackendTestEnv::new();
    let marker = env
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("test");
    let label = "team/service";
    let contents = "secret-value\nusername: alice";
    let store_root = env.store_root().to_string_lossy().to_string();

    env.activate_profile("base");
    let key_a =
        SystemBackendTestEnv::generate_secret_key(&format!("Recipient A <a-{marker}@example.com>"))
            .expect("generate first recipient key");
    let key_b =
        SystemBackendTestEnv::generate_secret_key(&format!("Recipient B <b-{marker}@example.com>"))
            .expect("generate second recipient key");
    SystemBackendTestEnv::import_public_key(&key_a.public_key_bytes)
        .expect("import first public recipient key");
    SystemBackendTestEnv::trust_public_key(&key_a.fingerprint_hex)
        .expect("trust first recipient key");
    SystemBackendTestEnv::import_public_key(&key_b.public_key_bytes)
        .expect("import second public recipient key");
    SystemBackendTestEnv::trust_public_key(&key_b.fingerprint_hex)
        .expect("trust second recipient key");

    initialize_store(
        &store_root,
        &[key_a.fingerprint_hex.clone(), key_b.fingerprint_hex.clone()],
    )
    .expect("initialize store recipients");
    save_entry(&store_root, label, contents).expect("save password entry");
    let ciphertext = fs::read(PathBuf::from(&store_root).join("team/service.gpg"))
        .expect("read encrypted password entry");

    env.activate_profile("key-a");
    assert_eq!(
        decrypt_entry_with_generated_key(&key_a, &ciphertext)
            .expect("decrypt entry with first recipient")
            .trim_end_matches('\n'),
        contents
    );

    env.activate_profile("key-b");
    assert_eq!(
        decrypt_entry_with_generated_key(&key_b, &ciphertext)
            .expect("decrypt entry with second recipient")
            .trim_end_matches('\n'),
        contents
    );
}
