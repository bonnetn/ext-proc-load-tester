use crate::app::error::Result;

#[allow(dead_code)]
pub(crate) trait Worker {
    fn run(&self) -> impl Future<Output = Result<()>> + Send;
}
