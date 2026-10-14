use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::model::Product;

const STORAGE_LATENCY: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Clone)]
pub(crate) struct Catalog {
    path: Arc<Path>,
    products: Arc<RwLock<HashMap<u64, Product>>>,
}

impl Catalog {
    pub(crate) async fn load(path: PathBuf) -> io::Result<Self> {
        let products = read_products(&path).await?;
        Ok(Self {
            path: path.into(),
            products: Arc::new(RwLock::new(products)),
        })
    }

    pub(crate) fn get(&self, id: u64) -> Option<Product> {
        self.products.read().ok()?.get(&id).cloned()
    }

    pub(crate) fn all(&self) -> Vec<Product> {
        self.products
            .read()
            .map(|products| products.values().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn search(&self, query: &str, limit: usize) -> Vec<Product> {
        let query = query.to_ascii_lowercase();
        let Ok(products) = self.products.read() else {
            return Vec::new();
        };
        let mut matches: Vec<_> = products
            .values()
            .filter(|product| {
                product.name.to_ascii_lowercase().contains(&query)
                    || product.category.to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        matches.sort_by_key(|product| product.id);
        matches.truncate(limit);
        matches
    }

    pub(crate) async fn refresh(&self) -> io::Result<()> {
        let products = read_products_from_storage(&self.path)?;
        *self
            .products
            .write()
            .map_err(|_| io::Error::other("catalog lock poisoned"))? = products;
        Ok(())
    }
}

async fn read_products(path: &Path) -> io::Result<HashMap<u64, Product>> {
    let contents = tokio::fs::read_to_string(path).await?;
    parse_products(&contents)
}

fn read_products_from_storage(path: &Path) -> io::Result<HashMap<u64, Product>> {
    let contents = std::fs::read_to_string(path)?;
    std::thread::sleep(STORAGE_LATENCY); // Simulates a slow storage read, do not modify.
    parse_products(&contents)
}

fn parse_products(contents: &str) -> io::Result<HashMap<u64, Product>> {
    let products: Vec<Product> = serde_json::from_str(contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(products
        .into_iter()
        .map(|product| (product.id, product))
        .collect())
}
