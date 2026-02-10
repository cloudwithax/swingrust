//! CLI tools

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::config::UserConfig;
use crate::db::UserTable;
use crate::utils::auth::hash_password;

/// check if running in an interactive terminal
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// create default admin user from environment variables or fallback to admin:admin
pub async fn create_default_admin() -> Result<()> {
    // check if admin user already exists
    if UserTable::get_by_username("admin").await?.is_some() {
        return Ok(());
    }

    // get credentials from environment variables or use defaults
    let username = env::var("SWING_ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = env::var("SWING_ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());

    // check if user with custom username already exists
    if UserTable::get_by_username(&username).await?.is_some() {
        tracing::info!("User '{}' already exists; skipping creation.", username);
        return Ok(());
    }

    let hash = hash_password(&password)?;
    UserTable::insert_admin(&username, &hash).await?;
    tracing::info!("Admin user '{}' created.", username);

    Ok(())
}

/// configure root directories from the SWING_ROOT_DIRS environment variable.
/// paths can be colon or semicolon separated (e.g. /music or /music:/podcasts).
///
/// when this env var is set it is always treated as authoritative -- it will
/// overwrite whatever root_dirs is in settings.json. this is the expected
/// behavior for docker deployments where env vars are the primary config.
pub fn configure_root_dirs_from_env() -> Result<bool> {
    let root_dirs_env = match env::var("SWING_ROOT_DIRS") {
        Ok(v) if !v.is_empty() => v,
        _ => return Ok(false),
    };

    // split on : or ; to support both unix and windows style path separators
    let dirs: Vec<String> = root_dirs_env
        .split(|c| c == ':' || c == ';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| {
            let path = Path::new(s);
            if path.is_dir() {
                true
            } else {
                tracing::warn!(
                    "SWING_ROOT_DIRS: path does not exist or is not a directory: {} \
                     (is the volume mounted?)",
                    s
                );
                false
            }
        })
        .collect();

    if dirs.is_empty() {
        tracing::warn!(
            "SWING_ROOT_DIRS was set to '{}' but no valid directories were found. \
             make sure the volume is mounted correctly.",
            root_dirs_env
        );
        return Ok(false);
    }

    let mut config = UserConfig::load()?;

    // env var is authoritative -- always overwrite the persisted root_dirs so
    // docker users can change SWING_ROOT_DIRS between restarts and have it
    // take effect without nuking their config volume.
    if config.root_dirs != dirs {
        tracing::info!("Setting root directories from SWING_ROOT_DIRS: {:?}", dirs);
        config.root_dirs = dirs;
        config.save()?;
        Ok(true)
    } else {
        tracing::debug!("root_dirs already match SWING_ROOT_DIRS, no update needed");
        Ok(false)
    }
}

/// Password reset tool
pub async fn password_reset() -> Result<()> {
    println!("=== SwingMusic Password Reset ===\n");

    // Get username
    print!("Username: ");
    io::stdout().flush()?;

    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();

    if username.is_empty() {
        println!("Error: Username cannot be empty");
        return Ok(());
    }

    // Get password (without echo would require a crate like rpassword)
    print!("New password: ");
    io::stdout().flush()?;

    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim();

    if password.is_empty() {
        println!("Error: Password cannot be empty");
        return Ok(());
    }

    // Confirm password
    print!("Confirm password: ");
    io::stdout().flush()?;

    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm)?;
    let confirm = confirm.trim();

    if password != confirm {
        println!("Error: Passwords do not match");
        return Ok(());
    }

    // Hash and update
    let password_hash = hash_password(password)?;

    // Update in database
    if let Some(mut user) = UserTable::get_by_username(username).await? {
        user.password = password_hash;
        UserTable::update(&user).await?;
        println!("\nPassword updated successfully for user: {}", username);
    } else {
        println!("Error: User '{}' not found", username);
    }

    Ok(())
}

/// Interactive first-run setup (required when no users exist and no setup file is provided)
/// In non-interactive mode (docker), creates default admin user
pub async fn interactive_setup() -> Result<()> {
    // non-interactive mode (docker, ci, etc.) - just create the default admin and return.
    // root dir configuration from SWING_ROOT_DIRS is handled earlier in run_setup()
    // so it applies on every restart, not just the first one.
    if !is_interactive() {
        tracing::info!("Non-interactive mode detected, creating default admin...");
        create_default_admin().await?;
        return Ok(());
    }

    println!("=== SwingMusic Interactive Setup ===\n");

    // gather music root directories
    println!("Enter one or more music root directories (comma-separated):");
    print!("Music paths: ");
    io::stdout().flush()?;
    let mut roots_line = String::new();
    io::stdin().read_line(&mut roots_line)?;
    let root_dirs: Vec<String> = roots_line
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // toggle watchdog
    let enable_watchdog = prompt_yes_no("Enable file watcher (watchdog)? [Y/n]: ", true)?;

    // save config updates
    let mut config = UserConfig::load()?;
    if !root_dirs.is_empty() {
        config.root_dirs = root_dirs;
    }
    config.enable_watchdog = enable_watchdog;
    if config.server_id.is_empty() {
        config.server_id = uuid::Uuid::new_v4().to_string();
    }
    config.save()?;

    // admin user creation
    println!("\nCreate admin user:");
    print!("Username [admin]: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();
    let username = if username.is_empty() {
        "admin"
    } else {
        username
    };

    let password = prompt_password("Password: ")?;
    let confirm = prompt_password("Confirm password: ")?;
    if password != confirm {
        println!("Error: passwords do not match");
        return Ok(());
    }

    let hash = hash_password(&password)?;
    if UserTable::get_by_username(username).await?.is_some() {
        println!("User '{}' already exists; skipping creation.", username);
    } else {
        UserTable::insert_admin(username, &hash).await?;
        println!("Admin user '{}' created.", username);
    }

    // optional guest user
    if prompt_yes_no("Create guest user? [y/N]: ", false)? {
        if UserTable::get_by_username("guest").await?.is_some() {
            println!("Guest already exists; skipping.");
        } else {
            UserTable::insert_guest().await?;
            println!("Guest user created.");
        }
    }

    println!("\nSetup complete. Restarting server...");
    Ok(())
}

/// Apply setup from a JSON file (skips interactive prompts)
pub async fn apply_setup_file(path: &Path) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct SetupFile {
        #[serde(flatten)]
        config: UserConfig,
        admin_username: Option<String>,
        admin_password: Option<String>,
    }

    let data = fs::read_to_string(path)
        .with_context(|| format!("Failed to read setup file: {}", path.display()))?;
    let mut setup: SetupFile =
        serde_json::from_str(&data).with_context(|| "Invalid setup file JSON")?;

    // Ensure server id exists
    if setup.config.server_id.is_empty() {
        setup.config.server_id = uuid::Uuid::new_v4().to_string();
    }
    setup.config.save()?;

    // Create admin if credentials supplied
    if let (Some(user), Some(pass)) = (
        setup.admin_username.as_deref(),
        setup.admin_password.as_deref(),
    ) {
        let hash = hash_password(pass)?;
        if UserTable::get_by_username(user).await?.is_none() {
            UserTable::insert_admin(user, &hash).await?;
            println!("Admin user '{}' created from setup file.", user);
        }
    }

    Ok(())
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let val = input.trim().to_lowercase();
    Ok(match val.as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    })
}

fn prompt_password(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut pw = String::new();
    io::stdin().read_line(&mut pw)?;
    Ok(pw.trim().to_string())
}
