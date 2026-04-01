use async_trait::async_trait;

#[async_trait]
pub trait Notifier: Send + Sync {
    fn name(&self) -> &str;
    async fn test(&self) -> Result<(), crate::error::CoreError>;
}
