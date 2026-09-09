use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    fn name(&self) -> &str;
    async fn notify(&self, event: &Notification) -> Result<(), crate::error::CoreError>;
    async fn test(&self) -> Result<(), crate::error::CoreError>;
}
