//! Process-local storage. Data is lost on restart and **not shared between
//! replicas**, so it suits one replica or a demo; replace it with a database
//! adapter before relying on the data.
//!
//! Every operation takes the lock once and never awaits while holding it, so
//! uniqueness and capacity checks are atomic with the write.

use std::collections::BTreeMap;
use std::future::ready;
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::modules::items::application::ports::{
    BoxFuture, ItemPage, ItemRepository, Page, RepositoryError,
};
use crate::modules::items::domain::{Item, ItemId, ItemName};

type Store = BTreeMap<ItemId, Item>;

#[derive(Debug)]
pub struct InMemoryItemRepository {
    items: RwLock<Store>,
    capacity: usize,
}

impl InMemoryItemRepository {
    /// `capacity` bounds memory use: inserts beyond it fail with
    /// `CapacityExceeded` instead of growing until the pod is OOM-killed.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            items: RwLock::new(Store::new()),
            capacity,
        }
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, Store>, RepositoryError> {
        self.items.read().map_err(poisoned)
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, Store>, RepositoryError> {
        self.items.write().map_err(poisoned)
    }

    fn insert_now(&self, item: Item) -> Result<(), RepositoryError> {
        let mut items = self.write()?;
        if items.len() >= self.capacity {
            return Err(RepositoryError::CapacityExceeded);
        }
        if name_taken(&items, item.name(), None) {
            return Err(RepositoryError::DuplicateName);
        }
        items.insert(item.id(), item);
        Ok(())
    }

    fn list_now(&self, page: Page) -> Result<ItemPage, RepositoryError> {
        let items = self.read()?;
        Ok(ItemPage {
            total: items.len(),
            items: items
                .values()
                .skip(page.offset() as usize)
                .take(page.limit() as usize)
                .cloned()
                .collect(),
        })
    }

    fn update_now(&self, item: Item) -> Result<bool, RepositoryError> {
        let mut items = self.write()?;
        if !items.contains_key(&item.id()) {
            return Ok(false);
        }
        if name_taken(&items, item.name(), Some(item.id())) {
            return Err(RepositoryError::DuplicateName);
        }
        items.insert(item.id(), item);
        Ok(true)
    }
}

impl ItemRepository for InMemoryItemRepository {
    fn insert(&self, item: Item) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(ready(self.insert_now(item)))
    }

    fn get(&self, id: ItemId) -> BoxFuture<'_, Result<Option<Item>, RepositoryError>> {
        Box::pin(ready(self.read().map(|items| items.get(&id).cloned())))
    }

    fn list(&self, page: Page) -> BoxFuture<'_, Result<ItemPage, RepositoryError>> {
        Box::pin(ready(self.list_now(page)))
    }

    fn update(&self, item: Item) -> BoxFuture<'_, Result<bool, RepositoryError>> {
        Box::pin(ready(self.update_now(item)))
    }

    fn delete(&self, id: ItemId) -> BoxFuture<'_, Result<bool, RepositoryError>> {
        Box::pin(ready(
            self.write().map(|mut items| items.remove(&id).is_some()),
        ))
    }
}

fn name_taken(items: &Store, name: &ItemName, except: Option<ItemId>) -> bool {
    let key = name.unique_key();
    items
        .values()
        .any(|other| Some(other.id()) != except && other.name().unique_key() == key)
}

fn poisoned<T>(_: PoisonError<T>) -> RepositoryError {
    RepositoryError::Unavailable("item store lock poisoned".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::items::domain::Description;

    fn item(name: &str) -> Item {
        Item::new(ItemName::parse(name).unwrap(), Description::default())
    }

    #[tokio::test]
    async fn pagination_is_stable_and_reports_total() {
        let repo = InMemoryItemRepository::new(100);
        for i in 0..25 {
            repo.insert(item(&format!("item-{i}"))).await.unwrap();
        }

        let first = repo.list(Page::new(Some(10), None).unwrap()).await.unwrap();
        let second = repo
            .list(Page::new(Some(10), Some(10)).unwrap())
            .await
            .unwrap();
        let last = repo
            .list(Page::new(Some(10), Some(20)).unwrap())
            .await
            .unwrap();
        let beyond = repo
            .list(Page::new(Some(10), Some(500)).unwrap())
            .await
            .unwrap();

        assert_eq!(first.total, 25);
        assert_eq!(
            (first.items.len(), second.items.len(), last.items.len()),
            (10, 10, 5)
        );
        assert_eq!(beyond.items, Vec::new());

        let mut ids: Vec<_> = [first, second, last]
            .into_iter()
            .flat_map(|p| p.items)
            .map(|i| i.id())
            .collect();
        let before = ids.clone();
        ids.sort();
        ids.dedup();
        assert_eq!(ids, before, "pages are ordered by id with no overlap");
    }

    #[tokio::test]
    async fn update_and_delete_report_missing_ids() {
        let repo = InMemoryItemRepository::new(10);
        assert!(!repo.update(item("x")).await.unwrap());
        assert!(!repo.delete(ItemId::generate()).await.unwrap());
    }
}
