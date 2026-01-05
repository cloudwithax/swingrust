//! User model

use serde::{Deserialize, Serialize};

/// User roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    User,
    Guest,
    Curator,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::User => "user",
            UserRole::Guest => "guest",
            UserRole::Curator => "curator",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(UserRole::Admin),
            "user" => Some(UserRole::User),
            "guest" => Some(UserRole::Guest),
            "curator" => Some(UserRole::Curator),
            _ => None,
        }
    }
}

impl Default for UserRole {
    fn default() -> Self {
        UserRole::User
    }
}

/// A user account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Database ID
    pub id: i64,
    /// Username
    pub username: String,
    /// Password hash (not serialized to JSON)
    #[serde(skip_serializing)]
    pub password: String,
    /// Profile image path
    #[serde(default)]
    pub image: Option<String>,
    /// User roles
    #[serde(default)]
    pub roles: Vec<UserRole>,
    /// Extra metadata
    #[serde(default)]
    pub extra: serde_json::Value,
    /// First name / display name
    #[serde(default)]
    pub firstname: String,
    /// Email address
    #[serde(default)]
    pub email: String,
}

impl User {
    /// Create a new user
    pub fn new(username: String, password_hash: String) -> Self {
        Self {
            id: 0,
            username,
            password: password_hash,
            image: None,
            roles: vec![UserRole::User],
            extra: serde_json::Value::Null,
            firstname: String::new(),
            email: String::new(),
        }
    }

    /// Create an admin user
    pub fn admin(username: String, password_hash: String) -> Self {
        Self {
            id: 0,
            username,
            password: password_hash,
            image: None,
            roles: vec![UserRole::Admin],
            extra: serde_json::Value::Null,
            firstname: String::new(),
            email: String::new(),
        }
    }

    /// Create a guest user
    pub fn guest() -> Self {
        Self {
            id: 0,
            username: "Guest".to_string(),
            password: String::new(),
            image: None,
            roles: vec![UserRole::Guest],
            extra: serde_json::Value::Null,
            firstname: "Guest".to_string(),
            email: String::new(),
        }
    }

    /// Check if user is admin
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&UserRole::Admin)
    }

    /// Check if user is guest
    pub fn is_guest(&self) -> bool {
        self.roles.contains(&UserRole::Guest)
    }

    /// Serialize without password (for API responses)
    pub fn to_public(&self) -> PublicUser {
        PublicUser {
            id: self.id,
            username: self.username.clone(),
            image: self.image.clone(),
            roles: self.roles.clone(),
            firstname: self.firstname.clone(),
            email: self.email.clone(),
        }
    }

    /// Serialize minimal info
    pub fn to_minimal(&self) -> MinimalUser {
        MinimalUser {
            id: self.id,
            username: self.username.clone(),
            firstname: self.firstname.clone(),
        }
    }
}

impl Default for User {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}

/// Public user info (no password)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUser {
    pub id: i64,
    pub username: String,
    pub image: Option<String>,
    pub roles: Vec<UserRole>,
    pub firstname: String,
    pub email: String,
}

/// Minimal user info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalUser {
    pub id: i64,
    pub username: String,
    pub firstname: String,
}
