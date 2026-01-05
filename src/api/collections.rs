//! Collections API routes

use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::getall::{to_album_card_map, to_artist_card_map};
use crate::db::tables::CollectionTable;
use crate::stores::{AlbumStore, ArtistStore};
use crate::utils::hashing::create_hash;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CollectionItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub hash: String,
    #[serde(default)]
    pub help_text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CollectionResponse {
    pub id: i64,
    pub name: String,
    pub items: Vec<CollectionItem>,
    pub extra: Value,
    pub userid: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: String,
    pub items: Vec<CollectionItem>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCollectionRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct CollectionItemRequest {
    pub item: CollectionItem,
}

#[get("")]
pub async fn get_collections() -> impl Responder {
    match CollectionTable::get_all().await {
        Ok(collections) => {
            let response: Vec<_> = collections
                .into_iter()
                .map(|c| CollectionResponse {
                    id: c.id,
                    name: c.name,
                    items: parse_items(&c.settings),
                    extra: parse_extra(c.extra_data),
                    userid: 0,
                })
                .collect();

            HttpResponse::Ok().json(response)
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to fetch collections: {}", e) })),
    }
}

#[get("/{id}")]
pub async fn get_collection(path: web::Path<i64>) -> impl Responder {
    let id = path.into_inner();

    match CollectionTable::get_by_id(id).await {
        Ok(Some(collection)) => {
            let items = parse_items(&collection.settings);
            let recovered = recover_page_items(&items, false);
            let extra = parse_extra(collection.extra_data);

            HttpResponse::Ok().json(json!({
                "id": collection.id,
                "name": collection.name,
                "items": recovered,
                "extra": extra
            }))
        }
        Ok(None) => HttpResponse::NotFound().json(json!({ "error": "Collection not found" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to fetch collection: {}", e) })),
    }
}

#[post("")]
pub async fn create_collection(body: web::Json<CreateCollectionRequest>) -> impl Responder {
    let validated = match validate_page_items(&body.items, &[]) {
        Ok(items) => items,
        Err(resp) => return resp,
    };

    if validated.is_empty() {
        return HttpResponse::BadRequest().json(json!({ "error": "No items to add" }));
    }

    let settings = serde_json::to_string(&validated).unwrap_or_else(|_| "[]".to_string());
    let extra = json!({ "description": body.description.clone() });
    let extra_str = serde_json::to_string(&extra).unwrap_or_else(|_| "{}".to_string());

    match CollectionTable::insert(&body.name, &settings, Some(&extra_str)).await {
        Ok(_) => HttpResponse::Created().json(json!({ "message": "collection created" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to create collection: {}", e) })),
    }
}

#[put("/{id}")]
pub async fn update_collection(
    path: web::Path<i64>,
    body: web::Json<UpdateCollectionRequest>,
) -> impl Responder {
    let id = path.into_inner();

    let collection = match CollectionTable::get_by_id(id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return HttpResponse::NotFound().json(json!({ "error": "Collection not found" }))
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to fetch collection: {}", e) }))
        }
    };

    let extra = json!({ "description": body.description.clone() });
    let extra_str = serde_json::to_string(&extra).unwrap_or_else(|_| "{}".to_string());

    if let Err(e) = CollectionTable::update(
        id,
        Some(&body.name),
        Some(&collection.settings),
        Some(&extra_str),
    )
    .await
    {
        return HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to update collection: {}", e) }));
    }

    HttpResponse::Ok().json(json!({
        "id": id,
        "name": body.name,
        "extra": extra
    }))
}

#[delete("/{id}")]
pub async fn delete_collection(path: web::Path<i64>) -> impl Responder {
    let id = path.into_inner();

    match CollectionTable::delete(id).await {
        Ok(_) => HttpResponse::Ok().json(json!({ "message": "Collection deleted" })),
        Err(e) => HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to delete collection: {}", e) })),
    }
}

#[post("/{id}/items")]
pub async fn add_collection_item(
    path: web::Path<i64>,
    body: web::Json<CollectionItemRequest>,
) -> impl Responder {
    let id = path.into_inner();
    let collection = match CollectionTable::get_by_id(id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return HttpResponse::NotFound().json(json!({ "error": "Collection not found" }))
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to fetch collection: {}", e) }))
        }
    };

    let mut items = parse_items(&collection.settings);
    let new_items = match validate_page_items(&[body.item.clone()], &items) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if new_items.is_empty() {
        return HttpResponse::BadRequest().json(json!({ "error": "items already in collection" }));
    }
    items.extend(new_items);

    let settings_str = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = CollectionTable::update(
        id,
        None,
        Some(&settings_str),
        collection.extra_data.as_deref(),
    )
    .await
    {
        return HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to update collection: {}", e) }));
    }

    HttpResponse::Ok().json(json!({ "message": "Items added to collection" }))
}

#[delete("/{id}/items")]
pub async fn remove_collection_item(
    path: web::Path<i64>,
    body: web::Json<CollectionItemRequest>,
) -> impl Responder {
    let id = path.into_inner();
    let collection = match CollectionTable::get_by_id(id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return HttpResponse::NotFound().json(json!({ "error": "Collection not found" }))
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(json!({ "error": format!("Failed to fetch collection: {}", e) }))
        }
    };

    let items = parse_items(&collection.settings);
    let updated = remove_page_items(&items, &body.item);
    let settings_str = serde_json::to_string(&updated).unwrap_or_else(|_| "[]".to_string());

    if let Err(e) = CollectionTable::update(
        id,
        None,
        Some(&settings_str),
        collection.extra_data.as_deref(),
    )
    .await
    {
        return HttpResponse::InternalServerError()
            .json(json!({ "error": format!("Failed to update collection: {}", e) }));
    }

    HttpResponse::Ok().json(json!({ "message": "Item removed from collection" }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_collections)
        .service(get_collection)
        .service(create_collection)
        .service(update_collection)
        .service(delete_collection)
        .service(add_collection_item)
        .service(remove_collection_item);
}

fn parse_items(settings: &str) -> Vec<CollectionItem> {
    serde_json::from_str(settings).unwrap_or_default()
}

fn parse_extra(extra: Option<String>) -> Value {
    extra
        .and_then(|e| serde_json::from_str(&e).ok())
        .unwrap_or_else(|| json!({}))
}

fn validate_page_items(
    items: &[CollectionItem],
    existing: &[CollectionItem],
) -> Result<Vec<CollectionItem>, HttpResponse> {
    let mut validated: Vec<CollectionItem> = Vec::new();
    let mut indexed = std::collections::HashSet::new();

    for item in existing {
        indexed.insert(hash_item(item));
    }

    for item in items {
        let hash = hash_item(item);
        if indexed.contains(&hash) {
            continue;
        }

        match item.item_type.as_str() {
            "album" => {
                if AlbumStore::get().get_by_hash(&item.hash).is_some() {
                    validated.push(item.clone());
                    indexed.insert(hash);
                }
            }
            "artist" => {
                if ArtistStore::get().get_by_hash(&item.hash).is_some() {
                    validated.push(item.clone());
                    indexed.insert(hash);
                }
            }
            _ => {
                return Err(HttpResponse::BadRequest().json(json!({"error": "Invalid item type"})));
            }
        }
    }

    Ok(validated)
}

fn remove_page_items(existing: &[CollectionItem], item: &CollectionItem) -> Vec<CollectionItem> {
    let target = hash_item(item);
    existing
        .iter()
        .cloned()
        .filter(|i| hash_item(i) != target)
        .collect()
}

fn recover_page_items(items: &[CollectionItem], for_homepage: bool) -> Vec<Value> {
    let mut recovered: Vec<Value> = Vec::new();

    for item in items {
        match item.item_type.as_str() {
            "album" => {
                if let Some(mut album) = AlbumStore::get().get_by_hash(&item.hash) {
                    let mut map = to_album_card_map(&mut album);
                    if for_homepage {
                        map.remove("type");
                        recovered.push(json!({ "item": map, "type": "album" }));
                    } else {
                        recovered.push(Value::Object(map));
                    }
                }
            }
            "artist" => {
                if let Some(mut artist) = ArtistStore::get().get_by_hash(&item.hash) {
                    let mut map = to_artist_card_map(&mut artist);
                    if for_homepage {
                        map.remove("type");
                        recovered.push(json!({ "item": map, "type": "artist" }));
                    } else {
                        recovered.push(Value::Object(map));
                    }
                }
            }
            _ => {}
        }
    }

    recovered.reverse();
    recovered
}

fn hash_item(item: &CollectionItem) -> String {
    let payload = serde_json::to_string(item).unwrap_or_else(|_| String::new());
    create_hash(&[&payload], true)
}
