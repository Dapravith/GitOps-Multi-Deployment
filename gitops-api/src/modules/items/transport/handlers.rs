//! Thin handlers: extract → call the service → shape the response.
//! No business rules live here.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

use super::dto::{ItemListResponse, ItemRequest, ItemResponse, ListParams};
use crate::app::ApiError;
use crate::app::extract::{ApiJson, ApiQuery};
use crate::modules::items::application::{ItemInput, ItemService};

pub(super) async fn create(
    State(items): State<ItemService>,
    ApiJson(body): ApiJson<ItemRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let item = items.create(&ItemInput::from(body)).await?;
    let location = format!("/items/{}", item.id());
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(ItemResponse::from(&item)),
    ))
}

pub(super) async fn list(
    State(items): State<ItemService>,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<ItemListResponse>, ApiError> {
    let (page, window) = items.list(params.limit, params.offset).await?;
    Ok(Json(ItemListResponse::new(&page, window)))
}

pub(super) async fn get(
    State(items): State<ItemService>,
    Path(id): Path<String>,
) -> Result<Json<ItemResponse>, ApiError> {
    let item = items.get(&id).await?;
    Ok(Json(ItemResponse::from(&item)))
}

pub(super) async fn replace(
    State(items): State<ItemService>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ItemRequest>,
) -> Result<Json<ItemResponse>, ApiError> {
    let item = items.replace(&id, &ItemInput::from(body)).await?;
    Ok(Json(ItemResponse::from(&item)))
}

pub(super) async fn delete(
    State(items): State<ItemService>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    items.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
