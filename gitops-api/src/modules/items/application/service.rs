use std::fmt;
use std::sync::Arc;

use super::error::ItemError;
use super::ports::{ItemPage, ItemRepository, Page};
use crate::modules::items::domain::{Description, Item, ItemId, ItemName};

/// Unvalidated input for create and full replace (`PUT`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemInput {
    pub name: String,
    pub description: String,
}

impl ItemInput {
    fn validate(&self) -> Result<(ItemName, Description), ItemError> {
        Ok((
            ItemName::parse(&self.name)?,
            Description::parse(&self.description)?,
        ))
    }
}

/// Items use cases. Cheap to clone (one `Arc`).
#[derive(Clone)]
pub struct ItemService {
    repo: Arc<dyn ItemRepository>,
}

impl fmt::Debug for ItemService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemService").finish_non_exhaustive()
    }
}

impl ItemService {
    #[must_use]
    pub fn new(repo: Arc<dyn ItemRepository>) -> Self {
        Self { repo }
    }

    /// # Errors
    /// `Invalid`, `NameTaken`, `CapacityExceeded` or `Storage`.
    pub async fn create(&self, input: &ItemInput) -> Result<Item, ItemError> {
        let (name, description) = input.validate()?;
        let item = Item::new(name, description);
        self.repo.insert(item.clone()).await?;
        Ok(item)
    }

    /// # Errors
    /// `Invalid` (malformed id), `NotFound` or `Storage`.
    pub async fn get(&self, id: &str) -> Result<Item, ItemError> {
        let id = ItemId::parse(id)?;
        self.repo.get(id).await?.ok_or(ItemError::NotFound)
    }

    /// # Errors
    /// `Invalid` (bad limit) or `Storage`.
    pub async fn list(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<(ItemPage, Page), ItemError> {
        let page = Page::new(limit, offset)?;
        Ok((self.repo.list(page).await?, page))
    }

    /// Full replace; the id never changes.
    ///
    /// # Errors
    /// `Invalid`, `NotFound`, `NameTaken` or `Storage`.
    pub async fn replace(&self, id: &str, input: &ItemInput) -> Result<Item, ItemError> {
        let id = ItemId::parse(id)?;
        let (name, description) = input.validate()?;
        let item = Item::with_id(id, name, description);
        if self.repo.update(item.clone()).await? {
            Ok(item)
        } else {
            Err(ItemError::NotFound)
        }
    }

    /// # Errors
    /// `Invalid`, `NotFound` or `Storage`.
    pub async fn delete(&self, id: &str) -> Result<(), ItemError> {
        let id = ItemId::parse(id)?;
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(ItemError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::items::infrastructure::InMemoryItemRepository;

    fn service(capacity: usize) -> ItemService {
        ItemService::new(Arc::new(InMemoryItemRepository::new(capacity)))
    }

    fn input(name: &str) -> ItemInput {
        ItemInput {
            name: name.to_owned(),
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let svc = service(10);
        let created = svc
            .create(&ItemInput {
                name: "  Widget ".into(),
                description: "blue".into(),
            })
            .await
            .unwrap();
        assert_eq!(created.name().as_str(), "Widget");

        let fetched = svc.get(&created.id().to_string()).await.unwrap();
        assert_eq!(fetched, created);
    }

    #[tokio::test]
    async fn invalid_input_never_reaches_storage() {
        let svc = service(10);
        assert!(matches!(
            svc.create(&input("")).await,
            Err(ItemError::Invalid(e)) if e.field == "name"
        ));
        let (page, _) = svc.list(None, None).await.unwrap();
        assert_eq!(page.total, 0);
    }

    #[tokio::test]
    async fn names_are_unique_case_insensitively() {
        let svc = service(10);
        svc.create(&input("Widget")).await.unwrap();
        assert!(matches!(
            svc.create(&input("wIdGeT")).await,
            Err(ItemError::NameTaken)
        ));
    }

    #[tokio::test]
    async fn replace_keeps_id_and_enforces_uniqueness() {
        let svc = service(10);
        let a = svc.create(&input("a")).await.unwrap();
        svc.create(&input("b")).await.unwrap();

        let renamed = svc
            .replace(&a.id().to_string(), &input("A2"))
            .await
            .unwrap();
        assert_eq!(renamed.id(), a.id());
        // Renaming to its own name (different case) is not a conflict.
        svc.replace(&a.id().to_string(), &input("a2"))
            .await
            .unwrap();
        assert!(matches!(
            svc.replace(&a.id().to_string(), &input("B")).await,
            Err(ItemError::NameTaken)
        ));
    }

    #[tokio::test]
    async fn missing_and_malformed_ids() {
        let svc = service(10);
        let unknown = ItemId::generate().to_string();
        assert!(matches!(svc.get(&unknown).await, Err(ItemError::NotFound)));
        assert!(matches!(
            svc.replace(&unknown, &input("x")).await,
            Err(ItemError::NotFound)
        ));
        assert!(matches!(
            svc.delete(&unknown).await,
            Err(ItemError::NotFound)
        ));
        assert!(matches!(svc.get("nope").await, Err(ItemError::Invalid(_))));
    }

    #[tokio::test]
    async fn delete_removes_the_item() {
        let svc = service(10);
        let id = svc.create(&input("gone")).await.unwrap().id().to_string();
        svc.delete(&id).await.unwrap();
        assert!(matches!(svc.get(&id).await, Err(ItemError::NotFound)));
        // The name is free again.
        svc.create(&input("gone")).await.unwrap();
    }

    #[tokio::test]
    async fn capacity_is_enforced() {
        let svc = service(2);
        svc.create(&input("1")).await.unwrap();
        svc.create(&input("2")).await.unwrap();
        assert!(matches!(
            svc.create(&input("3")).await,
            Err(ItemError::CapacityExceeded)
        ));
    }
}
