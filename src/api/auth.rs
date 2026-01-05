//! authentication api routes cookie based jwt upstream parity

use actix_web::cookie::{time::Duration as CookieDuration, Cookie};
use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse, Responder};
use anyhow::Result as AnyResult;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::UserConfig;
use crate::db::tables::UserTable;
use crate::models::{User, UserRole};
use crate::utils::auth::{create_jwt, hash_password, verify_jwt, verify_password, UserIdentity};

const ACCESS_MAX_AGE: i64 = 30 * 24 * 3600; // 30 days in seconds
const REFRESH_MAX_AGE: i64 = 30 * 24 * 3600;

/// global pair token storage one code at a time consumed once
static PAIR_TOKENS: Lazy<RwLock<HashMap<String, TokenResponse>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// login refresh response
#[derive(Debug, Serialize, Clone)]
pub struct TokenResponse {
    pub msg: String,
    pub accesstoken: String,
    pub refreshtoken: String,
    pub maxage: i64,
}

#[derive(Debug, Deserialize)]
pub struct PairQuery {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct UsersQuery {
    pub simplified: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub id: Option<i64>,
    pub email: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub id: Option<i64>,
    pub email: Option<String>,
    pub username: String,
    pub password: String,
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteUserRequest {
    pub username: String,
}

/// login endpoint
#[post("/login")]
pub async fn login(body: web::Json<LoginRequest>) -> impl Responder {
    match UserTable::get_by_username(&body.username).await {
        Ok(Some(user)) => {
            if verify_password(&body.password, &user.password).unwrap_or(false) {
                let config = match UserConfig::load() {
                    Ok(cfg) => cfg,
                    Err(_) => {
                        return HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": "Failed to load config"
                        }))
                    }
                };

                match create_tokens(&user, &config.server_id) {
                    Ok(tokens) => HttpResponse::Ok()
                        .cookie(build_access_cookie(&tokens.accesstoken))
                        .json(tokens),
                    Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
                        "msg": "Failed to create token"
                    })),
                }
            } else {
                HttpResponse::Unauthorized().json(serde_json::json!({
                    "msg": "Hehe! invalid password"
                }))
            }
        }
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({
            "msg": "User not found"
        })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Database error"
        })),
    }
}

/// refresh token expects refresh token in authorization header
#[post("/refresh")]
pub async fn refresh_token(req: HttpRequest) -> impl Responder {
    let token = match bearer_token(&req) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "msg": "No token provided"
            }));
        }
        Err(resp) => return resp,
    };

    let config = match UserConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Config error"
            }))
        }
    };

    match verify_jwt(&token, &config.server_id, Some("refresh")) {
        Ok(claims) => {
            match create_tokens_with_identity(claims.sub, &config.server_id) {
                Ok(tokens) => HttpResponse::Ok().json(tokens),
                Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
                    "msg": "Failed to create token"
                })),
            }
        }
        Err(_) => HttpResponse::Unauthorized().json(serde_json::json!({
            "msg": "Invalid token"
        })),
    }
}

/// get a pair code auth required via cookie
#[get("/getpaircode")]
pub async fn get_pair_code(req: HttpRequest) -> impl Responder {
    let user = match require_user(&req).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let config = match UserConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Config error"
            }))
        }
    };

    let token = match create_tokens(&user, &config.server_id) {
        Ok(t) => t,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create token"
            }))
        }
    };

    let code = token
        .accesstoken
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    let mut tokens = PAIR_TOKENS.write();
    tokens.clear();
    tokens.insert(code.clone(), token);

    HttpResponse::Ok().json(serde_json::json!({ "code": code }))
}

/// pair with a code one time use
#[get("/pair")]
pub async fn pair_with_code(query: web::Query<PairQuery>) -> impl Responder {
    let code = &query.code;

    let token = {
        let mut tokens = PAIR_TOKENS.write();
        tokens.remove(code)
    };

    match token {
        Some(pair) => HttpResponse::Ok().json(pair),
        None => HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Invalid code"
        })),
    }
}

/// update profile current user or specified id honoring admin rules
#[put("/profile/update")]
pub async fn update_profile(
    req: HttpRequest,
    body: web::Json<UpdateProfileRequest>,
) -> impl Responder {
    let current_user = match require_user(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    if current_user.username.to_lowercase() == "guest"
        || body
            .username
            .as_ref()
            .map(|u| u.to_lowercase() == "guest")
            .unwrap_or(false)
    {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Cannot update guest user"
        }));
    }

    let target_id = body.id.unwrap_or(current_user.id);
    let target_user = match UserTable::get_by_id(target_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "msg": "User not found"
            }))
        }
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Database error"
            }))
        }
    };

    if target_user.roles.contains(&UserRole::Guest) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Cannot update guest user"
        }));
    }

    let mut updated = target_user.clone();

    if let Some(username) = body.username.as_ref() {
        if !username.is_empty() && username != &updated.username {
            if let Ok(Some(existing)) = UserTable::get_by_username(username).await {
                if existing.id != updated.id {
                    return HttpResponse::BadRequest().json(serde_json::json!({
                        "msg": "Username already exists"
                    }));
                }
            }
            updated.username = username.clone();
        }
    }

    if let Some(email) = body.email.as_ref() {
        updated.email = email.clone();
    }

    if let Some(pass) = body.password.as_ref() {
        if !pass.is_empty() {
            match hash_password(pass) {
                Ok(h) => updated.password = h,
                Err(_) => {
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "msg": "Failed to hash password"
                    }))
                }
            }
        }
    }

    if let Some(role_names) = body.roles.as_ref() {
        if !current_user.roles.contains(&UserRole::Admin) {
            return HttpResponse::Forbidden().json(serde_json::json!({
                "msg": "Only admins can update roles"
            }));
        }

        let all_users = match UserTable::get_all().await {
            Ok(list) => list,
            Err(_) => {
                return HttpResponse::InternalServerError().json(serde_json::json!({
                    "msg": "Database error"
                }))
            }
        };

        if !role_names.iter().any(|r| r.to_lowercase() == "admin") {
            let admins: Vec<&User> = all_users
                .iter()
                .filter(|u| u.roles.contains(&UserRole::Admin))
                .collect();
            if admins.len() == 1 && admins[0].id == updated.id {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "msg": "Cannot remove the only admin"
                }));
            }
        }

        if updated.roles.contains(&UserRole::Guest) {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "msg": "Cannot update guest user"
            }));
        }

        updated.roles = parse_roles(role_names);
    }

    match UserTable::update(&updated).await {
        Ok(_) => match UserTable::get_by_id(updated.id).await {
            Ok(Some(u)) => HttpResponse::Ok().json(user_to_public_value(&u)),
            _ => HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to fetch user"
            })),
        },
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to update user"
        })),
    }
}

/// create a new user admin only
#[post("/profile/create")]
pub async fn create_user(req: HttpRequest, body: web::Json<CreateUserRequest>) -> impl Responder {
    if let Err(resp) = require_admin(&req).await.map(|_| ()) {
        return resp;
    }

    if body.username.is_empty() || body.password.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Username and password are required"
        }));
    }

    if let Ok(Some(_)) = UserTable::get_by_username(&body.username).await {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Username already exists"
        }));
    }

    let password_hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to hash password"
            }))
        }
    };

    let mut user = User::new(body.username.clone(), password_hash);
    if let Some(email) = body.email.as_ref() {
        user.email = email.clone();
    }
    if let Some(role_names) = body.roles.as_ref() {
        user.roles = parse_roles(role_names);
    } else {
        user.roles = vec![];
    }

    match UserTable::insert(&user).await {
        Ok(_) => match UserTable::get_by_username(&body.username).await {
            Ok(Some(u)) => HttpResponse::Ok().json(user_to_public_value(&u)),
            _ => HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to fetch user"
            })),
        },
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to create user"
        })),
    }
}

/// create guest user admin only
#[post("/profile/guest/create")]
pub async fn create_guest(req: HttpRequest) -> impl Responder {
    if let Err(resp) = require_admin(&req).await.map(|_| ()) {
        return resp;
    }

    if let Ok(Some(_)) = UserTable::get_by_username("guest").await {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Guest user already exists"
        }));
    }

    let password_hash = match hash_password("guest") {
        Ok(h) => h,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Failed to hash password"
            }))
        }
    };

    let mut user = User::guest();
    user.username = "guest".to_string();
    user.password = password_hash;

    match UserTable::insert(&user).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "msg": "Guest user created"
        })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to create guest user"
        })),
    }
}

/// delete user admin only
#[delete("/profile/delete")]
pub async fn delete_user(req: HttpRequest, body: web::Json<DeleteUserRequest>) -> impl Responder {
    let current_user = match require_admin(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    if body.username == current_user.username {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Sorry! you cannot delete yourselfu"
        }));
    }

    let all_users = match UserTable::get_all().await {
        Ok(u) => u,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Database error"
            }))
        }
    };

    let admins: Vec<&User> = all_users
        .iter()
        .filter(|u| u.roles.contains(&UserRole::Admin))
        .collect();
    if admins.len() == 1 && admins[0].username == body.username {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "msg": "Cannot delete the only admin"
        }));
    }

    match UserTable::delete_by_username(&body.username).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "msg": format!("User {} deleted", body.username)
        })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Failed to delete user"
        })),
    }
}

/// get all users optional auth admin sees settings
#[get("/users")]
pub async fn get_users(req: HttpRequest, query: web::Query<UsersQuery>) -> impl Responder {
    let current_user = match auth_user_optional(&req).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let mut config = UserConfig::load().unwrap_or_default();
    let enable_guest = UserTable::get_by_username("guest")
        .await
        .ok()
        .flatten()
        .is_some();
    config.enable_guest = enable_guest;

    let mut res = serde_json::json!({
        "settings": {},
        "users": [],
    });

    let is_admin = current_user
        .as_ref()
        .map(|u| u.roles.contains(&UserRole::Admin))
        .unwrap_or(false);

    if is_admin {
        res["settings"] = serde_json::json!({
            "enableGuest": config.enable_guest,
            "usersOnLogin": config.users_on_login,
        });
    } else if current_user.is_some() {
        return HttpResponse::Ok().json(res);
    } else if !config.users_on_login && !config.enable_guest {
        return HttpResponse::Ok().json(res);
    }

    let mut users = match UserTable::get_all().await {
        Ok(list) => list,
        Err(_) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "msg": "Database error"
            }))
        }
    };

    if !config.users_on_login {
        users.retain(|u| u.username.to_lowercase() == "guest");
    }

    users.reverse();
    users.sort_by_key(|u| !u.roles.contains(&UserRole::Admin));
    if let Some(ref me) = current_user {
        users.sort_by_key(|u| u.username != me.username);
    }

    let list: Vec<serde_json::Value> = if query.simplified.unwrap_or(false) {
        users.iter().map(user_to_simplified_value).collect()
    } else {
        users.iter().map(user_to_public_value).collect()
    };

    res["users"] = serde_json::Value::Array(list);
    HttpResponse::Ok().json(res)
}

/// get logged in user empty object if not logged in
#[get("/user")]
pub async fn get_logged_in_user(req: HttpRequest) -> impl Responder {
    match auth_user_optional(&req).await {
        Ok(Some(user)) => HttpResponse::Ok().json(user_to_public_value(&user)),
        Ok(None) => HttpResponse::Ok().json(serde_json::json!({})),
        Err(resp) => resp,
    }
}

/// logout
#[get("/logout")]
pub async fn logout() -> impl Responder {
    let cookie = Cookie::build("access_token_cookie", "")
        .path("/")
        .max_age(CookieDuration::seconds(0))
        .http_only(true)
        .finish();

    HttpResponse::Ok().cookie(cookie).json(serde_json::json!({
        "msg": "Logged out"
    }))
}

// helpers

fn build_access_cookie(token: &str) -> Cookie<'static> {
    Cookie::build("access_token_cookie", token.to_string())
        .path("/")
        .http_only(true)
        .max_age(CookieDuration::seconds(ACCESS_MAX_AGE))
        .finish()
}

fn user_to_identity(user: &User) -> UserIdentity {
    let roles: Vec<String> = user.roles.iter().map(|r| r.as_str().to_string()).collect();
    UserIdentity {
        id: user.id,
        username: user.username.clone(),
        image: user.image.clone(),
        roles,
        extra: user.extra.clone(),
    }
}

fn create_tokens(user: &User, server_id: &str) -> AnyResult<TokenResponse> {
    let identity = user_to_identity(user);
    create_tokens_with_identity(identity, server_id)
}

fn create_tokens_with_identity(
    identity: UserIdentity,
    server_id: &str,
) -> AnyResult<TokenResponse> {
    let username = identity.username.clone();
    let accesstoken = create_jwt(
        identity.clone(),
        server_id,
        "access",
        ACCESS_MAX_AGE as u64,
    )?;
    let refreshtoken = create_jwt(
        identity,
        server_id,
        "refresh",
        REFRESH_MAX_AGE as u64,
    )?;

    Ok(TokenResponse {
        msg: format!("Logged in as {}", username),
        accesstoken,
        refreshtoken,
        maxage: ACCESS_MAX_AGE,
    })
}

async fn require_user(req: &HttpRequest) -> Result<User, HttpResponse> {
    match auth_user_optional(req).await? {
        Some(user) => Ok(user),
        None => Err(HttpResponse::Unauthorized().json(serde_json::json!({
            "msg": "Not authenticated"
        }))),
    }
}

async fn require_admin(req: &HttpRequest) -> Result<User, HttpResponse> {
    let user = require_user(req).await?;
    if user.roles.contains(&UserRole::Admin) {
        Ok(user)
    } else {
        Err(HttpResponse::Forbidden().json(serde_json::json!({
            "msg": "Only admins can do that!"
        })))
    }
}

async fn auth_user_optional(req: &HttpRequest) -> Result<Option<User>, HttpResponse> {
    let token = match access_token(req) {
        Ok(Some(t)) => t,
        Ok(None) => return Ok(None),
        Err(resp) => return Err(resp),
    };

    let config = match UserConfig::load() {
        Ok(cfg) => cfg,
        Err(_) => {
            return Err(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Config error"
            })));
        }
    };

    let claims = match verify_jwt(&token, &config.server_id, Some("access")) {
        Ok(c) => c,
        Err(_) => {
            return Err(HttpResponse::Unauthorized().json(serde_json::json!({
                "msg": "Invalid token"
            })));
        }
    };

    match UserTable::get_by_id(claims.sub.id).await {
        Ok(Some(user)) => Ok(Some(user)),
        Ok(None) => Err(HttpResponse::Unauthorized().json(serde_json::json!({
            "msg": "Invalid token"
        }))),
        Err(_) => Err(HttpResponse::InternalServerError().json(serde_json::json!({
            "msg": "Database error"
        }))),
    }
}

fn bearer_token(req: &HttpRequest) -> Result<Option<String>, HttpResponse> {
    match req.headers().get("Authorization") {
        Some(header_value) => {
            let header_str = header_value.to_str().unwrap_or("").trim();
            if header_str.is_empty() {
                return Err(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "Invalid token format"
                })));
            }

            let token = if let Some(rest) = header_str.strip_prefix("Bearer ") {
                rest
            } else {
                header_str
            };

            if token.is_empty() {
                return Err(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "Invalid token format"
                })));
            }

            Ok(Some(token.to_string()))
        }
        None => Ok(None),
    }
}

fn access_token(req: &HttpRequest) -> Result<Option<String>, HttpResponse> {
    if let Some(cookie) = req.cookie("access_token_cookie") {
        return Ok(Some(cookie.value().to_string()));
    }

    bearer_token(req)
}

fn parse_roles(role_names: &[String]) -> Vec<UserRole> {
    role_names
        .iter()
        .filter_map(|r| UserRole::from_str(r))
        .collect()
}

fn user_to_public_value(user: &User) -> serde_json::Value {
    let roles: Vec<String> = user.roles.iter().map(|r| r.as_str().to_string()).collect();
    serde_json::json!({
        "id": user.id,
        "username": user.username,
        "image": user.image,
        "roles": roles,
        "firstname": user.firstname,
        "email": user.email,
        "extra": user.extra,
    })
}

fn user_to_simplified_value(user: &User) -> serde_json::Value {
    serde_json::json!({
        "id": user.id,
        "username": user.username,
        "firstname": user.firstname,
    })
}

/// configure auth routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(login)
        .service(refresh_token)
        .service(get_pair_code)
        .service(pair_with_code)
        .service(update_profile)
        .service(create_user)
        .service(create_guest)
        .service(delete_user)
        .service(get_users)
        .service(get_logged_in_user)
        .service(logout);
}
