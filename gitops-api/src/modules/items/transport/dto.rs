//! Wire format. Kept separate from domain types so the JSON contract and the
//! domain model can evolve independently.

use serde::{Deserialize, Serialize};

use crate::modules::items::application::ItemInput;
use crate::modules::items::application::ports::{ItemPage, Page};
use crate::modules::items::domain::Item;

/// Body of `POST /items` and `PUT /items/{id}`. Unknown fields are rejected
/// so a typo (or an attempt to set `id`) is an error, not silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemRequest {
    name: String,
    #[serde(default)]
    description: String,
}

impl From<ItemRequest> for ItemInput {
    fn from(req: ItemRequest) -> Self {
        Self {
            name: req.name,
            description: req.description,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListParams {
    pub(super) limit: Option<u32>,
    pub(super) offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(super) struct ItemResponse {
    id: String,
    name: String,
    description: String,
}

impl From<&Item> for ItemResponse {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id().to_string(),
            name: item.name().as_str().to_owned(),
            description: item.description().as_str().to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ItemListResponse {
    items: Vec<ItemResponse>,
    total: usize,
    limit: u32,
    offset: u32,
}

impl ItemListResponse {
    pub(super) fn new(page: &ItemPage, window: Page) -> Self {
        Self {
            items: page.items.iter().map(ItemResponse::from).collect(),
            total: page.total,
            limit: window.limit(),
            offset: window.offset(),
        }
    }
}
