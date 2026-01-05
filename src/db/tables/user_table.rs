//! User table operations

use anyhow::Result;
use sqlx::FromRow;

use crate::db::DbEngine;
use crate::models::{User, UserRole};

/// Database row for user table
#[derive(Debug, FromRow)]
struct UserRow {
    id: i64,
    image: Option<String>,
    password: String,
    username: String,
    roles: String,
    extra: String,
}

impl UserRow {
    fn into_user(self) -> User {
        let roles: Vec<UserRole> =
            serde_json::from_str(&self.roles).unwrap_or_else(|_| vec![UserRole::User]);
        let extra: serde_json::Value =
            serde_json::from_str(&self.extra).unwrap_or(serde_json::Value::Null);

        // Extract firstname and email from extra if present
        let firstname = extra
            .get("firstname")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let email = extra
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        User {
            id: self.id,
            username: self.username,
            password: self.password,
            image: self.image,
            roles,
            extra,
            firstname,
            email,
        }
    }
}

/// User table operations
pub struct UserTable;

impl UserTable {
    /// Get all users
    pub async fn all() -> Result<Vec<User>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let rows: Vec<UserRow> = sqlx::query_as("SELECT * FROM user").fetch_all(pool).await?;

        Ok(rows.into_iter().map(|r| r.into_user()).collect())
    }

    /// Alias for all()
    pub async fn get_all() -> Result<Vec<User>> {
        Self::all().await
    }

    /// Get user by ID
    pub async fn get_by_id(id: i64) -> Result<Option<User>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<UserRow> = sqlx::query_as("SELECT * FROM user WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.into_user()))
    }

    /// Get user by username
    pub async fn get_by_username(username: &str) -> Result<Option<User>> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: Option<UserRow> = sqlx::query_as("SELECT * FROM user WHERE username = ?")
            .bind(username)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| r.into_user()))
    }

    /// Insert a user
    pub async fn insert(user: &User) -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let roles = serde_json::to_string(&user.roles)?;

        // Build extra with firstname and email
        let mut extra = user.extra.clone();
        if let serde_json::Value::Object(ref mut map) = extra {
            map.insert("firstname".to_string(), serde_json::json!(user.firstname));
            map.insert("email".to_string(), serde_json::json!(user.email));
        } else {
            extra = serde_json::json!({
                "firstname": user.firstname,
                "email": user.email,
            });
        }
        let extra_str = serde_json::to_string(&extra)?;

        let result = sqlx::query(
            "INSERT INTO user (image, password, username, roles, extra) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&user.image)
        .bind(&user.password)
        .bind(&user.username)
        .bind(&roles)
        .bind(&extra_str)
        .execute(pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Insert admin user
    pub async fn insert_admin(username: &str, password_hash: &str) -> Result<i64> {
        let user = User::admin(username.to_string(), password_hash.to_string());
        Self::insert(&user).await
    }

    /// Insert guest user
    pub async fn insert_guest() -> Result<i64> {
        let user = User::guest();
        Self::insert(&user).await
    }

    /// Update user
    pub async fn update(user: &User) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let roles = serde_json::to_string(&user.roles)?;

        let mut extra = user.extra.clone();
        if let serde_json::Value::Object(ref mut map) = extra {
            map.insert("firstname".to_string(), serde_json::json!(user.firstname));
            map.insert("email".to_string(), serde_json::json!(user.email));
        }
        let extra_str = serde_json::to_string(&extra)?;

        sqlx::query(
            "UPDATE user SET image = ?, password = ?, username = ?, roles = ?, extra = ? WHERE id = ?"
        )
        .bind(&user.image)
        .bind(&user.password)
        .bind(&user.username)
        .bind(&roles)
        .bind(&extra_str)
        .bind(user.id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Delete user by username
    pub async fn delete_by_username(username: &str) -> Result<bool> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let result = sqlx::query("DELETE FROM user WHERE username = ?")
            .bind(username)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get user count
    pub async fn count() -> Result<i64> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user")
            .fetch_one(pool)
            .await?;

        Ok(row.0)
    }

    /// Update only the password for a user
    pub async fn update_password(id: i64, password_hash: &str) -> Result<()> {
        let engine = DbEngine::get()?;
        let pool = engine.pool();

        sqlx::query("UPDATE user SET password = ? WHERE id = ?")
            .bind(password_hash)
            .bind(id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Check if any users exist
    pub async fn has_users() -> Result<bool> {
        Ok(Self::count().await? > 0)
    }
}
