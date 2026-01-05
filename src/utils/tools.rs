//! CLI tools

use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::config::UserConfig;
use crate::db::UserTable;
use crate::utils::auth::hash_password;

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
pub async fn interactive_setup() -> Result<()> {
    println!("=== SwingMusic Interactive Setup ===\n");

    // Gather music root directories
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

    // Toggle watchdog
    let enable_watchdog = prompt_yes_no("Enable file watcher (watchdog)? [Y/n]: ", true)?;

    // Save config updates
    let mut config = UserConfig::load()?;
    if !root_dirs.is_empty() {
        config.root_dirs = root_dirs;
    }
    config.enable_watchdog = enable_watchdog;
    if config.server_id.is_empty() {
        config.server_id = uuid::Uuid::new_v4().to_string();
    }
    config.save()?;

    // Admin user creation
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

    // Optional guest user
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
