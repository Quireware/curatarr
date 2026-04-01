use curatarr_core::traits::repository::Repository;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Repository>,
}
